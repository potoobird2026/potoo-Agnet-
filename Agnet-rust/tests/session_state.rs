use aagnet::core::runtime::SessionState;
use aagnet::core::types::Timestamp;
use aagnet::shared_types::{ContentBlock, Message, MessageRole};

fn make_msg(role: MessageRole, text: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text(text.to_string())],
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: None,
        created_at: Timestamp::now(),
    }
}

#[test]
fn test_session_new_is_empty() {
    let s = SessionState::new("test-session".to_string(), 10, 1000);
    assert_eq!(s.session_id, "test-session");
    assert_eq!(s.max_turns, 10);
    assert_eq!(s.context_window, 1000);
    assert!(s.messages.is_empty());
}

#[test]
fn test_session_with_system_prompt() {
    let s = SessionState::new("s1".to_string(), 10, 1000)
        .with_system_prompt("You are a helpful assistant.".to_string());
    assert_eq!(s.messages.len(), 1);
    assert_eq!(s.messages[0].role, MessageRole::System);
    assert_eq!(
        s.messages[0].content[0].as_text(),
        Some("You are a helpful assistant.")
    );
}

#[test]
fn test_session_with_empty_system_prompt() {
    let s = SessionState::new("s1".to_string(), 10, 1000).with_system_prompt(String::new());
    assert!(s.messages.is_empty());
}

#[test]
fn test_session_push_message() {
    let mut s = SessionState::new("s1".to_string(), 10, 1000);
    s.push_message(make_msg(MessageRole::User, "hello"));
    s.push_message(make_msg(MessageRole::Assistant, "world"));
    assert_eq!(s.messages.len(), 2);
    assert_eq!(s.messages[0].role, MessageRole::User);
    assert_eq!(s.messages[1].role, MessageRole::Assistant);
}

#[test]
fn test_session_replace_messages() {
    let mut s = SessionState::new("s1".to_string(), 10, 1000);
    s.push_message(make_msg(MessageRole::User, "old"));
    let new_msgs = vec![
        make_msg(MessageRole::System, "sys"),
        make_msg(MessageRole::User, "new"),
    ];
    s.replace_messages(new_msgs);
    assert_eq!(s.messages.len(), 2);
    assert_eq!(s.messages[0].role, MessageRole::System);
    assert_eq!(s.messages[1].role, MessageRole::User);
}
