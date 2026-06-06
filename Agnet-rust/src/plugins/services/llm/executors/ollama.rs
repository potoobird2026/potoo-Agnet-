//! Ollama 执行器
//!
//! 设计文档 §4.3：Ollama 使用 OpenAI-compatible /chat/completions 端点，委托 OpenAiExecutor。

use async_trait::async_trait;

use crate::plugins::services::llm::executors::provider_executor::{
    ProviderDispatcher, ProviderExecutor,
};
use crate::shared_types::llm::{ChatResponse, LlmConfig, LlmError};
use crate::shared_types::{Message, ToolDefinition};

/// Ollama executor — delegates to OpenAiExecutor because Ollama serves an
/// OpenAI-compatible /chat/completions endpoint (design doc §3.6.1).
pub struct OllamaExecutor;

impl OllamaExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OllamaExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderExecutor for OllamaExecutor {
    async fn execute(
        &self,
        dispatcher: &ProviderDispatcher,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        // design doc §3.6.4: log provider = "ollama" then delegate to OpenAI executor
        tracing::info!(
            trace_id,
            provider = "ollama",
            "Delegating to OpenAI-compatible executor"
        );
        dispatcher
            .openai
            .execute(dispatcher, config, messages, tools, trace_id)
            .await
    }
}
