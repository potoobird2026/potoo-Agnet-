# tool_executor 槽口开发文档

> 文档版本：v3.1  
> 编写日期：2026-05-30  
> 状态：待开发（按三份规范从零设计，旧代码全部废弃）  
> 优先级：P0（Pipeline EXECUTE 阶段核心 Slot，无此槽口工具调用不执行）  
> 执行规范：《跨平台与硬编码规范》《protocol-Slot接入协议》《protocol-模块内部组件协议》

---

## 0. 设计约束

### 0.1 规范红线

| 来源 | 红线 | 本设计如何遵守 |
|------|------|---------------|
| 跨平台规范 §1 | 禁止硬编码 URL/模型名/超时/路径 | 超时值定义为 `DEFAULT_*` 常量，从配置读取；无 URL、无模型名、无硬编码路径 |
| 跨平台规范 §2 | 禁止裸用 `/tmp/`、`~`、相对路径 | `working_dir` 通过 `std::env::current_dir()` 获取，路径拼接用 `PathBuf::join()` |
| 跨平台规范 §3 | 测试禁止硬编码路径、禁止访问真实 API | 测试使用 Mock Provider，无网络调用、无文件 I/O |
| 跨平台规范 §4 | 自查清单 10 项全部通过 | §9.1 逐项检查 |
| Slot协议 §1 | SlotPlugin 单入口（init→run→shutdown） | 严格实现三方法生命周期 |
| Slot协议 §2 | 只通过 SlotAccessPoint 与核心交互 | 不直接访问任何核心状态 |
| Slot协议 §2.2 | Provider 通过 provider_raw + downcast 获取 | 严格按示例代码模式 |
| Slot协议 §3 | 元数据声明 permissions/requires | 声明 context:read、context:write；依赖 "tool" Provider |
| Slot协议 §4 | 权限 tag 与实际调用一致 | 只声明实际需要的方法对应权限 |
| Slot协议 §5 | SlotDirective 所有变体被正确处理 | Continue 覆盖所有路径，AbortStep 预留 |
| Slot协议 §6 | 生命周期：init→run→shutdown | 严格遵循 |
| Slot协议 §7 | 依赖的 Provider 未注册时优雅降级 | provider_raw 返回 None 时降级，不中断 Pipeline |
| Slot协议 §9 S-R01 | 所有 SlotDirective 变体必须被正确处理 | 每个分支都有明确的返回值 |
| Slot协议 §9 S-R02 | init 失败意味着插件不加载 | init 返回 Err 后不允许退化运行 |
| Slot协议 §9 S-R03 | run() 中禁止持有跨次调用的可变状态 | 熔断状态存入 StepContext，不在 Slot 字段中持有 |
| 组件协议 §0 | 本协议解决子模块各自为战问题 | 本槽口有内部子模块（熔断器、安全策略、用户确认），使用 Orchestrator 编排 |
| 组件协议 §1 | 组件单入口（Component trait） | 每个子模块实现 Component |
| 组件协议 §2 | 组件句柄 ComponentHandle | 通过 AccessPoint::call() + downcast 调用兄弟组件 |
| 组件协议 §3 | 内部数据共享通过 AccessPoint | 组件间不直接引用，通过 AccessPoint 读写 |
| 组件协议 §4 | 处理结果 Processing 枚举 | Continue/BreakChain/Restart/Warn |
| 组件协议 §5 | 组件元数据 ComponentMeta | 每个组件声明 name/version/priority/provides/requires/config_key |
| 组件协议 §6 | 模块边界：mod.rs 只暴露三样东西 | 对外只暴露 Slot 入口、配置、Orchestrator |
| 组件协议 §9.1 | Component 做生命周期，具体 trait 做业务接口 | 分离生命周期与业务接口 |
| 组件协议 C-R01 | call() 获取句柄后必须 downcast | 拿到 ComponentHandle 后必须 as_any() 向下转型 |
| 组件协议 C-R02 | requires 声明必须真实可验证 | 声明依赖的组件必须在代码中实际调用 |
| 组件协议 C-R03 | process() 必须可重入 | 组件不保留跨 process() 的隐式状态 |

### 0.2 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 是否有内部组件 | 是 | 熔断器、安全策略检查、用户确认是独立子模块 |
| 是否需要 Orchestrator | 是 | 组件协议 §0：有子模块需要编排 |
| 跨 run() 状态存储 | 存入 StepContext | Slot协议 S-R03：禁止在 Slot 字段中持有 |
| Provider 不可用时的行为 | 降级为空 Observation | Slot协议 §7：优雅降级 |
| Thought/Action/Observation 定义位置 | shared_types | 跨平台规范：类型归属统一 |
| ToolProvider trait 定义位置 | shared_types | 被 tool_registry、tool_executor、ToolsService 共同引用 |

