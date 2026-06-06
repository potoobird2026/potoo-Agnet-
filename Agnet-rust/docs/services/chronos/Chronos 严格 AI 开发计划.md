# Chronos（自适应定时调度服务）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/chronos/chronos开发文档.md` 生成 chronos 模块的全部代码。

---

## 项目背景

- **模块名称**：chronos（自适应定时调度服务）
- **模块定位**：后台常驻定时调度服务，根据用户状态（时间阶段、空闲等级）动态调整轮询间隔，支持任务队列管理、规则决策、动作执行和反馈学习。
- **外部接口**：
  - `ChronosServicePlugin` — ServicePlugin 入口
  - `ChronosConfig` — 配置
  - `ChronosError` — 错误类型
- **内部结构**：9 个 Component（P0: 6 + P1: 2 + P2: 1），由 `ChronosOrchestrator` 编排，所有组件的 `process()` 为 no-op，业务逻辑由主循环直接调用各组件业务方法
- **依赖项**：`tokio`、`serde`、`serde_json`、`tracing`、`async-trait`、`chrono`、`uuid`

---

## 硬编码分类定义（chronos 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 轮询间隔 | `5` 秒 | 从 `TimingConfig.polling_interval_base_secs` 读取 |
| 自适应系数 | `1.5` | 从 `TimingConfig.idle_multiplier` 读取 |
| 生成超时 | `30` 秒 | 从 `DecisionConfig.generation_timeout_secs` 读取 |
| 升级超时 | `120` 秒 | 从 `DecisionConfig.escalation.timeout_secs` 读取 |
| 模型名 | `"claude-3-haiku"` | 从 `DecisionConfig.generation_llm_model` 读取 |
| 提示模板 | `"You have pending tasks..."` | 从 `DecisionConfig.remind_template` 读取 |
| 最大样本保留 | `1000` | 从 `SampleStoreConfig.max_samples` 读取 |
| 文件路径 | `"~/.aagnet/chronos/tasks.json"` | 从 `StorageConfig` 用 `dirs::home_dir()` + `join()` 构建，`resolve_paths()` 展开 `~` |

---

## 项目目录结构

```
src/plugins/services/chronos/
├── mod.rs                  # 模块入口：ChronosServicePlugin / ChronosConfig / ChronosError
├── config.rs               # ChronosConfig + TimingConfig + DecisionConfig + StateConfig + StorageConfig + ...
├── service.rs              # ChronosServicePlugin（ServicePlugin 实现）
├── orchestrator.rs         # ChronosOrchestrator（register/sort/init_all/process_all/shutdown_all）
├── error.rs                # ChronosError
├── components/
│   ├── mod.rs              # 子模块声明
│   ├── timer.rs            # AdaptiveTimerComponent + AdaptiveTimerService trait
│   ├── task_queue.rs       # TaskQueueComponent + TaskQueueService trait
│   ├── state_encoder.rs    # StateEncoderComponent + StateEncoderService trait
│   ├── rule_engine.rs      # RuleEngineComponent + RuleEngineService trait
│   ├── decision_engine.rs  # DecisionEngineComponent + DecisionEngineService trait
│   ├── action_executor.rs  # ActionExecutorComponent + ActionExecutor trait
│   ├── feedback.rs         # FeedbackEngineComponent + FeedbackService trait
│   ├── sample_store.rs     # SampleStoreComponent + SampleStore trait
│   └── tool_bridge.rs      # ToolBridgeComponent + ToolBridge trait
└── types.rs                # ScheduledTask / StateSnapshot / Decision / RuleDecision / Action / ...
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 chronos开发文档.md 一致。

2. **零幻觉**：
   a. Chronos 只有 9 个组件（AdaptiveTimer/TaskQueue/StateEncoder/RuleEngine/DecisionEngine/ActionExecutor/FeedbackEngine/SampleStore/ToolBridge），不存在额外组件。
   b. 所有组件的 process() 都是 no-op（主循环驱动业务逻辑）。
   c. 不存在虚构的“Slot”或“Pipeline”概念——Chronos 是 Service，不是 Slot。

3. **零硬编码**：
   a. 所有定时参数（轮询间隔、空闲系数、升级超时）从 TimimgConfig/DecisionConfig 读取。
   b. 模型名从 DecisionConfig.generation_llm_model 读取。
   c. 提示模板从 DecisionConfig.remind_template/proactive_template 读取。
   d. 文件路径通过 StoragetConfig 用 dirs::home_dir() + PathBuf::join() 构建。
   e. ~ 展开在 ChronosConfig::resolve_paths() 中处理。

4. **完整实现**：9 个 Component 全部实现 Component trait（含 init/process/shutdown）+ 业务接口 trait。

5. **错误处理**：
   - 任务队列持久化失败记录 warn 不影响启动。
   - 决策中的 LLM 调用超时视为跳过 LLM 路径（回退规则决策）。
   - 单个任务执行失败不影响主循环。

6. **测试同步生成**：
   - 每个 Component 的 init/process/shutdown 生命周期测试。
   - 业务接口：Timer 间隔计算、TaskQueue CRUD/持久化、StateEncoder 编码、Rule 决策、Decision 决策（超时/升级）、Action 执行、Feedback 处理、Sample 持久化。
   - 主循环集成测试：单 tick 流程覆盖。
   - Orchestrator：注册/排序/init_all/process_all/shutdown_all 顺序。
   - 配置：resolve_paths() 展开/validate() 校验。
```

