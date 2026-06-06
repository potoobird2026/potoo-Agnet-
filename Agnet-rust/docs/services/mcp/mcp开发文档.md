# MCP（Model Context Protocol 连接器）开发文档

## 0. 协议依据

本文档严格遵循以下三份协议标准，逐条对标：

| 协议 | 应用层级 | 关键条款 |
|------|---------|---------|
| **protocol-Service集成协议** | 模块对框架的接入方式 | §1 ServicePlugin 单入口、§2 ServiceAccessPoint 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§8 协议特有红线 |
| **protocol-模块内部组件协议** | 模块内部子模块组织方式 | §1 Component 单入口、§3 AccessPoint 内部数据共享通道、§4 Processing 处理结果、§5 Orchestrator 模块协调器、§6 模块边界规范 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 自查清单 |

---

## 1. 模块定位

### 1.1 一句话描述

通过 **stdio 子进程** 连接外部 MCP Server，将其暴露的工具 **代理为 `ToolContract`**，供 `ToolExecutorSlot` 统一调度调用，使 Agent 能透明使用外部工具。

### 1.2 架构定位

MCP 模块定位为 **ServicePlugin**（服务插件），但当前实现聚焦于**连接器 + 代理**两个核心组件：

```
┌────────────────────────────────────────────────────────────┐
│  McpService (impl ServicePlugin) ← 待补齐                      │
│  - init(): 读取 McpConfig                                     │
│  - start(): 建立连接 → 工具代理 → 注册为 mcp_tools Provider    │
│  - handle_signal(): ConfigReload / HealthCheck                │
│  - shutdown(): 关闭连接                                       │
└────────────────────────────────────────────────────────────┘
          │
          ▼
┌────────────────────────────────────────────────────────────┐
│  StdioMcpConnector (impl McpConnectorContract)              │
│  - connect():    initialize → tools/list → 返回工具清单      │
│  - execute():    tools/call → 转发参数 → 返回结果            │
│  - disconnect(): 清理子进程                                  │
└────────────────────────────────────────────────────────────┘
          │ 工具发现后创建
          ▼
┌────────────────────────────────────────────────────────────┐
│  McpToolProxy (impl ToolContract)                           │
│  - 一个 MCP 远程工具 = 一个 McpToolProxy 实例                │
│  - name = "{connector_name}/{tool_name}"                    │
│  - execute() → 委托 connector.execute()                     │
└────────────────────────────────────────────────────────────┘
          │ stdio (stdin/stdout JSON-RPC 2.0)
          ▼
┌────────────────────────────────────────────────────────────┐
│  外部 MCP Server 进程（Python / Node.js / Rust）             │
│  - initialize → 返回 ServerCapabilities                     │
│  - tools/list → 返回 [tool1, tool2, ...]                    │
│  - tools/call → 执行并返回 {content, isError}               │
└────────────────────────────────────────────────────────────┘
```

---

## 2. 文件结构

```
src/plugins/services/mcp/
├── mod.rs        # 模块入口：子模块声明 + 公开类型 re-export
├── config.rs     # McpConfig 配置结构体 + Default 实现
├── connector.rs  # StdioMcpConnector — 子进程 stdio 通信
├── protocol.rs   # JSON-RPC 2.0 消息类型 + MCP 协议常量
└── proxy.rs      # McpToolProxy — 将远程工具包装为 ToolContract
```

> **模块边界规范（§6.1）**：`mod.rs` 仅暴露 `McpConfig`、`StdioMcpConnector`、`McpToolProxy` 三个公共类型，内部协议类型（`JsonRpcRequest` 等）均为 `pub(crate)`。

---

## 3. 功能清单

