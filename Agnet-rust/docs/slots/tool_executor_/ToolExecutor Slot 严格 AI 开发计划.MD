# ToolExecutor Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 tool-executor 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：tool-executor（工具执行槽口）
- **模块定位**：Pipeline EXECUTE 阶段核心 Slot，负责从 StepContext 取出 LLM 产生的 `Thought::Action`，经过熔断检查/安全策略/用户确认（3 个内部组件，Orchestrator 编排）后，通过 Provider 执行工具调用，并将结果 `Observation` 写入 StepContext
- **内部组件**：3 个（CircuitBreakerComponent、SecurityPolicyComponent、UserConfirmationComponent），通过 Orchestrator 编排。**注意：`record_success`/`record_failure` 在 Slot::run() 中调用，不在 Component 内部**
- **Orchestrator**：`ToolExecutorOrchestrator`（专用类型，非泛型），所有组件 `requires: &[]`，串行执行
- **代码目录**：`src/plugins/slots/tool_executor/`
- **依赖项**：`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror`
- **设计文档**：`docs/slots/tool_executor_/01_tool_executor_槽口开发文档.md`
- **外部类型引用**：
  - `Thought`、`Action`、`Observation`、`ToolDefinition`、`ToolError`、`ToolProvider` → `crate::shared_types`
  - `CircuitBreakerState` → 本模块 `types.rs` 定义

---

## 硬编码专项预防纲领

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 超时秒数 | `Duration::from_secs(30)` 写在 execute 调用处 | 从 `ToolExecutorConfig.timeout_secs` 读取，默认值 `DEFAULT_TOOL_TIMEOUT_SECS` 常量 |
| 数字阈值 | `threshold = 5` 写在熔断判断处 | 从 `ToolExecutorConfig.circuit_breaker_threshold` 读取，默认值 `DEFAULT_CIRCUIT_BREAKER_THRESHOLD` 常量 |
| 日志前缀 | `"[tool_executor]"` 散落 | 定义为 `LOG_PREFIX` 常量 |

---

## 项目目录结构

```
src/plugins/slots/tool_executor/
├── mod.rs                        # 模块入口（暴露 ToolExecutorSlot + ToolExecutorConfig + ToolExecutorOrchestrator）
├── plugin.rs                     # ToolExecutorSlot 实现（SlotPlugin trait）
├── config.rs                     # ToolExecutorConfig + 4 个 DEFAULT_* 常量
├── error.rs                      # ToolExecutorError + Into<PluginError>
├── types.rs                      # CircuitBreakerState 定义
├── component.rs                  # 组件基础设施：Component trait + ComponentHandle + AccessPoint + Processing + ComponentMeta + ComponentError + InitContext + ModuleConfig + MetricsHandle
├── orchestrator.rs               # ToolExecutorOrchestrator + InternalAccessPointImpl
└── components/
    ├── mod.rs                    # pub mod 声明 + re-export
    ├── circuit_breaker.rs        # CircuitBreakerComponent
    ├── security_policy.rs        # SecurityPolicyComponent
    └── user_confirmation.rs      # UserConfirmationComponent
```

---

## AI 宪法

