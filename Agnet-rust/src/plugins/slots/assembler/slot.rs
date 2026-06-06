/*! AssemblerSlot —— 上下文组装器 SlotPlugin（设计文档 §11）

遵循 Slot接入协议 §1/§6。
*/

use async_trait::async_trait;
use std::sync::Arc;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::assembler::*;
use crate::shared_types::context::{CONTEXT_ASSEMBLER_MESSAGES, CONTEXT_LLM_CONFIG, CONTEXT_TOOLS};
use crate::shared_types::llm::LlmConfig;

use super::assembly::budget::compute_budget;
use super::assembly::builder::MessageBuilder;
use super::assembly::collector::BlockCollector;
use super::assembly::quota::allocate_quotas;
use super::compaction::DocumentCompactor;
use super::config::LOG_PREFIX;
use super::providers::build_providers;
use super::rule_pool::RuleLlmSelector;

pub struct AssemblerSlot {
    config: AssemblerConfig,
    providers: Vec<Arc<dyn ContextProvider>>,
    _rule_selector: Option<RuleLlmSelector>,
}

impl AssemblerSlot {
    pub fn new() -> Self {
        Self {
            config: AssemblerConfig::default(),
            providers: vec![],
            _rule_selector: None,
        }
    }
}

impl Default for AssemblerSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for AssemblerSlot {
    fn name(&self) -> &str {
        "assembler"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut config: AssemblerConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("assembler 配置解析失败: {}", e)))?;
        // 遵循 S-R02：init 失败 = 不加载

        config.resolve_paths(&ctx.data_dir);

        // 加载模板文件（设计文档 §5.1，宪法 §7f：文件不存在时使用默认字符串）
        let base_template = super::config::load_template(
            &config.base_prompt_path,
            "You are aagnet, an AI agent.\n\n{{rules}}\n\n<env>\n{{env_info}}\n</env>",
        );
        let injection_template = super::config::load_template(
            &config.injection_layout_path,
            "## Agent Identity\n{{identity}}\n\n## Working Memory\n{{working_memory}}\n\n## Related Knowledge\n{{vector_memory}}",
        );

        let compactor = DocumentCompactor::new(config.compaction.clone());
        let rule_selector = if config.rule_pool.enabled {
            Some(RuleLlmSelector::new(config.rule_pool.clone()))
        } else {
            None
        };
        let providers = build_providers(
            &config,
            &compactor,
            &rule_selector,
            &base_template,
            &injection_template,
        );

        self.config = config;
        self.providers = providers;
        self._rule_selector = rule_selector;
        tracing::info!(
            "{} 初始化完成 (enabled={})",
            LOG_PREFIX,
            self.config.enabled
        );
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        if !self.config.enabled {
            tracing::debug!("{} disabled, skipping", LOG_PREFIX);
            return Ok(SlotDirective::Continue);
        }

        let start = std::time::Instant::now();

        // Phase 1: 读取历史消息
        let history_messages = ap.messages().to_vec();
        let history_tokens: usize = history_messages.iter().map(|m| m.estimate_tokens()).sum();

        // Phase 2: 估算工具 token
        let tools_tokens = ap
            .read_context_raw(CONTEXT_TOOLS)
            .and_then(|any| any.downcast_ref::<Vec<crate::shared_types::ToolDefinition>>())
            .map(|tools| tools.len() * 50)
            .unwrap_or(0);

        // Phase 3: context_window（从 LlmConfig 读取）
        let context_window = ap
            .read_context_raw(CONTEXT_LLM_CONFIG)
            .and_then(|any| any.downcast_ref::<LlmConfig>())
            .map(|c| c.context_window as usize)
            .unwrap_or(128_000);

        // Phase 4: 预算计算（设计文档 §7.1）
        let budget = compute_budget(context_window, tools_tokens, history_tokens, &self.config);
        let injection_budget = budget
            .total_available
            .saturating_sub(history_tokens)
            .min(self.config.max_injection_tokens);

        // Phase 5: 配额分配（设计文档 §7.3）
        let quotas = allocate_quotas(
            injection_budget,
            &self.config.injection_policy,
            &self.config,
        );

        // Phase 6: 内容收集（设计文档 §7.4）
        let (mut blocks, _provider_stats, _warnings) =
            BlockCollector::collect(&self.providers, ap, &quotas, &self.config.providers).await;

        // Phase 7: 厂商输出适配（设计文档 §9）
        let output_adapter: Option<Arc<dyn LlmOutputAdapter>> =
            if self.config.output_adapter_enabled {
                ap.provider_raw(crate::shared_types::llm::PROVIDER_LLM_OUTPUT_ADAPTER)
                    .and_then(|raw| {
                        raw.downcast::<crate::shared_types::DynProvider<dyn LlmOutputAdapter>>()
                            .ok()
                            .map(|wrapper| {
                                let adapter = &wrapper.0;
                                for block in &mut blocks {
                                    block.content = adapter
                                        .adapt_context_block(&block.section_title, &block.content);
                                }
                                tracing::debug!(
                                    "{} 已应用厂商适配: {}",
                                    LOG_PREFIX,
                                    adapter.provider_name()
                                );
                                wrapper.0.clone()
                            })
                    })
            } else {
                None
            };

        // Phase 8: 消息拼装（设计文档 §7.5）- 内部已包含紧急裁剪
        let (mut messages, truncation_applied) =
            MessageBuilder::build(&blocks, &history_messages, &self.config);

        // 在 System 消息上应用 adapt_system_prompt
        if let Some(ref adapter) = output_adapter {
            if let Some(msg) = messages.first_mut() {
                if msg.role == crate::shared_types::MessageRole::System {
                    if let Some(content_block) = msg.content.first_mut() {
                        if let crate::shared_types::ContentBlock::Text(ref text) = content_block {
                            let adapted =
                                adapter.adapt_system_prompt(text.as_str(), context_window);
                            *content_block = crate::shared_types::ContentBlock::Text(adapted);
                        }
                    }
                }
            }
        }

        // Phase 9: 写入 StepContext
        let final_total_tokens: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        ap.write_context_raw(CONTEXT_ASSEMBLER_MESSAGES, Box::new(messages))
            .unwrap_or_else(|e| {
                tracing::warn!("{} 写入 assembler_messages 失败: {}", LOG_PREFIX, e)
            });

        // Phase 10: AssemblyReport（设计文档 §13）
        if self.config.debug {
            let adapter_name = output_adapter
                .as_ref()
                .map(|a| a.provider_name().to_string());
            let report = AssemblyReport {
                request_id: format!(
                    "asm-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                ),
                session_id: ap.session_id().to_string(),
                context_window,
                total_available: budget.total_available,
                history_tokens,
                injection_budget,
                final_total_tokens,
                selected_policy: self.config.injection_policy.clone(),
                provider_stats: _provider_stats,
                rules_group: String::new(),
                adapter_used: adapter_name,
                truncation_applied,
                warnings: _warnings,
                assembly_duration: start.elapsed(),
            };
            tracing::info!(
                "[assembler] 组装报告: {} blocks",
                report.provider_stats.len()
            );
        }

        tracing::debug!("{} 组装完成 ({:?})", LOG_PREFIX, start.elapsed());
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("{} shutdown", LOG_PREFIX);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::sync::Arc;

    use super::*;
    use crate::core::access::SlotAccessPoint;
    use crate::core::types::error::PluginError;
    use crate::core::types::plugin::{AgentConfig, PluginInitContext};
    use crate::shared_types::Message;

    struct MockAccessPoint {
        messages: Vec<Message>,
        context: std::collections::HashMap<String, Box<dyn Any + Send + Sync>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                messages: Vec::new(),
                context: std::collections::HashMap::new(),
            }
        }

        fn with_message(mut self, msg: Message) -> Self {
            self.messages.push(msg);
            self
        }
    }

    impl SlotAccessPoint for MockAccessPoint {
        fn messages(&self) -> &[Message] {
            &self.messages
        }
        fn session_id(&self) -> &str {
            "test-session"
        }
        fn phase_name(&self) -> &str {
            "assembler"
        }
        fn current_iteration(&self) -> usize {
            1
        }
        fn write_observation(&mut self, _: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            self.context.insert(key.to_string(), val);
            Ok(())
        }
        fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
            self.context.get(key).map(|b| b.as_ref())
        }
        fn request_jump(&self, _: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, _: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            None
        }

        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn make_ctx() -> PluginInitContext {
        PluginInitContext::new(
            "assembler",
            serde_json::json!({}),
            AgentConfig::default(),
            std::env::temp_dir(),
        )
    }

    #[tokio::test]
    async fn test_init_success() {
        let mut slot = AssemblerSlot::new();
        let ctx = make_ctx();
        let result = slot.init(&ctx).await;
        assert!(result.is_ok());
        assert!(!slot.config.enabled);
    }

    #[tokio::test]
    async fn test_init_invalid_config() {
        let mut slot = AssemblerSlot::new();
        let ctx = PluginInitContext::new(
            "assembler",
            serde_json::json!({"enabled": "not_a_bool"}),
            AgentConfig::default(),
            std::env::temp_dir(),
        );
        let result = slot.init(&ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_disabled_returns_continue() {
        let mut slot = AssemblerSlot::new();
        let ctx = make_ctx();
        slot.init(&ctx).await.expect("init should succeed");
        let mut ap = MockAccessPoint::new();
        let directive = slot.run(&mut ap).await.expect("run should succeed");
        assert_eq!(directive, SlotDirective::Continue);
    }

    #[tokio::test]
    async fn test_run_enabled_with_messages() {
        let mut slot = AssemblerSlot::new();
        let ctx = PluginInitContext::new(
            "assembler",
            serde_json::json!({"enabled": true}),
            AgentConfig::default(),
            std::env::temp_dir(),
        );
        slot.init(&ctx).await.expect("init should succeed");
        let mut ap = MockAccessPoint::new().with_message(Message::text(
            crate::shared_types::MessageRole::User,
            "Hello",
        ));
        let directive = slot.run(&mut ap).await.expect("run should succeed");
        assert_eq!(directive, SlotDirective::Continue);
        assert!(ap.read_context_raw("assembler_messages").is_some());
    }

    #[tokio::test]
    async fn test_shutdown_ok() {
        let mut slot = AssemblerSlot::new();
        let result = slot.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_name_returns_assembler() {
        let slot = AssemblerSlot::new();
        assert_eq!(slot.name(), "assembler");
    }
}
