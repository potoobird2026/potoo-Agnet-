//! LlmThinkerSlot（精简版）
//!
//! 设计文档 §5：精简后的 SlotPlugin，只负责 Pipeline 编排：
//! 读上下文 → 调 LlmService（通过 provider_raw）→ 处理流 → 写回 Thought
//!
//! 不再持有 Orchestrator、Component、executors 等——这些已移入 LlmService。
//! 不再需要 types.rs 中的 ThinkerError、ProviderKind 等——使用 shared_types/llm.rs。

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedReceiver;
use uuid::Uuid;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::Timestamp;
use crate::shared_types::context::{CONTEXT_THOUGHT, CONTEXT_TOOLS};
use crate::shared_types::llm::{
    ChatResponse, LlmConfig, LlmContract, LlmError, StreamEvent, PROVIDER_LLM,
};
use crate::shared_types::DynProvider;
use crate::shared_types::{Message, Thought, ToolDefinition};

/// LlmThinkerSlot — 模块顶层入口（精简版，设计文档 §5.2）
pub struct LlmThinkerSlot {
    llm_config: Option<LlmConfig>,
}

impl Default for LlmThinkerSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmThinkerSlot {
    pub fn new() -> Self {
        Self { llm_config: None }
    }

    /// 处理 ChatResponse → Thought（设计文档 §5.1 Step 7）
    async fn process_chat_response(response: ChatResponse, trace_id: String) -> Thought {
        match response {
            ChatResponse::Complete(thought) => thought,
            ChatResponse::Stream(rx) => Self::process_stream(rx, &trace_id).await,
        }
    }

    /// 流式事件循环（设计文档 §5.1 Step 7 stream）
    async fn process_stream(
        mut rx: UnboundedReceiver<Result<StreamEvent, LlmError>>,
        trace_id: &str,
    ) -> Thought {
        let mut answer = String::new();

        loop {
            let result = rx.recv().await;
            match result {
                Some(Ok(StreamEvent::TextDelta(text))) => {
                    answer.push_str(&text);
                }
                Some(Ok(StreamEvent::ToolCallDelta { .. })) => {
                    // 设计文档 §5.1: ToolCallDelta → 暂忽略
                }
                Some(Ok(StreamEvent::End(thought))) => {
                    return thought;
                }
                Some(Err(e)) => {
                    tracing::error!(trace_id, error = ?e, "Stream error");
                    return Thought::Final {
                        answer: format!("Stream error: {e}"),
                        reasoning: String::new(),
                        generated_at: Timestamp::now(),
                    };
                }
                None => {
                    return Thought::Final {
                        answer,
                        reasoning: String::new(),
                        generated_at: Timestamp::now(),
                    };
                }
            }
        }
    }

    /// 合并 session 级别的配置覆盖（设计文档 §5.2 build_config）
    fn merge_session_overrides(&self, ap: &dyn SlotAccessPoint) -> LlmConfig {
        let mut config = self.llm_config.clone().unwrap_or_default();

        if let Some(session_raw) =
            ap.provider_raw(crate::shared_types::context::PROVIDER_SESSION_CONTEXT)
        {
            if let Some(overrides) = session_raw.downcast_ref::<serde_json::Value>() {
                if let Ok(patched) = serde_json::from_value::<LlmConfig>(overrides.clone()) {
                    if !patched.model.is_empty() {
                        config.model = patched.model;
                    }
                    if patched.temperature.is_some() {
                        config.temperature = patched.temperature;
                    }
                    if patched.max_tokens.is_some() {
                        config.max_tokens = patched.max_tokens;
                    }
                    if patched.top_p.is_some() {
                        config.top_p = patched.top_p;
                    }
                    if patched.stream {
                        config.stream = true;
                    }
                }
            }
        }

        config
    }
}

#[async_trait]
impl SlotPlugin for LlmThinkerSlot {
    fn name(&self) -> &str {
        "llm_thinker"
    }

    /// 初始化：只读取 LLM 配置，不初始化 Orchestrator（设计文档 §5.2）
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let llm_value = ctx
            .plugin_config
            .get("llm")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let llm_config: LlmConfig = serde_json::from_value(llm_value)
            .map_err(|e| PluginError::Config(format!("解析 LLM 配置失败: {e}")))?;

