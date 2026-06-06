# MCP → ToolExecutor 集成开发计划

> 上位约束：`docs/development/AI开发红线与纪律.md`
> 现有计划：`docs/services/mcp/MCP 严格 AI 开发计划.md`（**不替代**——本计划是它的"集成补全"）
> 集成方向：MCP Service → ToolsService（Option A 合并）→ ToolExecutor Slot
> 计划日期：2026-06-01

---

## 0. 目标

把 MCP Service 从"内部工具"升级为"通过 `PROVIDER_MCP_TOOLS` 对外暴露"，
并通过 **Option A 策略**（在 `ToolsService::start()` 启动时合并 MCP 代理到 `ToolRegistry`），
让 LLM 通过 `PROVIDER_TOOL` 一个接口看到所有工具（本地 + MCP），ToolExecutor 无需任何改动即可调用。

**完成定义**：
1. `cargo check` 0 errors, 0 new warnings
2. `cargo test` 通过
3. 跑 4 协议 grep 守卫全部 0 匹配
4. E2E：起一个 mock MCP server（`echo` 工具），跑一个用户消息 "调用 echo hello"，确认 Observation 包含 echo 的输出

---

## 1. 协议与红线引用

| 红线 | 来源 | 本计划如何遵守 |
|------|------|---------------|
| **K-R01** | shared_types §2 | MCP 注册用 `PROVIDER_MCP_TOOLS` 常量 |
| **K-R02** | shared_types §2 | `PROVIDER_MCP_TOOLS` 先在 shared_types 定义 |
| **T-R01** | shared_types §3 | `ToolProvider` 已在 shared_types；MCP 用**已有**的，不造新 |
| **D-R01** | shared_types §4 | 用现有 `DynProvider<T>`，不造 `DynMcpProvider` |
| **P-R01** | Service §6 | 不留 `Arc::new(())`，`McpService::start` 用 `DynProvider` 真注册 |
| **P-R02** | Service §6 | 至少 ToolsService 1 个消费者 |
| **V-R01** | Service §8 | `HealthCheck` 5s 内返回 |
| **V-R02** | Service §8 | `ConfigReload` 重连接 `tokio::spawn` |
| **V-R03** | Service §8 | 插件 metadata YAML 与 `start()` 一致 |
| **S-R01** | Slot §9 | ToolExecutor 不动，但下游 `tool_executor::run()` 已有 7 个 `SlotDirective` 处理 |
| **C-R04** | 内部组件 | ToolExecutor 的 orchestrator 仍 `process_all()`，不能停 |
| 跨平台 | `docs/跨平台与硬编码规范.md` | 子进程命令路径用配置，不用裸字符串 |

---

## 2. 架构决策（已与用户拍板）

### 2.1 Option A vs Option B（已选 A）

| 维度 | A（合并到 ToolRegistry） | B（ToolExecutor 兜底查询） |
|------|------------------------|---------------------------|
| 改动文件 | `services/mcp/*` (3) + `services/tools/service.rs` + `services/tools/registry.rs` + `shared_types/tool.rs` | `services/mcp/*` (3) + `slots/tool_executor/plugin.rs` + `shared_types/tool.rs` |
| 破坏"统一调度"承诺 | ❌ 不破坏 | ✅ 破坏 |
| LLM 视角 | 看到 `mcp/{conn}/{tool}`，与本地工具无差别 | 看到 2 个 namespace |
| ToolExecutor 改动 | 0 行 | ~20 行 |
| **决策** | ✅ **采用** | ❌ |

### 2.2 关于 `ToolContract` trait

**MCP 文档**（`docs/services/mcp/mcp开发文档.md` §4.4）提到 `ToolContract` trait。

**现状**：`ToolContract` 在代码中**不存在**。实际共享 trait 是 `ToolProvider`（`src/shared_types/tool.rs:35-46`）。

**决策**：
- 复用已有的 `ToolProvider`（不造新 `ToolContract`）
- `McpToolProxy` 实现 `ToolProvider`
- 在 `MCP 严格 AI 开发计划.md` 后续版本中，把 `ToolContract` 全部替换为 `ToolProvider`

### 2.3 关于连接策略