| 功能 | 描述 | 实现状态 | 对应源码 |
|------|------|:---:|---------|
| MCP 连接 | 通过 stdio 启动子进程，发送 `initialize` 握手 | ✅ | `connector.rs:connect()` |
| 工具发现 | `tools/list` 获取 MCP Server 工具列表 | ✅ | `connector.rs:connect()` 后半段 |
| 工具代理 | 将 MCP 工具包装为 `ToolContract`，命名 `{connector}/{tool}` | ✅ | `proxy.rs:McpToolProxy` |
| 工具调用 | `tools/call` 转发参数并返回结果 | ✅ | `connector.rs:execute()` |
| 超时控制 | 连接超时 + 请求超时独立配置 | ✅ | `config.rs → McpConnectionConfig` |
| 自动重连 | 连接失败后指数退避重试 | ✅ | `config.rs:auto_reconnect` / `max_reconnect_attempts` |
| ServicePlugin | 完整的 Service 生命周期（init/start/stop/shutdown） | ❌ 待补齐 | — |
| Provider 注册 | 通过 `ServiceAccessPoint::register_provider()` 注册 | ❌ 待补齐 | — |
| 心跳检测 | `ping` 定期检测连接健康状态 | ❌ 待补齐 | — |
| 多 Server | 同时连接多个 MCP Server | ❌ 待补齐 | — |

---

## 4. 核心设计

### 4.1 McpConfig（配置）

**文件**：`config.rs`

```rust
pub struct McpConfig {
    pub enabled: bool,                  // 是否启用 MCP 服务
    pub connect_timeout_secs: u64,      // 连接超时（秒），默认 10
    pub request_timeout_secs: u64,      // 请求超时（秒），默认 30
    pub max_retries: u32,               // 最大重试次数，默认 3
    pub auto_reconnect: bool,           // 自动重连，默认 true
    pub max_reconnect_attempts: u32,    // 最大重连尝试，默认 5
}
```

**跨平台与硬编码规范（§1）对标**：

| # | 类别 | 合规 | 说明 |
|---|------|:---:|------|
| 3 | 超时秒数 | ✅ | `connect_timeout_secs` / `request_timeout_secs` 由配置读取，非硬编码在请求处 |
| 7 | 数字阈值 | ✅ | `max_retries` / `max_reconnect_attempts` 从配置读取 |

`McpConfig::to_connection_config()` 将业务配置转换为框架层 `McpConnectionConfig`（含 `Duration` 转换和指数退避参数）。

### 4.2 StdioMcpConnector（连接器）

**文件**：`connector.rs`

**实现 trait**：`McpConnectorContract`（框架层 MCP 连接器契约）

```
                    StdioMcpConnector
                    ═══════════════════
字段：
  name            &'static str                 连接器标识
  endpoint        McpEndpoint                  端点配置（Stdio {command, args, env}）
  conn_config     McpConnectionConfig          连接参数（超时、重试）
  state           Mutex<McpConnectionState>    连接状态（Disconnected/Connecting/Connected）
  next_id         Mutex<u64>                   JSON-RPC 请求自增 ID
  reconnect_count Mutex<u32>                  重连计数

方法：
  new()           → 构造实例，初始状态 Disconnected
  connect()       → 启动子进程 → initialize 握手 → tools/list 发现工具
  execute()       → tools/call 调用单个工具
  disconnect()    → 标记 Disconnected（子进程 kill_on_drop 自动终止）
  spawn_and_communicate()  → 内部方法：启动子进程、写入请求、读取响应
```

#### 4.2.1 connect() 流程

```
connect()
  │
  ├─ 1. 状态 → Connecting
  │
  ├─ 2. 构造 initialize 请求（JsonRpcRequest）
  │      method: "initialize"
  │      params: { protocolVersion, capabilities, clientInfo }
  │
  ├─ 3. spawn_and_communicate() → 启动子进程、发送、等待响应
  │      超时：conn_config.connect_timeout
  │
  ├─ 4. 校验响应（is_error?）
  │
  ├─ 5. 构造 tools/list 请求
  │      method: "tools/list"
  │
  ├─ 6. spawn_and_communicate() → 同上
  │
  ├─ 7. 解析 ToolsListResult → Vec<McpToolManifest>
  │
  ├─ 8. 状态 → Connected
  │
  └─ 9. 返回 Vec<McpToolManifest>
```

#### 4.2.2 execute() 流程

