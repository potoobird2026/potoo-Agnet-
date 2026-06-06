use aagnet::core::runtime::SharedMessageStore;
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

#[tokio::test]
async fn test_store_new_is_empty() {
    let store = SharedMessageStore::new();
    assert!(store.is_empty().await);
    assert_eq!(store.session_count().await, 0);
}

#[tokio::test]
async fn test_store_write_read() {
    let store = SharedMessageStore::new();
    let msgs = vec![make_msg(MessageRole::User, "hello")];
    let version = store.write("s1", msgs.clone()).await;
    assert_eq!(version, 1);
    let (read_msgs, read_ver) = store.read("s1").await;
    assert_eq!(read_msgs.len(), 1);
    assert_eq!(read_ver, 1);
    assert_eq!(read_msgs[0].role, MessageRole::User);
}

#[tokio::test]
async fn test_store_multiple_sessions() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "msg1")])
        .await;
    store
        .write("s2", vec![make_msg(MessageRole::System, "sys")])
        .await;
    assert_eq!(store.session_count().await, 2);
    let (msgs1, _) = store.read("s1").await;
    assert_eq!(msgs1[0].content[0].as_text(), Some("msg1"));
}

#[tokio::test]
async fn test_store_compare_and_write_ok() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "v1")])
        .await;
    let new_msgs = vec![make_msg(MessageRole::User, "v2")];
    let result = store.compare_and_write("s1", 1, new_msgs).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 2);
}

#[tokio::test]
async fn test_store_compare_and_write_version_mismatch() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "v1")])
        .await;
    let new_msgs = vec![make_msg(MessageRole::User, "v2")];
    let result = store.compare_and_write("s1", 999, new_msgs).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_store_remove_session() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "x")])
        .await;
    assert!(!store.is_empty().await);
    store.remove_session("s1").await;
    assert!(store.is_empty().await);
}

#[tokio::test]
async fn test_store_get_messages() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "hi")])
        .await;
    let msgs = store.get_messages("s1").await;
    assert_eq!(msgs.len(), 1);
    let empty = store.get_messages("nonexistent").await;
    assert!(empty.is_empty());
}

#[tokio::test]
async fn test_store_snapshot() {
    let store = SharedMessageStore::new();
    store
        .write("s1", vec![make_msg(MessageRole::User, "a")])
        .await;
    store
        .write("s2", vec![make_msg(MessageRole::Assistant, "b")])
        .await;
    let snap = store.snapshot().await;
    assert_eq!(snap.len(), 2);
    assert!(snap.contains_key("s1"));
    assert!(snap.contains_key("s2"));
}

#[tokio::test]
async fn test_store_version_increments() {
    let store = SharedMessageStore::new();
    let v1 = store
        .write("s1", vec![make_msg(MessageRole::User, "1")])
        .await;
    let v2 = store
        .write("s1", vec![make_msg(MessageRole::User, "2")])
        .await;
    let v3 = store
        .write("s1", vec![make_msg(MessageRole::User, "3")])
        .await;
    assert_eq!(v1, 1);
    assert_eq!(v2, 2);
    assert_eq!(v3, 3);
}
