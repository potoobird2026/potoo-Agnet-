# MCP（Model Context Protocol 连接器）严格 AI 开发计划

> 集成补全版本：2026-06-01（v0.2.0 集成）—— MCP→ToolExecutor 集成计划已完成

本计划用于指导 AI 严格按照 `docs/services/mcp/mcp开发文档.md` 生成 mcp 模块的全部代码。

---

## 项目背景

- **模块名称**：mcp（Model Context Protocol 连接器）
- **模块定位**：通过 stdio 子进程连接外部 MCP Server，将远程工具代理为 `ToolContract`，供 `ToolExecutorSlot` 统一调度。
- **外部接口**：
  - `McpService` — ServicePlugin 入口（待创建）
  - `McpConfig` — 配置
  - `StdioMcpConnector` — stdio 子进程连接器
  - `McpToolProxy` — 远程工具代理（ToolContract 实现）
- **当前状态**：config.rs ✅、connector.rs ✅、protocol.rs ✅、proxy.rs ✅、service.rs ❌
- **待修复**：`"2024-11-05"` 协议版本待迁移为 `const`（§4 违规 #4）、`"aagnet"` clientInfo.name 待迁移为 `const`（§4 违规 #5）
- **依赖项**：`tokio::process`、`serde_json`、`dirs`

---

## 硬编码分类定义（mcp 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 连接超时 | `10` 秒 | 从 `McpConfig.connect_timeout_secs` 读取 |
| 请求超时 | `30` 秒 | 从 `McpConfig.request_timeout_secs` 读取 |
| 最大重试 | `3` | 从 `McpConfig.max_retries` 读取 |
| 协议版本 | `"2024-11-05"` 字面量 | 迁移为 `const MCP_PROTOCOL_VERSION: &str` |
| 客户端名 | `"aagnet"` 字面量 | 迁移为 `const CLIENT_NAME: &str` |
| 方法名 | `"initialize"` | 定义为 `const MCP_METHOD_INITIALIZE`（已有） |
| 工具名分隔 | `/` | 固定分隔符 conn/tool（工具名构造规则） |
| 分组名 | `"mcp"` | 固定返回（非配置） |

---

## 项目目录结构

```
src/plugins/services/mcp/
├── mod.rs        # 模块入口：McpService / McpConfig / StdioMcpConnector / McpToolProxy
├── service.rs    # McpService（ServicePlugin 实现，新建）
├── config.rs     # McpConfig（已有）
├── connector.rs  # StdioMcpConnector — stdio 子进程通信（已有，修复协议版本 const）
├── protocol.rs   # JSON-RPC 2.0 类型 + MCP 协议常量（已有，新增 client_info const）
└── proxy.rs      # McpToolProxy — ToolContract 包装（已有）
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 mcp开发文档.md 一致。

2. **零幻觉**：
   a. MCP 只有 1 种传输方式（stdio），不存在 HTTP/SSE 传输。
   b. StdioMcpConnector 使用短连接模式（每次请求新进程），不存在长连接池。
   c. McpToolProxy 的 name 格式为 "{connector}/{tool}"，group 固定为 "mcp"。

3. **零硬编码**：
   a. 超时值（connect_timeout_secs / request_timeout_secs）从 McpConfig 读取。
   b. 重试参数（max_retries / max_reconnect_attempts）从 McpConfig 读取。
   c. MCP 协议版本（"2024-11-05"）定义为 const MCP_PROTOCOL_VERSION。
   d. clientInfo.name 定义为 const CLIENT_NAME。
   e. MCP 方法名定义为 const（initialize/tools/list/tools/call）。

4. **完整实现**：McpService 的 init/start/handle_signal/stop/shutdown 齐全。

5. **错误处理**：
   - connect() 失败（子进程退出/超时）返回 Err，不 panic。
   - execute() 中 MCP Server 返回 error → isError 标记，不 panic。
   - Mutex 毒锁 panic 可接受（严重内部错误）。

6. **测试同步生成**：
   - connector: 创建/初始状态/端点类型。
   - proxy: 使用 FakeConnector mock 测试 ToolContract 实现。
   - service: 完整生命周期 init/start/signal/stop/shutdown。
```