---

## 1. 功能概述

### 1.1 功能定位

`tool_executor` 是 Pipeline **EXECUTE 阶段**的核心槽口，负责：

1. 从 `StepContext` 取出 LLM 产生的 `Thought::Action`
2. 通过内部组件链处理：熔断检查 → 安全策略检查 → 用户确认
3. 通过 Provider 扩展机制执行工具调用
4. 将执行结果 `Observation` 写入 `StepContext`，供 react_loop（loop 阶段）读取

**没有此槽口，LLM 产生的工具调用永远不会被执行。**

### 1.2 在 Pipeline 中的位置

```
Phase::init()       → InitPhaseSlot（会话初始化）
Phase::context()    → tool_registry（收集工具定义）
Phase::think()      → llm_thinker（生成 Thought）
Phase::audit()      → AuditPhaseSlot（安全审计）
Phase::execute()    → ★ tool_executor（本文档）
Phase::loop()       → react_loop（读取 Observation 决定下一轮）
Phase::memorize()   → memory_saver + compression_hook
```

### 1.3 数据流

```
llm_thinker (think 阶段)
    │
    ▼
StepContext["thought"] = Thought::Action { tool_name, arguments }
    │
    ▼
tool_executor (execute 阶段)
    │
    ├─ 1. 从 StepContext 取出 Thought
    ├─ 2. 判断 Thought 类型（Action / Final）
    ├─ 3. 熔断检查（CircuitBreaker Component）
    ├─ 4. 安全策略检查（SecurityPolicy Component，可选）
    ├─ 5. 用户确认（UserConfirmation Component，可选）
    ├─ 6. 通过 Provider 执行工具
    ├─ 7. 将 Observation 写入 StepContext
    │
    ▼
react_loop (loop 阶段)
    │
    └─ 从 StepContext["observation"] 读取，决定 Continue/JumpToThink/ForceBreak
```

---

## 2. 接口契约

### 2.1 SlotPlugin 实现（Slot协议 §1）

```rust
pub struct ToolExecutorSlot {
    orch: Option<Orchestrator>,
    config: ToolExecutorConfig,
}

#[async_trait]
impl SlotPlugin for ToolExecutorSlot {
    fn name(&self) -> &str { "tool_executor" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

### 2.2 SlotAccessPoint 使用（Slot协议 §2）

| 方法 | 权限 tag（Slot协议 §4） | 方向 | 说明 |
|------|------------------------|------|------|
| `read_context_raw("thought")` | `context:read` | 读 | 取出 llm_thinker 写入的 Thought |
| `write_context_raw("observation", ...)` | `context:write` | 写 | 写入工具执行结果 |
| `write_context_raw("circuit_breaker", ...)` | `context:write` | 写 | 写回熔断状态（S-R03 合规） |
| `session_id()` | 无 | 读 | 获取当前会话 ID |
| `phase_name()` | 无 | 读 | 确认当前阶段 |
| `provider_raw("tool")` | 无（Provider 扩展） | 读 | 获取工具执行 Provider |

### 2.3 插件元数据（Slot协议 §3）

| 字段 | 值 |
|------|-----|
| name | `"tool_executor"` |
| category | `"slot"` |
| version | `"0.1.0"` |
| permissions | `["context:read", "context:write"]` |
| requires | `["tool"]` |
| conflicts | `[]` |
| config_schema | `ToolExecutorConfig` 的 JSON Schema |

### 2.4 依赖的 Provider（Slot协议 §2.2）

| Provider Key | Trait 类型 | 注册者 | 用途 |
|-------------|-----------|--------|------|
| `"tool"` | `Arc<dyn ToolProvider>` | `ToolsService::start()` | 执行工具调用 |

**ToolProvider trait 定义**（归属 shared_types）：

```rust
/// 工具 Provider —— 由 ToolsService 实现并注册到 ProviderRegistry
///
/// 归属：shared_types
/// 引用者：tool_registry（list）、tool_executor（execute）、ToolsService（实现）
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// 返回所有已注册工具的定义列表
    fn list(&self) -> Vec<ToolDefinition>;

    /// 执行工具调用
    ///
    /// 入参：
    /// - tool_name: &str，工具名称
    /// - arguments: Value，工具参数
    /// - timeout: Duration，超时时间
    ///
    /// 出参：
    /// - String：工具执行结果（文本格式）
    ///
    /// 错误：
    /// - 工具未找到 → ToolError::NotFound
    /// - 执行超时 → ToolError::Timeout
    /// - 执行失败 → ToolError::ExecutionFailed
    async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<String, ToolError>;
}
```

### 2.5 StepContext 数据读写

| Key | 类型 | 方向 | 消费者 |
|-----|------|------|--------|
| `"thought"` | `Thought` | 读 | 本槽口（来自 llm_thinker） |
| `"observation"` | `Observation` | 写 | react_loop |
| `"circuit_breaker"` | `CircuitBreakerState` | 读/写 | 本槽口（跨 run() 状态，S-R03 合规） |

### 2.6 共享类型归属（跨平台规范 §1）

以下类型统一定义在 `shared_types` 中，禁止在其他模块重复定义：

```rust
// ── shared_types ──

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// LLM 推理结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Thought {
    Action { action: Action, reasoning: String, generated_at: Timestamp },
    Final { answer: String, reasoning: String, generated_at: Timestamp },
}