```
[宪法已生效，本次对话必须无条件遵守]

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与 `01_tool_executor_槽口开发文档.md` 完全一致，不得自行增删改。

2. **零幻觉**：
   - tool_executor 有且只有 3 个组件（CircuitBreaker、SecurityPolicy、UserConfirmation），不凭空生成第 4 个
   - SecurityPolicyComponent 和 UserConfirmationComponent 是**可选的**——仅当 `enable_security_policy=true` 或 `require_confirmation=true` 时才注册
   - `CircuitBreakerComponent::process()` **只负责**检查熔断状态+写入 Denied Observation，**不负责** record_success/record_failure——这些在 Slot::run() 步骤 6-7 中调用
   - `run()` 返回 Continue（所有路径）或 AbortStep（预留，当前版本不返回）
   - 所有 `Thought`、`Action`、`Observation`、`ToolDefinition`、`ToolError`、`ToolProvider` 来自 `crate::shared_types`，不在本模块定义

3. **零硬编码**：
   a. 4 个 DEFAULT_* 常量：`DEFAULT_TOOL_TIMEOUT_SECS`、`DEFAULT_CONFIRMATION_TIMEOUT_SECS`、`DEFAULT_CIRCUIT_BREAKER_THRESHOLD`、`DEFAULT_CIRCUIT_BREAKER_RESET_SECS`
   b. `LOG_PREFIX` 常量统一日志前缀
   c. `Duration::from_secs(self.config.timeout_secs)` 从配置读取

4. **完整实现**：每个函数必须完整实现，不允许 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理**：
   a. `init()` 配置解析失败 → `PluginError::Config`（S-R02）
   b. 组件 `init_all()` 失败 → 传播 Err，插件不加载
   c. StepContext 写入失败（observation, circuit_breaker, final_answer）→ 传播 Err
   d. Provider 不可用 → 降级为 Observation::Error，返回 Continue
   e. 工具执行失败 → 记录错误 Observation，`record_failure`，返回 Continue
   f. 无 Thought → 返回 Continue（跳过执行）
   g. 不允许 `unwrap()`（测试除外）

6. **组件协议红线**：
   a. C-R01：`call()` 获取句柄后必须 downcast
   b. C-R02：`requires` 声明必须真实可验证（tool_executor 所有组件 requires=[]，但类型依赖 Observation 必须真实写入）
   c. C-R03：`process()` 必须可重入（状态通过 AccessPoint 读写）

7. **职责分离**：
   - `CircuitBreakerState.record_success()` / `record_failure()` 在 Slot::run() 中调用，**绝不在 Component::process() 中调用**
   - Component::process() 只做检查，不改变熔断计数
   - Slot::run() 负责执行工具、记录结果、写回状态

8. **组件注册逻辑**（按优先级串行）：
   - CircuitBreakerComponent：始终注册（priority=10）
   - SecurityPolicyComponent：仅 `enable_security_policy=true` 时注册（priority=20）
   - UserConfirmationComponent：仅 `require_confirmation=true` 时注册（priority=30）

9. **run() 处理 Thought::Final**：写入 `final_answer` 到 StepContext，返回 Continue（不执行工具、不触发组件链）

10. **禁止额外依赖**：只能使用 `std`、`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror` 以及项目内部模块。

11. **测试同时生成**：MockSlotAccessPoint + MockToolProvider + 各组件单元测试 + Slot 集成测试
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**操作**：
1. 确认 Cargo.toml 依赖：`tokio`（features=["sync"]）、`tracing`、`async-trait`、`serde`（features=["derive"]）、`serde_json`、`thiserror`
2. 确认 `crate::shared_types` 已定义 `Thought`、`Action`、`Observation`、`ToolDefinition`、`ToolError`、`ToolProvider` trait
3. 创建目录结构（7 个文件）
4. 在 `src/plugins/slots/mod.rs` 添加 `pub mod tool_executor;`

**验收**：`cargo check` 无 error，目录完整。

---

### 步骤 1：config.rs + error.rs + types.rs

#### 1a：config.rs

```rust
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_CONFIRMATION_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: u32 = 5;
pub const DEFAULT_CIRCUIT_BREAKER_RESET_SECS: u64 = 60;
pub const LOG_PREFIX: &str = "tool_executor:";
```

`ToolExecutorConfig`（设计文档 §4.2）：6 个字段，注意 `circuit_breaker_threshold: u32`。

#### 1b：error.rs

`ToolExecutorError`（设计文档 §5）：10 个变体。
`Into<PluginError>` 映射规则：
- `NoThought` → `PluginError::Internal`
- `SecurityDenied` → `PluginError::PermissionDenied { required: "tool_execute" }`
- `ConfigError` → `PluginError::Config`
- `ProviderUnavailable` → `PluginError::Internal`
- 其他 → `PluginError::Internal`

#### 1c：types.rs

```rust
/// 熔断状态——跨 run() 存入 StepContext（S-R03 合规）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CircuitBreakerState {
    failures: HashMap<String, u32>,
}

