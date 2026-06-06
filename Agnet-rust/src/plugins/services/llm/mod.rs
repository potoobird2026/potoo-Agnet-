//! LlmService — LLM 调用服务（ServicePlugin）
//!
//! 持有 HTTP 客户端、API 密钥、LLM 配置，对外暴露 LlmContract Provider。
//! 由 LlmThinkerSlot（消费方）通过 provider_raw(PROVIDER_LLM) 调用。
//!
//! 设计文档：docs/services/llm/LlmService-开发设计文档.md
//! 开发计划：docs/services/llm/LlmService 严格 AI 开发计划.md

mod chat;
mod config;
mod error;
mod executors;
mod formatter;
mod retry;
mod service;
mod stream;

pub use service::LlmService;