---

## 详细开发步骤

### 步骤 0：确认骨架

**操作**：创建目录和全部文件骨架。确认 Cargo.toml 依赖。

**验收**：`cargo check` 通过

---

### 步骤 1：Config 层（config.rs）

| 结构体 | 关键字段 |
|--------|---------|
| `ChronosConfig` | timing, decision, state, storage, actions, preferences, max_polling_interval_secs(300), resolve_paths() |
| `TimingConfig` | polling_interval_base_secs(5), idle_multiplier(1.5), active_multiplier(0.5), max_interval_secs(300), min_interval_secs(1) |
| `DecisionConfig` | generation_llm_model("gpt-4o-mini"), generation_timeout_secs(30), remind_template, proactive_template, escalation |
| `StateConfig` | idle_threshold_minutes(5), active_threshold_secs(30) |
| `StorageConfig` | task_queue_file, sample_store_dir, max_samples(1000), base_dir |
| `ActionsConfig` | max_concurrent_actions(5), action_timeout_secs(60) |

`ChronosConfig::resolve_paths()` 展开 `~` 并处理所有子路径。
`ChronosConfig::validate()`：`polling_interval_base_secs > 0`、`max_polling_interval_secs` 合理。

### 步骤 2：Types 层（types.rs）

```rust
pub struct StateSnapshot { pub time_category: TimeCategory, pub idle_level: IdleLevel, pub pending_task_count: usize, pub urgent_count: usize, pub last_interaction_age: Duration }
pub enum TimeCategory { Morning, Afternoon, Evening, Night }
pub enum IdleLevel { Active, Normal, Idle, Dormant }
pub struct ScheduledTask { pub id: String, pub task_type: TaskType, pub scheduled_at: DateTime<Utc>, pub payload: Value, pub status: TaskStatus, pub retry_count: u8 }
pub enum TaskStatus { Pending, Running, Completed, Failed, Cancelled }
pub enum TaskType { Reminder, Maintenance, ProactiveAction }
pub enum Decision { Execute { actions: Vec<Action> }, Skip { reason: String }, Escalate { reason: String, timeout: Duration } }
pub enum RuleDecision { Execute, Skip, Escalate, None }
pub struct Action { pub action_type: ActionType, pub payload: Value, pub priority: u8 }
pub struct FeedbackSignal { pub action_id: String, pub feedback_type: FeedbackType, pub timestamp: DateTime<Utc> }
pub enum FeedbackType { Positive, Negative, Neutral }
```