```
execute(tool_name, args, cancel)
  │
  ├─ 1. 构造 tools/call 请求
  │      method: "tools/call"
  │      params: { name: tool_name, arguments: args }
  │
  ├─ 2. spawn_and_communicate()
  │      超时：conn_config.request_timeout
  │
  ├─ 3. 校验响应（is_error?）
  │
  ├─ 4. 解析 ToolCallResult
  │
  ├─ 5. 提取 text 类型 content → 拼接
  │
  └─ 6. 返回 ToolOutput
         如果 is_error == true → ToolError::Execution
```

#### 4.2.3 spawn_and_communicate() 内部细节

这是连接器的核心通信方法，**每次调用都启动新的子进程**（短连接模式）：

1. 从 `McpEndpoint::Stdio` 解构 `command`、`args`、`env`
2. `tokio::process::Command` 配置 stdin/stdout 管道、stderr 继承
3. 设置 `kill_on_drop(true)` 确保析构时清理
4. `BufWriter` 写入 `{json}\n`，`flush()`
5. `BufReader::read_line()` 读取一行 JSON 响应（带 timeout）
6. 读取完成后 `start_kill()` 终止子进程
7. 返回响应字符串

> **短连接模式的设计取舍**：每次请求都启动新进程，简单可靠但开销大。MCP 规范本身支持长连接复用（`notifications/initialized` 后保持），后续可优化为连接池。

#### 4.2.4 跨平台与硬编码规范（§1.9）对标

| 规则 | 合规 | 说明 |
|------|:---:|------|
| 平台指令 | ✅ | `command` / `args` 从 `McpEndpoint::Stdio` 配置读取，由外部指定平台正确的可执行文件路径；不假设 `sh` 或 `cmd` |
| 文件路径 | ✅ | MCP Server 可执行文件路径从配置读取，非硬编码 |

### 4.3 MCP 协议层（JSON-RPC 2.0）

**文件**：`protocol.rs`

#### 4.3.1 常量定义

```rust
pub const MCP_METHOD_INITIALIZE: &str = "initialize";   // MCP 握手
pub const MCP_METHOD_TOOLS_LIST: &str = "tools/list";   // 工具发现
pub const MCP_METHOD_TOOLS_CALL: &str = "tools/call";   // 工具调用
```

> **跨平台与硬编码规范（§1.4）对标**：MCP 方法名定义为模块级 `const`，集中管理，避免字符串散落各处。

#### 4.3.2 核心类型

| 类型 | 用途 | 关键字段 |
|------|------|---------|
| `JsonRpcRequest` | JSON-RPC 请求 | `jsonrpc`, `id`, `method`, `params?` |
| `JsonRpcResponse` | JSON-RPC 响应 | `jsonrpc`, `id`, `result?`, `error?` |
| `JsonRpcError` | RPC 错误 | `code`, `message`, `data?` |
| `InitializeParams` | initialize 参数 | `protocolVersion`, `capabilities`, `clientInfo` |
| `InitializeResult` | initialize 返回 | `protocolVersion`, `capabilities`, `serverInfo` |
| `ToolsListResult` | tools/list 返回 | `tools: Vec<McpProtocolTool>` |
| `ToolCallParams` | tools/call 参数 | `name`, `arguments` |
| `ToolCallResult` | tools/call 返回 | `content: Vec<ToolCallContent>`, `isError` |

#### 4.3.3 序列化辅助方法

- `JsonRpcRequest::to_json()` — 序列化为 JSON 字符串（内部使用 `serde_json::to_string`，`unwrap_or_default` 降级处理）
- `JsonRpcResponse::from_json(json)` — 从字符串反序列化
- `JsonRpcResponse::is_error()` — 判断响应是否为错误
- `JsonRpcResponse::error_message()` — 提取错误消息

> **红线对标（aagnet-lessons）**：`to_json()` 中使用 `unwrap_or_default()`，根据代码质量红线"不可在库代码中使用 unwrap/expect"，此处为防御性降级——序列化结构体到 JSON 不会失败，`unwrap_or_default` 用于极端边界情况。

### 4.4 McpToolProxy（工具代理）

**文件**：`proxy.rs`

**实现 trait**：`ToolContract` + `Describe`

