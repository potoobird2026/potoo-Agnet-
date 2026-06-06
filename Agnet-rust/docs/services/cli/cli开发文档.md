# CliChannel(命令行接口通道服务) 设计文档

## 0. 协议依据

本文档严格遵循以下协议，每条设计决策均可追溯到协议具体条款。协议是规则，不是建议。

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **Service 集成协议** | 模块对外接口 | §1 插件单入口、§2 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§7 新增/替换流程、§8 红线 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |

**注意**：CliChannel 是一个功能单一的通道服务，不包含内部组件（无 Orchestrator、无 Component），因此模块内部组件协议不适用。

---

## 0.5 功能清单

| 功能 | 描述 | 优先级 |
|------|------|--------|
| 标准输入读取 | 从 stdin 读取用户输入，支持多行 | P0 |
| 标准输出输出 | 将 AgentRuntime 响应输出到 stdout | P0 |
| 授权确认交互 | 处理工具调用的用户授权确认（允许/拒绝/始终允许） | P1 |
| Chronos 通知 | 用户交互时通知 Chronos 服务 | P2 |
| 输入长度限制 | 防止恶意输入耗尽内存 | P0 |

---

## 1. 模块定位（Service 集成协议视角）

### 1.1 外部身份

遵循 Service 集成协议 §1——`CliChannel` 实现 `ServicePlugin` trait，作为标准输入输出通道服务运行。

**§1 要求的 6 个方法必须全部实现：**

```rust
#[async_trait]
impl ServicePlugin for CliChannel {
    fn name(&self) -> &str;                                              // §1
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;  // §1
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;  // §1
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>; // §1
    async fn stop(&mut self) -> Result<(), PluginError>;                 // §1
    async fn shutdown(&mut self) -> Result<(), PluginError>;             // §1
}
```

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `name` | 多次 | 返回 `"cli_channel"` |
| `init` | 1 | 校验配置、初始化输入缓冲 |
| `start` | 1 | 启动 stdin 读取循环，注册 Chronos 通知 Provider |
| `handle_signal` | 多次 | 响应信号（§3） |
| `stop` | 多次 | 停止读取循环 |
| `shutdown` | 1 | 释放资源 |

### 1.2 受控访问句柄（ServiceAccessPoint）

遵循 Service 集成协议 §2——`ServiceAccessPoint` 是 CliChannel 与 core 交互的**唯一通道**：

```rust
async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
    // §2.1 Core 内建方法
    let config = ap.get_config();     // 读取 Agent 配置
    ap.log("info", "CLI 通道启动");    // 写入日志

    // §2.2 Provider 注册——将 Chronos 通知能力暴露给其他插件
    let notifier: Arc<dyn ChronosNotifier> = Arc::new(CliChronosNotifier::new(...));
    ap.register_provider("chronos_notify", notifier);

    // 启动 stdin 读取循环
    self.run_input_loop().await;
    Ok(())
}
```

### 1.3 元数据声明

遵循 Service 集成协议 §4：

```yaml
name: cli_channel
category: service
version: 0.1.0
run_mode: background
provides:
  - cli_channel
requires: []
conflicts: []
```

| 字段 | 值 | 协议约束 |
|------|---|---------|
| `name` | `"cli_channel"` | 必须与 `ServicePlugin::name()` 一致（§4） |
| `category` | `"service"` | 固定值（§4） |
| `version` | `"0.1.0"` | 语义版本（§4） |
| `run_mode` | `"background"` | 后台常驻（§4） |
| `provides` | `["cli_channel"]` | 必须与 `start()` 中 `register_provider` 一致（§8 V-R03） |

### 1.4 生命周期映射

遵循 Service 集成协议 §5：

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

| 阶段 | 具体操作 |
|------|---------|
| `init(ctx)` | 初始化 stdin 缓冲区；校验配置 |
| `start(ap)` | 注册 Chronos 通知 Provider；启动 stdin 读取循环（`tokio::spawn`） |
| `handle_signal(signal)` | 6 种信号处理（§3） |
| `stop()` | 设置 `running = false`，读取循环退出 |
| `shutdown()` | 释放 stdin 缓冲区；反注册 Provider |