/// 工具调用动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub tool_call_id: Option<String>,
    pub created_at: Timestamp,
}

/// 工具执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Observation {
    Success { tool_name: String, output: String },
    Error { tool_name: String, error: String },
    Denied { tool_name: String, reason: String },
}

/// 工具执行错误
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("工具未找到: {0}")]
    NotFound(String),
    #[error("执行超时: {0}")]
    Timeout(String),
    #[error("执行失败: {0}")]
    ExecutionFailed(String),
}
```

### 2.7 SlotDirective 返回值（Slot协议 §5）

| 返回值 | 场景 |
|--------|------|
| `Continue` | 正常执行完成（含成功、失败、降级、跳过） |
| `AbortStep` | **预留**：致命安全错误（当前版本不返回） |

**设计说明**：按 Slot协议 §7，tool_executor 的 run() 在几乎所有场景下都返回 Continue。工具执行失败、Provider 不可用、安全策略拒绝、用户拒绝——这些都不应该中断 Pipeline，而是写入 Observation 让 react_loop 决定下一步。只有致命安全错误（如安全策略引擎本身被篡改）才返回 AbortStep，当前版本预留。

---

## 3. 内部组件架构（组件协议）

### 3.1 组件清单

tool_executor 有 3 个内部组件，通过 Orchestrator 编排：

| 组件 | 职责 | provides | requires |
|------|------|---------|---------|
| `CircuitBreakerComponent` | 熔断检查（检查是否熔断 + 写入 Denied Observation）；**注意：record_success/record_failure 在 Slot::run() 中调用，不在组件内部** | `"circuit_breaker_check"` | `[]`（能力依赖）；**类型依赖：`shared_types::Observation`** |
| `SecurityPolicyComponent` | 安全策略检查（可选） | `"security_check"` | `[]` |
| `UserConfirmationComponent` | 用户确认流程（可选） | `"user_confirmation"` | `[]` |

### 3.2 Orchestrator 编排

```
SlotPlugin::run()
    │
    ├─ 从 StepContext 取出 Thought
    ├─ 判断 Thought 类型
    │
    ▼
Orchestrator::process_all()
    │
    ├─ CircuitBreakerComponent::process() → Processing::Continue | BreakChain
    ├─ SecurityPolicyComponent::process() → Processing::Continue | BreakChain
    ├─ UserConfirmationComponent::process() → Processing::Continue | BreakChain
    │
    ▼
全部 Continue → 执行工具 → 写入 Observation
任一 BreakChain → 写入错误 Observation，跳过执行
```

### 3.3 组件详细设计

#### 3.3.1 CircuitBreakerComponent

```rust
/// 熔断器组件
///
/// 职责：检查工具是否处于熔断状态，若熔断则写入 Denied Observation 并返回 BreakChain
///
/// **重要设计说明**：
/// - 本组件的 `process()` 只负责"检查熔断状态 + 写入 Denied Observation"，不负责记录成功/失败次数
/// - `record_success()` / `record_failure()` 的调用位于 **Slot::run() 的步骤 6-7** 中
/// - 原因：工具执行发生在 Orchestrator::process_all() 之后（步骤 5），组件无法在 process() 内获取执行结果
/// - 如果未来其他 Slot 复用此组件，必须在 Slot::run() 中自行调用 record_success/record_failure
///
/// 状态存储：通过 AccessPoint 读写，不持有跨 process() 的状态（C-R03）
///
/// 类型依赖：`shared_types::Observation`（写入 Denied Observation）
pub struct CircuitBreakerComponent {
    threshold: u32,
    reset_duration: Duration,
}

impl CircuitBreakerComponent {
    pub fn meta() -> ComponentMeta {
        ComponentMeta {
            name: "circuit_breaker",
            version: "0.1.0",
            priority: 10,
            provides: &["circuit_breaker_check"],
            requires: &[],
            config_key: Some("circuit_breaker"),
        }
    }
}