将一个 MCP 远程工具包装为标准 `ToolContract`，使 `ToolExecutorSlot` 无需区分本地工具和远程 MCP 工具。

#### 4.4.1 命名规则

```rust
// 全名 = "{connector_name}/{tool_name}"
pub fn static_name(manifest_name: &str, connector_name: &str) -> String {
    format!("{}/{}", connector_name, manifest_name)
}
```

> **命名规范对标（aagnet-lessons）**：跨边界对象在构造时完成命名转换。`connector_name` 是 `McpConnectorContract::name()` 的全名（如 `aagnet.mcp.filesystem`），通过 `/` 分隔符组合，确保工具名全局唯一。

#### 4.4.2 关键字段

```rust
pub struct McpToolProxy {
    manifest: McpToolManifest,                   // MCP 工具清单（name, description, parameters）
    connector: Arc<dyn McpConnectorContract>,    // 所属连接器引用（共享）
    cached_name: String,                         // 缓存的全名 "{connector}/{tool}"
    cached_description: String,                  // 缓存的描述 "[MCP:connector] description"
}
```

- `connector` 使用 `Arc<dyn McpConnectorContract>` 共享引用，多个代理可复用同一连接器
- `cached_name` / `cached_description` 在构造时预计算，避免每次 `name()` / `description()` 调用时重复格式化

#### 4.4.3 ToolContract 实现

| 方法 | 行为 |
|------|------|
| `name()` | 返回 `cached_name` |
| `group()` | 固定返回 `"mcp"` |
| `description()` | 返回 `"[MCP:{connector}] {description}"` |
| `definition()` | 委托 `ToolDefinition::new()` 构造，使用全名 |
| `required_permissions()` | 透传 `manifest.required_permissions` |
| `validate()` | 始终返回 `Ok(())`（MCP Server 自行校验） |
| `execute()` | 委托 `connector.execute(tool_name, args, cancel)` |

#### 4.4.4 批量构造

```rust
McpToolProxy::from_manifests(manifests, connector) -> Vec<Arc<dyn ToolContract>>
```

一次性将连接器发现的所有工具清单批量转换为代理实例。

---

## 5. 通信协议流程

```
Client (aagnet)                          MCP Server
     │                                        │
     │── initialize ──────────────────────→  │  ① 握手
     │   { protocolVersion: "2024-11-05"      │     协商协议版本
     │     clientInfo: { name, version }       │     交换能力声明
     │     capabilities: { roots, sampling } } │
     │←── { protocolVersion,                   │
     │      serverInfo, capabilities } ────── │
     │                                        │
     │── tools/list ───────────────────────→  │  ② 工具发现
     │←── { tools: [{ name, description,      │     获取全部可用工具
     │                inputSchema }] } ────── │
     │                                        │
     │── tools/call ───────────────────────→  │  ③ 工具调用
     │   { name: "read_file",                  │
     │     arguments: { path: "/x.txt" } }     │
     │←── { content: [{ type: "text",          │
     │                  text: "..." }],        │
     │      isError: false } ──────────────── │
```

**关键约束**：
- 每次 `connect()` 启动**新进程**完成 ① + ②，`connect()` 返回后进程终止
- 每次 `execute()` 启动**新进程**完成 ③，执行完后进程终止
- 协议版本 `"2024-11-05"` 当前硬编码在 `connector.rs:connect()` 中 → **待迁移为配置常量**

---

## 6. 协议合规性分析

### 6.1 Service 集成协议（protocol-Service集成协议）对标

#### 6.1.1 ServicePlugin 方法职责（协议 §1）

| 方法 | 调用次数 | 用途 | 当前状态 |
|------|---------|------|:---:|
| `name()` | 多次 | 返回全局唯一服务标识 `"mcp"` | ❌ 无 McpService |
| `init(ctx)` | 1 | 校验 McpConfig、预检查 MCP Server 可执行文件存在性 | ❌ |
| `start(ap)` | 1 | 通过 `ap.register_provider("mcp_tools", ...)` 注册工具代理 | ❌ |
| `handle_signal(signal)` | 多次 | 响应运行时信号（见 6.1.2） | ❌ |
| `stop()` | 多次 | 暂停服务，Provider 仍可用但不更新 | ❌ |
| `shutdown()` | 1 | 调用 `connector.disconnect()` + 反注册 Provider | ❌ |