---

## 2. 核心设计

### 2.1 输入处理流程

```
用户输入 (stdin)
  │
  ├── 1. 检查输入长度（MAX_INPUT_LENGTH = 100,000）
  │     └── 超长 → 提示用户简化输入
  │
  ├── 2. 检查退出命令
  │     └── "exit" / "quit" → 退出循环
  │
  ├── 3. 通知 Chronos
  │     └── ChronosNotification::UserInteraction
  │
  ├── 4. 构造 StepInput
  │     └── StepInput::new(session_id, &input).with_source("cli").with_response()
  │
  ├── 5. 发送到 AgentRuntime
  │     └── agent_handle.step(input).await
  │
  └── 6. 等待响应并输出
        └── rx.await → print(answer)
```

### 2.2 授权确认交互

当 AgentRuntime 需要用户授权时（如工具调用），通过 `UserConfirmation` 通道发送确认请求：

```
授权确认流程：
  │
  ├── 1. 收到 UserConfirmation
  │     └── 包含 tool_name, operation, is_conflict, is_shell_cmd, response_tx
  │
  ├── 2. 显示授权请求
  │     └── 打印工具名、操作、冲突提示
  │
  ├── 3. 根据情况显示选项
  │     ├── 冲突情况：(d)拒绝, (o)单次, (a)始终
  │     ├── Shell 命令：(y)单次, (n)拒绝
  │     └── 普通情况：(y)单次, (a)始终, (n)拒绝
  │
  ├── 4. 读取用户选择
  │
  └── 5. 发送 AuthDecision 响应
        └── response.send(decision)
```

### 2.3 Chronos 通知

用户交互时通知 Chronos 服务，用于自适应定时调度：

```rust
if let Some(tx) = chronos_tx {
    let _ = tx.send(ChronosNotification::UserInteraction);
}
```

**§9 设计决策**：通知使用 `let _ =` 忽略发送失败。这是因为 Chronos 是可选服务——如果 Chronos 未启动，通知失败不应影响 CLI 正常工作。

---

## 3. 运行时信号（§3）

| 信号 | 处理方式 | 协议依据 |
|------|---------|---------|
| `GracefulShutdown` | 设置 `running = false`，读取循环退出 | §3 |
| `ImmediateShutdown` | 设置 `running = false`，立即退出 | §3 |
| `ConfigReload` | 记录日志，重新读取配置 | §3 |
| `HealthCheck` | 检查 `running == true`，否则返回 `Err` | §3、§8 V-R01 |
| `Suspend` | 设置 `suspended = true`，暂停读取 | §3 |
| `Resume` | 设置 `suspended = false`，恢复读取 | §3 |

**约束**：
- `handle_signal()` 不得阻塞超过 5 秒（§8 V-R02）
- `HealthCheck` 须在 5 秒内返回（§8 V-R01）

---

## 4. 主循环

`CliChannel` 在 `start()` 中通过 `tokio::spawn` 启动后台 stdin 读取循环：

```
主循环：
  │
  ├── 1. 打印提示符 "> "
  │
  ├── 2. 读取一行输入
  │     └── lines.next_line().await
  │
  ├── 3. 处理输入
  │     ├── 超长 → 提示用户
  │     ├── 空行 → 跳过
  │     ├── "exit"/"quit" → 退出
  │     └── 正常输入 → 发送到 AgentRuntime
  │
  └── 4. 等待响应并输出
        └── rx.await → match response → print
```

**分支**：如果有授权确认通道（`confirmation_rx`），主循环同时监听 stdin 和确认通道（`tokio::select!`）。

---

## 5. 跨平台与硬编码规范视角

### 5.1 硬编码值分类（§1 逐条对照）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 不涉及 | ✅ |
| 3 | 超时秒数 | 不涉及 | ✅ |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 不涉及 | ✅ |
| 7 | 数字阈值 | 涉及 | ⚠️ `MAX_INPUT_LENGTH = 100_000` 硬编码 |
| 8 | 字符串模板 | 不涉及 | ✅ |
| 9 | 平台指令 | 不涉及 | ✅ |

