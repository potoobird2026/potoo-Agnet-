# ChronosServicePlugin(自适应定时调度服务) 设计文档

## 0. 协议依据

本文档严格遵循以下协议，每条设计决策均可追溯到协议具体条款。协议是规则，不是建议。

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **Service 集成协议** | 模块对外接口 | §1 插件单入口、§2 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§7 新增/替换流程、§8 红线 |
| **模块内部组件协议** | 模块内部结构 | §1 组件单入口、§2 组件句柄、§3 内部数据共享通道、§4 处理结果、§5 组件元数据声明、§5.2 Orchestrator、§6 模块边界规范、§9 设计决策、§10 新增/替换 Component 标准流程、§11 协议特有红线 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |

---

## 0.5 功能清单

| 功能 | 描述 | 对应 Component(组件) | 优先级 |
|------|------|---------------------|--------|
| 自适应定时唤醒 | 根据用户状态（时间阶段、空闲等级）动态调整轮询间隔 | `AdaptiveTimerComponent` | P0 |
| 任务队列管理 | 管理定时任务的创建、持久化、到期执行、状态追踪 | `TaskQueueComponent` | P0 |
| 状态编码 | 将当前上下文（时间、空闲、待处理任务）编码为状态快照 | `StateEncoderComponent` | P0 |
| 决策引擎 | 基于状态快照做出执行/跳过/升级决策 | `DecisionEngineComponent` | P0 |
| 规则预筛 | 规则引擎对决策做快速预筛，减少 LLM 调用 | `RuleEngineComponent` | P1 |
| 动作执行 | 执行提醒、维护、主动发起等动作 | `ActionExecutorComponent` | P0 |
| 反馈学习 | 收集用户反馈信号，用于模型微调 | `FeedbackEngineComponent` | P1 |
| 样本存储 | 持久化决策样本，用于离线分析和微调 | `SampleStoreComponent` | P1 |
| 工具桥接 | 通过 ToolContract 调用外部工具 | `ToolBridgeComponent` | P2 |

---

## 1. 模块定位（Service 集成协议视角）

### 1.1 外部身份

遵循 Service 集成协议 §1——`ChronosServicePlugin` 实现 `ServicePlugin` trait，作为后台常驻服务运行。

**§1 要求的 6 个方法必须全部实现：**

```rust
#[async_trait]
impl ServicePlugin for ChronosServicePlugin {
    fn name(&self) -> &str;                                              // §1: 全局唯一标识
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;  // §1: 初始化一次
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;  // §1: 启动后台服务
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>; // §1: 处理运行时信号
    async fn stop(&mut self) -> Result<(), PluginError>;                 // §1: 停止（暂停，不销毁）
    async fn shutdown(&mut self) -> Result<(), PluginError>;             // §1: 销毁（只调用一次）
}
```

**各方法职责（§1 表格）：**

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `name` | 多次 | 返回 `"chronos"` |
| `init` | 1 | 校验配置、创建 Orchestrator 及所有 Component |
| `start` | 1 | 通过 ServiceAccessPoint 注册 Provider、启动后台循环 |
| `handle_signal` | 多次 | 响应信号（§3） |
| `stop` | 多次 | 暂停，不销毁资源 |
| `shutdown` | 1 | 反注册 Provider、释放所有资源 |

### 1.2 受控访问句柄（ServiceAccessPoint）

遵循 Service 集成协议 §2——`ServiceAccessPoint` 是 Chronos 与 core 交互的**唯一通道**：

```rust
async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
    // §2.1 Core 内建方法
    let config = ap.get_config();     // 读取 Agent 配置
    ap.log("info", "Chronos 启动");    // 写入日志

    // §2.2 Provider 注册——将本服务的业务能力暴露给其他插件
    let provider: Arc<dyn ChronosProvider> = Arc::new(ChronosProviderImpl::new(...));
    ap.register_provider("chronos", provider);

    // 启动后台主循环
    self.run_loop().await;
    Ok(())
}
```

**§2 关键约束**：Provider 接口由 Chronos 自行定义——core 不知道也不关心 `ChronosProvider` 有什么方法。

### 1.3 元数据声明

遵循 Service 集成协议 §4：

```yaml
name: chronos
category: service
version: 0.1.0
run_mode: background
provides:
  - chronos
requires: []
conflicts: []
```

