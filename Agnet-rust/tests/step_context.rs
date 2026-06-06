use std::sync::Arc;

use aagnet::core::access::{ProviderRegistry, SlotAccessPoint};
use aagnet::core::context::StepContext;
use aagnet::core::types::Timestamp;
use aagnet::shared_types::{ContentBlock, Message, MessageRole};

fn make_msg(text: &str) -> Message {
    Message {
        role: MessageRole::User,
        content: vec![ContentBlock::Text(text.to_string())],
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: None,
        created_at: Timestamp::now(),
    }
}

#[tokio::test]
async fn test_step_context_new() {
    let ctx = StepContext::new("s1".to_string(), vec![], 10);
    assert_eq!(ctx.session_id, "s1");
    assert!(ctx.messages.is_empty());
    assert_eq!(ctx.max_turns, 10);
    assert_eq!(ctx.current_turn, 0);
    assert_eq!(ctx.phase_name, "");
}

#[tokio::test]
async fn test_step_context_with_messages() {
    let msgs = vec![make_msg("hello")];
    let ctx = StepContext::new("s1".to_string(), msgs, 10);
    assert_eq!(ctx.messages.len(), 1);
    assert_eq!(ctx.messages[0].role, MessageRole::User);
}

#[tokio::test]
async fn test_step_context_set_get_context() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    ctx.set_context("key1", 42u32);
    ctx.set_context("key2", "hello".to_string());
    assert_eq!(*ctx.get_context::<u32>("key1").unwrap(), 42);
    assert_eq!(ctx.get_context::<String>("key2").unwrap(), "hello");
    assert!(ctx.get_context::<f64>("key1").is_none());
}

#[tokio::test]
async fn test_step_context_slot_access_messages() {
    let msgs = vec![make_msg("test")];
    let mut ctx = StepContext::new("s1".to_string(), msgs, 10);
    let ap: &mut dyn SlotAccessPoint = &mut ctx;
    assert_eq!(ap.messages().len(), 1);
    assert_eq!(ap.messages()[0].content[0].as_text(), Some("test"));
}

#[tokio::test]
async fn test_step_context_slot_access_session_id() {
    let mut ctx = StepContext::new("my-session".to_string(), vec![], 10);
    let ap: &mut dyn SlotAccessPoint = &mut ctx;
    assert_eq!(ap.session_id(), "my-session");
}

#[tokio::test]
async fn test_step_context_slot_access_phase_name() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    ctx.phase_name = "test_phase".to_string();
    let ap: &mut dyn SlotAccessPoint = &mut ctx;
    assert_eq!(ap.phase_name(), "test_phase");
}

#[tokio::test]
async fn test_step_context_slot_access_write_observation() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = ctx.write_observation(Box::new("obs_data".to_string()));
    // Without permissions, this should succeed (empty permissions = no check)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_step_context_slot_access_request_jump() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    {
        let ap: &mut dyn SlotAccessPoint = &mut ctx;
        let result = ap.request_jump("target_phase");
        assert!(result.is_ok());
    }
    let directive = ctx.take_pending_directive();
    assert!(directive.is_some());
}

#[tokio::test]
async fn test_step_context_slot_access_request_abort() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    {
        let ap: &mut dyn SlotAccessPoint = &mut ctx;
        let result = ap.request_abort();
        assert!(result.is_ok());
    }
    let directive = ctx.take_pending_directive();
    assert!(directive.is_some());
}

#[tokio::test]
async fn test_step_context_write_context_raw() {
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = ctx.write_context_raw("custom", Box::new(100u32));
    assert!(result.is_ok());
    let val = ctx.read_context_raw("custom");
    assert!(val.is_some());
}

#[tokio::test]
async fn test_step_context_provider_raw() {
    let reg = Arc::new(ProviderRegistry::new());
    reg.register("test_svc", Arc::new(42u32));
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10).with_provider_registry(reg);
    let ap: &mut dyn SlotAccessPoint = &mut ctx;
    let provided = ap.provider_raw("test_svc");
    assert!(provided.is_some());
    let no_provider = ap.provider_raw("nonexistent");
    assert!(no_provider.is_none());
}