#### 6.1.2 运行时信号处理（协议 §3）

| 信号 | 说明 | 当前处理 | 合规 |
|------|------|:---:|:---:|
| `GracefulShutdown` | 正常关闭，完成后台任务再退出 | ❌ 无 | — |
| `ImmediateShutdown` | 强制关闭，立即停止 | ❌ 无 | — |
| `ConfigReload` | 重载配置，重新连接 MCP Server | ❌ 无 | — |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） | ❌ 无 | V-R01 ❌ |
| `Suspend` | 暂停服务，释放临时资源 | ❌ 无 | — |
| `Resume` | 从暂停中恢复 | ❌ 无 | — |

#### 6.1.3 生命周期（协议 §5）

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

当前状态：**全部未实现**。`StdioMcpConnector` 和 `McpToolProxy` 作为独立组件存在，无 McpService 外壳串联生命周期。

#### 6.1.4 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 ServicePlugin 单入口 | 模块需实现 `ServicePlugin` trait | ❌ | 未实现 `McpService`；当前仅有 `StdioMcpConnector` 和 `McpToolProxy` 两个独立组件 |
| §2.1 ServiceAccessPoint | 通过 `get_config()` / `log()` 与 core 交互 | ❌ | 无 `ServiceAccessPoint` 注入 |
| §2.2 register_provider() | 在 `start()` 中注册 Provider | ❌ | 无 Provider 注册逻辑 |
| §3 运行时信号 | 响应 `HealthCheck` / `ConfigReload` / `GracefulShutdown` 等 | ❌ | 无 `handle_signal()` 实现（详见 6.1.2） |
| §4 插件元数据 | YAML 声明 `provides` / `requires` / `run_mode` | ❌ | 元数据已设计（见 §9），但未接入 PluginLoader |
| §5 生命周期 | init → start → stop → shutdown | ❌ | 无完整生命周期管理（详见 6.1.3） |
| §6 补充说明 | ServiceAccessPoint 可 Clone、handle_signal<5s、不假设 start/stop 配对 | ❌ | 待实现时确保 |
| §7 标准流程 | 8 步骤从零到运行 | ⚠️ | 步骤 1-4 已完成（config/connector/protocol/proxy），步骤 5-8 待完成（见 §7） |
| §8 V-R01 HealthCheck | 5s 内返回 `Ok(())` | ❌ | 无实现 |
| §8 V-R02 handle_signal 不阻塞 | 超 5s 须 spawn | ❌ | 无实现 |
| §8 V-R03 provides 一致 | 声明 = 实际注册 | ❌ | 无注册 |

### 6.2 模块内部组件协议（protocol-模块内部组件协议）对标

#### 6.2.1 依赖方向（协议 §6.2）

```
┌───────────────────┐
│  模块 mod.rs       │  （对外暴露的公共 API）
│  McpConfig         │
│  StdioMcpConnector │
│  McpToolProxy      │
└────────┬──────────┘
         │
         ▼
┌────────────────────────────────────────────┐
│  组件（无 Orchestrator — 组件单一）          │
│                                            │
│  StdioMcpConnector ──→ McpConnectorContract │
│         │                                  │
│         │ 生成                              │
│         ▼                                  │
│  McpToolProxy ──→ ToolContract             │
│                                            │
│  组件间关系：Connector 产出工具清单          │
│  → Proxy 批量包装 → 注册到 ContractRegistry  │
└────────────────────────────────────────────┘
```

- ✅ MCP 模块内部无子组件间直接引用（Connector ↔ Proxy 通过 `McpToolManifest` 数据传递）
- ✅ Proxy 不直接引用 Connector 的具体 struct，仅引用 `Arc<dyn McpConnectorContract>` trait object