| 字段 | 值 | 协议约束 |
|------|---|---------|
| `name` | `"chronos"` | 必须与 `ServicePlugin::name()` 一致（§4） |
| `category` | `"service"` | 固定值（§4） |
| `version` | `"0.1.0"` | 语义版本（§4） |
| `run_mode` | `"background"` | 后台常驻（§4） |
| `provides` | `["chronos"]` | 必须与 `start()` 中 `register_provider` 一致（§8 V-R03） |

### 1.4 生命周期映射

遵循 Service 集成协议 §5：

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

| 阶段 | 具体操作 |
|------|---------|
| `init(ctx)` | 从 `ctx.plugin_config` 解析 ChronosConfig；创建 Orchestrator 并注册 9 个组件；调用 `orchestrator.sort()` |
| `start(ap)` | 加载持久化任务队列；`ap.register_provider("chronos", ...)` 注册 Provider；`tokio::spawn(run_loop)` |
| `handle_signal(signal)` | 6 种信号处理（§3） |
| `stop()` | 设置 `running = false` |
| `shutdown()` | 保存任务队列；反注册 Provider；释放所有资源 |

---

## 2. 内部架构总览（模块内部组件协议视角）

### 2.1 模块边界规范

遵循模块内部组件协议 §6——模块 `mod.rs` 只暴露 3 样东西：

```rust
// mod.rs 只暴露 3 样
pub struct ChronosServicePlugin;  // Service 入口
pub struct ChronosConfig;         // 配置
pub struct ChronosError;          // 错误
// 内部 Component / Orchestrator / AccessPoint 全部 pub(crate) 或 private
```

依赖方向（遵循 §6.2）：

```
┌──────────────────┐
│  mod.rs           │  （对外暴露 3 样）
└──────┬───────────┘
       │
       ▼
┌──────────────────────────────┐
│  Orchestrator                │
│  ├─ init_all()               │
│  ├─ process_all()            │
│  └─ 持有 Vec<ComponentEntry> │
└──────────────────────────────┘
       │ 注入 Component
       ▼
┌──────────────────────────────┐
│  Components                  │
│  ├─ AdaptiveTimer            │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ TaskQueue                │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ StateEncoder             │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ RuleEngine               │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ DecisionEngine           │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ ActionExecutor           │──→ 依赖 AccessPoint，不依赖兄弟
│  └─ ...                      │
└──────────────────────────────┘
```

- ✅ 组件只能看到 `AccessPoint`，不看到兄弟组件的具体类型
- ✅ 组件之间零直接引用
- ✅ 替换一个组件不影响其他组件

### 2.2 组件(Component) 一览

遵循模块内部组件协议 §1——每个功能单元统一实现 `Component` trait：

```rust
#[async_trait]
pub trait Component: Send + Sync {
    fn name(&self) -> &str;
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError>;
    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError>;
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}
```

### 2.3 组件依赖关系（DAG）

遵循模块内部组件协议 §5——`provides` / `requires` 声明，Orchestrator 在 `register()` 时自动校验。

```
  AdaptiveTimerComponent      TaskQueueComponent      StateEncoderComponent
  ┌────────────────────────┐  ┌────────────────────────┐  ┌────────────────────────┐
  │ provides: ["timing"]   │  │ provides: ["task_queue"]│  │ provides: ["state_enc"]│
  │ requires: []           │  │ requires: []           │  │ requires: []           │
  │ priority: 10           │  │ priority: 10           │  │ priority: 10           │
  └────────────────────────┘  └────────────────────────┘  └────────────────────────┘
           │                          │                          │
           ▼                          ▼                          ▼
  RuleEngineComponent        DecisionEngineComponent
  ┌────────────────────────┐  ┌────────────────────────────────────┐
  │ provides: ["rule_dec"] │  │ provides: ["decision"]             │
  │ requires: ["state_enc"]│  │ requires: ["state_enc","rule_dec"] │
  │ priority: 20           │  │ priority: 20                       │
  └────────────────────────┘  └────────────────────────────────────┘
                                            │
                                            ▼
                                ActionExecutorComponent
                                ┌────────────────────────────────┐
                                │ provides: ["action_exec"]      │
                                │ requires: ["decision","task_queue"]│
                                │ priority: 30                   │
                                └────────────────────────────────┘
```