**§9 设计决策**：`MAX_INPUT_LENGTH` 硬编码为 100,000 字符。这是安全阈值，不是业务配置。将其设为常量而非配置项的理由：
1. 安全阈值不应由用户配置——用户可能将其设为无限大，失去保护作用
2. 100,000 字符足以覆盖正常输入，不会误伤合法用户
3. 如果需要调整，修改常量即可，不需要配置系统

### 5.2 跨平台路径规则（§2 逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ 不涉及 |
| 2.2 | 禁止裸用 `~` | ✅ 不涉及 |
| 2.3 | 禁止相对路径依赖 CWD | ✅ 不涉及 |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ 不涉及 |
| 2.5 | 路径分隔符判断 | ✅ 不涉及 |
| 2.6 | 文件扩展名判断 | ✅ 不涉及 |
| 2.7 | 临时文件/目录 | ✅ 不涉及 |
| 2.8 | 数据目录 | ✅ 不涉及 |

### 5.3 测试代码规范（§3 逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 3.1 | 临时路径用 `std::env::temp_dir()` | ✅ 不涉及 |
| 3.2 | 平台特定测试用 `#[cfg()]` | ✅ 不涉及 |
| 3.3 | 网络测试用 mock 或 `#[ignore]` | ✅ 不涉及 |

### 5.4 自查清单（§4 逐项）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ 不涉及 |
| 2 | 模型名来自配置 | ✅ 不涉及 |
| 3 | 超时值来自配置或常量 | ✅ 不涉及 |
| 4 | API 版本号为模块级 const | ✅ 不涉及 |
| 5 | User-Agent 为 const | ✅ 不涉及 |
| 6 | 路径用 `dirs` + `join()` | ✅ 不涉及 |
| 7 | 数字阈值从配置读取 | ⚠️ `MAX_INPUT_LENGTH` 硬编码（§9 已说明理由） |
| 8 | 平台指令用 `OsKind` | ✅ 不涉及 |
| 9 | 测试无硬编码路径 | ✅ 不涉及 |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 6. 红线

### Service 集成协议红线（§8）

| 编号 | 红线 | 合规 |
|------|------|:----:|
| V-R01 | 必须响应 `HealthCheck` | ✅ |
| V-R02 | `handle_signal` 不得阻塞超过 5 秒 | ✅ |
| V-R03 | `provides` 与 `register_provider` 一致 | 待 start() 实现 |

### 跨平台与硬编码规范红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| §1 | URL/模型名/超时/版本号/User-Agent 不硬编码 | ✅ 不涉及 |
| §2 | 文件路径不使用 `~`、相对路径、Unix-only 路径 | ✅ 不涉及 |
| §3 | 测试中不使用硬编码路径 | ✅ 不涉及 |

---

## 7. 设计决策

### 7.1 MAX_INPUT_LENGTH 硬编码

**决策**：`MAX_INPUT_LENGTH = 100_000` 硬编码为常量，不通过配置读取。

**理由**：安全阈值不应由用户配置。100,000 字符足以覆盖正常输入（约 50,000 个汉字或 100,000 个英文字符），不会误伤合法用户。如果需要调整，修改常量即可。

### 7.2 Chronos 通知使用 let _ =

**决策**：`let _ = tx.send(ChronosNotification::UserInteraction)` 忽略发送失败。

**理由**：Chronos 是可选服务。如果 Chronos 未启动，通知失败不应影响 CLI 正常工作。使用 `let _ =` 显式忽略错误是 Rust 的惯用写法。

### 7.3 stdin 循环使用 tokio::select!

**决策**：有授权确认通道时，使用 `tokio::select!` 同时监听 stdin 和确认通道。

**理由**：授权确认是异步事件，可能在用户输入的任意时刻到达。使用 `tokio::select!` 可以同时处理两个事件源，避免阻塞。

### 7.4 AgentHandle 通过构造函数注入

**决策**：`AgentHandle` 通过 `CliChannel::new(agent_handle)` 注入，不由 ServicePlugin 生命周期管理。

**理由**：`AgentHandle` 是 AgentRuntime 的内部句柄，不属于 ServicePlugin 的 `init/start/stop/shutdown` 生命周期。通过构造函数注入比在 `init()` 或 `start()` 中获取更清晰——它明确表达了"CLI 通道依赖 AgentRuntime"这一关系。