impl CircuitBreakerState {
    pub fn record_failure(&mut self, tool_name: &str) {
        *self.failures.entry(tool_name.to_string()).or_insert(0) += 1;
    }
    pub fn record_success(&mut self, tool_name: &str) {
        self.failures.remove(tool_name);
    }
    pub fn failure_count(&self, tool_name: &str) -> u32 {
        self.failures.get(tool_name).copied().unwrap_or(0)
    }
}
```

**验收**：
- `circuit_breaker_threshold` 类型为 `u32`
- `timeout_secs` 类型为 `u64`
- `ToolExecutorError` 10 个变体名称与文档一致
- `CircuitBreakerState` 的 `record_failure`/`record_success`/`failure_count` 方法与签名正确
- 无 `batch_size`、`max_retries` 等幻觉字段

---

### 步骤 2：component.rs（组件基础设施）

**文件**：`src/plugins/slots/tool_executor/component.rs`

按协议 §1-5 定义，包含：

1. **`ComponentMeta`**：name/version/priority/provides/requires/config_key
2. **`Component` trait**：`name()` + `init(ctx)` + `process(ap)` + `shutdown()` + `clone_box() → Box<dyn ComponentHandle>`
3. **`ComponentHandle` trait** + blanket impl
4. **`AccessPoint` trait**：`read<T>(key)` / `write<T>(key, val)` / `call(name)` / `config()` / `metrics()` / `log()`——按协议 §3 的泛型签名 `read<T>` / `write<T>`：组件内部使用 `ap.read::<Action>("current_action")` 和 `ap.read::<CircuitBreakerState>("circuit_breaker")`
5. **`Processing`**：Continue / BreakChain / Restart / Warn
6. **`ComponentError`**：Config / Internal / NotFound
7. **`ModuleConfig`**（ComponentModuleConfig）：`pub` 字段 `circuit_breaker_threshold: u32`、`circuit_breaker_reset_secs: u64`、`confirmation_timeout_secs: u64`（设计文档 §3.3.1 使用 `ctx.config.circuit_breaker_threshold` 直接字段访问，非 getter 方法）
8. **`InitContext`**：包含 `config: ModuleConfig`
9. **`MetricsHandle`**：占位

**注意**：`AccessPoint::read<T>` 和 `write<T>` 使用泛型方法（non-object-safe）。这是故意的——ToolExecutorOrchestrator 知道具体类型 `InternalAccessPointImpl`，在 `process_all()` 中传 `&mut dyn AccessPoint`。泛型方法在 dyn trait 上不可调用，但 **`process()` 内部** 通过 `&mut dyn AccessPoint` 调用的 `read<T>`/`write<T>` 是泛型方法——实际上这在 Rust 中不允许（object safety）。正确做法是把 read/write 改为非泛型（使用 Any 类型擦除）。

**修正**：遵循协议 §3 的 `InternalAccessPointImpl` 模式——read/write 使用 `read_any` / `write_any` 风格（返回 `Option<&dyn Any>`），或遵循协议中的泛型签名但通过手动实现。查看协议 §3 的 AccessPoint trait 定义——它确实使用了泛型 `read<T>` / `write<T>`，这些在 `dyn AccessPoint` 上不可调用。

**解决方案**：参考 protocol-模块内部组件协议.md §3 的 AccessPoint trait 定义，它使用泛型 `read<T>` / `write<T>`，但 protocol 本身的示例代码也显示在 `impl AccessPoint for InternalAccessPointImpl` 中实现了这些泛型方法。在 `process()` 内部调用时，由于 `ap: &mut dyn AccessPoint` 是 trait object，这会导致 object safety 错误。

正确方案：将 read/write 改为非泛型，使用类型擦除：

```rust
pub trait AccessPoint: Send + Sync {
    fn read_any(&self, key: &str) -> Option<&dyn Any>;
    fn write_any(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), ComponentError>;
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;
    fn config(&self) -> &ModuleConfig;
    fn metrics(&self) -> &MetricsHandle;
    // 便利方法（有默认实现，非 trait 方法）
}
```

然后在组件中通过 `ap.read_any(key).and_then(|v| v.downcast_ref::<T>())` 使用。

**验收**：
- `AccessPoint` trait 使用 `read_any`/`write_any` 类型擦除（object-safe）
- `Component` trait 包含 `clone_box()` 返回 `Box<dyn ComponentHandle>`
- `ComponentHandle` blanket impl 正确
- `Processing` 4 个变体
- `ComponentError` 3 个变体
- `cargo check` 通过

---

### 步骤 3：orchestrator.rs

**文件**：`src/plugins/slots/tool_executor/orchestrator.rs`

**`ToolExecutorOrchestrator`**：

```rust
pub struct ToolExecutorOrchestrator {
    components: Vec<Box<dyn Component>>,
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ComponentModuleConfig,
}
```

**方法**：
- `new(config)` → 创建空 Orchestrator
- `register(component)` → 插入组件列表（工具执行器组件无依赖校验——所有组件 `requires: &[]`）
- `init_all()` → 串行调用所有组件的 `init(&InitContext { config })`
- `process_all()` → 串行执行所有组件的 `process()`，遇到 BreakChain 短路返回
- `shutdown_all()` → 逆序调用 shutdown
- `access_point()` → 返回 `Arc<RwLock<InternalAccessPointImpl>>`

**`InternalAccessPointImpl`**：
```rust
pub(crate) struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    data_share: HashMap<String, Box<dyn Any + Send>>,
    config: ComponentModuleConfig,
}
```

**注意**：`register()` 后需要同步 `InternalAccessPointImpl.components`——每次 register 时重建 HashMap。由于工具执行器只有 3 个组件且串行执行，`process_all()` 使用**串行模式**：

```rust
pub async fn process_all(&self) -> Result<(), ComponentError> {
    let mut ap = self.access_point.write().await;
    for component in &self.components {
        match component.process(&mut *ap).await? {
            Processing::Continue => continue,
            Processing::BreakChain => return Ok(()),
            Processing::Restart => return self.process_all().await,
            Processing::Warn { message } => tracing::warn!("{} {message}", LOG_PREFIX),
        }
    }
    Ok(())
}
```

**验收**：
- `process_all()` 串行执行，BreakChain 短路
- `InternalAccessPointImpl` 实现了 `AccessPoint` trait
- `access_point()` 返回 `Arc<RwLock<InternalAccessPointImpl>>`
- 无 `unwrap()`（测试除外）

---

### 步骤 4：components 目录（3 个组件）

#### 4a：components/mod.rs

```rust
pub mod circuit_breaker;
pub mod security_policy;
pub mod user_confirmation;
```

#### 4b：CircuitBreakerComponent（circuit_breaker.rs）

```rust
pub struct CircuitBreakerComponent {
    threshold: u32,
}