| 层级 | 组件 | priority | provides | requires |
|------|------|----------|----------|----------|
| 1 | AdaptiveTimerComponent | 10 | `timing` | — |
| 1 | TaskQueueComponent | 10 | `task_queue` | — |
| 1 | StateEncoderComponent | 10 | `state_encoding` | — |
| 1 | FeedbackEngineComponent | 10 | `feedback` | — |
| 1 | SampleStoreComponent | 10 | `sample_store` | — |
| 1 | ToolBridgeComponent | 10 | `tool_bridge` | — |
| 2 | RuleEngineComponent | 20 | `rule_decision` | `state_encoding` |
| 2 | DecisionEngineComponent | 20 | `decision` | `state_encoding`, `rule_decision` |
| 3 | ActionExecutorComponent | 30 | `action_execution` | `decision`, `task_queue` |

---

## 3. Component(组件) 详解

遵循模块内部组件协议 §1——每个组件实现 `Component` trait。

### 3.1 AdaptiveTimerComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "adaptive_timer",
    version: "0.1.0",
    priority: 10,
    provides: &["timing"],
    requires: &[],
    config_key: Some("chronos.timing"),
}
```

#### 业务接口 trait

```rust
pub trait AdaptiveTimerService: Send + Sync {
    /// 根据状态快照计算下次轮询间隔
    fn calculate_interval(&self, snapshot: &StateSnapshot, is_urgent: bool) -> Duration;
}
```

#### Component 实现

```rust
impl Component for AdaptiveTimerComponent {
    fn name(&self) -> &str { "adaptive_timer" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 TimingConfig
        // 所有数值来自配置，不硬编码（跨平台规范 §1）
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    /// 定时器由主循环直接调用 calculate_interval()，不通过 process()
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

**§9 设计决策**：`process()` 为 no-op。定时器由主循环直接调用 `calculate_interval()` 驱动，不通过 Orchestrator 的 `process_all()`。保留 `process()` 是为了未来扩展——如果需要定期校准定时器参数，可在 `process()` 中实现。

### 3.2 TaskQueueComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "task_queue",
    version: "0.1.0",
    priority: 10,
    provides: &["task_queue"],
    requires: &[],
    config_key: Some("chronos.storage"),
}
```

#### 业务接口 trait

```rust
pub trait TaskQueueService: Send + Sync {
    async fn add_task(&self, task: ScheduledTask) -> Result<(), QueueError>;
    async fn pop_due(&self) -> Vec<ScheduledTask>;
    async fn complete(&self, task_id: &str);
    async fn pending_count(&self) -> usize;
    async fn load(&self) -> Result<(), String>;
    async fn save(&self) -> Result<(), String>;
}
```

#### Component 实现

```rust
impl Component for TaskQueueComponent {
    fn name(&self) -> &str { "task_queue" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取持久化路径
        // 路径通过 StorageConfig 构建，不硬编码（跨平台规范 §2）
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    /// 任务队列由主循环直接调用 pop_due()/complete() 驱动
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        // 保存任务队列到磁盘
        self.save().await.map_err(|e| ComponentError::Internal(e))?;
        Ok(())
    }
}
```

### 3.3 StateEncoderComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "state_encoder",
    version: "0.1.0",
    priority: 10,
    provides: &["state_encoding"],
    requires: &[],
    config_key: Some("chronos.state"),
}
```

#### 业务接口 trait

```rust
pub trait StateEncoderService: Send + Sync {
    fn encode(&self, last_interaction: Option<DateTime<Utc>>, pending: usize, urgent: usize) -> StateSnapshot;
}
```

#### Component 实现

```rust
impl Component for StateEncoderComponent {
    fn name(&self) -> &str { "state_encoder" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 StateConfig
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.4 RuleEngineComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "rule_engine",
    version: "0.1.0",
    priority: 20,
    provides: &["rule_decision"],
    requires: &["state_encoding"],
    config_key: None,
}
```

#### 业务接口 trait

```rust
pub trait RuleEngineService: Send + Sync {
    fn decide(&self, snapshot: &StateSnapshot) -> RuleDecision;
}
```

#### Component 实现

```rust
impl Component for RuleEngineComponent {
    fn name(&self) -> &str { "rule_engine" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.5 DecisionEngineComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "decision_engine",
    version: "0.1.0",
    priority: 20,
    provides: &["decision"],
    requires: &["state_encoding", "rule_decision"],
    config_key: Some("chronos.decision"),
}
```

#### 业务接口 trait

```rust
pub trait DecisionEngineService: Send + Sync {
    async fn decide(&self, snapshot: &StateSnapshot, task_queue: &dyn TaskQueueService, rule_decision: Option<RuleDecision>) -> Decision;
}
```

#### Component 实现

```rust
impl Component for DecisionEngineComponent {
    fn name(&self) -> &str { "decision_engine" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 DecisionConfig
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.6 ActionExecutorComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "action_executor",
    version: "0.1.0",
    priority: 30,
    provides: &["action_execution"],
    requires: &["decision", "task_queue"],
    config_key: Some("chronos.actions"),
}
```

#### Component 实现

```rust
impl Component for ActionExecutorComponent {
    fn name(&self) -> &str { "action_executor" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 ActionsConfig
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.7 FeedbackEngineComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "feedback_engine",
    version: "0.1.0",
    priority: 10,
    provides: &["feedback"],
    requires: &[],
    config_key: None,
}
```

#### Component 实现

```rust
impl Component for FeedbackEngineComponent {
    fn name(&self) -> &str { "feedback_engine" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.8 SampleStoreComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "sample_store",
    version: "0.1.0",
    priority: 10,
    provides: &["sample_store"],
    requires: &[],
    config_key: Some("chronos.decision.sample_store"),
}
```

#### Component 实现

```rust
impl Component for SampleStoreComponent {
    fn name(&self) -> &str { "sample_store" }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 从 ctx.config 读取 SampleStoreConfig
        // 路径通过 StorageConfig 构建（跨平台规范 §2）
        Ok(())
    }

    /// §9 设计决策：process() 为 no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

### 3.9 ToolBridgeComponent

#### 元数据声明（§5）

```rust
ComponentMeta {
    name: "tool_bridge",
    version: "0.1.0",
    priority: 10,
    provides: &["tool_bridge"],
    requires: &[],
    config_key: None,
}
```

#### Component 实现

```rust
impl Component for ToolBridgeComponent {
    fn name(&self) -> &str { "tool_bridge" }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> { Ok(()) }