### 7.5 并发状态共享使用 watch 通道

**决策**：`running`/`suspended` 状态通过 `watch::channel` 广播，不由 `CliChannel` 直接持有。

**理由**：
1. **读多写少**：`run_loop()` 每秒读一次状态，`handle_signal()` 偶尔写一次
2. **无锁**：`watch::Receiver::changed()` 是异步的，不阻塞
3. **单写者**：只有 `handle_signal()` 写入，不需要 Mutex 的多写者保护
4. **简洁**：比 `Arc<Mutex<CliInner>>` 更简单

### 7.6 无内部组件

**决策**：CliChannel 不使用 Orchestrator 和 Component 体系。

**理由**：模块内部组件协议（§1）适用于有多个内部功能单元的模块。CliChannel 的功能单一（读取 stdin → 发送 → 输出），不需要组件化拆分。如果未来需要扩展（如添加命令解析、历史记录等功能），可以引入组件体系。

---

## 8. 新增/替换流程

### 新增功能到 CliChannel

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 在 `handle_input()` 中添加处理逻辑 | `plugin.rs` |
| 2 | 如果需要新的 Provider，添加 `ap.register_provider()` | `plugin.rs` |
| 3 | 运行 `cargo check` | — |

**共需改 1 个文件**：`plugin.rs`

### 替换为新的通道实现

| 步骤 | 做什么 |
|------|--------|
| 1 | 确认新实现的 `name()` 返回值与旧实现一致 |
| 2 | 编写新 `impl ServicePlugin`，替换原文件 |
| 3 | 在 `plugins/services/mod.rs` 修改模块声明（如有需要） |
| 4 | `cargo check` + 功能测试 |

---

## 9. ServicePlugin 完整实现

### 9.1 AgentHandle 来源（§7.4 设计决策）

**问题**：`CliChannel` 需要 `AgentHandle` 来发送 `StepInput` 到 AgentRuntime，但 `ServiceAccessPoint` 没有 `get_agent_handle()` 方法。

**解决方案**：`AgentHandle` 通过构造函数注入，不由 ServicePlugin 生命周期管理：

```rust
// main.rs 中的使用方式
let runtime = AgentRuntime::new(pipeline);
let handle = runtime.handle();  // 获取 AgentHandle

let mut cli = CliChannel::new(handle);  // 通过构造函数注入
cli.init(&ctx).await.unwrap();
cli.start(ap).await.unwrap();
```

**理由**：`AgentHandle` 是 AgentRuntime 的内部句柄，不属于 ServicePlugin 生命周期。通过构造函数注入比在 `init()` 或 `start()` 中获取更清晰——它明确表达了"CLI 通道依赖 AgentRuntime"这一关系。

### 9.2 并发模型（§7.5 设计决策）

**问题**：`CliChannel` 的 `running`/`suspended` 状态需要在 `handle_signal()`（主任务）和 `run_loop()`（后台任务）之间共享。

**解决方案**：使用 `Arc<watch::Receiver<bool>>` 广播关闭信号：

```rust
use tokio::sync::watch;

pub struct CliChannel {
    agent_handle: AgentHandle,
    session_id: String,
    shutdown_rx: watch::Receiver<bool>,  // 共享关闭信号
}

impl CliChannel {
    pub fn new(agent_handle: AgentHandle) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            agent_handle,
            session_id: "default".to_string(),
            shutdown_rx,
        }
    }
}
```

**§9 设计决策**：使用 `watch` 而非 `Mutex` 的理由：
1. **读多写少**：`run_loop()` 每秒读一次状态，`handle_signal()` 偶尔写一次
2. **无锁**：`watch::Receiver::changed()` 是异步的，不阻塞
3. **单写者**：只有 `handle_signal()` 写入，不需要 Mutex 的多写者保护
4. **简洁**：比 `Arc<Mutex<CliInner>>` 更简单

### 9.3 ServicePlugin 完整实现