impl CircuitBreakerComponent {
    pub fn new(threshold: u32) -> Self {
        Self { threshold }
    }
    pub fn meta() -> ComponentMeta { /* name: "circuit_breaker", priority: 10, provides: ["circuit_breaker_check"], requires: [], config_key: Some("circuit_breaker") */ }
}
```

**`init()`**：读取 `ctx.config.circuit_breaker_threshold()` 设置 `self.threshold`。

**`process()`**（设计文档 §3.3.1，严格遵循职责分离）：
```
1. ap.read_any("circuit_breaker") → CircuitBreakerState
2. ap.read_any("current_action") → Action
3. state.failure_count(tool_name) >= self.threshold
   → is_open → 写 Observation::Denied → return BreakChain
4. 否则 → return Continue
```

**注意**：`process()` **不做** `record_success` / `record_failure`——这些在 Slot::run() 中调用。

**`clone_box()`**：`Box::new(Self { threshold: self.threshold })`

#### 4c：SecurityPolicyComponent（security_policy.rs）

```rust
pub struct SecurityPolicyComponent { enabled: bool }
```

**process()**：`!self.enabled` → Continue；否则读取 `current_action`，执行安全策略检查（当前版本始终通过）。

#### 4d：UserConfirmationComponent（user_confirmation.rs）

```rust
pub struct UserConfirmationComponent { enabled: bool, timeout_secs: u64 }
```

**process()**：`!self.enabled` → Continue；否则读取 `current_action`，执行用户确认（当前版本始终通过，预留扩展点）。

**验收**：
- 3 个组件的 `meta()` 与文档完全一致
- `CircuitBreakerComponent::process()` 中**没有** `record_success` / `record_failure`
- 所有组件 `requires: &[]`
- 每个组件实现 `Component` trait 的 5 个方法

---

### 步骤 5：plugin.rs（ToolExecutorSlot）

**文件**：`src/plugins/slots/tool_executor/plugin.rs`

**`ToolExecutorSlot`**：
```rust
pub struct ToolExecutorSlot {
    orch: Option<ToolExecutorOrchestrator>,
    config: ToolExecutorConfig,
}
```

**`init()`**（设计文档 §8.1）：
```
1. 解析 ToolExecutorConfig（serde_json::from_value → PluginError::Config）
2. 创建 ToolExecutorOrchestrator
3. 始终注册 CircuitBreakerComponent
4. 条件注册 SecurityPolicyComponent（enable_security_policy=true）
5. 条件注册 UserConfirmationComponent（require_confirmation=true）
6. orch.init_all() → 失败则 Err
```

**`run()`**（设计文档 §8.2，精确 9 段流程）：
```
[步骤 1] 读取 Thought：read_context_raw("thought") → downcast_ref
         None → 返回 Continue（跳过）