### 步骤 3：Orchestrator（orchestrator.rs）

```rust
pub struct ChronosOrchestrator { entries: Vec<ComponentEntry> }
// new(), register(component, priority), sort()（按 priority 升序）
// init_all() → 依次 init
// process_all() → 依次 process（当前全部 no-op → Continue）
// shutdown_all() → 逆序 shutdown
```

### 步骤 4：Component 实现（9 个，按优先级分组）

**组 A — 优先级 10（P0，无依赖）**，每个实现 Component trait + 业务接口 trait：

| 组件 | 业务接口 | 关键方法 | 配置 |
|------|---------|---------|------|
| AdaptiveTimerComponent | `AdaptiveTimerService` | `calculate_interval(snapshot, is_urgent) -> Duration` | TimingConfig |
| TaskQueueComponent | `TaskQueueService` | `add_task/pop_due/complete/pending_count/save/load` | StorageConfig |
| StateEncoderComponent | `StateEncoderService` | `encode(last_interaction, pending, urgent) -> StateSnapshot` | StateConfig |
| FeedbackEngineComponent | `FeedbackService` | `process_feedback(signal)` / `get_feedback_stats()` | — |
| SampleStoreComponent | `SampleStoreService` | `store_sample/query_samples/cleanup` | SampleStoreConfig |
| ToolBridgeComponent | `ToolBridgeService` | `execute_tool(name, args) -> Result<Value>` | — |

**组 B — 优先级 20（依赖 state_encoding）**：

| 组件 | 业务接口 | requires | 关键方法 |
|------|---------|---------|---------|
| RuleEngineComponent | `RuleEngineService` | state_encoding | `decide(snapshot) -> RuleDecision` |
| DecisionEngineComponent | `DecisionEngineService` | state_encoding, rule_decision | `decide(snapshot, task_queue, rule_decision) -> Decision` |

**组 C — 优先级 30**：

| 组件 | 业务接口 | requires | 关键方法 |
|------|---------|---------|---------|
| ActionExecutorComponent | `ActionExecutorService` | decision, task_queue | `execute(decision, task_queue)` |

所有组件的 `process()` 均为 `Ok(Processing::Continue)`。

### 步骤 5：ChronosServicePlugin（service.rs）

```rust
pub struct ChronosServicePlugin { inner: Arc<RwLock<Option<ChronosInner>>> }
struct ChronosInner { config, orchestrator, task_queue, state_encoder, timer, last_interaction_at, running, suspended }
```

| 方法 | 行为 |
|------|------|
| `init()` | 解析 ChronosConfig → validate() → resolve_paths() → 创建 9 个 Component → Orchestrator.register() → sort() |
| `start()` | running=true → task_queue.load() → ap.register_provider("chronos", ...) → tokio::spawn(run_loop) |
| `handle_signal()` | 6 种信号（HealthCheck/ConfigReload/Suspend/Resume/GracefulShutdown/ImmediateShutdown） |
| `stop()` | running=false |
| `shutdown()` | task_queue.save() → clear inner |

主循环（每秒 tick）：
```
1. 检查 running / suspended
2. StateEncoder.encode(last_interaction, pending, urgent)
3. AdaptiveTimer.calculate_interval(snapshot, is_urgent)
4. RuleEngine.decide(snapshot)
5. DecisionEngine.decide(snapshot, task_queue, rule_decision)
6. ActionExecutor.execute(decision, task_queue)
7. FeedbackEngine.process_feedback()
```

### 步骤 6：mod.rs

```
pub use service::ChronosServicePlugin;
pub use config::ChronosConfig;
pub use error::ChronosError;
```

### 步骤 7：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. 对照 chronos开发文档.md §7.4 的 10 项自查清单
3. 9 个 Component 的 init/process/shutdown 生命周期完整
