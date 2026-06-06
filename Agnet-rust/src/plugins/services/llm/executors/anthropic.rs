//! Anthropic 执行器
//!
//! 设计文档 §4.3：HTTP POST /v1/messages 请求 + 响应解析。
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

// design doc §3.6.1 routing table: Anthropic endpoint path
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";
// design doc §3.6.1: Anthropic API version header
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
// design doc §3.1 common: JSON content type
const CONTENT_TYPE_JSON: &str = "application/json";
// design doc §3.6.1: user-agent header value
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Anthropic executor (design doc §3.6.1).
pub struct AnthropicExecutor;

impl AnthropicExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AnthropicExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderExecutor for AnthropicExecutor {
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
            ANTHROPIC_MESSAGES_PATH
        );
        let body = build_anthropic_body(config, messages, tools)?;
        let headers = build_anthropic_headers(config);

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
                "anthropic",
                &config.model,
            ));
        }

        if config.stream {
            let rx = StreamProcessor::parse_anthropic(response, trace_id.to_owned());
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
            let thought = parse_anthropic_response(&resp_body, &raw_text, trace_id)?;
            Ok(ChatResponse::Complete(thought))
        }
    }
}

// design doc §3.6.3: Anthropic-specific request headers
fn build_anthropic_headers(config: &LlmConfig) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        CONTENT_TYPE_JSON
            .parse()
            .expect("CONTENT_TYPE_JSON is a valid header value"),
    );

    // 认证方式: 显式 auth_mode → executor 默认 (XApiKey)
    let auth_mode = config.auth_mode.unwrap_or(AuthMode::XApiKey);
    let extra_overrides_auth = config.extra_headers.iter().any(|(k, _)|
        k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("x-api-key")
    );

    if !extra_overrides_auth {
        match auth_mode {
            AuthMode::XApiKey => {
                if let Some(ref key) = config.api_key {
                    headers.insert(
                        "x-api-key",
                        key.parse()
                            .expect("api_key is a valid header value"),
                    );
                }
            }
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
            AuthMode::None => {}
        }
    }

    headers.insert(
        "anthropic-version",
        ANTHROPIC_API_VERSION
            .parse()
            .expect("ANTHROPIC_API_VERSION is a valid header value"),
    );
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

// design doc §3.6.3: build JSON request body for /v1/messages
fn build_anthropic_body(
    config: &LlmConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Result<serde_json::Value, LlmError> {
    // design doc §3.6.3: extract system messages to top-level system field
    let mut system_text = String::new();
    let mut chat_msgs: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        if msg.role == MessageRole::System {
            // design doc §3.6.3 extract_system_prompt: concatenate text blocks
            let text = msg
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join(" ");
            if !system_text.is_empty() {
                system_text.push('\n');
            }
            system_text.push_str(&text);
        } else {
            let converted = message_to_anthropic(msg);
            if !converted.is_null() {
                chat_msgs.push(converted);
            }
        }
    }

    // design doc §3.6.3: max_tokens is required for Anthropic; fallback to 4096
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": chat_msgs,
        "max_tokens": config.max_tokens.unwrap_or(4096),
    });

    if !system_text.is_empty() {
        body["system"] = serde_json::json!(system_text);
    }
    if let Some(t) = config.temperature {
        body["temperature"] = serde_json::json!(t);
    }
    if let Some(tp) = config.top_p {
        body["top_p"] = serde_json::json!(tp);
    }
    if config.stream {
        body["stream"] = serde_json::json!(true);
    }
    if config.tools_enabled && !tools.is_empty() {
        // design doc §3.6.3: Anthropic uses name/description/input_schema (no type wrapper)
        body["tools"] = serde_json::json!(tools.iter().map(tool_to_anthropic).collect::<Vec<_>>());
    }

    Ok(body)
}

// design doc §3.6.3: message role → Anthropic API message object
fn message_to_anthropic(msg: &Message) -> serde_json::Value {
    let has_multimodal = msg
        .content
        .iter()
        .any(|c| !matches!(c, ContentBlock::Text(_)));
    match msg.role {
        MessageRole::User => {
            if has_multimodal {
                let formatter = MultimodalFormatter::new();
                let content = formatter.to_anthropic(&msg.content, true);
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

            let mut content_blocks: Vec<serde_json::Value> = Vec::new();

            // design doc §3.6.3: preserve text as content block when tool_calls also present
            let has_tool_calls = msg.tool_calls.is_some();
            if !text.is_empty() {
                content_blocks.push(serde_json::json!({"type": "text", "text": text}));
            }

            if let Some(ref tcs) = msg.tool_calls {
                for tc in tcs {
                    content_blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": tc.arguments,
                    }));
                }
            }

            let mut obj = serde_json::json!({"role": "assistant"});
            if has_tool_calls || !text.is_empty() {
                obj["content"] = serde_json::json!(content_blocks);
            } else {
                obj["content"] = serde_json::json!([]);
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
            // design doc §3.6.3: content → tool_result block
            let text = msg
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join(" ");
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": text,
                }]
            })
        }
        MessageRole::System => {
            // System messages are extracted to the top-level field by
            // build_anthropic_body — this arm should not be reached.
            serde_json::Value::Null
        }
    }
}

