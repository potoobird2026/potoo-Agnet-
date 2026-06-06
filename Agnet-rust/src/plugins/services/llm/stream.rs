//! StreamProcessor — SSE 流解析器
//!
//! 设计文档 §4.7：原 StreamProcessor，去掉 Component trait 后降级为普通 struct。
//! SSE 解析函数（parse_openai_sse, parse_anthropic_sse）从旧 llm_thinker 移入此处。
//!
//! 职责：
//! - parse_openai() — OpenAI SSE 流 → UnboundedReceiver<StreamEvent>
//! - parse_anthropic() — Anthropic SSE 流 → UnboundedReceiver<StreamEvent>
//!
//! 注：StreamEvent 定义在 shared_types/llm.rs 中（生产者+消费者共享）。

use reqwest::Response;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::core::types::Timestamp;
use crate::shared_types::llm::{LlmError, StreamEvent};
use crate::shared_types::{Action, Thought, ToolCall};

// ─── internal tool-call accumulator ──────────────────────────────────

#[derive(Default)]
struct AccToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

// ─── StreamProcessor ─────────────────────────────────────────────────

pub struct StreamProcessor;

impl StreamProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StreamProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamProcessor {
    /// Parse an OpenAI SSE stream response into an event channel.
    pub fn parse_openai(
        response: Response,
        trace_id: String,
    ) -> UnboundedReceiver<Result<StreamEvent, LlmError>> {
        let (tx, rx) = unbounded_channel();
        tokio::spawn(async move {
            let body = match response.text().await {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Err(LlmError::StreamError {
                        trace_id,
                        message: format!("读取响应失败: {e}"),
                    }));
                    return;
                }
            };
            for event in parse_openai_sse(&body, &trace_id) {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        rx
    }

    /// Parse an Anthropic SSE stream response into an event channel.
    pub fn parse_anthropic(
        response: Response,
        trace_id: String,
    ) -> UnboundedReceiver<Result<StreamEvent, LlmError>> {
        let (tx, rx) = unbounded_channel();
        tokio::spawn(async move {
            let body = match response.text().await {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Err(LlmError::StreamError {
                        trace_id,
                        message: format!("读取响应失败: {e}"),
                    }));
                    return;
                }
            };
            for event in parse_anthropic_sse(&body, &trace_id) {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        rx
    }
}

// ─── OpenAI SSE parser (design doc §3.4) ─────────────────────────────

/// Internal: parse OpenAI SSE body into event list (testable).
pub(crate) fn parse_openai_sse(body: &str, _trace_id: &str) -> Vec<Result<StreamEvent, LlmError>> {
    let mut events = Vec::new();
    let mut full_text = String::new();
    let mut tool_accs: Vec<AccToolCall> = Vec::new();

    for line in body.lines() {
        // Step 4a: skip non-data lines
        if !line.starts_with("data: ") {
            continue;
        }
        // Step 4b: strip "data: " prefix
        let data = line["data: ".len()..].trim();

        // Step 4c: [DONE] sentinel
        if data == "[DONE]" {
            break;
        }

        // Step 4d: parse JSON
        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => {
                // design doc: log warn + skip
                tracing::warn!("failed to parse SSE JSON line: {}", data);
                continue;
            }
        };

        // Step 4e: extract choices[0]
        let choices = match value.get("choices").and_then(|c| c.as_array()) {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };
        let choice = &choices[0];
        let delta = match choice.get("delta") {
            Some(d) => d,
            None => continue,
        };

        // delta.content → TextDelta
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            if !content.is_empty() {
                full_text.push_str(content);
                events.push(Ok(StreamEvent::TextDelta(content.to_owned())));
            }
        }

        // delta.tool_calls → ToolCallDelta
        if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                // grow accumulator vector
                while tool_accs.len() <= idx {
                    tool_accs.push(AccToolCall::default());
                }
                let acc = &mut tool_accs[idx];
                if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                    acc.id = Some(id.to_owned());
                }
                if let Some(name) = tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                {
                    acc.name = Some(name.to_owned());
                }
                if let Some(args) = tc
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                {
                    acc.arguments.push_str(args);
                }
                events.push(Ok(StreamEvent::ToolCallDelta {
                    index: idx,
                    delta: tc.clone(),
                }));
            }
        }

        // finish_reason → End
        if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
            match reason {
                "tool_calls" => {
                    events.push(Ok(StreamEvent::End(build_tool_call_thought(
                        &tool_accs, &full_text,
                    ))));
                    return events;
                }
                // design doc: "stop" or any other reason → Final
                _ => {
                    events.push(Ok(StreamEvent::End(Thought::Final {
                        answer: full_text,
                        reasoning: String::new(),
                        generated_at: Timestamp::now(),
                    })));
                    return events;
                }
            }
        }
    }

    // Step 5: loop ended without finish_reason but text accumulated
    if !full_text.is_empty() {
        events.push(Ok(StreamEvent::End(Thought::Final {
            answer: full_text,
            reasoning: String::new(),
            generated_at: Timestamp::now(),
        })));
    }

    events
}

