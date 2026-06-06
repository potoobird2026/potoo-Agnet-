//! ChatInvoker — LLM 调用编排器
//!
//! 设计文档 §4.2：原 ChatInvoker，去掉 Component trait 后降级为普通 struct。
//!
//! 编排流程：
//! 1. ProviderDispatcher.dispatch() → 路由到对应厂商 executor
//! 2. RetryManager.call_with_retry() → 重试包装
//!   3. Executor.execute() → HTTP POST + 响应解析

use crate::plugins::services::llm::executors::provider_executor::ProviderDispatcher;
use crate::plugins::services::llm::retry::RetryManager;
use crate::shared_types::llm::{ChatResponse, LlmConfig, LlmError};
use crate::shared_types::{Message, ToolDefinition};

/// ChatInvoker: orchestrates the full LLM invocation pipeline
/// (design doc §3.5).
pub struct ChatInvoker {
    pub(crate) dispatcher: ProviderDispatcher,
}

impl ChatInvoker {
    pub fn new() -> Self {
        Self {
            dispatcher: ProviderDispatcher::new(),
        }
    }

    /// Execute a chat request with routing and retry logic.
    pub async fn invoke(
        &self,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        // 1. Route to the correct provider executor
        let executor = self.dispatcher.dispatch(&config.provider);

        // 2. Wrap with RetryManager (design doc §3.5 + §3.7 step 5)
        let retry_manager = RetryManager;
        retry_manager
            .call_with_retry(config, || async {
                executor
                    .execute(&self.dispatcher, config, messages, tools, trace_id)
                    .await
            })
            .await
    }
}

impl Default for ChatInvoker {
    fn default() -> Self {
        Self::new()
    }
}