[步骤 2] 判断类型：Thought::Final → 写入 final_answer → 返回 Continue
         Thought::Action → 继续

[步骤 3] 写 Action 到内部 AccessPoint：ap_int.write("current_action", action)

[步骤 4] Orchestrator::process_all()
         → BreakChain 短路 → 读取内部 AP 的 observation → 写入 StepContext → 返回 Continue
         → 全部 Continue → 继续

[步骤 5] 通过 Provider 执行工具
         provider_raw("tool") → downcast → execute(name, args, timeout)
         None → 写 Observation::Error → 返回 Continue

[步骤 6] 处理执行结果（★ 此处调用 record_success/record_failure）
         成功 → Observation::Success + state.record_success(tool_name)
         超时 → Observation::Error + state.record_failure(tool_name)
         失败 → Observation::Error + state.record_failure(tool_name)

[步骤 7] 写回熔断状态到 StepContext（S-R03 合规）
         write_context_raw("circuit_breaker", state)

[步骤 8] 写入 Observation 到 StepContext
         write_context_raw("observation", observation)

[步骤 9] 返回 Continue
```

**`shutdown()`**：`orch.shutdown_all()`

**关键设计说明**（来自设计文档 §3.3.1）：
> `record_success()` / `record_failure()` 的调用位于 Slot::run() 的步骤 6-7 中。原因：工具执行发生在 Orchestrator::process_all() 之后（步骤 5），组件无法在 process() 内获取执行结果。这是设计上的职责分离：组件负责检查，Slot 负责记录。

**`CircuitBreakerState` 跨 run() 传递（S-R03 合规）**：
```rust
let mut state: CircuitBreakerState = ap
    .read_context_raw("circuit_breaker")
    .and_then(|any| any.downcast_ref::<CircuitBreakerState>().cloned())
    .unwrap_or_default();
// ... 使用 state ...
ap.write_context_raw("circuit_breaker", Box::new(state))?;
```

**`Observation::Denied` 提取逻辑**（步骤 4 中 BreakChain 后的处理）：
```rust
// 运行组件链
let result = orch.process_all().await;
match result {
    Ok(()) => {
        // 检查组件是否写入了 observation（BreakChain 可能已写入 Denied）
        let ap_guard = orch.access_point().write().await;
        let denied_obs = ap_guard.read_any("observation")
            .and_then(|any| any.downcast_ref::<Observation>().cloned());
        drop(ap_guard);
        
        if let Some(obs) = denied_obs {
            ap.write_context_raw("observation", Box::new(obs))
                .map_err(|e| PluginError::Internal(e.to_string()))?;
            return Ok(SlotDirective::Continue);
        }
    }
    Err(e) => {
        tracing::error!("{LOG_PREFIX} 组件链执行失败: {e}");
        // 降级：不执行工具，写错误 observation
        // ...
    }
}
```

**验收**：
- `init()` 条件注册三个组件
- `run()` 中 `record_success`/`record_failure` 在 `process_all()` **之后**调用
- `CircuitBreakerState` 从 StepContext 读取/写入（不在 Slot 字段中）
- `Thought::Final` 路径写入 `final_answer`，不触发组件链
- 无 Thought → 直接返回 Continue
- 所有 `write_context_raw` 传播 Err（observation、circuit_breaker、final_answer）

---

### 步骤 6：mod.rs（模块入口）

**协议 §6.1**：只暴露三样东西——`ToolExecutorSlot` + `ToolExecutorConfig` + `ToolExecutorOrchestrator`。

```rust
pub mod components;
pub mod config;
pub mod error;
pub mod orchestrator;
pub mod plugin;

