//! ProviderExecutor trait + ProviderDispatcher
//!
//! 设计文档 §4.3：处理一个 LLM API 提供商的 HTTP 请求/响应周期。
//! 格式转换、HTTP 调用、响应解析都在 execute() 中完成。

use async_trait::async_trait;

use crate::plugins::services::llm::error::ErrorClassifier;
use crate::shared_types::llm::{ChatResponse, LlmConfig, LlmError, ProviderKind};
use crate::shared_types::{Message, ToolDefinition};

/// ProviderExecutor: handles one LLM API provider's HTTP request/response
/// cycle. Format conversion, HTTP call, and response parsing all happen
/// inside `execute()` (design doc §3.6.1).
#[async_trait]
pub trait ProviderExecutor: Send + Sync {
    async fn execute(
        &self,
        dispatcher: &ProviderDispatcher,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError>;
}

/// Routes provider kinds to their executor implementations.
pub struct ProviderDispatcher {
    pub(crate) openai: super::openai::OpenAiExecutor,
    pub(crate) anthropic: super::anthropic::AnthropicExecutor,
    pub(crate) ollama: super::ollama::OllamaExecutor,
}

impl Default for ProviderDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderDispatcher {
    pub fn new() -> Self {
        Self {
            openai: super::openai::OpenAiExecutor::new(),
            anthropic: super::anthropic::AnthropicExecutor::new(),
            ollama: super::ollama::OllamaExecutor::new(),
        }
    }

    /// Route to the correct executor (design doc §3.6.1 routing table).
    pub fn dispatch(&self, provider: &ProviderKind) -> &dyn ProviderExecutor {
        match provider {
            ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => &self.openai,
            ProviderKind::Anthropic => &self.anthropic,
            ProviderKind::Ollama => &self.ollama,
        }
    }

    /// Classify HTTP error status codes into LlmError variants
    /// (design doc §3.6.1 error classification table).
    pub fn classify_http_error(
        &self,
        status: u16,
        body: &str,
        trace_id: &str,
        provider: &str,
        model: &str,
    ) -> LlmError {
        ErrorClassifier::new().classify_http_error(status, body, trace_id, provider, model)
    }
}