/// Build a Thought::Action from accumulated tool-call deltas.
fn build_tool_call_thought(accs: &[AccToolCall], full_text: &str) -> Thought {
    let tool_calls: Vec<ToolCall> = accs
        .iter()
        .filter(|a| a.name.is_some())
        .map(|a| {
            let args: serde_json::Value =
                serde_json::from_str(&a.arguments).unwrap_or(serde_json::Value::Null);
            ToolCall {
                id: a.id.clone().unwrap_or_default(),
                name: a.name.clone().unwrap_or_default(),
                arguments: args,
            }
        })
        .collect();

    // Fall back to Final if no valid tool calls were accumulated
    if tool_calls.is_empty() {
        return Thought::Final {
            answer: full_text.to_owned(),
            reasoning: String::new(),
            generated_at: Timestamp::now(),
        };
    }

    let first = &tool_calls[0];
    let action = Action {
        tool_name: first.name.clone(),
        arguments: first.arguments.clone(),
        tool_call_id: Some(first.id.clone()),
        tool_calls: Some(tool_calls),
        created_at: Timestamp::now(),
    };
    Thought::Action {
        action,
        reasoning: String::new(),
        generated_at: Timestamp::now(),
    }
}

// ─── Anthropic SSE parser (design doc §3.4) ──────────────────────────