pub use config::ToolExecutorConfig;
pub use orchestrator::ToolExecutorOrchestrator;
pub use plugin::ToolExecutorSlot;
pub(crate) use error::ToolExecutorError;
```

---

### 步骤 7：注册到 Pipeline

`plugins/slots/mod.rs` → `pub mod tool_executor;`

Pipeline 构建代码：
```rust
pipeline.add_slot(
    Phase::execute(),
    Box::new(ToolExecutorSlot::new()),
);
```

---

### 步骤 8：集成测试

**Mock 实现**：
- `MockSlotAccessPoint`：实现 `SlotAccessPoint`，存储 thought/context_data
- `MockToolProvider`：实现 `ToolProvider`，可控执行成功/失败
- `MockAccessPoint`：实现 `AccessPoint`，用于组件单元测试

**测试场景**（设计文档 §10）：

**正常路径**（§10.1）：
| # | 名称 | 场景 | 断言 |
|:-:|------|------|------|
| 1 | `test_tool_executed_successfully` | Action + ToolProvider 返回成功 | Observation::Success，Continue |
| 2 | `test_thought_final_skips_execution` | Thought::Final | 写入 final_answer，不执行工具，Continue |
| 3 | `test_no_thought_skips` | 无 Thought | Continue，不 panic，不写 observation |

**边界条件**（§10.2）：
| # | 名称 | 场景 | 断言 |
|:-:|------|------|------|
| 4 | `test_empty_arguments` | arguments=Null | 正常传递给 Provider，Continue |
| 5 | `test_zero_timeout_uses_default` | timeout_secs=0 | init 使用默认值 DEFAULT_TOOL_TIMEOUT_SECS |
| 6 | `test_threshold_one` | threshold=1 | 失败 1 次后打开熔断 |

**异常路径**（§10.3）：
| # | 名称 | 场景 | 断言 |
|:-:|------|------|------|
| 7 | `test_circuit_breaker_blocks` | 连续失败达阈值 | process_all 返回 BreakChain，Observation::Denied，Continue |
| 8 | `test_tool_execution_error` | execute 返回 Err | Observation::Error，熔断计数+1，Continue |
| 9 | `test_tool_execution_timeout` | execute 超时 | Observation::Error，熔断计数+1，Continue |
| 10 | `test_provider_not_registered` | provider_raw 返回 None | Observation::Error("Provider 未注册")，Continue |
| 11 | `test_circuit_breaker_not_open_below_threshold` | 失败次数 < threshold | 继续执行，不阻断 |

**组件协议测试**（§10.6）：
| # | 名称 | 场景 | 断言 |
|:-:|------|------|------|
| 12 | `test_circuit_breaker_reentrant` | 连续两次 process() | 状态通过 AccessPoint 读写，无跨 process 隐式状态 |
| 13 | `test_circuit_breaker_does_not_record` | process() 后检查 state | state 的 failure_count 未改变（record 在 Slot 中） |
| 14 | `test_orchestrator_serial_order` | 注册 3 个组件 | 按 priority 10→20→30 顺序执行 |

**S-R01 完整性**（§10.5）：
| # | 名称 | 场景 | 断言 |
|:-:|------|------|------|
| 15 | `test_all_paths_return_continue` | 所有异常路径 | 返回 Continue（当前版本不返回 AbortStep） |

**验收**：`cargo test` 全部通过，无 `#[ignore]`。

---

### 步骤 9：终态自检

```bash
cargo test
cargo clippy -- -D warnings
# 常量检查
rg 'DEFAULT_TOOL_TIMEOUT_SECS|DEFAULT_CIRCUIT_BREAKER_THRESHOLD' src/plugins/slots/tool_executor/
# record_success/record_failure 位置检查（必须在 plugin.rs，不在 component 中）
rg 'record_success|record_failure' src/plugins/slots/tool_executor/ --type rust
# 无 unwrap/panic（测试除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/tool_executor/ --type rust --glob '!*test*'
# 无 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/tool_executor/ --type rust --glob '!*test*'
```

**合规清单**（设计文档 §9）：

| 检查项 | 状态 |
|--------|:----:|
| 4 个 DEFAULT_* 常量 | ✅ |
| S-R01：所有路径返回 Continue（AbortStep 预留） | ✅ |
| S-R02：init 失败返回 Err | ✅ |
| S-R03：熔断状态存入 StepContext | ✅ |
| C-R01：call() + downcast | ✅ 适用 |
| C-R02：requires 真实可验证（tool_executor 所有组件 requires=[]） | ✅ |
| C-R03：process() 可重入 | ✅ |
| record_success/record_failure 在 Slot::run() 中（不在组件中） | ✅ |
| mod.rs 只暴露三样东西 | ✅ |

---

## 持续纪律

- 每轮对话发送完整宪法
- 拒绝包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断改正
- 每步人工校验不得跳过
- `record_success`/`record_failure` 的位置是最容易出错的设计约束，必须重点审查
