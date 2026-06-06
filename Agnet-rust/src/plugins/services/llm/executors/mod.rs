//! LLM 提供商执行器模块
//!
//! 设计文档 §4.3：每个 executor 处理一个 LLM API 提供商的 HTTP 请求/响应。
//!
//! 包含：
//! - provider_executor.rs: ProviderExecutor trait + ProviderDispatcher
//! - openai.rs: OpenAI / OpenAI-compatible executor
//! - anthropic.rs: Anthropic executor
//! - ollama.rs: Ollama executor（委托 OpenAiExecutor）

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod provider_executor;