/// Internal: parse Anthropic SSE body into event list (testable).
pub(crate) fn parse_anthropic_sse(
    body: &str,
    _trace_id: &str,
) -> Vec<Result<StreamEvent, LlmError>> {
    let mut events = Vec::new();
    let mut full_text = String::new();
    let mut tool_use_id: Option<String> = None;
    let mut tool_name: Option<String> = None;
    let mut tool_input_parts: Vec<String> = Vec::new();

    for line in body.lines() {
        // Step 3a: skip non-data lines
        if !line.starts_with("data: ") {
            continue;
        }
        let data = line["data: ".len()..].trim();

        // Step 3b: parse JSON
        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = match value.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        match event_type {
            // Step 3c — content_block_start
            "content_block_start" => {
                if let Some(block) = value.get("content_block") {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        tool_use_id = block.get("id").and_then(|i| i.as_str()).map(String::from);
                        tool_name = block.get("name").and_then(|n| n.as_str()).map(String::from);
                        tool_input_parts.clear();
                    }
                }
            }

            // Step 3c — content_block_delta
            "content_block_delta" => {
                if let Some(delta) = value.get("delta") {
                    match delta.get("type").and_then(|t| t.as_str()) {
                        Some("text_delta") => {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                full_text.push_str(text);
                                events.push(Ok(StreamEvent::TextDelta(text.to_owned())));
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(partial) =
                                delta.get("partial_json").and_then(|p| p.as_str())
                            {
                                tool_input_parts.push(partial.to_owned());
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Step 3c — message_delta
            "message_delta" => {
                if let Some(msg_delta) = value.get("delta") {
                    match msg_delta.get("stop_reason").and_then(|s| s.as_str()) {
                        Some("tool_use") => {
                            let input: serde_json::Value =
                                serde_json::from_str(&tool_input_parts.concat())
                                    .unwrap_or(serde_json::Value::Null);
                            let tc = ToolCall {
                                id: tool_use_id.clone().unwrap_or_default(),
                                name: tool_name.clone().unwrap_or_default(),
                                arguments: input.clone(),
                            };
                            let action = Action {
                                tool_name: tool_name.clone().unwrap_or_default(),
                                arguments: input,
                                tool_call_id: tool_use_id.clone(),
                                tool_calls: Some(vec![tc]),
                                created_at: Timestamp::now(),
                            };
                            events.push(Ok(StreamEvent::End(Thought::Action {
                                action,
                                reasoning: String::new(),
                                generated_at: Timestamp::now(),
                            })));
                            return events;
                        }
                        // design doc: "end_turn" or any other reason → Final
                        _ => {
                            events.push(Ok(StreamEvent::End(Thought::Final {
                                answer: full_text,
                                reasoning: String::new(),
                                generated_at: Timestamp::now(),
                            })));
                            return events;
                        }
                    }
                }
            }

            // Step 3c — message_stop
            "message_stop" => {
                if !full_text.is_empty() {
                    events.push(Ok(StreamEvent::End(Thought::Final {
                        answer: full_text,
                        reasoning: String::new(),
                        generated_at: Timestamp::now(),
                    })));
                }
                return events;
            }

            // unknown event types: ignore (design doc)
            _ => {}
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── OpenAI parse_openai_sse ──────────────────────────────────────

    #[test]
    fn test_section_3_4_openai_text_delta() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n";
        let events = parse_openai_sse(body, "t1");
        assert_eq!(events.len(), 3);
        match &events[0] {
            Ok(StreamEvent::TextDelta(t)) => assert_eq!(t, "Hello"),
            _ => panic!("expected TextDelta"),
        }
        match &events[1] {
            Ok(StreamEvent::TextDelta(t)) => assert_eq!(t, " world"),
            _ => panic!("expected TextDelta"),
        }
        match &events[2] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => {
                assert_eq!(answer, "Hello world")
            }
            _ => panic!("expected End(Final)"),
        }
    }

    #[test]
    fn test_section_3_4_openai_done_sentinel() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\
                     data: [DONE]\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ignored\"},\"finish_reason\":null}]}\n";
        let events = parse_openai_sse(body, "t2");
        // [DONE] stops processing; no End is emitted because we never got a finish_reason
        // and full_text is not empty, so Step 5 appends an End(Final)
        assert_eq!(events.len(), 2);
        match &events[0] {
            Ok(StreamEvent::TextDelta(t)) => assert_eq!(t, "Hi"),
            _ => panic!("expected TextDelta"),
        }
        match &events[1] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => assert_eq!(answer, "Hi"),
            _ => panic!("expected End(Final)"),
        }
    }

    #[test]
    fn test_section_3_4_openai_skip_non_data_lines() {
        let body = "event: test\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\"},\"finish_reason\":null}]}\n\
                     :comment\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n";
        let events = parse_openai_sse(body, "t3");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_section_3_4_openai_malformed_json_skipped() {
        let body = "data: not json\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n";
        let events = parse_openai_sse(body, "t4");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_section_3_4_openai_empty_stream() {
        let events = parse_openai_sse("", "t5");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_section_3_4_openai_no_finish_reason_with_text() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"incomplete\"},\"finish_reason\":null}]}\n";
        let events = parse_openai_sse(body, "t6");
        // Step 5: no finish_reason, but text non-empty → End(Final)
        assert_eq!(events.len(), 2);
        match &events[1] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => assert_eq!(answer, "incomplete"),
            _ => panic!("expected End(Final)"),
        }
    }

    #[test]
    fn test_section_3_4_openai_tool_call_deltas() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"location\\\":\"}}]},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\" \\\"NYC\\\"}\"}}]},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n";
        let events = parse_openai_sse(body, "t7");
        assert_eq!(events.len(), 4);
        match &events[3] {
            Ok(StreamEvent::End(Thought::Action { action, .. })) => {
                assert_eq!(action.tool_name, "get_weather");
                assert_eq!(action.arguments, json!({"location": "NYC"}));
                assert!(action.tool_call_id.is_some());
                assert!(action.tool_calls.is_some());
                let tcs = action.tool_calls.as_ref().unwrap();
                assert_eq!(tcs.len(), 1);
                assert_eq!(tcs[0].id, "call_1");
                assert_eq!(tcs[0].name, "get_weather");
            }
            other => panic!("expected End(Action), got {other:?}"),
        }
    }

    #[test]
    fn test_section_3_4_openai_multiple_tool_calls() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"fn_a\",\"arguments\":\"{\\\"a\\\":1}\"}},{\"index\":1,\"id\":\"c2\",\"function\":{\"name\":\"fn_b\",\"arguments\":\"{\\\"b\\\":2}\"}}]},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n";
        let events = parse_openai_sse(body, "t8");
        assert_eq!(events.len(), 3);
        match &events[2] {
            Ok(StreamEvent::End(Thought::Action { action, .. })) => {
                let tcs = action.tool_calls.as_ref().unwrap();
                assert_eq!(tcs.len(), 2);
                assert_eq!(tcs[0].name, "fn_a");
                assert_eq!(tcs[1].name, "fn_b");
            }
            _ => panic!("expected End(Action)"),
        }
    }

    #[test]
    fn test_section_3_4_openai_tool_call_no_valid_calls_falls_back_to_final() {
        // tool_calls delta with no name/id → build_tool_call_thought returns Final
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0}]},\"finish_reason\":null}]}\n\
                     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n";
        let events = parse_openai_sse(body, "t9");
        assert_eq!(events.len(), 2);
        match &events[1] {
            Ok(StreamEvent::End(Thought::Final { .. })) => {}
            other => panic!("expected End(Final) fallback, got {other:?}"),
        }
    }

    #[test]
    fn test_section_3_4_openai_finish_reason_other_treated_as_final() {
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":\"length\"}]}\n";
        let events = parse_openai_sse(body, "t10");
        assert_eq!(events.len(), 2);
        match &events[1] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => assert_eq!(answer, "done"),
            _ => panic!("expected End(Final)"),
        }
    }

    // ── Anthropic parse_anthropic_sse ────────────────────────────────

    #[test]
    fn test_section_3_4_anthropic_text_delta() {
        let body = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\
                     data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\
                     data: {\"type\":\"message_stop\"}\n";
        let events = parse_anthropic_sse(body, "t11");
        assert_eq!(events.len(), 3);
        match &events[0] {
            Ok(StreamEvent::TextDelta(t)) => assert_eq!(t, "Hello"),
            _ => panic!("expected TextDelta"),
        }
        match &events[1] {
            Ok(StreamEvent::TextDelta(t)) => assert_eq!(t, " world"),
            _ => panic!("expected TextDelta"),
        }
        match &events[2] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => {
                assert_eq!(answer, "Hello world")
            }
            _ => panic!("expected End(Final)"),
        }
    }

    #[test]
    fn test_section_3_4_anthropic_tool_use() {
        let body = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"location\\\":\"}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\" \\\"NYC\\\"}\"}}\n\
                     data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"}}\n\
                     data: {\"type\":\"message_stop\"}\n";
        let events = parse_anthropic_sse(body, "t12");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::End(Thought::Action { action, .. })) => {
                assert_eq!(action.tool_name, "get_weather");
                assert_eq!(action.arguments, json!({"location": "NYC"}));
                assert_eq!(action.tool_call_id.as_deref(), Some("toolu_1"));
            }
            _ => panic!("expected End(Action)"),
        }
    }

    #[test]
    fn test_section_3_4_anthropic_message_stop_without_delta() {
        // Simulate a final response that ends with message_stop directly
        // (no message_delta before it), which happens with some Anthropic API versions.
        let body = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"bye\"}}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" bye\"}}\n\
                     data: {\"type\":\"message_stop\"}\n";
        let events = parse_anthropic_sse(body, "t13");
        assert_eq!(events.len(), 3);
        match &events[2] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => {
                assert_eq!(answer, "bye bye");
            }
            _ => panic!("expected End(Final)"),
        }
    }

    #[test]
    fn test_section_3_4_anthropic_empty_stream() {
        let events = parse_anthropic_sse("", "t14");
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_section_3_4_anthropic_ignore_unknown_event_type() {
        let body = "data: {\"type\":\"ping\"}\n\
                     data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\
                     data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\
                     data: {\"type\":\"message_stop\"}\n";
        let events = parse_anthropic_sse(body, "t15");
        assert_eq!(events.len(), 2);
        match &events[1] {
            Ok(StreamEvent::End(Thought::Final { answer, .. })) => assert_eq!(answer, "hello"),
            _ => panic!("expected End(Final)"),
        }
    }
}