#[async_trait]
impl Component for CircuitBreakerComponent {
    fn meta(&self) -> &ComponentMeta { Self::meta() }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            threshold: self.threshold,
            reset_duration: self.reset_duration,
        })
    }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从配置读取阈值和恢复时间
        self.threshold = ctx.config.circuit_breaker_threshold;
        self.reset_duration = Duration::from_secs(ctx.config.circuit_breaker_reset_secs);
        Ok(())
    }

    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        // 1. 从 AccessPoint 读取熔断状态（C-R03：不持有跨 process() 状态）
        let mut state: CircuitBreakerState = ap
            .read::<CircuitBreakerState>("circuit_breaker")
            .cloned()
            .unwrap_or_default();

        // 2. 获取当前工具名
        let tool_name = ap
            .read::<Action>("current_action")
            .map(|a| a.tool_name.clone())
            .ok_or_else(|| ComponentError::NotFound("current_action".into()))?;

        // 3. 检查是否熔断
        if state.is_open(&tool_name) {
            // 写入 Denied Observation
            ap.write("observation", Observation::Denied {
                tool_name: tool_name.clone(),
                reason: format!("熔断器打开，工具 {} 暂时不可用", tool_name),
            })?;
            return Ok(Processing::BreakChain);
        }

        // 4. 写回状态
        ap.write("circuit_breaker", state)?;
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
```

#### 3.3.2 SecurityPolicyComponent

```rust
/// 安全策略组件（可选）
///
/// 职责：评估工具调用是否违反安全策略
/// 配置：enable_security_policy = false 时，Orchestrator 不注册此组件
pub struct SecurityPolicyComponent {
    enabled: bool,
}

impl SecurityPolicyComponent {
    pub fn meta() -> ComponentMeta {
        ComponentMeta {
            name: "security_policy",
            version: "0.1.0",
            priority: 20,
            provides: &["security_check"],
            requires: &[],
            config_key: None,
        }
    }
}

#[async_trait]
impl Component for SecurityPolicyComponent {
    fn meta(&self) -> &ComponentMeta { Self::meta() }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self { enabled: self.enabled })
    }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
        Ok(())
    }

    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        if !self.enabled {
            return Ok(Processing::Continue);
        }

        let action = ap
            .read::<Action>("current_action")
            .ok_or_else(|| ComponentError::NotFound("current_action".into()))?;

        // 安全策略检查逻辑
        // 当前版本：始终通过（预留扩展点）
        // 未来：检查工具名黑名单、参数范围、调用频率等

        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
```

#### 3.3.3 UserConfirmationComponent

```rust
/// 用户确认组件（可选）
///
/// 职责：向用户请求确认后才执行工具
/// 配置：require_confirmation = false 时，Orchestrator 不注册此组件
pub struct UserConfirmationComponent {
    enabled: bool,
    timeout_secs: u64,
}

impl UserConfirmationComponent {
    pub fn meta() -> ComponentMeta {
        ComponentMeta {
            name: "user_confirmation",
            version: "0.1.0",
            priority: 30,
            provides: &["user_confirmation"],
            requires: &[],
            config_key: None,
        }
    }
}

#[async_trait]
impl Component for UserConfirmationComponent {
    fn meta(&self) -> &ComponentMeta { Self::meta() }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            enabled: self.enabled,
            timeout_secs: self.timeout_secs,
        })
    }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        self.timeout_secs = ctx.config.confirmation_timeout_secs;
        Ok(())
    }

    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        if !self.enabled {
            return Ok(Processing::Continue);
        }

        let action = ap
            .read::<Action>("current_action")
            .ok_or_else(|| ComponentError::NotFound("current_action".into()))?;

        // 通过 Provider 获取用户确认
        // 当前版本：始终通过（预留扩展点）
        // 未来：通过 CLI/GUI 服务请求用户确认

        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
