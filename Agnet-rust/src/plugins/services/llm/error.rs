//! ErrorClassifier — LLM 错误分类器
//!
//! 设计文档 §4.5：原 ErrorClassifier（llm_thinker/components/error_classifier.rs），
//! 去掉 Component trait 后降级为普通 struct。
//!
//! 职责：
//! - classify_http_error() — HTTP 状态码 + 响应体 → LlmError
//! - classify_http_client_error() — reqwest 错误 → LlmError
//! - classify_parse_error() — JSON 解析失败 → LlmError
//! - is_retryable() — LlmError 是否可重试

use std::time::Duration;

use crate::shared_types::llm::LlmError;

/// 无状态错误分类器（设计文档 §4.5）
pub struct ErrorClassifier;

#[allow(dead_code)]
impl ErrorClassifier {
    /// 创建新的错误分类器
    pub fn new() -> Self {
        Self
    }

    /// 根据 HTTP 状态码和响应体分类 API 错误（设计文档 §3.3）
    pub fn classify_http_error(
        &self,
        status: u16,
        body: &str,
        trace_id: &str,
        provider: &str,
        model: &str,
    ) -> LlmError {
        LlmError::ApiError {
            provider: provider.to_owned(),
            model: model.to_owned(),
            status: Some(status),
            message: body.to_owned(),
            trace_id: trace_id.to_owned(),
            // 设计文档 §3.3: status >= 500 可重试（服务端错误），429 也是可重试的
            retryable: status >= 500 || status == 429,
        }
    }

    /// 根据 reqwest 客户端错误分类（设计文档 §3.3）
    pub fn classify_http_client_error(
        &self,
        err: reqwest::Error,
        trace_id: &str,
        timeout: Duration,
    ) -> LlmError {
        // 设计文档 §3.3: 先检查超时标志
        if err.is_timeout() {
            LlmError::Timeout {
                trace_id: trace_id.to_owned(),
                timeout,
            }
        } else {
            LlmError::NetworkError {
                trace_id: trace_id.to_owned(),
                source: err,
            }
        }
    }

    /// 分类 JSON 解析失败错误（设计文档 §3.3）
    pub fn classify_parse_error(&self, raw: &str, trace_id: &str) -> LlmError {
        LlmError::ParseError {
            trace_id: trace_id.to_owned(),
            raw_response: raw.to_owned(),
        }
    }

    /// 错误是否可重试
    pub fn is_retryable(&self, error: &LlmError) -> bool {
        error.is_retryable()
    }

    /// 用户友好的错误建议
    pub fn suggestion(&self, error: &LlmError) -> String {
        error.suggestion()
    }
}

impl Default for ErrorClassifier {
    fn default() -> Self {
        Self::new()
    }
}