```rust
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::watch;

use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::error::PluginError;
use crate::core::access::ServiceAccessPoint;
use crate::core::context::AgentHandle;

/// 安全阈值：最大输入长度（§7.1 设计决策）
const MAX_INPUT_LENGTH: usize = 100_000;

pub struct CliChannel {
    agent_handle: AgentHandle,
    session_id: String,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl CliChannel {
    pub fn new(agent_handle: AgentHandle) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            agent_handle,
            session_id: "default".to_string(),
            shutdown_tx,
            shutdown_rx,
        }
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }
}

impl Default for CliChannel {
    fn default() -> Self { Self::new(AgentHandle::dummy()) }
}

#[async_trait]
impl ServicePlugin for CliChannel {
    fn name(&self) -> &str { "cli_channel" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 从 ctx.plugin_config 解析配置（如有）
        // 校验配置合法性
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // §2.2 Provider 注册（V-R03：provides 必须与 register_provider 一致）
        // ap.register_provider("cli_channel", Arc::new(CliProviderImpl::new(...)));

        // 启动 stdin 读取循环
        let agent_handle = self.agent_handle.clone();
        let session_id = self.session_id.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        tokio::spawn(async move {
            Self::run_loop(agent_handle, session_id, &mut shutdown_rx).await;
        });

        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                if *self.shutdown_rx.borrow() {
                    return Err(PluginError::Runtime("服务未运行".into()));
                }
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                tracing::info!("[cli] 配置重载");
                Ok(())
            }
            ServiceSignal::Suspend => {
                tracing::info!("[cli] 已暂停");
                Ok(())
            }
            ServiceSignal::Resume => {
                tracing::info!("[cli] 已恢复");
                Ok(())
            }
            ServiceSignal::GracefulShutdown => {
                let _ = self.shutdown_tx.send(true);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}
```

### 9.4 run_loop 实现

```rust
impl CliChannel {
    async fn run_loop(
        agent_handle: AgentHandle,
        session_id: String,
        shutdown_rx: &mut watch::Receiver<bool>,
    ) {
        println!("aagnet CLI 通道已启动。输入 'exit' 或 'quit' 退出。");

        let stdin = BufReader::new(io::stdin());
        let mut lines = stdin.lines();

        loop {
            // 检查关闭信号
            if *shutdown_rx.borrow() {
                break;
            }

            print!("> ");
            use std::io::Write;
            let _ = std::io::stdout().flush();

            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            if !Self::handle_input(&session_id, &agent_handle, line).await {
                                break;
                            }
                        }
                        Ok(None) => { println!("输入流已关闭。再见！"); break; }
                        Err(e) => { eprintln!("读取输入失败: {}", e); break; }
                    }
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("[cli] 收到关闭信号");
                    break;
                }
            }
        }

        tracing::info!("[cli] 通道已停止");
    }

    async fn handle_input(
        session_id: &str,
        agent_handle: &AgentHandle,
        line: String,
    ) -> bool {
        if line.len() > MAX_INPUT_LENGTH {
            eprintln!("输入过长（最大 {} 字符），请简化输入", MAX_INPUT_LENGTH);
            return true;
        }

        let trimmed = line.trim().to_string();
        if trimmed.is_empty() { return true; }
        if trimmed == "exit" || trimmed == "quit" {
            println!("再见！");
            return false;
        }

        let (input, rx) = StepInput::new(session_id, &trimmed)
            .with_source("cli")
            .with_response();

        if let Err(e) = agent_handle.step(input).await {
            eprintln!("发送失败: {}", e);
            return false;
        }

        match rx.await {
            Ok(Ok(response)) => match response {
                StepResponse::Done { answer, .. } => { println!("{}", answer); }
                StepResponse::NeedAction { reasoning, .. } => { println!("需要执行操作: {}", reasoning); }
                StepResponse::MaxTurnsReached { partial, .. } => { println!("达到最大轮次: {}", partial); }
                StepResponse::BreakStep { message, .. } => { println!("步骤中断: {}", message); }
                StepResponse::RestartStep { .. } => { println!("步骤重试中..."); }
            },
            Ok(Err(e)) => { eprintln!("错误: {}", e); }
            Err(_) => { eprintln!("运行时已关闭"); return false; }
        }

        true
    }
}
```