        self.llm_config = Some(llm_config);
        Ok(())
    }

    /// 运行：8 步算法（设计文档 §5.1）
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // Step 1: 生成 trace_id
        let trace_id = Uuid::new_v4().to_string();

        // Step 2: 从上下文读取 ToolDefinition
        let tools: Vec<ToolDefinition> = ap
            .read_context_raw(CONTEXT_TOOLS)
            .and_then(|any| any.downcast_ref::<Vec<ToolDefinition>>().cloned())
            .unwrap_or_default();

        // Step 3: 合并 session 级别的模型覆盖
        let config = self.merge_session_overrides(ap);

        // Step 4: 获取消息列表
        let messages: Vec<Message> = ap.messages().to_vec();

        // Step 5: 从 ProviderRegistry 获取 LlmContract（设计文档 §5.1 Step 6）
        let raw = ap
            .provider_raw(PROVIDER_LLM)
            .ok_or_else(|| PluginError::NotFound("LLM 服务未注册".into()))?;

        let wrapper = raw
            .downcast::<DynProvider<dyn LlmContract>>()
            .map_err(|_| PluginError::Internal("LLM Provider 类型不匹配".into()))?;

        let result = wrapper
            .0
            .chat(Some(config), &messages, &tools, &trace_id)
            .await;

        // Step 6: 处理错误或流
        let thought = match result {
            Ok(response) => Self::process_chat_response(response, trace_id.clone()).await,
            Err(err) => {
                tracing::error!(trace_id, error = ?err, "Chat invocation failed");
                Thought::Final {
                    answer: format!("LLM API error: {err}"),
                    reasoning: String::new(),
                    generated_at: Timestamp::now(),
                }
            }
        };

        // Step 7: 后处理——写入 Thought 到上下文
        ap.write_context_raw(CONTEXT_THOUGHT, Box::new(thought))
            .map_err(|e| PluginError::Runtime(format!("写入 thought 失败: {e}")))?;

        // Step 8: 返回 Continue
        Ok(SlotDirective::Continue)
    }

    /// 关闭（设计文档 §5.2）
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.llm_config = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::sync::Arc;

    use super::*;
    use crate::core::types::plugin::{AgentConfig, PluginInitContext};
    use crate::shared_types::{DynProvider, MessageRole};

    // ── Mock SlotAccessPoint ─────────────────────────────────────────
    struct MockAccessPoint {
        messages: Vec<Message>,
        context: std::collections::HashMap<String, Box<dyn Any + Send + Sync>>,
        providers: std::collections::HashMap<String, Arc<dyn Any + Send + Sync>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                messages: Vec::new(),
                context: std::collections::HashMap::new(),
                providers: std::collections::HashMap::new(),
            }
        }

        fn with_message(mut self, msg: Message) -> Self {
            self.messages.push(msg);
            self
        }

        #[allow(dead_code)]
        fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
            self.context.insert("tools".into(), Box::new(tools));
            self
        }

        fn with_provider(mut self, key: &str, provider: Arc<dyn Any + Send + Sync>) -> Self {
            self.providers.insert(key.into(), provider);
            self
        }

        fn thought(&self) -> Option<&Thought> {
            self.context
                .get("thought")
                .and_then(|b| b.downcast_ref::<Thought>())
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
            "test-phase"
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
            self.context
                .get(key)
                .map(|b| b.as_ref() as &(dyn Any + Send + Sync))
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

    /// Fake LlmContract 用于测试
    struct FakeLlmContract;

    #[async_trait]
    impl LlmContract for FakeLlmContract {
        async fn chat(
            &self,
            _config: Option<LlmConfig>,
            _messages: &[Message],
            _tools: &[ToolDefinition],
            _trace_id: &str,
        ) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse::Complete(Thought::Final {
                answer: "Test response".into(),
                reasoning: String::new(),
                generated_at: Timestamp::now(),
            }))
        }

        fn get_public_config(&self) -> crate::shared_types::llm::LlmPublicConfig {
            crate::shared_types::llm::LlmPublicConfig {
                provider: crate::shared_types::llm::ProviderKind::OpenAi,
                base_url: String::new(),
                model: "test-model".into(),
                stream: false,
                max_tokens: None,
            }
        }
    }

    fn make_context() -> PluginInitContext {
        PluginInitContext::new(
            "llm_thinker",
            serde_json::json!({
                "llm": {
                    "provider": "OpenAi",
                    "model": "gpt-4o-mini",
                    "base_url": "https://api.openai.com/v1",
                    "api_key": "sk-test",
                    "max_retries": 0,
                    "timeout": {
                        "secs": 1,
                        "nanos": 0
                    }
                }
            }),
            AgentConfig::default(),
            std::path::PathBuf::from("/tmp/test"),
        )
    }

    #[tokio::test]
    async fn test_section_3_7_name() {
        let slot = LlmThinkerSlot::new();
        assert_eq!(slot.name(), "llm_thinker");
    }

    #[tokio::test]
    async fn test_section_3_7_init_success() {
        let mut slot = LlmThinkerSlot::new();
        let ctx = make_context();
        slot.init(&ctx).await.unwrap();
        assert!(slot.llm_config.is_some());
    }

    #[tokio::test]
    async fn test_section_3_7_init_invalid_config() {
        let mut slot = LlmThinkerSlot::new();
        let ctx = PluginInitContext::new(
            "llm_thinker",
            serde_json::json!({"llm": {"provider": "InvalidProvider"}}),
            AgentConfig::default(),
            std::path::PathBuf::from("/tmp/test"),
        );
        let result = slot.init(&ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_section_3_7_run_final() {
        let mut slot = LlmThinkerSlot::new();
        slot.init(&make_context()).await.unwrap();

        // 注入 fake LlmContract Provider
        let contract: Arc<dyn LlmContract> = Arc::new(FakeLlmContract);
        let mut ap = MockAccessPoint::new()
            .with_message(Message::text(MessageRole::User, "Hello"))
            .with_provider(PROVIDER_LLM, Arc::new(DynProvider(contract)));

        let directive = slot.run(&mut ap).await.unwrap();
        assert_eq!(directive, SlotDirective::Continue);

        // 验证 thought 被写入
        let thought = ap.thought().expect("expected thought in context");
        match thought {
            Thought::Final { answer, .. } => {
                assert_eq!(answer, "Test response");
            }
            _ => panic!("expected Final thought"),
        }
    }
}
