use aagnet::core::access::ProviderRegistry;
use std::sync::Arc;

#[tokio::test]
async fn test_provider_registry_new_is_empty() {
    let reg = ProviderRegistry::new();
    assert!(reg.list().is_empty());
    assert!(!reg.has("anything"));
}

#[tokio::test]
async fn test_provider_registry_register_and_get() {
    let reg = ProviderRegistry::new();
    reg.register("my_provider", Arc::new(42u32));
    let val = reg.get::<u32>("my_provider");
    assert!(val.is_some());
    assert_eq!(*val.unwrap(), 42);
}

#[tokio::test]
async fn test_provider_registry_get_nonexistent() {
    let reg = ProviderRegistry::new();
    reg.register("existing", Arc::new("hello".to_string()));
    assert!(reg.get::<String>("nonexistent").is_none());
    assert!(reg.get::<String>("existing").is_some());
}

#[tokio::test]
async fn test_provider_registry_wrong_type() {
    let reg = ProviderRegistry::new();
    reg.register("x", Arc::new(42u32));
    assert!(reg.get::<String>("x").is_none());
    assert!(reg.get::<u32>("x").is_some());
}

#[tokio::test]
async fn test_provider_registry_unregister() {
    let reg = ProviderRegistry::new();
    reg.register("tmp", Arc::new(true));
    assert!(reg.has("tmp"));
    reg.unregister("tmp");
    assert!(!reg.has("tmp"));
    assert!(reg.list().is_empty());
}

#[tokio::test]
async fn test_provider_registry_list() {
    let reg = ProviderRegistry::new();
    reg.register("a", Arc::new(1u32));
    reg.register("b", Arc::new(2u32));
    let mut names = reg.list();
    names.sort();
    assert_eq!(names, vec!["a", "b"]);
}

#[tokio::test]
async fn test_provider_registry_overwrite() {
    let reg = ProviderRegistry::new();
    reg.register("key", Arc::new(1u32));
    reg.register("key", Arc::new(2u32));
    let val = reg.get::<u32>("key").unwrap();
    assert_eq!(*val, 2);
}

#[tokio::test]
async fn test_provider_registry_get_raw() {
    let reg = ProviderRegistry::new();
    reg.register("x", Arc::new(99u64));
    let raw = reg.get_raw("x");
    assert!(raw.is_some());
    let downcasted = raw.unwrap().downcast::<u64>().ok();
    assert!(downcasted.is_some());
    assert_eq!(*downcasted.unwrap(), 99);
}
