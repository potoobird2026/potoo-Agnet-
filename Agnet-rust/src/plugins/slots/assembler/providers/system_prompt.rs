/*! SystemPromptProvider（设计文档 §5.1，pri=0）

提供基础 System Prompt 模板 + 规则注入 + 环境信息。
*/

use async_trait::async_trait;
use std::sync::Arc;

use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::context::{
    CONTEXT_AGENT_CONFIG, CONTEXT_IDENTITY, CONTEXT_WORKING_MEMORY,
};
use crate::shared_types::IdentitySection;

use super::super::config::{
    COMPRESSION_SUMMARY_PLACEHOLDER, ENV_INFO_PLACEHOLDER, IDENTITY_PLACEHOLDER,
    PLATFORM_PLACEHOLDER, RULES_PLACEHOLDER, TODAY_PLACEHOLDER, VECTOR_MEMORY_PLACEHOLDER,
    WORKING_MEMORY_PLACEHOLDER, WORK_DIR_PLACEHOLDER,
};
use super::super::rule_pool::RuleLlmSelector;

pub struct SystemPromptProvider {
    rule_selector: Option<Arc<RuleLlmSelector>>,
    base_template: String,
    injection_template: String,
}

impl SystemPromptProvider {
    pub fn new(
        rule_selector: &Option<RuleLlmSelector>,
        base_template: &str,
        injection_template: &str,
    ) -> Self {
        Self {
            rule_selector: rule_selector.clone().map(Arc::new),
            base_template: base_template.to_string(),
            injection_template: injection_template.to_string(),
        }
    }

    fn build_env_info(&self, ap: &dyn SlotAccessPoint) -> String {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let platform = if cfg!(target_os = "windows") {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };
        let agent_info = ap
            .read_context_raw(CONTEXT_AGENT_CONFIG)
            .and_then(|any| any.downcast_ref::<String>())
            .cloned()
            .unwrap_or_default();
        let base = format!(
            "工作目录: {}\n平台: {}\n会话: {}",
            cwd,
            platform,
            ap.session_id()
        );
        if agent_info.is_empty() {
            base
        } else {
            format!("{}\n{}", base, agent_info)
        }
    }
}

#[async_trait]
impl ContextProvider for SystemPromptProvider {
    fn name(&self) -> &str {
        "system_prompt"
    }
    fn priority(&self) -> u8 {
        0
    }
    fn allow_truncation(&self) -> bool {
        false
    }
    fn silent_on_empty(&self) -> bool {
        false
    }

    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize {
        config.max_tokens
    }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        let mut base = if self.base_template.is_empty() {
            String::from("You are aagnet, an AI agent.\n\n{{rules}}\n\n<env>\n{{env_info}}\n</env>")
        } else {
            self.base_template.clone()
        };

        // 替换规则
        if let Some(selector) = &self.rule_selector {
            let rules_group = selector.select(ap).await;
            if !rules_group.rules.is_empty() {
                let rules_text = rules_group
                    .rules
                    .iter()
                    .map(|r| format!("- {}", r))
                    .collect::<Vec<_>>()
                    .join("\n");
                base = base.replace(RULES_PLACEHOLDER, &rules_text);
            } else {
                base = base.replace(RULES_PLACEHOLDER, "");
            }
        } else {
            base = base.replace(RULES_PLACEHOLDER, "");
        }

        // 替换环境信息
        let env_info = self.build_env_info(ap);
        base = base.replace(ENV_INFO_PLACEHOLDER, &env_info);

        // 拼接注入布局模板
        let content = if self.injection_template.is_empty() {
            format!("{}\n\n## Conversation Context\n{{identity}}\n{{working_memory}}\n{{vector_memory}}", base)
        } else {
            format!("{}\n\n{}", base, self.injection_template)
        };

        // 替换 content 中剩余的占位符
        let content = replace_placeholders(&content, ap);

        let tokens = (content.len() as f64 / 4.0).ceil() as usize;
        let max_tokens = quota.max_tokens.min(tokens);

        Ok(ProvidedContext {
            blocks: vec![ContextBlock {
                section_title: "## System".into(),
                content,
                source: "system_prompt".into(),
                token_count: max_tokens,
            }],
            tokens_used: max_tokens,
        })
    }
}

fn replace_placeholders(content: &str, ap: &dyn SlotAccessPoint) -> String {
    let mut result = content.to_string();

    // {{identity}} — 从 context 读取 IdentitySection.content
    let identity_text = ap
        .read_context_raw(CONTEXT_IDENTITY)
        .and_then(|any| any.downcast_ref::<IdentitySection>())
        .map(|id| id.content.clone())
        .unwrap_or_default();
    result = result.replace(IDENTITY_PLACEHOLDER, &identity_text);

    // {{working_memory}} — 从 context 读取 Vec<MemoryFileEntry>
    let working_memory_text = ap
        .read_context_raw(CONTEXT_WORKING_MEMORY)
        .and_then(|any| any.downcast_ref::<Vec<crate::shared_types::MemoryFileEntry>>())
        .map(|entries| {
            entries
                .iter()
                .map(|e| format!("- {} [{}]: {}", e.summary, e.id, e.content.as_deref().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    result = result.replace(WORKING_MEMORY_PLACEHOLDER, &working_memory_text);

    // {{vector_memory}} — 非直接上下文值，留空
    result = result.replace(VECTOR_MEMORY_PLACEHOLDER, "");

    // {{compression_summary}} — 非直接上下文值，留空
    result = result.replace(COMPRESSION_SUMMARY_PLACEHOLDER, "");

    // {{work_dir}}
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    result = result.replace(WORK_DIR_PLACEHOLDER, &cwd);

    // {{platform}}
    let platform = if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    };
    result = result.replace(PLATFORM_PLACEHOLDER, platform);

    // {{today}}
    use std::time::{SystemTime, UNIX_EPOCH};
    let today = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let days = secs / 86400;
            let y = 1970 + (days as f64 / 365.25) as u64;
            format!("{}", y)
        })
        .unwrap_or_else(|_| "unknown".to_string());
    result = result.replace(TODAY_PLACEHOLDER, &today);

    result
}
