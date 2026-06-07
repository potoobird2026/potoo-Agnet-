#![cfg(not(target_os = "windows"))]
/*! MCP 集成测试 —— 真子进程 E2E
 *
 * C-3: 起真 mcp_mock_server 子进程（CARGO_BIN_EXE_mcp_mock_server），
 *      走完整 McpService init + start + ToolProvider.execute 路径
 *
 * 红线遵守：
 * - 不许 MockProvider 跳过 stdio（用户口头强约束）
 * - 起真子进程，验证真实 JSON-RPC over stdio 协议路径
 */
use aagnet::core::access::{ServiceAccessImpl, ServiceAccessPoint};
use aagnet::core::service::ServicePlugin;
use aagnet::core::types::plugin::PluginInitContext;
use aagnet::core::AgentConfig;
use aagnet::plugins::services::mcp::McpService;
use aagnet::shared_types::{DynProvider, McpBundle, PROVIDER_MCP_TOOLS};
use serde_json::json;
use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Mock ServiceAccessImpl —— 只记录 register，不验证协议
struct MockServiceAccess {
    providers: std::sync::Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>,
}

impl MockServiceAccess {
    fn new() -> Self {
        Self {
            providers: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl ServiceAccessImpl for MockServiceAccess {
    fn get_config(&self) -> AgentConfig {
        AgentConfig::default()
    }
    fn log(&self, _level: &str, _message: &str) {}
    fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>) {
        self.providers
            .lock()
            .unwrap()
            .insert(name.to_string(), provider);
    }
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.providers.lock().unwrap().get(name).cloned()
    }
    fn unregister_provider(&self, name: &str) {
        self.providers.lock().unwrap().remove(name);
    }
}

/// 工具：把 mock server 路径解析为字符串
fn mock_server_path() -> String {
    std::env::var("CARGO_BIN_EXE_mcp_mock_server").unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_connects_to_real_subprocess() {
    // 1. 构造 PluginInitContext
    let config = json!({
        "servers": [{
            "name": "mock",
            "command": mock_server_path(),
            "args": [],
            "enabled": true
        }],
        "connect_timeout_secs": 5,
        "request_timeout_secs": 5
    });
    let ctx = PluginInitContext::new("mcp", config, AgentConfig::default(), PathBuf::from("."));

    // 2. 启动 McpService
    let mock = Arc::new(MockServiceAccess::new());
    let ap = ServiceAccessPoint::new(mock.clone() as Arc<dyn ServiceAccessImpl>);
    let mut svc = McpService::new();
    svc.init(&ctx).await.expect("init 应成功");
    svc.start(ap).await.expect("start 应成功");

    // 3. 验证 errors 记录为空（mock server 正常）
    let errs = svc.errors().await;
    assert!(errs.is_empty(), "mock server 应无错误，实测: {:?}", errs);

    // 4. 验证 mock 注册了 PROVIDER_MCP_TOOLS
    assert!(
        mock.providers
            .lock()
            .unwrap()
            .contains_key(PROVIDER_MCP_TOOLS),
        "McpService 应注册 PROVIDER_MCP_TOOLS"
    );

    // 5. 取出 McpBundle
    let raw = mock
        .providers
        .lock()
        .unwrap()
        .get(PROVIDER_MCP_TOOLS)
        .cloned()
        .expect("PROVIDER_MCP_TOOLS 必须存在");
    let wrapper = raw
        .downcast::<DynProvider<dyn McpBundle>>()
        .expect("downcast 失败");
    let bundle: Arc<dyn McpBundle> = wrapper.0.clone();

    // 6. 验证工具列表
    let providers = bundle.all();
    assert_eq!(providers.len(), 1, "mock server 应暴露 1 个 tool");
    let proxy = &providers[0];
    let defs = proxy.list();
    assert_eq!(defs.len(), 1);
    let def = &defs[0];
    assert!(
        def.name.contains("echo"),
        "tool 名应含 'echo'，实为: {}",
        def.name
    );
    assert_eq!(def.entry, format!("mcp:mock"), "entry 应为 mcp:mock");

    // 7. 执行 echo 工具（真子进程通信）
    let result = proxy
        .execute("echo", json!({"msg": "hello"}), Duration::from_secs(5))
        .await
        .expect("execute 应成功");
    assert!(
        result.contains("hello") || result.contains("echo"),
        "响应应含 'hello' 或 'echo'，实为: {}",
        result
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mcp_graceful_shutdown() {
    let config = json!({
        "servers": [{
            "name": "mock2",
            "command": mock_server_path(),
            "args": [],
            "enabled": true
        }],
        "connect_timeout_secs": 5,
        "request_timeout_secs": 5
    });
    let ctx = PluginInitContext::new("mcp", config, AgentConfig::default(), PathBuf::from("."));
    let mock = Arc::new(MockServiceAccess::new());
    let ap = ServiceAccessPoint::new(mock.clone() as Arc<dyn ServiceAccessImpl>);
    let mut svc = McpService::new();
    svc.init(&ctx).await.expect("init");
    svc.start(ap).await.expect("start");
    assert!(svc.metadata().running, "running 应为 true");
    // shutdown 应清理连接
    svc.shutdown().await.expect("shutdown");
    let errs_after = svc.errors().await;
    assert!(errs_after.is_empty(), "shutdown 后 errors 应清空");
}