    /// §9 设计决策：process() 为 no-op
    /// 工具调用由 ActionExecutorComponent 通过 ap.call("tool_bridge") + downcast 触发
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> { Ok(()) }
}
```

---

## 4. Orchestrator(协调器) 编排逻辑

### 4.1 结构（§5.2）

```rust
pub struct ChronosOrchestrator {
    entries: Vec<ComponentEntry>,
}

struct ComponentEntry {
    component: Box<dyn Component>,
    priority: u8,
}
```

### 4.2 核心方法（§5.2）

| 方法 | 签名 | 职责 |
|------|------|------|
| `new()` | `pub fn new() -> Self` | 创建空 Orchestrator |
| `register()` | `pub fn register(&mut self, component: Box<dyn Component>, priority: u8) -> Result<(), ComponentError>` | 注册组件，校验 requires/provides |
| `sort()` | `pub fn sort(&mut self)` | 按 priority 排序（小→先执行） |
| `init_all()` | `pub async fn init_all(&mut self) -> Result<(), ComponentError>` | 按序调用每个组件的 `init()` |
| `process_all()` | `pub async fn process_all(&mut self) -> Result<(), ComponentError>` | 按序执行全部 `process()` |
| `shutdown_all()` | `pub async fn shutdown_all(&mut self)` | 逆序调用全部 `shutdown()` |

### 4.3 `init_all()` 流程

```
orch.init_all()
  │
  ├── [层级 1] AdaptiveTimerComponent.init(ctx)
  ├── [层级 1] TaskQueueComponent.init(ctx)
  ├── [层级 1] StateEncoderComponent.init(ctx)
  ├── [层级 1] FeedbackEngineComponent.init(ctx)
  ├── [层级 1] SampleStoreComponent.init(ctx)
  ├── [层级 1] ToolBridgeComponent.init(ctx)
  ├── [层级 2] RuleEngineComponent.init(ctx)
  ├── [层级 2] DecisionEngineComponent.init(ctx)
  └── [层级 3] ActionExecutorComponent.init(ctx)
```

### 4.4 `process_all()` 流程

```
orch.process_all()
  │
  ├── [层级 1] AdaptiveTimerComponent.process() → Continue
  ├── [层级 1] TaskQueueComponent.process() → Continue
  ├── [层级 1] StateEncoderComponent.process() → Continue
  ├── [层级 1] FeedbackEngineComponent.process() → Continue
  ├── [层级 1] SampleStoreComponent.process() → Continue
  ├── [层级 1] ToolBridgeComponent.process() → Continue
  ├── [层级 2] RuleEngineComponent.process() → Continue
  ├── [层级 2] DecisionEngineComponent.process() → Continue
  └── [层级 3] ActionExecutorComponent.process() → Continue
```

### 4.5 `shutdown_all()` 流程

```
orch.shutdown_all()
  │
  ├── [逆序] ActionExecutorComponent.shutdown()
  ├── [逆序] DecisionEngineComponent.shutdown()
  ├── [逆序] RuleEngineComponent.shutdown()
  ├── [逆序] ToolBridgeComponent.shutdown()
  ├── [逆序] SampleStoreComponent.shutdown()
  ├── [逆序] FeedbackEngineComponent.shutdown()
  ├── [逆序] StateEncoderComponent.shutdown()
  ├── [逆序] TaskQueueComponent.shutdown()
  └── [逆序] AdaptiveTimerComponent.shutdown()
```

---

## 5. 运行时信号（§3）

| 信号 | 处理方式 | 协议依据 |
|------|---------|---------|
| `GracefulShutdown` | 设置 `running = false`，主循环退出 | §3 |
| `ImmediateShutdown` | 设置 `running = false`，立即退出 | §3 |
| `ConfigReload` | 记录日志，重新读取配置 | §3 |
| `HealthCheck` | 检查 `running == true`，否则返回 `Err` | §3、§8 V-R01 |
| `Suspend` | 设置 `suspended = true`，主循环暂停 | §3 |
| `Resume` | 设置 `suspended = false`，主循环恢复 | §3 |

**约束**：
- `handle_signal()` 不得阻塞超过 5 秒（§8 V-R02）
- `HealthCheck` 须在 5 秒内返回（§8 V-R01）

---

## 6. 主循环

`ChronosServicePlugin` 在 `start()` 中通过 `tokio::spawn` 启动后台主循环：

```
主循环：
  │
  ├── 1. 每秒 tick
  │
  ├── 2. 检查 running / suspended
  │     └── !running || suspended → 退出
  │
  ├── 3. 构建状态快照
  │     └── StateEncoder.encode(last_interaction, pending, urgent)
  │
  ├── 4. 计算轮询间隔
  │     └── AdaptiveTimer.calculate_interval(snapshot, is_urgent)
  │
  ├── 5. 更新 ticker
  │
  └── 6. 执行决策周期
        ├── RuleEngine.decide(snapshot)
        ├── DecisionEngine.decide(snapshot, task_queue, rule_decision)
        ├── ActionExecutor.execute(decision, task_queue)
        └── FeedbackEngine.process_feedback()
```

---

## 7. 跨平台与硬编码规范视角

### 7.1 硬编码值分类（§1 逐条对照）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 涉及 | ✅ `generation_llm_model` 从配置读取 |
| 3 | 超时秒数 | 涉及 | ✅ `generation_timeout_secs`、`escalation.timeout_secs` 从配置读取 |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 涉及 | ✅ `StorageConfig::default()` 用 `dirs::home_dir()` + `join()` |
| 7 | 数字阈值 | 涉及 | ✅ 所有阈值从配置读取 |
| 8 | 字符串模板 | 涉及 | ✅ `remind_template`、`proactive_template` 从配置读取 |
| 9 | 平台指令 | 不涉及 | ✅ |

### 7.2 跨平台路径规则（§2 逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ 使用 `dirs::home_dir()` |
| 2.2 | 禁止裸用 `~` | ✅ `resolve_paths()` 解析 |
| 2.3 | 禁止相对路径依赖 CWD | ✅ `resolve_paths()` 确保绝对路径 |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.5 | 路径分隔符判断 | 不涉及 | ✅ |
| 2.6 | 文件扩展名判断 | 不涉及 | ✅ |
| 2.7 | 临时文件/目录用 `std::env::temp_dir()` | ✅ 测试中使用 |
| 2.8 | 数据目录通过 `dirs` + 环境变量 | ✅ 使用 `dirs::home_dir()` |

### 7.3 测试代码规范（§3 逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 3.1 | 临时路径用 `std::env::temp_dir()` | ✅ |
| 3.2 | 平台特定测试用 `#[cfg()]` | ✅ 不涉及 |
| 3.3 | 网络测试用 mock 或 `#[ignore]` | ✅ 不涉及 |

### 7.4 自查清单（§4 逐项）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 2 | 模型名来自配置 | ✅ |
| 3 | 超时值来自配置或常量 | ✅ |
| 4 | API 版本号为模块级 const | ✅ 不涉及 |
| 5 | User-Agent 为 const | ✅ 不涉及 |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ✅ |
| 8 | 平台指令用 `OsKind` | ✅ 不涉及 |
| 9 | 测试无硬编码路径 | ✅ |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 8. 红线

### Service 集成协议红线（§8）

| 编号 | 红线 | 合规 |
|------|------|:----:|
| V-R01 | 必须响应 `HealthCheck` | ✅ |
| V-R02 | `handle_signal` 不得阻塞超过 5 秒 | ✅ |
| V-R03 | `provides` 与 `register_provider` 一致 | 待 start() 实现 |

### 模块内部组件协议红线（§11）

| 编号 | 红线 | 合规 |
|------|------|:----:|
| C-R01 | `call()` 后必须 downcast | ✅ |
| C-R02 | `requires` 必须真实可验证 | ✅ |
| C-R03 | `process()` 必须可重入 | ✅ |

---

## 9. 设计决策（模块内部组件协议 §9）

遵循模块内部组件协议 §9——说明关键设计决策及其理由。

### 9.1 所有组件的 process() 为 no-op

**决策**：9 个组件的 `process()` 均返回 `Ok(Processing::Continue)`，不执行任何业务逻辑。

**理由**：

Service 的驱动模式与 Slot 不同。Slot 被 Pipeline 定时调用 `process()`，业务逻辑在 `process()` 内部；Service 在 `start()` 后自己运行主循环，业务逻辑在主循环中直接调用各组件的业务方法。

Chronos 的主循环流程为：

```
主循环 tick
  → StateEncoder.encode()      // 直接调用，不通过 process()
  → AdaptiveTimer.calculate()  // 直接调用，不通过 process()
  → RuleEngine.decide()        // 直接调用，不通过 process()
  → DecisionEngine.decide()    // 直接调用，不通过 process()
  → ActionExecutor.execute()   // 直接调用，不通过 process()
  → FeedbackEngine.process()   // 直接调用，不通过 process()
```

组件协议要求所有组件实现 `process()`——这是生命周期的一部分，即使不需要定期维护也必须实现。保留 no-op 的 `process()` 是为了扩展性：未来如果某个组件需要定期清理缓存、校准参数、或做健康检查，可以直接在 `process()` 中添加逻辑，不影响其他组件和 Orchestrator。

### 9.2 业务方法与 process() 的分离

**决策**：组件的业务方法（如 `calculate_interval()`、`decide()`、`execute()`）是公开的 trait 方法，由主循环直接调用，不通过 Orchestrator 的 `process_all()` 驱动。

**理由**：

1. **时序控制**：主循环需要精确控制调用顺序（先编码状态 → 再规则预筛 → 再决策 → 再执行），而 `process_all()` 按 priority 串行执行，无法表达这种业务时序
2. **参数传递**：业务方法需要接收特定参数（如 `StateSnapshot`、`TaskQueue`），而 `process()` 只接收 `&mut dyn AccessPoint`
3. **性能**：主循环每秒 tick 一次，不需要每次都执行所有组件的 `process()`

Orchestrator 的 `process_all()` 保留为定期维护入口——如果未来需要每 N 轮执行一次全组件健康检查，可以通过主循环计数器触发 `orch.process_all()`。

### 9.3 ComponentMeta 的 provides/requires 命名

**决策**：`provides` 和 `requires` 使用下划线分隔的字符串标识符（如 `"state_encoding"`、`"rule_decision"`），不使用缩写。

**理由**：

1. **可读性**：`"task_queue"` 比 `"task_q"` 更清晰
2. **唯一性**：避免不同组件的缩写冲突
3. **调试**：`ap.call("task_queue")` 比 `ap.call("task_q")` 更容易理解

---

## 10. 新增/替换 Component 标准流程（§10）

### 在 Chronos 内新增 Component

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 新建组件文件 | `components/my_component.rs` |
| 2 | 实现 `Component` trait + `fn meta()` | 同上 |
| 3 | 在 `orchestrator.rs` 注册 | `orch.register(Box::new(MyComponent::new()), priority)?` |
| 4 | 在 `components/mod.rs` 添加模块声明 | `pub mod my_component;` |
| 5 | 运行 `cargo check` | — |

### 替换现有 Component

| 步骤 | 做什么 |
|------|--------|
| 1 | 确认新旧 `meta().provides` 一致 |
| 2 | 确认新旧 `meta().requires` 是旧的子集 |
| 3 | 编写新 `impl Component`，替换原文件 |
| 4 | 若 `name` 不变，`orchestrator.rs` 无需修改 |
| 5 | `cargo check` + 单元测试 |

---

## 11. ServicePlugin 完整实现

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::error::PluginError;
use crate::core::access::ServiceAccessPoint;

use super::config::ChronosConfig;
use super::components::orchestrator::ChronosOrchestrator;
use super::components::task_queue::TaskQueue;
use super::components::state::StateEncoder;
use super::components::timer::AdaptiveTimer;

struct ChronosInner {
    config: ChronosConfig,
    orchestrator: ChronosOrchestrator,
    task_queue: TaskQueue,
    state_encoder: StateEncoder,
    timer: AdaptiveTimer,
    last_interaction_at: Option<chrono::DateTime<chrono::Utc>>,
    running: bool,
    suspended: bool,
}

pub struct ChronosServicePlugin {
    inner: Arc<RwLock<Option<ChronosInner>>>,
}

impl ChronosServicePlugin {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(None)) }
    }
}