#### 6.2.2 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 Component 单入口 | 内部组件实现 `Component` trait | ❌ | `StdioMcpConnector` 实现的是 `McpConnectorContract`，非 `Component` |
| §3 AccessPoint | 组件通过 `AccessPoint` 通信 | N/A | MCP 模块内部无子组件间通信需求 |
| §4 Processing | 返回 `Processing` 枚举 | N/A | — |
| §5 Orchestrator | 由编排器调度组件 | N/A | MCP 模块组件单一，无需编排器 |
| §6 模块边界 | `mod.rs` 只暴露对外入口 + 配置 | ✅ | `mod.rs` 仅 `pub use` 三个公共类型 |

> **结论**：MCP 模块内部结构简单（无子组件拆分），模块内部组件协议的多数条款不适用。但 `StdioMcpConnector` 若未来拆分为连接管理 + 协议序列化两个子组件，则需引入 `Component` + `AccessPoint` 范式。

### 6.3 跨平台与硬编码规范对标

| # | 检查项 | 合规 | 说明 |
|---|--------|:---:|------|
| 1 | URL/端点非硬编码 | ✅ | stdio 通信无 HTTP URL |
| 2 | 模型名非硬编码 | ✅ | MCP 模块不涉及 LLM 模型 |
| 3 | 超时值来自配置 | ✅ | `connect_timeout_secs` / `request_timeout_secs` 从 `McpConfig` 读取 |
| 4 | API 版本号定义为 const | ⚠️ | `"2024-11-05"` MCP 协议版本在 `connector.rs` 中作为字面量，待迁移为 `const MCP_PROTOCOL_VERSION` |
| 5 | User-Agent 定义为 const | ⚠️ | `clientInfo.name` 使用 `"aagnet"` 字面量，待定义为 `const CLIENT_NAME` |
| 6 | 路径通过 dirs + join() | ✅ | MCP Server 命令路径从配置读取，不硬编码 |
| 7 | 数字阈值从配置读取 | ✅ | `max_retries` / `max_reconnect_attempts` 从配置读取 |
| 8 | 平台指令通过 OsKind | ✅ | 命令从配置获取，由外部指定平台正确路径 |
| 9 | 测试用 temp_dir() | ✅ | 测试中无路径硬编码 |
| 10 | build + test + clippy | 待验证 | — |

---

## 7. Service 接入待办（补齐 ServicePlugin）

按 **Service 集成协议 §7 新增 Service 标准流程**，补齐工作如下：

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 创建 `McpService` 结构体 | 新建 `service.rs` |
| 2 | 实现 `ServicePlugin` trait | `service.rs` |
| 3 | `init()` 中校验 `McpConfig`、预检查 MCP Server 可执行文件存在性 | `service.rs` |
| 4 | `start()` 中调用 `connector.connect()` → `McpToolProxy::from_manifests()` → `ap.register_provider("mcp_tools", tools)` | `service.rs` |
| 5 | `handle_signal()` 响应 `HealthCheck`（5s 内）、`ConfigReload`（重连）、`GracefulShutdown`（断连） | `service.rs` |
| 6 | `shutdown()` 中调用 `connector.disconnect()` | `service.rs` |
| 7 | 更新 `mod.rs` 导出 `McpService` | `mod.rs` |
| 8 | 编写 `PluginMetadata` YAML（见 §9） | 配置文件 |

---

## 8. 设计决策

### 8.1 为什么用 stdio 而不是 HTTP

**决策**：MCP 连接通过 stdio（子进程 stdin/stdout JSON-RPC 2.0）而非 HTTP。

**理由**：
1. **MCP 规范标准**：MCP 协议定义的标准传输层是 stdio
2. **零网络配置**：无需端口分配、TLS 证书、防火墙规则
3. **进程生命周期绑定**：子进程随 Agent 启停，`kill_on_drop` 自动回收

### 8.2 为什么包装为 ToolContract

**决策**：MCP 工具通过 `McpToolProxy` 实现 `ToolContract` trait。

**理由**：
1. **统一接口**：`ToolExecutorSlot` 不区分"本地工具"和"MCP 工具"，统一调度
2. **复用熔断器**：MCP 工具自动享受 `ToolRegistry` 的 `CircuitBreaker` 保护
3. **统一超时**：`ToolRegistry::call()` 超时控制覆盖 MCP 工具
4. **权限统一**：`Permission` 体系同时管控本地和远程工具