```

### 3.4 内部 AccessPoint 数据共享（组件协议 §3）

| Key | 类型 | 读写者 |
|-----|------|--------|
| `"current_action"` | `Action` | Slot 写入 → 所有组件读取 |
| `"circuit_breaker"` | `CircuitBreakerState` | CircuitBreakerComponent 读/写；Slot::run() 步骤 7 写回（record_success/record_failure 后） |
| `"observation"` | `Observation` | CircuitBreakerComponent / SecurityPolicyComponent / UserConfirmationComponent 写入 → Slot 读取后写入 StepContext |

**组件类型依赖**（C-R02 要求 requires 声明真实可验证，此处补充类型层面依赖）：

| 组件 | 类型依赖 | 说明 |
|------|---------|------|
| `CircuitBreakerComponent` | `shared_types::Observation` | process() 中写入 `Observation::Denied` |
| `SecurityPolicyComponent` | `shared_types::Observation` | process() 中写入 `Observation::Denied`（预留） |
| `UserConfirmationComponent` | `shared_types::Observation` | process() 中写入 `Observation::Error`（预留） |

---

## 4. 配置结构体

### 4.1 常量定义（跨平台规范 §1）

```rust
/// 工具调用默认超时秒数
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 30;
/// 用户确认默认超时秒数
pub const DEFAULT_CONFIRMATION_TIMEOUT_SECS: u64 = 60;
/// 熔断默认阈值（连续失败次数）
pub const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: u32 = 5;
/// 熔断恢复默认秒数
pub const DEFAULT_CIRCUIT_BREAKER_RESET_SECS: u64 = 60;
```

### 4.2 配置结构体

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolExecutorConfig {
    /// 工具调用超时秒数，默认 DEFAULT_TOOL_TIMEOUT_SECS
    #[serde(default = "default_tool_timeout_secs")]
    pub timeout_secs: u64,

    /// 是否启用用户确认流程，默认 false
    #[serde(default)]
    pub require_confirmation: bool,

    /// 确认超时秒数，默认 DEFAULT_CONFIRMATION_TIMEOUT_SECS
    #[serde(default = "default_confirmation_timeout_secs")]
    pub confirmation_timeout_secs: u64,

    /// 是否启用安全策略检查，默认 false
    #[serde(default)]
    pub enable_security_policy: bool,

    /// 熔断阈值：连续失败 N 次后熔断，默认 DEFAULT_CIRCUIT_BREAKER_THRESHOLD
    #[serde(default = "default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: u32,

    /// 熔断恢复时间（秒），默认 DEFAULT_CIRCUIT_BREAKER_RESET_SECS
    #[serde(default = "default_circuit_breaker_reset_secs")]
    pub circuit_breaker_reset_secs: u64,
}

fn default_tool_timeout_secs() -> u64 { DEFAULT_TOOL_TIMEOUT_SECS }
fn default_confirmation_timeout_secs() -> u64 { DEFAULT_CONFIRMATION_TIMEOUT_SECS }
fn default_circuit_breaker_threshold() -> u32 { DEFAULT_CIRCUIT_BREAKER_THRESHOLD }
fn default_circuit_breaker_reset_secs() -> u64 { DEFAULT_CIRCUIT_BREAKER_RESET_SECS }
```

---

## 5. 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutorError {
    #[error("StepContext 中无 Thought")]
    NoThought,

    #[error("安全策略拒绝: {reason}")]
    SecurityDenied { reason: String },

    #[error("用户拒绝或确认超时")]
    UserRejected,

    #[error("工具未找到: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("工具执行超时: {tool_name}，限制 {timeout_secs} 秒")]
    Timeout { tool_name: String, timeout_secs: u64 },

    #[error("工具执行错误: {tool_name}，原因: {source}")]
    ExecutionError { tool_name: String, source: String },

    #[error("熔断器打开: {tool_name}")]
    CircuitBroken { tool_name: String },

    #[error("工具 Provider 未注册")]
    ProviderUnavailable,

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("内部组件错误: {0}")]
    ComponentError(String),
}

impl From<ToolExecutorError> for PluginError {
    fn from(e: ToolExecutorError) -> Self {
        match e {
            ToolExecutorError::NoThought => PluginError::Internal(e.to_string()),
            ToolExecutorError::SecurityDenied { .. } => PluginError::PermissionDenied {
                required: "tool_execute".into(),
            },
            ToolExecutorError::ProviderUnavailable => PluginError::Internal(e.to_string()),
            ToolExecutorError::ConfigError(msg) => PluginError::Config(msg),
            _ => PluginError::Internal(e.to_string()),
        }
    }
}
```

---

## 6. 文件结构

```
plugins/slots/tool_executor/
├── mod.rs                    # 模块入口（组件协议 §6.1：只暴露 Slot 入口、配置、Orchestrator）
├── plugin.rs                 # ToolExecutorSlot 实现（SlotPlugin trait）
├── config.rs                 # ToolExecutorConfig + 常量定义
├── error.rs                  # ToolExecutorError 定义
├── orchestrator.rs           # Orchestrator 实现（组件协议 §5）
└── components/
    ├── mod.rs                # 组件模块声明
    ├── circuit_breaker.rs    # CircuitBreakerComponent
    ├── security_policy.rs    # SecurityPolicyComponent
    └── user_confirmation.rs  # UserConfirmationComponent
