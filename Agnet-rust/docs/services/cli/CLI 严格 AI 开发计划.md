# CLI（命令行接口通道服务）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/cli/cli开发文档.md` 生成 cli 模块的全部代码。

---

## 项目背景

- **模块名称**：cli（命令行接口通道服务）
- **模块定位**：标准输入输出通道，读取 stdin → 发送到 AgentRuntime → 输出响应到 stdout，支持授权确认交互和 Chronos 通知。
- **外部接口**：
  - `CliChannel` — ServicePlugin 入口
- **特点**：无内部组件（无 Orchestrator / Component），单一文件 `plugin.rs`，`AgentHandle` 通过构造函数注入，`watch` 通道管理运行状态
- **依赖项**：`tokio`、`async-trait`

---

## 硬编码分类定义（cli 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 输入长度限制 | — | `MAX_INPUT_LENGTH = 100_000` 模块级 const（安全阈值，不配置化） |
| 退出命令 | — | `"exit"` / `"quit"` 模块级 const |
| 提示符 | — | `"> "` 固定（可未来配置化） |

---

## 项目目录结构

```
src/plugins/services/cli/
├── mod.rs        # 模块入口：CliChannel
└── plugin.rs     # CliChannel（ServicePlugin 实现 + run_loop + handle_input）
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 cli开发文档.md 一致。

2. **零幻觉**：
   a. CliChannel 只有 2 个源文件（mod.rs + plugin.rs），无内部组件。
   b. AgentHandle 通过构造函数注入，不在 init/start 中获取。
   c. 运行状态通过 watch::channel 同步，非 Arc<Mutex>。

3. **零硬编码**：
   a. MAX_INPUT_LENGTH = 100_000 为模块级 const（安全阈值）
   b. 退出命令名称为模块级 const

4. **完整实现**：CliChannel 的 init/start/handle_signal/stop/shutdown 齐全，run_loop 完整可运行。

5. **错误处理**：
   - Chronos 通知失败：`let _ =` 忽略（Chronos 可选服务）
   - 输入读取失败 → eprintln + break 退出
   - 授权确认超时 → 默认拒绝

6. **测试同步生成**：
   - handle_input：超长截断/空行跳过/exit 退出/正常发送
   - CliChannel：生命周期 init/start/signal/stop/shutdown
   - 使用 mock AgentHandle 避免实际运行时依赖
```

---

## 详细开发步骤

### 步骤 0：确认骨架

**操作**：确认 mod.rs 和 plugin.rs 存在，Cargo.toml 依赖（tokio + async-trait）。

**验收**：`cargo check` 通过

---

### 步骤 1：plugin.rs — CliChannel

**结构体**：

```rust
const MAX_INPUT_LENGTH: usize = 100_000;

pub struct CliChannel {
    agent_handle: AgentHandle,
    session_id: String,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl CliChannel {
    pub fn new(agent_handle: AgentHandle) -> Self
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self
}
impl Default for CliChannel { fn default() -> Self { Self::new(AgentHandle::dummy()) } }
```

**ServicePlugin 实现**：

| 方法 | 行为 |
|------|------|
| `name()` | `"cli_channel"` |
| `init(ctx)` | 可选从配置解析；设置 session_id |
| `start(ap)` | 注册 Provider（`"cli_channel"`）；`tokio::spawn(run_loop)` |
| `handle_signal()` | 6 种信号：HealthCheck（检查 running）/ Suspend/Resume/GracefulShutdown（watch send true）/ ConfigReload |
| `stop()` | watch send true |
| `shutdown()` | watch send true |

**run_loop()**：
```
1. 打印欢迎消息
2. loop:
   - 检查 shutdown_rx.borrow()（关闭信号）
   - 打印 "> " 提示符
   - tokio::select!:
     - lines.next_line() → handle_input()
     - shutdown_rx.changed() → break
```

**handle_input()**：
```
1. 超长检查（> MAX_INPUT_LENGTH → 提示 + return true）
2. 空行跳过
3. exit/quit → return false
4. StepInput::new().with_source("cli").with_response()
5. agent_handle.step(input)
6. 匹配 StepResponse{Done/NeedAction/MaxTurnsReached/BreakStep/RestartStep}
7. return true
```

### 步骤 2：mod.rs

```
pub use plugin::CliChannel;
```

### 步骤 3：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. 对照 cli开发文档.md §5.4 的 10 项自查清单
3. CliChannel 完整生命周期测试 + handle_input 单元测试