### 8.3 短连接 vs 长连接

**当前选择**：短连接模式（每次请求启动新进程）。

**权衡**：
- 优点：实现简单，进程状态隔离，无连接泄漏风险
- 缺点：每次启动进程有开销（~100-500ms），高频调用场景性能不足

**演进方向**：后续可升级为长连接模式，在 `connect()` 后保持子进程存活，`execute()` 复用同一子进程的 stdin/stdout 通道。

---

## 9. 插件元数据

```yaml
name: mcp
category: service
version: 0.2.0
run_mode: background
provides:
  - mcp_tools
requires:
  - tools
conflicts: []
config_schema:
  type: object
  properties:
    enabled:
      type: boolean
      default: false
      description: 是否启用 MCP 服务
    connect_timeout_secs:
      type: integer
      default: 10
      description: MCP Server 连接超时（秒）
    request_timeout_secs:
      type: integer
      default: 30
      description: 单次工具调用超时（秒）
    max_retries:
      type: integer
      default: 3
      description: 最大重试次数
    auto_reconnect:
      type: boolean
      default: true
      description: 是否自动重连
    max_reconnect_attempts:
      type: integer
      default: 5
      description: 最大重连尝试次数
    servers:
      type: array
      description: MCP Server 配置列表
      items:
        type: object
        properties:
          name:
            type: string
            description: 连接器名称（全局唯一标识）
          command:
            type: string
            description: MCP Server 可执行文件路径（跨平台）
          args:
            type: array
            items:
              type: string
            description: 命令行参数
          env:
            type: object
            description: 环境变量
```

---

## 10. 红线与质量

| 编号 | 来源 | 红线 | 合规 |
|------|------|------|:---:|
| V-R01 | Service集成协议 | 必须响应 `HealthCheck` | ❌ 待补齐 |
| V-R02 | Service集成协议 | `handle_signal` 不阻塞超 5s | ❌ 待补齐 |
| V-R03 | Service集成协议 | `provides` = `register_provider` 一致 | ❌ 待补齐 |
| — | aagnet-lessons | 异步操作必须有超时 | ✅ `connect_timeout` + `request_timeout` |
| — | aagnet-lessons | 外部输入必须校验 | ✅ JSON-RPC 响应解析失败即报错 |
| — | aagnet-lessons | 不可在库代码中 unwrap/expect | ⚠️ `connector.rs` 中有 `Mutex::lock().expect("mutex poisoned")`，毒锁 panic 可接受（Mutex 毒锁表示严重内部错误，panic 是正确的失败策略） |
| — | 跨平台规范 | 文件路径非硬编码 | ✅ |

---

## 11. 测试

### 11.1 单元测试

**文件**：`connector.rs`（末尾 `#[cfg(test)]`）

| 测试 | 说明 | 合规（跨平台规范 §3） |
|------|------|:---:|
| `test_connector_creation` | 创建 connector 并验证初始状态 | ✅ 无路径硬编码 |
| `test_endpoint_type` | 验证 `McpEndpoint::Stdio` 返回 `"stdio"` | ✅ 无外部依赖 |

### 11.2 proxy 测试

**文件**：`proxy.rs`（末尾 `#[cfg(test)]`）

使用 `FakeConnector` mock 实现 `McpConnectorContract`，隔离外部 MCP Server 依赖。符合跨平台规范 §3.3（不访问真实端点）。

---

## 12. 依赖关系

```
McpToolProxy  ──→  ToolContract (core::contract::tool)
McpToolProxy  ──→  McpConnectorContract (core::contract::mcp)
StdioMcpConnector ──→  McpConnectorContract (core::contract::mcp)
StdioMcpConnector ──→  JsonRpcRequest/Response (protocol.rs)
StdioMcpConnector ──→  McpEndpoint, McpConnectionConfig (core::contract::mcp)
```

- 对外依赖：`tokio::process::Command`（子进程）、`serde_json`（序列化）
- 框架层依赖：`core::contract::mcp`（连接器契约）、`core::contract::tool`（工具契约）、`core::data_contract`（组件描述符）