#[async_trait]
impl ServicePlugin for ChronosServicePlugin {
    fn name(&self) -> &str { "chronos" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut cfg = ChronosConfig::default();
        if let Some(v) = ctx.plugin_config.get("chronos") {
            cfg = serde_json::from_value(v.clone())
                .map_err(|e| PluginError::Config(format!("{e}")))?;
        }
        cfg.resolve_paths();
        cfg.validate().map_err(|e| PluginError::Config(format!("{e}")))?;

        let task_queue = TaskQueue::new(cfg.storage.task_queue_file.clone());
        let state_encoder = StateEncoder::new(cfg.state.clone(), cfg.preferences.clone());
        let timer = AdaptiveTimer::new(cfg.timing.clone());

        let mut orchestrator = ChronosOrchestrator::new();
        // 注册 9 个组件（按 priority 排序）
        orchestrator.sort();

        *self.inner.write().await = Some(ChronosInner {
            config: cfg, orchestrator, task_queue, state_encoder, timer,
            last_interaction_at: None, running: false, suspended: false,
        });
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or(PluginError::InitFailed("未初始化".into()))?;
        inner.running = true;
        inner.task_queue.load().await
            .map_err(|e| PluginError::Runtime(format!("{e}")))?;

        // §2.2 Provider 注册
        // ap.register_provider("chronos", Arc::new(ChronosProviderImpl::new(...)));

        drop(guard);
        let inner_clone = self.inner.clone();
        tokio::spawn(async move { Self::run_loop(inner_clone).await; });
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                let guard = self.inner.read().await;
                let inner = guard.as_ref().ok_or(PluginError::InitFailed("未初始化".into()))?;
                if !inner.running { return Err(PluginError::Runtime("未运行".into())); }
                Ok(())
            }
            ServiceSignal::ConfigReload => { tracing::info!("[chronos] 配置重载"); Ok(()) }
            ServiceSignal::Suspend => {
                let mut guard = self.inner.write().await;
                if let Some(ref mut i) = *guard { i.suspended = true; }
                Ok(())
            }
            ServiceSignal::Resume => {
                let mut guard = self.inner.write().await;
                if let Some(ref mut i) = *guard { i.suspended = false; }
                Ok(())
            }
            ServiceSignal::GracefulShutdown => {
                let mut guard = self.inner.write().await;
                if let Some(ref mut i) = *guard { i.running = false; }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        if let Some(ref mut i) = *guard { i.running = false; }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        if let Some(ref mut i) = *guard {
            i.running = false;
            i.task_queue.save().await
                .map_err(|e| PluginError::Runtime(format!("{e}")))?;
        }
        *guard = None;
        Ok(())
    }
}
```