// design doc §3.6.3: ToolDefinition → Anthropic tool object
fn tool_to_anthropic(tool: &ToolDefinition) -> serde_json::Value {
    serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.parameters,
    })
}

// design doc §3.6.3: parse non-streaming /v1/messages response
fn parse_anthropic_response(
    body: &serde_json::Value,
    raw_text: &str,
    trace_id: &str,
) -> Result<Thought, LlmError> {
    let content = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::ParseError {
            trace_id: trace_id.to_owned(),
            raw_response: raw_text.to_owned(),
        })?;

    let stop_reason = body
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    if stop_reason == "tool_use" {
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                let id = block
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_owned();
                let name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_owned();
                let input = block
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let tc = ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                };
                let action = Action {
                    tool_name: name,
                    arguments: input,
                    tool_call_id: Some(id),
                    tool_calls: Some(vec![tc]),
                    created_at: Timestamp::now(),
                };
                return Ok(Thought::Action {
                    action,
                    reasoning: String::new(),
                    generated_at: Timestamp::now(),
                });
            }
        }
        return Err(LlmError::ParseError {
            trace_id: trace_id.to_owned(),
            raw_response: raw_text.to_owned(),
        });
    }

    let mut answer = String::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                if !answer.is_empty() {
                    answer.push('\n');
                }
                answer.push_str(text);
            }
        }
    }

    if answer.is_empty() {
        return Err(LlmError::ParseError {
            trace_id: trace_id.to_owned(),
            raw_response: raw_text.to_owned(),
        });
    }

    Ok(Thought::Final {
        answer,
        reasoning: String::new(),
        generated_at: Timestamp::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::ContentBlock;

    // design doc §3.6.3: parse_anthropic_response — final answer
    #[test]
    fn test_section_3_6_1_anthropic_parse_final() {
        let body = serde_json::json!({
            "content": [{"type": "text", "text": "Hello world"}],
            "stop_reason": "end_turn"
        });
        let raw = serde_json::to_string(&body).unwrap();
        let thought = parse_anthropic_response(&body, &raw, "trace-1").unwrap();
        match thought {
            Thought::Final { answer, .. } => assert_eq!(answer, "Hello world"),
            _ => panic!("expected Final"),
        }
    }

    // design doc §3.6.3: parse_anthropic_response — tool_use
    #[test]
    fn test_section_3_6_1_anthropic_parse_tool_use() {
        let body = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me check"},
                {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"loc": "NYC"}}
            ],
            "stop_reason": "tool_use"
        });
        let raw = serde_json::to_string(&body).unwrap();
        let thought = parse_anthropic_response(&body, &raw, "trace-1").unwrap();
        match thought {
            Thought::Action { action, .. } => {
                assert_eq!(action.tool_name, "get_weather");
                assert_eq!(action.tool_call_id.as_deref(), Some("toolu_1"));
            }
            _ => panic!("expected Action"),
        }
    }

    // design doc §3.6.3: build_anthropic_body — system extraction
    #[test]
    fn test_section_3_6_1_anthropic_build_body_system() {
        fn make_msg(role: MessageRole, text: &str) -> Message {
            Message {
                role,
                content: vec![ContentBlock::text(text)],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: crate::core::types::Timestamp::now(),
            }
        }
        let sys = make_msg(MessageRole::System, "You are a helpful assistant");
        let user = make_msg(MessageRole::User, "Hi");
        let config = LlmConfig {
            provider: crate::shared_types::llm::ProviderKind::Anthropic,
            model: "claude-3-opus-20240229".into(),
            base_url: "https://api.anthropic.com".into(),
            api_key: Some("sk-ant-test".into()),
            max_tokens: Some(100),
            temperature: None,
            top_p: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            timeout: std::time::Duration::from_secs(30),
            idle_timeout: None,
            stream: false,
            tools_enabled: false,
            multimodal: false,
            max_retries: 3,
            retry_backoff: crate::shared_types::llm::RetryBackoff::default(),
            context_window: 8192,
            extra_headers: std::collections::HashMap::new(),
            auth_mode: None,
            enable_tracing: false,
        };
        let body = build_anthropic_body(&config, &[sys, user], &[]).unwrap();
        assert_eq!(body["system"], "You are a helpful assistant");
        assert_eq!(body["model"], "claude-3-opus-20240229");
        assert_eq!(body["max_tokens"], 100);
    }
}