```

---

## 7. mod.rs 规范（组件协议 §6.1）

```rust
// ============================================
// 模块：tool_executor 槽口
//
// 模块职责：
// 在 Pipeline EXECUTE 阶段执行 LLM 产生的工具调用动作
//
// 模块边界：
// - 本模块负责：工具执行、熔断检查、安全策略检查、用户确认
// - 本模块不负责：工具定义注册（tool_registry）、LLM 思考（llm_thinker）
//
// 依赖 Provider：
// - "tool"（由 ToolsService 注册，提供 ToolProvider trait）
//
// 被依赖模块：
// - react_loop 读取本模块写入的 Observation 决定循环
//
// 核心层实现：
// - SlotPlugin → ToolExecutorSlot
//
// 内部组件（Orchestrator 编排）：
// - CircuitBreakerComponent（熔断检查）
// - SecurityPolicyComponent（安全策略，可选）
// - UserConfirmationComponent（用户确认，可选）
//
// 协议合规：
// - S-R03：熔断状态存入 StepContext，不在 Slot 字段中持有跨 run() 状态
// - 组件协议 C-R03：所有组件 process() 可重入
// ============================================

pub mod components;
pub mod config;
pub mod error;
pub mod orchestrator;
pub mod plugin;

// 组件协议 §6.1：只暴露三样东西
pub use config::ToolExecutorConfig;
pub use orchestrator::ToolExecutorOrchestrator;
pub use plugin::ToolExecutorSlot;

// 内部类型不对外暴露
pub(crate) use error::ToolExecutorError;
```

---

## 8. 执行逻辑

### 8.1 init() 流程

```
init(ctx)
    │
    ├─ 1. 解析 ToolExecutorConfig（从 ctx.plugin_config）
    │     失败 → 返回 PluginError::Config（S-R02：插件不加载）
    │
    ├─ 2. 创建 Orchestrator
    │
    ├─ 3. 注册 CircuitBreakerComponent（必须）
    │
    ├─ 4. 注册 SecurityPolicyComponent（enable_security_policy = true 时）
    │
    ├─ 5. 注册 UserConfirmationComponent（require_confirmation = true 时）
    │
    ├─ 6. orch.init_all()
    │     任一组件 init 失败 → 返回 PluginError（S-R02）
    │
    └─ 7. 保存 orch 和 config
```

### 8.2 run() 流程

```
run(ap)
    │
    ├─ 1. 从 StepContext 读取 Thought
    │     ap.read_context_raw("thought") → downcast_ref::<Thought>()
    │     None → 返回 Continue（无 Thought 时跳过，不中断 Pipeline）
    │
    ├─ 2. 判断 Thought 类型
    │     Thought::Final → 写入 final_answer，返回 Continue
    │     Thought::Action → 继续
    │
    ├─ 3. 将 Action 写入内部 AccessPoint（供组件读取）
    │     ap_internal.write("current_action", action.clone())
    │
    ├─ 4. Orchestrator::process_all()
    │     CircuitBreakerComponent::process()
    │       → BreakChain → 读取 observation，写入 StepContext，返回 Continue
    │     SecurityPolicyComponent::process()
    │       → BreakChain → 读取 observation，写入 StepContext，返回 Continue
    │     UserConfirmationComponent::process()
    │       → BreakChain → 读取 observation，写入 StepContext，返回 Continue
    │     全部 Continue → 继续执行
    │
    ├─ 5. 通过 Provider 执行工具
    │     ap.provider_raw("tool") → downcast::<Arc<dyn ToolProvider>>()
    │     None → 写入 Observation::Error("Provider 未注册")，返回 Continue
    │     Some(provider) → provider.execute(tool_name, arguments, timeout)
    │
    ├─ 6. 处理执行结果（**此处调用 record_success/record_failure，不在 CircuitBreakerComponent 内部**）
    │     成功 → Observation::Success + state.record_success(&tool_name)
    │     失败 → Observation::Error + state.record_failure(&tool_name)
    │     超时 → Observation::Error + state.record_failure(&tool_name)
    │     **说明**：record_success/record_failure 必须在 Slot::run() 中调用，因为工具执行
    │     发生在 Orchestrator::process_all() 之后，组件的 process() 无法获取执行结果。
    │     这是设计上的职责分离：组件负责检查，Slot 负责记录。
    │
    ├─ 7. 写回熔断状态到 StepContext（S-R03 合规）
    │     ap.write_context_raw("circuit_breaker", state)
    │
    ├─ 8. 写入 Observation 到 StepContext
    │     ap.write_context_raw("observation", observation)
    │
    └─ 9. 返回 SlotDirective::Continue
```

### 8.3 shutdown() 流程

```
shutdown()
    │
    ├─ 1. orch.shutdown_all()
    │     逆序调用每个组件的 shutdown()
    │
    └─ 2. 释放资源
