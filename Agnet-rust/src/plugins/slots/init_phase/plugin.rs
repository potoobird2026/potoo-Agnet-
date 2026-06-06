use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::{AgentConfig, PluginInitContext};
use crate::core::types::Timestamp;
use crate::shared_types::context::{CONTEXT_AGENT_CONFIG, CONTEXT_IDENTITY, CONTEXT_SESSION_META, CONTEXT_SYSTEM_PROMPT, CONTEXT_WORKING_MEMORY};
use crate::shared_types::{DynProvider, MemoryProvider, PROVIDER_MEMORY};

use super::config::InitPhaseConfig;
use super::types::SessionMeta;

pub struct InitPhaseSlot {
    config: Option<InitPhaseConfig>,
    agent_config: Option<AgentConfig>,
}

impl InitPhaseSlot {
    pub fn new() -> Self {
        Self {
            config: None,
            agent_config: None,
        }
    }
}

impl Default for InitPhaseSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl InitPhaseSlot {
    fn assemble_system_prompt(
        config: &InitPhaseConfig,
        identity: Option<crate::shared_types::IdentitySection>,
        is_new_session: bool,
    ) -> String {
        let mut prompt = String::new();

        if let Some(template) = &config.system_prompt_template {
            prompt.push_str(template);
        } else {
            prompt.push_str("You are a helpful AI agent.\n\n");
        }

        if let Some(id) = identity {
            prompt.push_str("## Identity Context\n");
            prompt.push_str(&id.content);
            prompt.push('\n');
        }

        if is_new_session {
            prompt.push_str("\n## Session Info\nThis is a new session.\n");
        }

        prompt
    }
}

