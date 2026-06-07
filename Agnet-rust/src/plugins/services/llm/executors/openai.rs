//! OpenAI / OpenAI-compatible 执行器
//!
//! 设计文档 §4.3：HTTP POST /chat/completions 请求 + 响应解析。
//! 支持：
//! - 非流式：返回 ChatResponse::Complete(Thought)
//! - 流式：返回 ChatResponse::Stream(rx)

use async_trait::async_trait;
use reqwest::Client;

use crate::core::types::Timestamp;
use crate::plugins::services::llm::executors::provider_executor::{
    ProviderDispatcher, ProviderExecutor,
};
use crate::plugins::services::llm::formatter::MultimodalFormatter;
use crate::plugins::services::llm::stream::StreamProcessor;
use crate::shared_types::llm::{AuthMode, ChatResponse, LlmConfig, LlmError};
use crate::shared_types::{
    Action, ContentBlock, Message, MessageRole, Thought, ToolCall, ToolDefinition,
};

// design doc §3.6.1: OpenAI endpoint path suffix for /chat/completions
const OPENAI_CHAT_PATH: &str = "/chat/completions";
// design doc §3.1 common: JSON content type
const CONTENT_TYPE_JSON: &str = "application/json";
// design doc §3.6.1: user-agent header value
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// OpenAI / OpenAI-compatible executor (design doc §3.6.1).
pub struct OpenAiExecutor;

impl OpenAiExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OpenAiExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderExecutor for OpenAiExecutor {
    async fn execute(
        &self,
        dispatcher: &ProviderDispatcher,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LlmError::NetworkError {
                trace_id: trace_id.to_owned(),
                source: e,
            })?;

        let url = format!(
            "{}{}",
            config.base_url.trim_end_matches('/'),
            OPENAI_CHAT_PATH
        );
        let body = build_openai_body(config, messages, tools)?;
        let headers = build_openai_headers(config);

        let response = client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout {
                        trace_id: trace_id.to_owned(),
                        timeout: config.timeout,
                    }
                } else {
                    LlmError::NetworkError {
                        trace_id: trace_id.to_owned(),
                        source: e,
                    }
                }
            })?;

        let status = response.status().as_u16();
        if status >= 300 {
            let resp_body = response.text().await.unwrap_or_default();
            return Err(dispatcher.classify_http_error(
                status,
                &resp_body,
                trace_id,
                "openai",
                &config.model,
            ));
        }

        if config.stream {
            let rx = StreamProcessor::parse_openai(response, trace_id.to_owned());
            Ok(ChatResponse::Stream(rx))
        } else {
            let raw_text = response.text().await.map_err(|e| LlmError::ParseError {
                trace_id: trace_id.to_owned(),
                raw_response: format!("读取响应体失败: {e}"),
            })?;
            let resp_body: serde_json::Value =
                serde_json::from_str(&raw_text).map_err(|_| LlmError::ParseError {
                    trace_id: trace_id.to_owned(),
                    raw_response: raw_text.clone(),
                })?;
            let thought = parse_openai_response(&resp_body, trace_id)?;
            Ok(ChatResponse::Complete(thought))
        }
    }
}

// design doc §3.6.2: request headers
fn build_openai_headers(config: &LlmConfig) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        CONTENT_TYPE_JSON
            .parse()
            .expect("CONTENT_TYPE_JSON is a valid header value"),
    );

    // 认证方式: 显式 auth_mode → executor 默认 (Bearer)
    let auth_mode = config.auth_mode.unwrap_or(AuthMode::Bearer);
    let extra_overrides_auth = config.extra_headers.iter().any(|(k, _)| {
        k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("x-api-key")
    });

    if !extra_overrides_auth {
        match auth_mode {
            AuthMode::Bearer => {
                if let Some(ref key) = config.api_key {
                    let bearer = format!("Bearer {key}");
                    headers.insert(
                        reqwest::header::AUTHORIZATION,
                        bearer
                            .parse()
                            .expect("Bearer token is a valid header value"),
                    );
                }
            }
            AuthMode::XApiKey => {
                if let Some(ref key) = config.api_key {
                    headers.insert(
                        "x-api-key",
                        key.parse().expect("api_key is a valid header value"),
                    );
                }
            }
            AuthMode::None => {}
        }
    }

    headers.insert(
        reqwest::header::USER_AGENT,
        USER_AGENT
            .parse()
            .expect("USER_AGENT is a valid header value"),
    );
    for (k, v) in &config.extra_headers {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            headers.insert(name, val);
        }
    }
    headers
}