---

## 详细开发步骤

### 步骤 0：确认骨架 + 修复硬编码违规

**操作**：
1. 确认 config.rs / connector.rs / protocol.rs / proxy.rs 已存在
2. 在 protocol.rs 中新增常量：
   ```rust
   pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
   pub const CLIENT_NAME: &str = "aagnet";
   pub const CLIENT_VERSION: &str = "0.2.0";
   ```
3. 在 connector.rs 中将 `"2024-11-05"` 和 `"aagnet"` 字面量替换为引用上述 const
4. 创建 service.rs

**验收**：`cargo check` 通过

---

### 步骤 1：Config 层（config.rs，已有）

确认 `McpConfig`：
- enabled(false), connect_timeout_secs(10), request_timeout_secs(30), max_retries(3), auto_reconnect(true), max_reconnect_attempts(5)
- `to_connection_config()` 转换为框架 MCP 连接参数（Duration + 指数退避）

**验收**：配置解析 + 默认值测试

---

### 步骤 2：Protocol 层（protocol.rs，已有 + 修复）

确认类型：`JsonRpcRequest` / `JsonRpcResponse` / `JsonRpcError` / `InitializeParams` / `InitializeResult` / `ToolsListResult` / `ToolCallParams` / `ToolCallResult`

新增常量：`MCP_PROTOCOL_VERSION` / `CLIENT_NAME` / `CLIENT_VERSION`

**验收**：序列化/反序列化测试

---

### 步骤 3：Connector 层（connector.rs，已有）

确认 `StdioMcpConnector`：
- 字段：name, endpoint(Stdio), conn_config, state(Mutex<McpConnectionState>), next_id, reconnect_count
- connect() → initialize + tools/list
- execute() → tools/call（短连接模式，每次新进程）
- disconnect() → 标记 Disconnected
- spawn_and_communicate() 内部：tokio::process::Command + stdin/stdout + timeout + kill_on_drop

修复点：协议版本 `"2024-11-05"` → `MCP_PROTOCOL_VERSION`，`"aagnet"` → `CLIENT_NAME`

**验收**：创建/状态测试

---

### 步骤 4：Proxy 层（proxy.rs，已有）

确认 `McpToolProxy`：
- 名称格式：`"{connector}/{tool}"`
- ToolContract 实现：name()/group("mcp")/description()/definition()/execute()
- from_manifests() 批量构造

**验收**：FakeConnector mock 测试

---

### 步骤 5：McpService（service.rs，新建）

```rust
pub struct McpService {
    config: McpConfig,
    connector: Option<StdioMcpConnector>,
    proxies: Vec<Arc<dyn ToolContract>>,
    running: bool,
}

impl ServicePlugin for McpService {
    fn name(&self) -> &str { "mcp" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<()> {
        1. 解析 McpConfig
        2. 如 enabled=false → 跳过，日志 info
        3. 创建 StdioMcpConnector（用配置的 servers 列表）
        4. 预检查 server command 可执行文件存在性
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<()> {
        1. 如 enabled=false → 跳过
        2. running=true
        3. connector.connect() → 获取 McpToolManifest 列表
        4. McpToolProxy::from_manifests() → 批量构造代理
        5. ap.register_provider("mcp_tools", proxies)
    }

    async fn handle_signal(&self, signal: ServiceSignal) -> Result<()>
        HealthCheck → running 检查（5s 内返回）
        ConfigReload → 重连（disconnect + connect + 重新注册）
        GracefulShutdown → 标记不运行
        Suspend → 标记暂停
        Resume → 标记恢复
        _ → Ok(())
    }

    async fn stop(&self) -> Result<()> { running=false }

    async fn shutdown(&self) -> Result<()> {
        1. connector.disconnect()
        2. 反注册 Provider
        3. running=false
    }
}
```

### 步骤 6：mod.rs

```
pub use service::McpService;
pub use config::McpConfig;
pub use connector::StdioMcpConnector;
pub use proxy::McpToolProxy;
```

### 步骤 7：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. 对照 mcp开发文档.md §6.3 的 10 项自查清单（重点修复 #4 协议版本 const + #5 clientInfo const）
3. McpService 完整生命周期测试