**文档**（`docs/services/mcp/mcp开发文档.md` §4.2.3）承诺"短连接"（每次 execute 都 spawn 一次）。

**现状**（`src/plugins/services/mcp/connector.rs:22-77`）：**长连接**（connect 时 spawn，整个生命周期复用）。

**决策**：
- **MVP**：保持长连接（改动小，能跑通）
- **后续**：在 `connector.rs` 改实现为"按需 spawn"，加 `connection_mode: Long | Short` 配置
- **本次集成任务只实现 MVP**

### 2.4 关于"统一执行接口"的风险

**潜在问题**：
- 本地工具的 `entry` 字段（`registry.rs:29`）目前是 `"execute_command"`、`"read_file"` 等字符串
- MCP 工具的 `entry` 字段是什么？
- 如果 MCP 工具也用 `"mcp"` 这种 sentinel，`ToolRegistry::execute()` 的 `match def.entry.as_str()` 会走到 `_ => Err`

**决策**：
- MCP 工具的 `ToolDefinition.entry` 字段填 `format!("mcp:{}", connector_name)`（如 `"mcp:filesystem"`）
- `ToolRegistry::execute()` 的 match arm 加一个 `"mcp:..."` 分支（用 `starts_with("mcp:")` 模式匹配）
- 这个分支内部：查 `provider_map`（registry 内部维护）→ 调对应 `McpToolProxy::execute()`

**禁止**：
- ❌ 不要把 MCP 工具伪装成"execute_command"等本地 entry（违反"统一接口"语义）
- ❌ 不要在 `ToolRegistry` 里直接 `use McpToolProxy`（违反"Slot 不依赖 Service 内部"）
- 用 `Arc<dyn ToolProvider>` 持有——ToolProvider 是 shared_types 合法引用

---

## 3. 任务清单

### Phase A：定义契约 + 扩展 ToolProvider 能力（5 个任务）

#### A-1. 在 `shared_types/tool.rs` 新增 `MCP` 工具描述符

**文件**：`src/shared_types/tool.rs`