// design doc §3.6.2: build JSON request body for /chat/completions
fn build_openai_body(
    config: &LlmConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Result<serde_json::Value, LlmError> {
    let msgs: Vec<serde_json::Value> = messages
        .iter()
        .map(message_to_openai)
        .filter(|v| !v.is_null())
        .collect();

    let mut body = serde_json::json!({
        "model": config.model,
        "messages": msgs,
    });

    if let Some(mt) = config.max_tokens {
        body["max_tokens"] = serde_json::json!(mt);
    }
    if let Some(t) = config.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if let Some(tp) = config.top_p {
        body["top_p"] = serde_json::json!(tp);
    }
    if let Some(ref stop) = config.stop {
        body["stop"] = serde_json::json!(stop);
    }
    if config.stream {
        body["stream"] = serde_json::json!(true);
    }
    // design doc §3.6.2: tools only when enabled and non-empty
    if config.tools_enabled && !tools.is_empty() {
        body["tools"] = serde_json::json!(tools.iter().map(tool_to_openai).collect::<Vec<_>>());
    }

    Ok(body)
}

// design doc §3.6.2: message role → OpenAI API role mapping
fn message_to_openai(msg: &Message) -> serde_json::Value {
    let has_multimodal = msg
        .content
        .iter()
        .any(|c| !matches!(c, ContentBlock::Text(_)));
    match msg.role {
        MessageRole::System => {
            if has_multimodal {
                let formatter = MultimodalFormatter::new();
                let content = formatter.to_openai(&msg.content, true);
                serde_json::json!({"role": "system", "content": content})
            } else {
                let text = msg
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .collect::<Vec<_>>()
                    .join(" ");
                serde_json::json!({"role": "system", "content": text})
            }
        }
        MessageRole::User => {
            if has_multimodal {
                let formatter = MultimodalFormatter::new();
                let content = formatter.to_openai(&msg.content, true);
                serde_json::json!({"role": "user", "content": content})
            } else {
                let text = msg
                    .content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .collect::<Vec<_>>()
                    .join(" ");
                serde_json::json!({"role": "user", "content": text})
            }
        }
        MessageRole::Assistant => {
            let text = msg
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join(" ");
            let mut obj = serde_json::json!({"role": "assistant", "content": text});
            if let Some(ref tcs) = msg.tool_calls {
                obj["tool_calls"] =
                    serde_json::json!(tcs.iter().map(tool_call_to_openai).collect::<Vec<_>>());
            }
            obj
        }
        MessageRole::Tool => {
            let tool_call_id = match msg.tool_call_id {
                Some(ref id) => id.clone(),
                None => {
                    tracing::warn!("Tool message missing tool_call_id — skipping");
                    return serde_json::Value::Null;
                }
            };
            let text = msg
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join(" ");
            serde_json::json!({"role": "tool", "tool_call_id": tool_call_id, "content": text})
        }
    }
}

// design doc §3.6.2: ToolDefinition → OpenAI tool object
fn tool_to_openai(tool: &ToolDefinition) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

// design doc §3.6.2: ToolCall → OpenAI tool_calls entry
fn tool_call_to_openai(tc: &ToolCall) -> serde_json::Value {
    serde_json::json!({
        "id": tc.id,
        "type": "function",
        "function": {
            "name": tc.name,
            "arguments": tc.arguments.to_string(),
        }
    })
}

// design doc §3.6.2: parse non-streaming /chat/completions response
fn parse_openai_response(body: &serde_json::Value, trace_id: &str) -> Result<Thought, LlmError> {
    let choices = body
        .get("choices")
        .and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::ParseError {
            trace_id: trace_id.to_owned(),
            raw_response: "响应中没有 choices".into(),
        })?;
    let choice = choices.first().ok_or_else(|| LlmError::ParseError {
        trace_id: trace_id.to_owned(),
        raw_response: "choices 数组为空".into(),
    })?;

    let message = choice.get("message").ok_or_else(|| LlmError::ParseError {
        trace_id: trace_id.to_owned(),
        raw_response: "choice 中没有 message".into(),
    })?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|r| r.as_str())
        .unwrap_or("");

    if finish_reason == "tool_calls" {
        // design doc §3.6.2 step 5a: extract tool_calls array
        let tool_calls_value = message
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .ok_or_else(|| LlmError::ParseError {
                trace_id: trace_id.to_owned(),
                raw_response: "finish_reason=tool_calls 但 message 中没有 tool_calls".into(),
            })?;

        let tool_calls: Vec<ToolCall> = tool_calls_value
            .iter()
            .map(|tc| {
                let id = tc
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_owned();
                let name = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned();
                let args = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(serde_json::Value::Null);
                ToolCall {
                    id,
                    name,
                    arguments: args,
                }
            })
            .collect();

        // design doc §3.6.2 step 5b: first tool_call → Action
        if tool_calls.is_empty() {
            return Err(LlmError::ParseError {
                trace_id: trace_id.to_owned(),
                raw_response: "finish_reason=tool_calls 但 tool_calls 数组为空".into(),
            });
        }
        let first = &tool_calls[0];
        let action = Action {
            tool_name: first.name.clone(),
            arguments: first.arguments.clone(),
            tool_call_id: Some(first.id.clone()),
            tool_calls: Some(tool_calls),
            created_at: Timestamp::now(),
        };
        Ok(Thought::Action {
            action,
            reasoning: String::new(),
            generated_at: Timestamp::now(),
        })
    } else {
        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        if content.is_empty() {
            return Err(LlmError::ParseError {
                trace_id: trace_id.to_owned(),
                raw_response: "content 为空".into(),
            });
        }
        Ok(Thought::Final {
            answer: content.to_owned(),
            reasoning: String::new(),
            generated_at: Timestamp::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // design doc §3.6.2: parse_openai_response — final answer
    #[test]
    fn test_section_3_6_1_openai_parse_final() {
        let body = serde_json::json!({
            "choices": [{
                "message": { "content": "Hello world", "role": "assistant" },
                "finish_reason": "stop"
            }]
        });
        let thought = parse_openai_response(&body, "trace-1").unwrap();
        match thought {
            Thought::Final { answer, .. } => assert_eq!(answer, "Hello world"),
            _ => panic!("expected Final"),
        }
    }

    // design doc §3.6.2: parse_openai_response — tool_calls
    #[test]
    fn test_section_3_6_1_openai_parse_tool_calls() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "get_weather", "arguments": "{\"loc\":\"NYC\"}" }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let thought = parse_openai_response(&body, "trace-1").unwrap();
        match thought {
            Thought::Action { action, .. } => {
                assert_eq!(action.tool_name, "get_weather");
                assert_eq!(action.tool_call_id.as_deref(), Some("call_1"));
            }
            _ => panic!("expected Action"),
        }
    }

    // design doc §3.6.2: parse_openai_response — empty choices error
    #[test]
    fn test_section_3_6_1_openai_parse_empty_choices() {
        let body = serde_json::json!({"choices": []});
        assert!(parse_openai_response(&body, "trace-1").is_err());
    }

    // design doc §3.6.2: build_openai_body — basic fields
    #[test]
    fn test_section_3_6_1_openai_build_body_basic() {
        let config = LlmConfig {
            model: "gpt-4".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: Some("sk-test".into()),
            temperature: Some(0.7),
            ..Default::default()
        };
        let body = build_openai_body(&config, &[], &[]).unwrap();
        assert_eq!(body["model"], "gpt-4");
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert!(body.get("messages").unwrap().as_array().unwrap().is_empty());
    }

    // design doc §3.6.2: build_openai_body — stream flag
    #[test]
    fn test_section_3_6_1_openai_build_body_stream() {
        let config = LlmConfig {
            model: "gpt-4".into(),
            stream: true,
            ..Default::default()
        };
        let body = build_openai_body(&config, &[], &[]).unwrap();
        assert_eq!(body["stream"], true);
    }

    // design doc §3.6.2: message_to_openai — tool role missing tool_call_id → Null
    #[test]
    fn test_section_3_6_1_openai_message_tool_missing_id_returns_null() {
        let msg = Message {
            role: MessageRole::Tool,
            tool_call_id: None,
            content: vec![],
            tool_calls: None,
            reasoning: None,
            metadata: None,
            created_at: Timestamp::now(),
        };
        let result = message_to_openai(&msg);
        assert!(result.is_null());
    }

    // design doc §3.6.2: message_to_openai — tool role with id
    #[test]
    fn test_section_3_6_1_openai_message_tool_with_id() {
        let msg = Message {
            role: MessageRole::Tool,
            tool_call_id: Some("call_xyz".into()),
            content: vec![],
            tool_calls: None,
            reasoning: None,
            metadata: None,
            created_at: Timestamp::now(),
        };
        let result = message_to_openai(&msg);
        assert_eq!(result["role"], "tool");
        assert_eq!(result["tool_call_id"], "call_xyz");
    }

    // design doc §3.6.2: build_openai_headers — Authorization present with api_key
    #[allow(clippy::field_reassign_with_default)]
    #[test]
    fn test_section_3_6_1_openai_build_headers_with_key() {
        let mut config = LlmConfig::default();
        config.api_key = Some("sk-test".into());
        let headers = build_openai_headers(&config);
        assert!(headers.get("authorization").is_some());
        assert_eq!(
            headers.get("authorization").unwrap().to_str().unwrap(),
            "Bearer sk-test"
        );
    }

    // design doc §3.6.2: build_openai_headers — no Authorization when api_key is None
    #[test]
    fn test_section_3_6_1_openai_build_headers_without_key() {
        let config = LlmConfig::default();
        let headers = build_openai_headers(&config);
        assert!(headers.get("authorization").is_none());
    }
}