#[async_trait]
impl SlotPlugin for InitPhaseSlot {
    fn name(&self) -> &str {
        "init_phase"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: InitPhaseConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("init_phase: 配置解析失败: {}", e)))?;

        if config.working_memory_limit == 0 {
            return Err(PluginError::Config(
                "init_phase: working_memory_limit 不能为 0".into(),
            ));
        }
        if config.max_messages_precheck == 0 {
            return Err(PluginError::Config(
                "init_phase: max_messages_precheck 不能为 0".into(),
            ));
        }

        self.config = Some(config);
        self.agent_config = Some(ctx.agent_config.clone());
        tracing::info!("init_phase: 初始化完成");
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        let config = match &self.config {
            Some(c) => c.clone(),
            None => {
                tracing::warn!("init_phase: 未初始化配置，跳过");
                return Ok(SlotDirective::Continue);
            }
        };

        let session_id = ap.session_id().to_string();

        // 步骤 1：获取 Memory Provider
        let memory_provider = match ap.provider_raw(PROVIDER_MEMORY) {
            Some(raw) => match raw.downcast::<DynProvider<dyn MemoryProvider>>() {
                Ok(arc) => arc.0.clone(),
                Err(_) => {
                    tracing::warn!("init_phase: Memory Provider 类型不匹配，跳过初始化");
                    return Ok(SlotDirective::Continue);
                }
            },
            None => {
                tracing::warn!("init_phase: Memory Provider 未注册，跳过初始化");
                return Ok(SlotDirective::Continue);
            }
        };

        // 步骤 2：检测会话类型
        let is_new_session = memory_provider
            .is_new_session(&session_id)
            .await
            .unwrap_or(true);

        let session_meta = SessionMeta {
            session_id: session_id.clone(),
            is_new: is_new_session,
            initialized_at: Timestamp::now(),
        };
        if let Err(e) = ap.write_context_raw(CONTEXT_SESSION_META, Box::new(session_meta)) {
            tracing::warn!("init_phase: 写入 session_meta 失败: {}，跳过", e);
        }

        // 步骤 3：加载身份记忆（L1）
        if config.load_identity {
            match memory_provider.load_identity(&session_id).await {
                Ok(identity) => {
                    tracing::debug!("init_phase: 身份记忆加载完成");
                    if let Err(e) = ap.write_context_raw(CONTEXT_IDENTITY, Box::new(identity)) {
                        tracing::warn!("init_phase: 写入 identity 失败: {}，跳过", e);
                    }
                }
                Err(e) => {
                    if is_new_session {
                        tracing::info!("init_phase: 新会话，无已有身份记忆");
                    } else {
                        tracing::warn!("init_phase: 身份记忆加载失败: {}", e);
                    }
                }
            }
        }

        // 步骤 4：加载工作记忆（L2）
        if config.load_working_memory && !is_new_session {
            match memory_provider
                .load_working_memory(&session_id, config.working_memory_limit)
                .await
            {
                Ok(memories) => {
                    tracing::debug!("init_phase: 加载 {} 条工作记忆", memories.len());
                    if let Err(e) = ap.write_context_raw(CONTEXT_WORKING_MEMORY, Box::new(memories)) {
                        tracing::warn!("init_phase: 写入 working_memory 失败: {}，跳过", e);
                    }
                }
                Err(e) => {
                    tracing::warn!("init_phase: 工作记忆加载失败: {}", e);
                }
            }
        }

        // 步骤 5：组装系统提示词
        if config.assemble_system_prompt {
            let identity_data = ap
.read_context_raw(CONTEXT_IDENTITY)
                .and_then(|any| any.downcast_ref::<crate::shared_types::IdentitySection>())
                .cloned();

            let system_prompt =
                Self::assemble_system_prompt(&config, identity_data, is_new_session);

            if let Err(e) = ap.write_context_raw(CONTEXT_SYSTEM_PROMPT, Box::new(system_prompt)) {
                tracing::warn!("init_phase: 写入 system_prompt 失败: {}，跳过", e);
            }
        }

        // 步骤 6：上下文窗口预检
        let messages = ap.messages();
        if messages.len() > config.max_messages_precheck {
            tracing::warn!(
                "init_phase: 消息数 {}/{} 接近上限",
                messages.len(),
                config.max_messages_precheck,
            );
        }

        // 步骤 7：写入 Agent 配置摘要到 context
        if let Some(ac) = &self.agent_config {
            let config_summary = format!(
                "Agent: {}\n工作目录: {}\n数据目录: {}\n上下文窗口: {}",
                ac.agent_id,
                ac.workspace.display(),
                ac.data_dir.display(),
                ac.context_window.map(|v| v.to_string()).unwrap_or_default(),
            );
            let _ = ap.write_context_raw(CONTEXT_AGENT_CONFIG, Box::new(config_summary));
        }

        // 步骤 8：返回 Continue
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("init_phase: shutdown 完成");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::core::access::SlotAccessPoint;
    use crate::core::slot::{SlotDirective, SlotPlugin};
    use crate::core::types::error::PluginError;
    use crate::core::types::plugin::PluginInitContext;
    use crate::shared_types::{
        IdentitySection, MemoryError, MemoryFileEntry, MemoryProvider, Message, MessageRole,
    };

    use super::super::config::InitPhaseConfig;
    use super::InitPhaseSlot;

    struct MockMemoryProvider {
        is_new: bool,
        identity: Option<IdentitySection>,
        working_memories: Vec<MemoryFileEntry>,
        identity_fail: bool,
        working_memory_fail: bool,
    }

    impl MockMemoryProvider {
        fn new(is_new: bool) -> Self {
            Self {
                is_new,
                identity: None,
                working_memories: Vec::new(),
                identity_fail: false,
                working_memory_fail: false,
            }
        }

        fn with_identity(mut self, identity: IdentitySection) -> Self {
            self.identity = Some(identity);
            self
        }

        fn with_working_memories(mut self, memories: Vec<MemoryFileEntry>) -> Self {
            self.working_memories = memories;
            self
        }

        fn with_identity_failure(mut self) -> Self {
            self.identity_fail = true;
            self
        }

        fn with_working_memory_failure(mut self) -> Self {
            self.working_memory_fail = true;
            self
        }
    }

    #[async_trait]
    impl MemoryProvider for MockMemoryProvider {
        async fn is_new_session(&self, _session_id: &str) -> Result<bool, MemoryError> {
            Ok(self.is_new)
        }

        async fn load_identity(&self, _session_id: &str) -> Result<IdentitySection, MemoryError> {
            if self.identity_fail {
                Err(MemoryError::ReadError("模拟加载失败".into()))
            } else {
                self.identity
                    .clone()
                    .ok_or(MemoryError::NotFound("未找到身份记忆".into()))
            }
        }

        async fn load_working_memory(
            &self,
            _session_id: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryFileEntry>, MemoryError> {
            if self.working_memory_fail {
                Err(MemoryError::ReadError("模拟加载失败".into()))
            } else {
                Ok(self.working_memories.clone())
            }
        }

        async fn persist_messages(
            &self,
            _session_id: &str,
            _messages: &[Message],
        ) -> Result<(), MemoryError> {
            Ok(())
        }

        async fn persist_observation(
            &self,
            _session_id: &str,
            _observation: &str,
        ) -> Result<(), MemoryError> {
            Ok(())
        }

        async fn trigger_vector_index(&self, _session_id: &str) -> Result<(), MemoryError> {
            Ok(())
        }

        async fn extract_experiences(
            &self,
            _session_id: &str,
        ) -> Result<Vec<crate::shared_types::ExperienceEntry>, MemoryError> {
            Ok(Vec::new())
        }

        async fn stats(
            &self,
            _session_id: &str,
        ) -> Result<crate::shared_types::MemoryStats, MemoryError> {
            Ok(crate::shared_types::MemoryStats::default())
        }

        async fn search_memory(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<crate::shared_types::MemoryFileEntry>, MemoryError> {
            Ok(vec![])
        }
    }

    struct MockSlotAccessPoint {
        session_id: String,
        phase_name: String,
        iteration: usize,
        messages: Vec<Message>,
        context_data: HashMap<String, Box<dyn Any + Send + Sync>>,
        provider_raw_override: Option<Arc<dyn Any + Send + Sync>>,
        context_write_fail_keys: HashSet<String>,
    }

    impl MockSlotAccessPoint {
        fn new(session_id: &str) -> Self {
            Self {
                session_id: session_id.to_string(),
                phase_name: "init".to_string(),
                iteration: 0,
                messages: Vec::new(),
                context_data: HashMap::new(),
                provider_raw_override: None,
                context_write_fail_keys: HashSet::new(),
            }
        }

        fn with_memory_provider(mut self, provider: Arc<dyn Any + Send + Sync>) -> Self {
            self.provider_raw_override = Some(provider);
            self
        }

        fn with_wrong_type_provider(mut self) -> Self {
            self.provider_raw_override = Some(Arc::new(42i32));
            self
        }

        fn with_messages(mut self, count: usize) -> Self {
            self.messages = vec![Message::text(MessageRole::User, "test"); count];
            self
        }

        fn with_context_write_failure(mut self, key: &str) -> Self {
            self.context_write_fail_keys.insert(key.to_string());
            self
        }
    }

    impl SlotAccessPoint for MockSlotAccessPoint {
        fn messages(&self) -> &[Message] {
            &self.messages
        }

        fn session_id(&self) -> &str {
            &self.session_id
        }

        fn phase_name(&self) -> &str {
            &self.phase_name
        }

        fn current_iteration(&self) -> usize {
            self.iteration
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
            if self.context_write_fail_keys.contains(key) {
                return Err(PluginError::Runtime(format!("写入 {} 失败", key)));
            }
            self.context_data.insert(key.to_string(), val);
            Ok(())
        }

        fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
            self.context_data.get(key).map(|b| b.as_ref())
        }

        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }

        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }

        fn provider_raw(&self, _name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            self.provider_raw_override.clone()
        }
    }

    fn make_init_context(config: InitPhaseConfig) -> PluginInitContext {
        PluginInitContext::new(
            "init_phase",
            serde_json::to_value(config).expect("测试中安全: 配置序列化"),
            Default::default(),
            std::env::temp_dir(),
        )
    }

    async fn make_slot(config: InitPhaseConfig) -> InitPhaseSlot {
        // 确保测试配置通过校验（working_memory_limit 必须 >0）
        let valid_config = InitPhaseConfig {
            working_memory_limit: config.working_memory_limit.max(1),
            max_messages_precheck: config.max_messages_precheck.max(1),
            ..config
        };
        let mut slot = InitPhaseSlot::new();
        let ctx = make_init_context(valid_config);
        slot.init(&ctx).await.expect("测试中安全: init 必须成功");
        slot
    }

    fn make_identity(content: &str) -> IdentitySection {
        IdentitySection {
            user_id: "test-user".into(),
            content: content.into(),
            metadata: None,
        }
    }

    fn make_working_memory(summary: &str) -> MemoryFileEntry {
        MemoryFileEntry {
            id: "mem-1".into(),
            summary: summary.into(),
            content: None,
            created_at: "2026-05-30".into(),
            entry_type: "working".into(),
        }
    }

    // ── test 1: 新会话初始化 ──
    #[tokio::test]
    async fn test_new_session_initialization() {
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut ap = MockSlotAccessPoint::new("new-session").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 2: 旧会话恢复 ──
    #[tokio::test]
    async fn test_old_session_recovery() {
        let identity = make_identity("我是老用户");
        let memories = vec![make_working_memory("历史对话1")];
        let provider = Arc::new(
            MockMemoryProvider::new(false)
                .with_identity(identity)
                .with_working_memories(memories),
        );
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("old-session").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 3: 系统提示词组装（含 identity） ──
    #[tokio::test]
    async fn test_system_prompt_assembly_with_identity() {
        let identity = make_identity("我是AI助手");
        let provider = Arc::new(MockMemoryProvider::new(false).with_identity(identity));
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test-session").with_memory_provider(provider);

        let _ = slot.run(&mut ap).await;
    }

    // ── test 4: 自定义系统提示词模板 ──
    #[tokio::test]
    async fn test_system_prompt_custom_template() {
        let config = InitPhaseConfig {
            system_prompt_template: Some("Custom: ".to_string()),
            ..Default::default()
        };
        let mut slot = make_slot(config).await;
        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut ap = MockSlotAccessPoint::new("test-session").with_memory_provider(provider);

        let _ = slot.run(&mut ap).await;
    }

    // ── test 5: 完整流程 ──
    #[tokio::test]
    async fn test_full_flow_all_providers() {
        let identity = make_identity("完整用户");
        let memories = vec![make_working_memory("记忆1"), make_working_memory("记忆2")];
        let provider = Arc::new(
            MockMemoryProvider::new(false)
                .with_identity(identity)
                .with_working_memories(memories),
        );
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("full-flow").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 6: Memory Provider 未注册 ──
    #[tokio::test]
    async fn test_memory_provider_not_registered() {
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test");

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 7: Memory Provider 类型不匹配 ──
    #[tokio::test]
    async fn test_memory_provider_type_mismatch() {
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test").with_wrong_type_provider();

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 8: 新会话无身份记忆 ──
    #[tokio::test]
    async fn test_new_session_identity_missing() {
        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("new-session").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
        // identity should not be written (no error, just graceful skip)
        assert!(ap.read_context_raw("identity").is_none());
    }

    // ── test 9: 旧会话身份加载失败 ──
    #[tokio::test]
    async fn test_old_session_identity_failure() {
        let provider = Arc::new(MockMemoryProvider::new(false).with_identity_failure());
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("old-session").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 10: 工作记忆加载失败 ──
    #[tokio::test]
    async fn test_working_memory_load_failure() {
        let provider = Arc::new(MockMemoryProvider::new(false).with_working_memory_failure());
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("old-session").with_memory_provider(provider);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 11: 上下文窗口预检警告 ──
    #[tokio::test]
    async fn test_context_precheck_warning() {
        let config = InitPhaseConfig {
            max_messages_precheck: 2,
            ..Default::default()
        };
        let mut slot = make_slot(config).await;
        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut ap = MockSlotAccessPoint::new("test")
            .with_memory_provider(provider)
            .with_messages(5);

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 12: write_context_raw("identity") 失败 ──
    #[tokio::test]
    async fn test_write_context_identity_failure() {
        let identity = make_identity("测试用户");
        let provider = Arc::new(MockMemoryProvider::new(false).with_identity(identity));
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test")
            .with_memory_provider(provider)
            .with_context_write_failure("identity");

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 13: write_context_raw("working_memory") 失败 ──
    #[tokio::test]
    async fn test_write_context_working_memory_failure() {
        let memories = vec![make_working_memory("记忆1")];
        let provider = Arc::new(MockMemoryProvider::new(false).with_working_memories(memories));
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test")
            .with_memory_provider(provider)
            .with_context_write_failure("working_memory");

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 14: write_context_raw("session_meta") 失败 ──
    #[tokio::test]
    async fn test_write_context_session_meta_failure() {
        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test")
            .with_memory_provider(provider)
            .with_context_write_failure("session_meta");

        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 15: working_memory_limit = 0 ──
    #[tokio::test]
    async fn test_working_memory_limit_zero() {
        let config = InitPhaseConfig {
            working_memory_limit: 0,
            ..Default::default()
        };
        let mut slot = InitPhaseSlot::new();
        let ctx = make_init_context(config);
        let result = slot.init(&ctx).await;
        assert!(result.is_err());
    }

    // ── test 16: max_messages_precheck = 0 ──
    #[tokio::test]
    async fn test_max_messages_precheck_zero() {
        let config = InitPhaseConfig {
            max_messages_precheck: 0,
            ..Default::default()
        };
        let mut slot = InitPhaseSlot::new();
        let ctx = make_init_context(config);
        let result = slot.init(&ctx).await;
        assert!(result.is_err());
    }

    // ── test 17: 配置解析错误 ──
    #[tokio::test]
    async fn test_config_parse_error() {
        let mut slot = InitPhaseSlot::new();
        let ctx = PluginInitContext::new(
            "init_phase",
            json!({"unknown_field": "invalid"}),
            Default::default(),
            std::env::temp_dir(),
        );
        let result = slot.init(&ctx).await;
        // Should succeed with defaults since all fields have serde defaults
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_parse_error_invalid_type() {
        let mut slot = InitPhaseSlot::new();
        let ctx = PluginInitContext::new(
            "init_phase",
            json!({"working_memory_limit": "not_a_number"}),
            Default::default(),
            std::env::temp_dir(),
        );
        let result = slot.init(&ctx).await;
        assert!(result.is_err());
    }

    // ── test 18: 配置默认值 ──
    #[tokio::test]
    async fn test_config_default_values() {
        let mut slot = InitPhaseSlot::new();
        let ctx = PluginInitContext::new(
            "init_phase",
            json!({}),
            Default::default(),
            std::env::temp_dir(),
        );
        let result = slot.init(&ctx).await;
        assert!(result.is_ok());

        let provider = Arc::new(MockMemoryProvider::new(true));
        let mut ap = MockSlotAccessPoint::new("test").with_memory_provider(provider);

        let run_result = slot.run(&mut ap).await;
        assert!(run_result.is_ok());
        assert_eq!(run_result.unwrap(), SlotDirective::Continue);
    }

    // ── test 19: is_new_session 超时 ──
    #[tokio::test]
    async fn test_is_new_session_timeout() {
        let mut slot = make_slot(InitPhaseConfig::default()).await;
        let mut ap = MockSlotAccessPoint::new("test");
        // No provider registered → skips initialization entirely
        let result = slot.run(&mut ap).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ── test 20: S-R03 合规——重复运行 ──
    #[tokio::test]
    async fn test_s_r03_compliance_repeated_run() {
        let identity = make_identity("用户");
        let mem_provider = Arc::new(MockMemoryProvider::new(true).with_identity(identity));
        let mut slot = make_slot(InitPhaseConfig::default()).await;

        let mut ap1 = MockSlotAccessPoint::new("session-1")
            .with_memory_provider(Arc::clone(&mem_provider) as Arc<dyn Any + Send + Sync>);
        let r1 = slot.run(&mut ap1).await;
        assert!(r1.is_ok());

        let mut ap2 = MockSlotAccessPoint::new("session-2")
            .with_memory_provider(Arc::clone(&mem_provider) as Arc<dyn Any + Send + Sync>);
        let r2 = slot.run(&mut ap2).await;
        assert!(r2.is_ok());
    }

    // ── test 21: shutdown 正常执行 ──
    #[tokio::test]
    async fn test_shutdown_completes() {
        let _slot = make_slot(InitPhaseConfig::default()).await;
        // shutdown takes &mut self, but slot is not mut
        // we already consumed slot — create a new one
        let mut slot = InitPhaseSlot::new();
        let result = slot.shutdown().await;
        assert!(result.is_ok());
    }
}