**操作**：
- 加 `pub const PROVIDER_MCP_TOOLS: &str = "mcp_tools";`（K-R01 + K-R02）
- re-export 到 `shared_types/mod.rs:45`
- 扩展 `ToolDefinition`：在 `entry: String` 旁加 `#[serde(default)] pub source: ToolSource`，其中 `ToolSource` 枚举：
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, Default)]
  pub enum ToolSource { #[default] Builtin, Installed, Mcp { connector: String } }
  ```
- **决策**：`ToolSource` 加字段会破坏 `ToolDefinition` 的 `Clone` 派生——必须**显式派生 PartialEq/Eq/Hash**（用于 `HashMap` key 已有，但加字段要重新跑测试）

**禁止**：
- ❌ 不要改 `PROVIDER_TOOL` 已有值
- ❌ 不要给 `ToolSource` 加方法（只用作标签，不做业务）

**验证**：
- 跑 `rg '"mcp_tools"' src/` 命中 0（除了 shared_types 中的常量定义）
- 跑 `rg "pub const PROVIDER_MCP_TOOLS" src/shared_types/tool.rs` 命中 1
- 跑 `cargo test` —— `tool_registry/plugin.rs` 的 5 个测试和 `tools/registry.rs` 的测试不破坏

#### A-2. `ToolProvider` trait 增加 `provider_id()` 方法（用于多 provider 调度）

**文件**：`src/shared_types/tool.rs:34-46`

**操作**：
- 在 `ToolProvider` trait 加默认方法：
  ```rust
  fn provider_id(&self) -> &str { "default" }
  ```
- 默认实现返回 `"default"`，老 `ToolRegistry` 不受影响
- `McpToolProxy` 覆写返回 `format!("mcp:{}", connector_name)`
- **不要改** `list()` / `execute()` 签名

**禁止**：
- ❌ 不要去掉 `&self` 变 `&mut self`（破坏 ABI）
- ❌ 不要让 `provider_id()` 是必须实现的方法（避免破坏老 Provider）

**验证**：
- `cargo check` 0 errors
- 跑 `rg "impl ToolProvider for" src/` 至少 3 个（ToolRegistry、MockToolProvider、新加的 McpToolProxy）

#### A-3. `ToolRegistry` 改造：增加 `provider_handles` 字段

**文件**：`src/plugins/services/tools/registry.rs:13-20`

**操作**：
- struct 增加 `provider_handles: Mutex<HashMap<String, Arc<dyn ToolProvider>>>`（key 是 `provider_id()`，value 是 provider handle）
- 新增方法 `pub fn register_provider(&self, provider_id: &str, provider: Arc<dyn ToolProvider>)`
- 新增方法 `pub fn get_provider(&self, provider_id: &str) -> Option<Arc<dyn ToolProvider>>`
- `impl ToolProvider for ToolRegistry`：覆写 `provider_id() -> &str { "tools" }`
- `impl ToolProvider for ToolRegistry::execute()` (line 67-111) 改造：
  - 当前 `match def.entry.as_str()` 4 个本地分支保持不变
  - 新增 match arm：
    ```rust
    s if s.starts_with("mcp:") => {
        let provider_id = s;  // 整个 "mcp:filesystem"
        match self.get_provider(provider_id) {
            Some(provider) => provider.execute(tool_name, arguments, _timeout).await,
            None => Err(ToolError::NotFound(format!("MCP provider {} 未注册", provider_id)))
        }
    }
    ```

**禁止**：
- ❌ 不要改本地 4 个 entry 的 match（保持原样）
- ❌ 不要把"找 provider"逻辑放在主循环外（动态查找）
- ❌ 不要用 `Arc::new(())` 占位 `provider_handles`

**验证**：
- `cargo check` 0 errors
- 跑 `rg "match def.entry" src/plugins/services/tools/registry.rs` 命中 1

#### A-4. `ToolDefinition.entry` 字段加 mcp: 前缀

**文件**：`src/plugins/services/tools/manifest.rs:13-18`（如需要）

**操作**：
- `ToolManifest` 加 `#[serde(default)] pub source: ToolSource`
- `ToolRegistry::register` 和 `register_builtin` 把 `entry` 字段填好
- `McpToolProxy` 后续在 A-5 中创建 `ToolDefinition` 时 entry 填 `"mcp:{connector_name}"`，source 填 `Mcp { connector }`

**验证**：
- 跑 `rg "entry: " src/plugins/services/tools/` 全部是已知字符串
- `cargo test` 不破坏

#### A-5. `McpToolProxy` 重写为实现 `ToolProvider`

**文件**：`src/plugins/services/mcp/proxy.rs:1-31`（全重写）

**操作**：
- 删除整个文件
- 写：
  ```rust
  use std::sync::Arc;
  use std::time::Duration;
  use async_trait::async_trait;
  use serde_json::Value;
  use tokio::sync::Mutex;
  use crate::shared_types::{ToolDefinition, ToolError, ToolProvider, ToolSource};
  use super::connector::StdioMcpConnector;
  use super::protocol::ToolManifest;

  pub struct McpToolProxy {
      name: String,
      description: String,
      connector: Arc<Mutex<StdioMcpConnector>>,
      connector_name: String,
      manifest: ToolManifest,
  }

  impl McpToolProxy {
      pub fn new(manifest: ToolManifest, connector: Arc<Mutex<StdioMcpConnector>>, connector_name: &str) -> Self {
          let name = format!("mcp/{}/{}", connector_name, manifest.name);
          let description = format!("[MCP:{}] {}", connector_name, manifest.description);
          Self { name, description, connector, connector_name: connector_name.to_string(), manifest }
      }

      pub fn connector_name(&self) -> &str { &self.connector_name }

      pub fn into_tool_definitions(self: &Arc<Self>) -> Vec<ToolDefinition> {
          vec![ToolDefinition {
              name: self.name.clone(),
              description: self.description.clone(),
              parameters: self.manifest.input_schema.clone(),
              entry: format!("mcp:{}", self.connector_name),
              source: ToolSource::Mcp { connector: self.connector_name.clone() },
          }]
      }
  }

  #[async_trait]
  impl ToolProvider for McpToolProxy {
      fn list(&self) -> Vec<ToolDefinition> {
          self.into_tool_definitions()
      }

      fn provider_id(&self) -> &str { "mcp" }

      async fn execute(&self, tool_name: &str, arguments: Value, _timeout: Duration) -> Result<String, ToolError> {
          let mut conn = self.connector.lock().await;
          match conn.execute(&self.manifest.name, arguments).await {
              Ok(value) => Ok(value.to_string()),
              Err(e) => Err(ToolError::ExecutionFailed(e)),
          }
      }
  }
  ```
- 同步更新 `mcp/mod.rs:10` 的 `pub use proxy::McpToolProxy;`（保留）
- 同步更新 `mcp/service.rs:13` 的 `use super::proxy::McpToolProxy;`（保留）

**禁止**：
- ❌ 不要保留原 `pub fn execute(&self, args: Value) -> Result<Value, String>`（签名不兼容）
- ❌ 不要让 `_timeout` 参数"消失"——保留以匹配 trait
- ❌ 不要在 proxy 里写 connector 子进程逻辑（那是 connector 的事）

**验证**：
- 跑 `rg "impl ToolProvider for McpToolProxy" src/` 命中 1
- 跑 `rg "Result<Value, String>" src/plugins/services/mcp/proxy.rs` 命中 0
- `cargo check` 0 errors

---

### Phase B：MCP 服务侧自完成（8 个任务）

#### B-1. `protocol.rs` 协议类型完善

**文件**：`src/plugins/services/mcp/protocol.rs:1-47`

**操作**：
- 改 `PROTOCOL_VERSION` 常量名 → `MCP_PROTOCOL_VERSION`（文档 §6.3 #4 要求）
- 加常量 `pub const CLIENT_NAME: &str = "aagnet";` 和 `pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");`
- 加 `JsonRpcRequest::to_json(&self) -> String` 和 `JsonRpcResponse::from_json(s: &str) -> Result<Self, String>`
- 加 `JsonRpcResponse::is_error(&self) -> bool` 和 `error_message(&self) -> Option<String>`
- 加新类型 `pub struct InitializeParams { pub protocol_version: String, pub capabilities: Value, pub client_info: ClientInfo }` 和 `pub struct ClientInfo { pub name: String, pub version: String }`（用于 M-4 强类型化）

**禁止**：
- ❌ 不要破坏 `JsonRpcRequest::new` / `JsonRpcResponse::new` 现有调用
- ❌ 不要给 `InitializeParams` 加 `Serialize` 之外的派生（不需要持久化）

**验证**：
- `cargo check` 0 errors
- 跑 `rg "JsonRpcRequest::new\|JsonRpcResponse::new" src/` 不变

#### B-2. `config.rs` 配置扩展

**文件**：`src/plugins/services/mcp/config.rs:1-23`

**操作**：
- `McpConfig` 加字段：
  - `pub enabled: bool`（默认 `true`）
  - `pub max_retries: u32`（默认 `3`）
  - `pub auto_reconnect: bool`（默认 `true`）
  - `pub max_reconnect_attempts: u32`（默认 `5`）
- `McpServerConfig` 加字段：`pub env: HashMap<String, String>`（默认 `HashMap::new()`）
- 加 `pub fn to_connection_config(&self) -> McpConnectionConfig` 方法（返回带 `Duration` 类型的中间结构）
- 加 `pub struct McpConnectionConfig { pub connect_timeout: Duration, pub request_timeout: Duration, pub server: McpServerConfig }`
- 把当前 `McpServerConfig` 的 `args: Vec<String>` 字段加注释"按 MCP §9 YAML schema 序列化为数组"

**禁止**：
- ❌ 不要把 `connect_timeout_secs: u64` 改成 `Duration`（破坏 YAML 兼容性）
- ❌ 不要新增依赖（如 `which` crate）

**验证**：
- 跑 `rg "connect_timeout_secs" src/` 命中位置不变
- `cargo check` 0 errors
- `MCP 严格 AI 开发计划.md` §6.3 #6 提到"必须从配置读取"——验证 `connect_timeout_secs/request_timeout_secs` 在 connector 中真的被消费

#### B-3. `connector.rs` 进程超时（最危险的任务）

**文件**：`src/plugins/services/mcp/connector.rs:22-77`

**操作**：
- `connect()` (line 22-48) 整体包 `tokio::time::timeout(Duration::from_secs(self.config.connect_timeout_secs.unwrap_or(10)), ...)`：
  ```rust
  pub async fn connect(&mut self) -> Result<Vec<ToolManifest>, String> {
      let timeout_dur = Duration::from_secs(10);
      match tokio::time::timeout(timeout_dur, self.connect_inner()).await {
          Ok(Ok(tools)) => Ok(tools),
          Ok(Err(e)) => Err(e),
          Err(_) => {
              // 杀掉子进程
              if let Some(mut child) = self.child.take() {
                  let _ = child.kill().await;
              }
              Err(format!("MCP Server '{}' 连接超时（10s）", self.config.name))
          }
      }
  }

  async fn connect_inner(&mut self) -> Result<Vec<ToolManifest>, String> {
      // 现有 connect 逻辑（line 23-47）
  }
  ```
- `send_request()` (line 56-77) 也加 timeout：
  ```rust
  let response = tokio::time::timeout(
      Duration::from_secs(self.config.request_timeout_secs.unwrap_or(30)),
      self.send_request_inner(method, params)
  ).await
  .map_err(|_| format!("MCP 请求 '{}' 超时", method))??;
  ```
- 抽 `send_request_inner()` 函数持有原逻辑
- 超时触发时也要尝试 kill 子进程

**⚠️ 阻塞项**：
- 当前 `McpServerConfig` 没有 `connect_timeout_secs` / `request_timeout_secs` 字段——在 `McpConfig` 上有
- 决定是：把 timeout 加到 `McpServerConfig` 还是让 connector 从 `McpConfig` 拿？
- **汇报前不写代码**——这影响 API 形状

**禁止**：
- ❌ 不要在 `connect()` 内部做"重试"逻辑（避免 startup 时间无界）
- ❌ 不要用 `std::time::Duration::from_secs` 硬编码常量（要从 config 拿）
- ❌ 不要忘记 `kill_on_drop(true)`（已经有 line 28，但要确认）
- ❌ 不要把 `_timeout` 改成 `_`（保留 timeout 信息便于调试）

**验证**：
- 跑 `rg "tokio::time::timeout" src/plugins/services/mcp/connector.rs` 命中至少 2 处
- `cargo check` 0 errors
- 跑 `rg "Duration::from_secs" src/plugins/services/mcp/connector.rs` 命中是常量引用（不是裸数字）

#### B-4. `connector.rs` 重试 / 退避（可推迟，先占位）

**文件**：`src/plugins/services/mcp/connector.rs`

**操作**：
- 加 `pub async fn connect_with_retry(&mut self) -> Result<Vec<ToolManifest>, String>` 方法
- 内部用指数退避：1s → 2s → 4s → ...
- 加 `pub struct RetryConfig { pub max_attempts: u32, pub base_delay: Duration }`
- 在 `McpConfig` 已有 `max_retries` 字段上加测试

**决策**：
- 本次只做**接口骨架**+`connect_with_retry` 占位返回 `connect().await`
- 完整退避留到 v0.2（标记 TODO）

**禁止**：
- ❌ 不要在 `start()` 中无限重试（违反 V-R02 5s 限制）
- ❌ 不要让重试时间超过 5s

#### B-5. `service.rs` 启动逻辑整理

**文件**：`src/plugins/services/mcp/service.rs:39-70`

**操作**：
- 删除 line 67 `ap.register_provider("mcp_tools", Arc::new(proxies_vec) as Arc<dyn std::any::Any + Send + Sync>)`（违反 K-R01 + D-R01 + P-R01）
- 改为：
  ```rust
  // 收集所有 proxies（不直接注册到 PROVIDER_TOOL——ToolsService 会合并）
  let proxies: Vec<Arc<McpToolProxy>> = proxies.clone();
  // 注册到 PROVIDER_MCP_TOOLS，供 ToolsService::start 拉取
  let bundle: Arc<dyn McpBundle> = Arc::new(McpBundleImpl { proxies });
  ap.register_provider(PROVIDER_MCP_TOOLS, Arc::new(DynProvider(bundle)));
  ```
- 新建 `pub trait McpBundle: Send + Sync { fn proxies(&self) -> Vec<Arc<McpToolProxy>>; }` 和 `struct McpBundleImpl { proxies: Vec<Arc<McpToolProxy>> }`
- 把 `McpBundle` 和 `McpBundleImpl` 加到 `mcp/mod.rs` 公开
- `McpBundle` trait 定义在 **`shared_types/mcp.rs`（新建）**——这是 Provider trait 的一部分（T-R01）！

**⚠️ 阻塞项**：
- `McpBundle` 是 Provider trait 吗？要不要给 `McpToolProxy` 加 Provider trait 本身（让 ToolsService 直接拿 Vec<Arc<dyn ToolProvider>>）？
- 备选方案：B-5' 把 `McpBundle` 简化成：
  ```rust
  pub trait McpBundle: Send + Sync {
      fn tool_providers(&self) -> Vec<Arc<dyn ToolProvider>>;
  }
  ```
  这样 ToolsService 拿到的是 `Vec<Arc<dyn ToolProvider>>`，直接调 `register_provider(provider_id, ...)`，不需要知道 McpToolProxy 的存在

**决策**：采用 B-5' 方案（更松耦合）
- `McpBundle::tool_providers() -> Vec<Arc<dyn ToolProvider>>`
- `McpBundleImpl` 内部 `proxies.iter().map(|p| p.clone() as Arc<dyn ToolProvider>).collect()`

**禁止**：
- ❌ 不要让 `McpBundle` 在 `services/mcp/` 内定义（T-R01）
- ❌ 不要把 proxies 注册到 `PROVIDER_TOOL`（MCP 不绕过 ToolsService）

**验证**：
- 跑 `rg "pub trait McpBundle" src/shared_types/mcp.rs` 命中 1
- 跑 `rg "register_provider" src/plugins/services/mcp/service.rs` 命中 1（且用 `PROVIDER_MCP_TOOLS`）
- 跑 `rg '"mcp_tools"' src/plugins/services/mcp/service.rs` 命中 0
- `cargo check` 0 errors

#### B-6. `service.rs` 错误处理 + 预检

**文件**：`src/plugins/services/mcp/service.rs:32-37, 47-63`

**操作**：
- `init` (line 32-37)：删除 `unwrap_or_default()`，改为 `?` + `PluginError::Config`：
  ```rust
  let config: McpConfig = serde_json::from_value(ctx.plugin_config.clone())
      .map_err(|e| PluginError::Config(format!("mcp 配置解析: {}", e)))?;
  ```
- `start` 中 `for server_config in &config.servers` 前加预检：
  ```rust
  for server_config in &config.servers {
      if !server_config.enabled { continue; }
      // 预检：可执行文件存在
      if let Err(e) = std::fs::metadata(&server_config.command) {
          tracing::warn!("MCP: 跳过 Server '{}'，命令 '{}' 不可访问: {}",
                         server_config.name, server_config.command, e);
          continue;
      }
      // 现有 connect 逻辑...
  }
  ```
- 删除 `McpServerConfig` 的 `command: String` 改 `Option<String>`——**等等**这破坏 YAML 兼容性
- **决策**：保持 `command: String` 必填，预检时 metadata 失败就 warn + skip

**禁止**：
- ❌ 不要让预检失败导致 `start()` 返回 Err（一个 server 失败不影响其他）
- ❌ 不要用 `which::which()`（增加依赖）

**验证**：
- 跑 `rg "unwrap_or_default" src/plugins/services/mcp/service.rs` 命中 0
- 跑 `rg "std::fs::metadata" src/plugins/services/mcp/service.rs` 命中 1

#### B-7. `service.rs` 信号处理完善

**文件**：`src/plugins/services/mcp/service.rs:72-84`

**操作**：
- `HealthCheck`：与 Skills 同样模式，`tokio::time::timeout(5s, ...)` 简单检查
- `ConfigReload`：`tokio::spawn` 重新连接所有 enabled server，更新 `self.proxies`
- `Suspend` / `Resume`：当前 `running` 字段已有，加处理逻辑
- `GracefulShutdown` / `ImmediateShutdown`：当前已处理

**禁止**：
- ❌ 不要在 `handle_signal` 同步 `connect()`（违反 V-R02）

#### B-8. 插件 metadata（V-R03 满足：代码内返回，不造 YAML 文件）

**文件**：在 `src/plugins/services/mcp/service.rs` 加方法 `pub fn metadata(&self) -> PluginMetadata`

**操作**：
- 第一步：**grep 确认 yaml 约定**——跑 `rg "\.yaml" src/infra/metadata/ docs/`，若 0 匹配说明没有 YAML 约定
- 第二步：加方法到 `McpService`：
  ```rust
  pub fn metadata(&self) -> PluginMetadata {
      PluginMetadata {
          name: "mcp".to_string(),
          version: env!("CARGO_PKG_VERSION").to_string(),
          run_mode: RunMode::Background,
          provides: vec!["mcp_tools".to_string()],
          requires: vec!["tools".to_string()],
          permissions: vec!["spawn_process".to_string()],
      }
  }
  ```
- `PluginMetadata` / `RunMode` 已定义在 `src/core/types/plugin.rs:39-41`
- 不要新建 YAML 文件——避免为单一服务造目录约定

**禁止**：
- ❌ 不要新建 `src/infra/metadata/mcp.yaml`（无 YAML 约定）
- ❌ 不要把 metadata 写死为 `String` 字面量（应来自 `McpService.name()` 等已有常量）

**验证**：
- 跑 `rg "\.yaml" src/infra/metadata/ docs/` → 0 匹配（确认无 YAML 约定）
- 跑 `rg "pub fn metadata" src/plugins/services/mcp/service.rs` 命中 1
- `cargo check` 0 errors
- V-R03 满足：`metadata().provides` 与 `start()` 中 `register_provider(PROVIDER_MCP_TOOLS, ...)` 一致

---

### Phase C：ToolsService 合并 MCP（4 个任务）

#### C-1. `ToolsService::start` 拉取 MCP providers 合并

**文件**：`src/plugins/services/tools/service.rs:49-56`

**操作**：
- 启动时先注册本地 tool：
  ```rust
  async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
      self.running = true;
      // 本地 provider
      ap.register_provider(
          PROVIDER_TOOL,
          Arc::new(DynProvider(self.registry.clone() as Arc<dyn ToolProvider>)),
      );
      // 合并 MCP
      if let Some(raw) = ap.provider_raw(PROVIDER_MCP_TOOLS) {
          match raw.downcast::<DynProvider<dyn McpBundle>>() {
              Ok(bundle_wrapper) => {
                  let bundle = bundle_wrapper.0.clone();
                  for provider in bundle.tool_providers() {
                      let pid = provider.provider_id();
                      self.registry.register_provider(pid, provider);
                  }
                  tracing::info!("ToolsService: 合并了 {} 个 MCP provider", /* count */);
              }
              Err(_) => tracing::warn!("ToolsService: MCP provider 类型不匹配"),
          }
      }
      Ok(())
  }
  ```
- 添加 `use crate::shared_types::{McpBundle, PROVIDER_MCP_TOOLS};` import

**禁止**：
- ❌ 不要把 MCP 工具伪装成本地工具
- ❌ 不要在 init 阶段拉 MCP（init 时 MCP service 可能还没 start）
- ❌ 不要忘记 `init` 阶段的 `unwrap_or_default` 错误吞掉

**验证**：
- 跑 `rg "provider_raw(PROVIDER_MCP_TOOLS)" src/plugins/services/tools/service.rs` 命中 1
- 跑 `rg "register_provider" src/plugins/services/tools/service.rs` 命中 2（PROVIDER_TOOL + 各 MCP）
- `cargo check` 0 errors

#### C-2. 启动顺序保证

**文件**：检查 `src/main.rs` 或 `src/core/runtime.rs`

**操作**：
- 验证 `McpService.start()` 在 `ToolsService.start()` **之前**执行
- 若不满足，加注释 + 调换顺序
- **不要硬编码顺序**——Pipeline 应有依赖图

**禁止**：
- ❌ 不要在 `ToolsService` 加 `tokio::time::sleep` 等待 MCP 启动（脆弱）

**验证**：
- 跑 `rg "McpService\|ToolsService" src/main.rs src/core/runtime.rs` 找到所有 start 调用
- 顺序：MCP first, Tools second

#### C-3. 端到端联调测试

**文件**：新建 `tests/integration_mcp_tools.rs`（如不存在 `tests/`）

**操作**：
- mock MCP server：写一个简单的 Python/Rust 子进程，stdio 上响应 `initialize`、`tools/list`、`tools/call`
- 跑 aagnet 启动 → 加载 MCP → MCP 注册 mcp_tools → Tools 拉取合并 → ToolRegistry 列出所有工具
- 验证 `provider_raw(PROVIDER_TOOL).downcast::<DynProvider<dyn ToolProvider>>().list()` 包含本地 + MCP 工具
- 跑 `wrapper.0.execute("mcp/<conn>/<tool>", ...)` 成功返回

#### C-4. `cargo test` + 4 协议 grep 守卫

**操作**：跑：
```bash
cargo test 2>&1 | tail -50
cargo clippy --lib 2>&1 | grep "field.*never read"

# 4 协议守卫
rg '"mcp_tools"' src/plugins/  # 0
rg "pub trait.*Provider" src/plugins/  # 0
rg "DynMcp\|DynTool" src/  # 0
rg "Arc::new\(\(\)\)" src/plugins/services/mcp/  # 0
```

---

## 4. 任务依赖图

```
A-1 ──┬── A-2 (provider_id trait)
      └── A-3 (ToolRegistry provider_handles)
              └── A-4 (ToolDefinition source 字段)
                      └── A-5 (McpToolProxy: impl ToolProvider)
                              └── B-1..B-4 (协议 + 配置 + connector)
                                      └── B-5 (McpBundle + service start)
                                              ├── B-6 (init 预检)
                                              ├── B-7 (信号处理)
                                              └── B-8 (metadata YAML)
                                                      └── C-1 (ToolsService 合并)
                                                              ├── C-2 (启动顺序)
                                                              ├── C-3 (E2E 测试)
                                                              └── C-4 (grep 守卫)
```

## 5. 汇报节奏

| Phase 完成 | 汇报内容 |
|-----------|---------|
| Phase A 完 | `cargo check` 输出、新增的 `ToolSource` 枚举 + `provider_id()` 默认方法 |
| Phase B 完 | `cargo check` 输出、`cargo test` 输出、`McpBundle` 接口 |
| Phase C 完 | `cargo test` 输出、合并后的 `ToolRegistry` 工具数（含本地 + MCP）、C-4 grep 守卫结果 |
| E2E 完 | mock MCP server 启动日志、ToolExecutor 调用 MCP 工具的 Observation 截图 |

## 6. 阻塞项汇报清单

下列问题**遇到时立即停手**：

1. **B-3**：timeout 配置在 `McpServerConfig` 还是 `McpConfig`？影响 connector 的 API 形状
2. **B-5**：McpBundle 简化方案 vs 复杂方案——需用户拍板
3. **B-8**：`infra/metadata/` 目录约定是否存在
4. **C-2**：启动顺序若不满足，是否改 main.rs 调换，还是改 PluginInitContext 加 dependency graph
5. **A-3**：若 `ToolDefinition` 加 `source` 字段破坏现有 5 个测试（`tool_registry/plugin.rs` 中的 5 个 mock），需汇报
6. **测试发现新 bug**：如发现 MCP 文档本身有内部矛盾，停下汇报

---

## 7. 风险评估

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 子进程在 Windows 上 `kill` 不可靠 | 中 | 高（卡死） | 用 `kill_on_drop(true)` + 短连接 fallback |
| `provider_id()` 默认方法破坏老 Provider | 低 | 中（编译错） | 默认 `default` 不影响 |
| `McpBundle` 拆出来太复杂 | 中 | 中（重构多） | 先实现 B-5 简单版，B-5' 复杂版留 TODO |
| ToolRegistry 加 provider_handles 字段破坏测试 | 高 | 低（修测试） | 5 个 mock 测试加 default field |
| E2E mock MCP server 调试时间长 | 中 | 高（拖进度） | 先用 Python 一行 `cat` 当 mock |

---

**预计总工作量**：
- Phase A：约 2-3 小时（4 个文件改动 + 重新测试）
- Phase B：约 3-4 小时（5 个文件 + 进程调试）
- Phase C：约 2-3 小时（合并逻辑 + 启动顺序 + E2E）
- **总计**：1 个工作日专注开发 + 0.5 天测试与修复
