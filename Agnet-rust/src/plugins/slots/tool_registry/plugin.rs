use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::context::CONTEXT_TOOLS;
use crate::shared_types::{DynProvider, ToolDefinition, ToolProvider, PROVIDER_TOOL};

/// 工具注册槽口 —— Pipeline CONTEXT 阶段
///
/// 职责：从 ProviderRegistry 获取工具列表，写入 StepContext。
///
/// 设计决策：
/// - 无状态：不持有跨 run() 的可变状态（S-R03）
/// - 无内部组件：职责单一，不需要 Orchestrator（组件协议 §0）
/// - 降级策略：Provider 不可用时返回空列表，不中断 Pipeline（Slot协议 §7）
///
/// 规范遵守：
/// - 跨平台规范：无硬编码值、无文件路径、无网络调用
/// - Slot协议 §1：SlotPlugin 单入口
/// - Slot协议 §2：只通过 SlotAccessPoint 交互
/// - Slot协议 §3：元数据 permissions=["context:write"], requires=["tool"]
/// - Slot协议 §4：权限 tag 与实际调用一致
/// - Slot协议 §5：返回 Continue
/// - Slot协议 §7：不缓存跨 run() 状态
pub struct ToolRegistrySlot;

impl ToolRegistrySlot {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolRegistrySlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for ToolRegistrySlot {
    fn name(&self) -> &str {
        "tool_registry"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 无状态 Slot，无需初始化
        // S-R02：init 失败意味着插件不加载——此处不会失败
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // Step 1: 获取 "tool" Provider
        let tools: Vec<ToolDefinition> = ap
            .provider_raw(PROVIDER_TOOL)
            .and_then(|raw| {
                raw.downcast::<DynProvider<dyn ToolProvider>>()
                    .ok()
                    .map(|wrapper| wrapper.0.list())
            })
            .unwrap_or_default();

        // Step 2: 写入 StepContext（注意：不要用 Arc 包裹——read_context_raw
        // 返回 &dyn Any，llm_thinker 直接 downcast_ref::<Vec<ToolDefinition>>()。
        // 如果写入 Arc<Vec<ToolDefinition>>，downcast 会因类型不匹配而失败。）
        ap.write_context_raw(CONTEXT_TOOLS, Box::new(tools))?;

        // Step 3: 返回 Continue（CONTEXT 阶段必须完成）
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // 无状态 Slot，无需清理
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::shared_types::Message;
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::*;
    use crate::core::slot::SlotDirective;
    use crate::shared_types::ToolDefinition;
    use crate::shared_types::ToolSource;

    // ── Mock ToolProvider ───────────────────────────────────────────

    struct MockToolProvider {
        tools: Vec<ToolDefinition>,
    }

    #[async_trait]
    impl ToolProvider for MockToolProvider {
        fn list(&self) -> Vec<ToolDefinition> {
            self.tools.clone()
        }

        async fn execute(
            &self,
            _tool_name: &str,
            _arguments: serde_json::Value,
            _timeout: std::time::Duration,
        ) -> Result<String, crate::shared_types::ToolError> {
            Ok(String::new())
        }
    }

    // ── Mock SlotAccessPoint ────────────────────────────────────────

    struct MockAccessPoint {
        providers: HashMap<String, Arc<dyn Any + Send + Sync>>,
        context: HashMap<String, Box<dyn Any + Send + Sync>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                providers: HashMap::new(),
                context: HashMap::new(),
            }
        }

        fn with_provider(mut self, name: &str, provider: Arc<dyn Any + Send + Sync>) -> Self {
            self.providers.insert(name.to_string(), provider);
            self
        }

        fn get_tools(&self) -> Option<Vec<ToolDefinition>> {
            self.context
                .get("tools")
                .and_then(|v| v.downcast_ref::<Vec<ToolDefinition>>())
                .cloned()
        }
    }

    impl SlotAccessPoint for MockAccessPoint {
        fn messages(&self) -> &[crate::shared_types::Message] {
            &[]
        }
        fn session_id(&self) -> &str {
            "test"
        }
        fn phase_name(&self) -> &str {
            "context"
        }
        fn current_iteration(&self) -> usize {
            0
        }
        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
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
        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            self.providers.get(name).cloned()
        }

        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    // ── 测试用例 ───────────────────────────────────────────────────

    /// 测试：正常流程 —— Provider 包含 2 个工具
    #[tokio::test]
    async fn 正常流程_两个工具() {
        let t1 = ToolDefinition {
            name: "read_file".into(),
            description: "读取文件".into(),
            parameters: serde_json::json!({"type":"object"}),
            entry: "read_file".into(),
            source: ToolSource::Builtin,
        };
        let t2 = ToolDefinition {
            name: "write_file".into(),
            description: "写入文件".into(),
            parameters: serde_json::json!({"type":"object"}),
            entry: "write_file".into(),
            source: ToolSource::Builtin,
        };

        let provider: Arc<dyn ToolProvider> = Arc::new(MockToolProvider {
            tools: vec![t1, t2],
        });
        // 使用 DynProvider 包装（遵循 shared_types契约协议 §4）
        let any_provider: Arc<dyn Any + Send + Sync> = Arc::new(DynProvider(provider));
        let mut ap = MockAccessPoint::new().with_provider("tool", any_provider);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
        assert_eq!(ap.get_tools().unwrap().len(), 2);
    }

    /// 测试：空工具列表
    #[tokio::test]
    async fn 空工具列表() {
        let provider: Arc<dyn ToolProvider> = Arc::new(MockToolProvider { tools: vec![] });
        let any_provider: Arc<dyn Any + Send + Sync> = Arc::new(DynProvider(provider));
        let mut ap = MockAccessPoint::new().with_provider("tool", any_provider);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert!(ap.get_tools().unwrap().is_empty());
    }

    /// 测试：Provider 未注册 —— 降级为空列表
    #[tokio::test]
    #[allow(non_snake_case)]
    async fn Provider未注册_降级() {
        let mut ap = MockAccessPoint::new();

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    /// 测试：downcast 失败 —— 降级为空列表
    #[tokio::test]
    async fn downcast失败_降级() {
        let wrong: Arc<dyn Any + Send + Sync> = Arc::new(String::from("not a provider"));
        let mut ap = MockAccessPoint::new().with_provider("tool", wrong);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
    }

    /// 测试：init 和 shutdown 不返回 Err
    #[tokio::test]
    async fn init_shutdown() {
        let mut slot = ToolRegistrySlot::new();
        let ctx = PluginInitContext {
            plugin_name: "tool_registry".into(),
            plugin_config: serde_json::Value::Null,
            agent_config: crate::core::types::plugin::AgentConfig::default(),
            data_dir: std::env::temp_dir().join("tool_registry_test"),
        };
        assert!(slot.init(&ctx).await.is_ok());
        assert!(slot.shutdown().await.is_ok());
    }
}