```

---

## 9. 规范检查清单

### 9.1 跨平台与硬编码规范（§4 自查清单）

| # | 检查项 | 结果 |
|---|--------|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | ✅ 不涉及（无 URL 构造） |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ 不涉及 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | ✅ `DEFAULT_TOOL_TIMEOUT_SECS` 等 4 个常量 |
| 4 | API 版本号定义为模块级 `const`，不散落 | ✅ 不涉及 |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ 不涉及（由 ToolProvider 处理） |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | ✅ 无文件 I/O |
| 7 | 数字阈值默认 `None` 或从配置读取 | ✅ 全部有 `DEFAULT_*` 常量 + serde default |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | ✅ 不涉及（由 ToolProvider 处理） |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ 测试使用 Mock，无路径 |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | ⬜ 待验证 |

### 9.2 protocol-Slot接入协议

| # | 检查项 | 条款 | 结果 |
|---|--------|------|------|
| 1 | 实现 SlotPlugin（init/run/shutdown） | §1 | ✅ |
| 2 | name() 返回全局唯一标识 | §1 | ✅ `"tool_executor"` |
| 3 | init 失败返回 Err，不退化运行 | S-R02 | ✅ 配置解析失败/组件 init 失败均返回 Err |
| 4 | run() 不缓存跨次可变状态 | S-R03 | ✅ 熔断状态存入 StepContext |
| 5 | 只通过 SlotAccessPoint 与核心交互 | §2 | ✅ |
| 6 | 权限声明与实际调用一致 | §3/§4 | ✅ context:read + context:write |
| 7 | requires 声明与实际依赖一致 | §3 | ✅ 声明 "tool" |
| 8 | SlotDirective 所有变体被正确处理 | §5/S-R01 | ✅ Continue 覆盖所有路径，AbortStep 预留 |
| 9 | Provider 不可用时优雅降级 | §7 | ✅ 降级为 Observation::Error |
| 10 | 通过 provider_raw + downcast 获取 Provider | §2.2 | ✅ |
| 11 | 元数据包含 name/category/version/permissions/requires/conflicts | §3 | ✅ |
| 12 | 生命周期：init→run(多次)→shutdown | §6 | ✅ |

### 9.3 protocol-模块内部组件协议

| # | 检查项 | 条款 | 结果 |
|---|--------|------|------|
| 1 | 内部子模块实现 Component trait | §1 | ✅ 3 个组件 |
| 2 | clone_box 返回 Box<dyn ComponentHandle> | §2 | ✅ |
| 3 | 组件间通过 AccessPoint 通信，不直接引用 | §3 | ✅ |
| 4 | AccessPoint 由 Orchestrator 统一注入 | §3.2 | ✅ |
| 5 | 处理结果使用 Processing 枚举 | §4 | ✅ Continue/BreakChain |
| 6 | 组件元数据 ComponentMeta 完整 | §5 | ✅ name/version/priority/provides/requires/config_key |
| 7 | Orchestrator 只做编排，不含业务代码 | §5.1 | ✅ |
| 8 | Orchestrator 校验 requires/provides | §5.2 | ✅ |
| 9 | mod.rs 只暴露三样东西 | §6.1 | ✅ Slot 入口、配置、Orchestrator |
| 10 | call() 获取句柄后必须 downcast | C-R01 | ✅ |
| 11 | requires 声明必须真实可验证 | C-R02 | ✅ |
| 12 | process() 必须可重入 | C-R03 | ✅ 状态通过 AccessPoint 读写 |
| 13 | Component 做生命周期，具体 trait 做业务接口 | §9.1 | ✅ |

---

## 10. 测试要点

### 10.1 正常路径测试

| 测试场景 | 前置条件 | 输入 | 期望 |
|---------|---------|------|------|
| 工具执行成功 | StepContext 中有 Thought::Action，tool Provider 已注册 | tool_name="read_file" | Continue，StepContext["observation"] = Success |
| Thought 为 Final | StepContext 中有 Thought::Final | answer="最终回答" | Continue，StepContext["final_answer"] 写入 |
| 无 Thought | StepContext 中无 "thought" | — | Continue，不 panic，不写 observation |

### 10.2 边界条件测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 参数为空 JSON | arguments = Value::Null | 正常传递给 Provider |
| 超时值为 0 | timeout_secs = 0 | 使用默认值 DEFAULT_TOOL_TIMEOUT_SECS |
| 熔断阈值 = 1 | circuit_breaker_threshold = 1 | 失败 1 次后熔断 |

### 10.3 异常路径测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 安全策略拒绝 | SecurityPolicyComponent 返回 BreakChain | Continue + Observation::Denied |
| 用户拒绝确认 | UserConfirmationComponent 返回 BreakChain | Continue + Observation::Error |
| 确认超时 | 超过 confirmation_timeout_secs | Continue + Observation::Error |
| 工具执行失败 | provider.execute 返回 Err | Continue + Observation::Error，熔断计数+1 |
| 工具执行超时 | execute 超过 timeout_secs | Continue + Observation::Error，熔断计数+1 |
| 熔断器打开 | 连续失败达到阈值 | Continue + Observation::Error，不执行工具 |
| Provider 未注册 | provider_raw("tool") 返回 None | Continue + Observation::Error("Provider 未注册") |

### 10.4 外部依赖测试

| 测试场景 | 前置条件 | 期望 |
|---------|---------|------|
| tool Provider 不可用 | Mock provider_raw 返回 None | 优雅降级，不 panic |
| 确认 channel 关闭 | UserConfirmation 组件内部 channel 关闭 | 返回 BreakChain，不 panic |

### 10.5 SlotDirective 完整性测试（S-R01）

| 返回值 | 场景 | Pipeline 行为 |
|--------|------|-------------|
| `Continue` | 正常执行完成 | 进入下一 Slot 或下一阶段 |
| `Continue` | 无 Thought / Final | 跳过执行 |
| `Continue` | 熔断/拒绝/超时/失败 | 写入错误 Observation，不中断 Pipeline |
| `Continue` | Provider 未注册 | 写入错误 Observation，不中断 Pipeline |
| `AbortStep` | **预留**：致命安全错误（当前版本不返回） | 终止本轮 Step 并标记错误 |

### 10.6 组件协议测试

| 测试场景 | 验证点 |
|---------|--------|
| CircuitBreakerComponent 可重入 | 连续调用 process()，状态通过 AccessPoint 读写，不持有跨 process() 隐式状态 |
| CircuitBreakerComponent requires 真实 | 组件代码中实际读取 "current_action"；**类型依赖 `shared_types::Observation`（写入 Denied）在 §3.4 中声明** |
| CircuitBreakerComponent 职责边界 | process() 只检查熔断+写 Denied；record_success/record_failure 在 Slot::run() 中调用，不混入组件 |
| SecurityPolicyComponent 可重入 | 连续调用 process()，无隐式状态 |
| UserConfirmationComponent 可重入 | 连续调用 process()，无隐式状态 |
| Orchestrator requires 校验 | 注册组件时 requires 不满足应报错 |
| Orchestrator 拓扑排序 | 按 priority 顺序执行组件 |

---

## 11. 开发清单

| 序号 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `shared_types` | 添加 `Thought`、`Action`、`Observation`、`ToolDefinition`、`ToolError`、`ToolProvider` trait | 统一定义 |
| 2 | `plugins/slots/tool_executor/config.rs` | 新建 | 常量 + ToolExecutorConfig |
| 3 | `plugins/slots/tool_executor/error.rs` | 新建 | ToolExecutorError + Into<PluginError> |
| 4 | `plugins/slots/tool_executor/components/circuit_breaker.rs` | 新建 | CircuitBreakerComponent；**注意：process() 只检查熔断+写 Denied Observation，record_success/record_failure 在 plugin.rs 的 run() 步骤 6 中调用** |
| 5 | `plugins/slots/tool_executor/components/security_policy.rs` | 新建 | SecurityPolicyComponent |
| 6 | `plugins/slots/tool_executor/components/user_confirmation.rs` | 新建 | UserConfirmationComponent |
| 7 | `plugins/slots/tool_executor/components/mod.rs` | 新建 | 组件模块声明 |
| 8 | `plugins/slots/tool_executor/orchestrator.rs` | 新建 | ToolExecutorOrchestrator |
| 9 | `plugins/slots/tool_executor/plugin.rs` | 新建 | ToolExecutorSlot 实现 |
| 10 | `plugins/slots/tool_executor/mod.rs` | 新建 | 模块入口（组件协议 §6.1） |
| 11 | `plugins/slots/mod.rs` | 添加 `pub mod tool_executor` | 模块注册 |
| 12 | `main.rs` | Pipeline 添加 `.add_slot(Phase::execute(), ...)` | 注册到 execute 阶段 |
| 13 | `plugins/services/tools/` | 实现 ToolProvider trait + 注册到 ProviderRegistry | Provider 实现 |

---

## 12. 依赖关系

### 12.1 上游依赖

| 依赖 | 类型 | 说明 |
|------|------|------|
| `ToolsService` | Provider `"tool"` | 注册 Arc<dyn ToolProvider> |
| `shared_types::Thought` | 类型 | 从 StepContext 读取 |
| `shared_types::Action` | 类型 | Thought 内部的工具调用动作 |
| `shared_types::Observation` | 类型 | 写入 StepContext；**CircuitBreakerComponent 的 process() 也直接写入 `Observation::Denied`** |
| `shared_types::ToolProvider` | trait | 工具执行接口 |

### 12.2 下游依赖

| 依赖者 | 说明 |
|--------|------|
| `react_loop` | 从 StepContext["observation"] 读取，决定 Continue/JumpToThink/ForceBreak |

### 12.3 执行顺序

Pipeline 阶段顺序保证 execute 在 think 之后、loop 之前，无需额外同步。

---

> 文档版本：v3.1  
> 最后更新：2026-05-30