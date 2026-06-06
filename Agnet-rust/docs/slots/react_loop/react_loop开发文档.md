# ReActLoopSlot(ReAct循环槽口) 设计文档

## 0. 协议依据

本文档严格遵循以下协议，每条设计决策均可追溯到协议具体条款：

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **Slot(槽口) 接入协议** | 模块对外接口 | §1 SlotPlugin trait(槽口插件特质)、§2 SlotAccessPoint(槽口访问点)、§3 元数据声明、§4 权限枚举、§5 SlotDirective(槽口指令)、§6 生命周期、§9 红线 |
| **模块内部组件协议** | 模块内部结构 | §1 Component trait(组件特质)、§2 ComponentHandle(组件句柄)、§3 AccessPoint(访问点)、§4 Processing(处理结果)、§5 ComponentMeta(组件元数据)、§5.2 Orchestrator(协调器)、§6 模块边界规范、§11 红线 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |

---

## 0.5 功能清单

| 功能 | 描述 | 对应 Component(组件) | 优先级 | 状态 |
|------|------|---------------------|--------|------|
| 最大轮次检查 | 当 Pipeline(管道) 当前迭代轮次达到或超过配置的 `max_turns`(最大轮次) 时，强制结束本轮 Step(步骤)，防止 Agent(代理) 无限循环 | `TurnLimitComponent(轮次限制组件)` | P0 | 已设计 |
| Action(动作) 跳转 | 当 LLM(大模型) 返回 `Thought::Action`(思考结果::动作) 时，让 Pipeline(管道) 跳回 `think`(思考) 阶段重新调用 LLM(大模型)，形成 ReAct(ReAct) 循环 | `LoopDecisionComponent(循环决策组件)` | P0 | 已设计 |
| Final(最终) 放行 | 当 LLM(大模型) 返回 `Thought::Final`(思考结果::最终) 或无 Thought(思考结果) 时，返回 `Continue`(继续) 让 Pipeline(管道) 正常进入下一阶段 | `LoopDecisionComponent(循环决策组件)` | P0 | 已设计 |

---

## 1. 模块定位（Slot(槽口) 接入协议视角）

### 1.1 外部身份

遵循 Slot 接入协议 §1——`ReActLoopSlot`(ReAct循环槽口) 实现 `SlotPlugin`(槽口插件) trait(特质)，作为组件体系的外层入口：

```rust
#[async_trait::async_trait]
impl SlotPlugin for ReActLoopSlot {
    fn name(&self) -> &str;

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;

    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

| 方法 | 调用方 | 调用次数 | 职责（Slot 接入协议 §1） |
|------|--------|---------|------------------------|
| `name()` | PluginLoader(插件加载器) / Pipeline(管道) / 日志 | 多次 | 返回 `"react_loop"`，全局唯一 |
| `init(ctx)` | PluginLoader(插件加载器) | **1** | 从 `PluginInitContext`(插件初始化上下文) 读取配置；构造并初始化 Orchestrator(协调器) 及所有内部 Component(组件) |
| `run(ap)` | Pipeline(管道) | 每轮 Step(步骤) 的 `loop`(循环) 阶段触发一次 | 通过 `SlotAccessPoint`(槽口访问点) 读取外部数据，通过 Orchestrator(协调器) 编排内部 Component(组件)，返回 `SlotDirective`(槽口指令) |
| `shutdown()` | PluginLoader(插件加载器) | **1** | 调用 `Orchestrator.shutdown_all()`(协调器全关闭)，释放所有组件资源 |

### 1.2 元数据声明

遵循 Slot 接入协议 §3：

```yaml
name: react-loop
category: slot
version: 0.1.0
permissions:
  - context:read
  - phase:jump
```

| 字段 | 值 | 协议约束 |
|------|---|---------|
| `name` | `"react-loop"` | 必须与 `SlotPlugin::name()` 返回值一致（§3） |
| `category` | `"slot"` | 固定值（§3） |
| `version` | `"0.1.0"` | 语义版本，升级时更新（§3） |
| `permissions` | `["context:read", "phase:jump"]` | 声明使用的核心内建方法权限（§4） |

### 1.3 通过 SlotAccessPoint(槽口访问点) 获取的外部能力

遵循 Slot 接入协议 §2——**`SlotAccessPoint`(槽口访问点) 是 Slot(槽口) 与核心交互的唯一通道**。`ReActLoopSlot`(ReAct循环槽口) 只通过这个通道获取外部数据。

| 能力 | 来源提供方 | SlotAccessPoint(槽口访问点) 方法 | 协议权限 | 获取方式 |
|------|-----------|-------------------------------|---------|---------|
| 当前迭代轮次 | Pipeline(管道) | `current_iteration()` | 总是允许（§2.1） | 直接调用，返回 `usize` |
| Thought(思考结果) | 上游 `llm_thinker`(大模型思考者) Slot(槽口) | `read_context_raw("thought")` | `context:read`（§4） | 返回 `Option<&dyn Any>`，调用方通过 `downcast_ref::<Thought>()` 转型 |
| Phase(阶段) 跳转 | Pipeline(管道) | `request_jump(_)` | `phase:jump`（§4） | 传递 Phase(阶段) 名称字符串 |

**不需要 Provider(提供商) 扩展**：`provider_raw()`(提供商原始获取) 用于获取 Service(服务) 注册的业务能力。react_loop(循环) 的决策仅依赖 Pipeline(管道) 内建数据（轮次和 Thought(思考结果)），不需要任何外部 Service(服务) 的 Provider(提供商)。遵循 Slot 接入协议 §2.2——"Core(核心) 不与"。

### 1.4 输出契约

遵循 Slot 接入协议 §5——`run()`(执行) 返回 `Result<SlotDirective, PluginError>`(结果<槽口指令, 插件错误>)：

```rust
pub enum SlotDirective {
    Continue,        // 正常继续
    BreakPhase,      // 跳过当前阶段剩余 Slot
    BreakStep,       // 跳出整个 Step
    RestartStep,     // 重新开始当前 Step
    AbortStep,       // 中止本 Step（标记错误）
    AbortPipeline,   // 中止整个 Pipeline（致命错误）
    JumpTo(Phase),  // 跳转到指定 Phase
}
```

| 条件 | 返回 `SlotDirective`(槽口指令) | Pipeline(管道) 行为 |
|------|-----------------------------|-------------------|
| `current_iteration >= max_turns`(当前轮次>=最大轮次) | `BreakStep`(跳出步骤) | 终止本轮 Step(步骤)，保留当前累积结果，不继续后续 Phase(阶段) |
| `thought == Some(Thought::Action{..})`(思考结果==Some(思考结果::动作{..})) 且未超轮次 | `JumpTo(Phase::think())`(跳转到(阶段::思考())) | Pipeline(管道) 中断当前 Phase(阶段) 链，跳转到 `think`(思考) 阶段从头执行 |
| `thought == Some(Thought::Final{..})`(思考结果==Some(思考结果::最终{..})) | `Continue`(继续) | 进入下一 Phase(阶段)（`memorize`(记忆)） |
| `thought == None`(思考结果==None) | `Continue`(继续) | 同上——未写入 Thought(思考结果) 时视为无决策需求，放行 |
| `init()` 失败 | PluginLoader(插件加载器) 不加载此 Slot(槽口)，Pipeline(管道) 继续 | 遵循 S-R02(槽口红线02) ——失败不退化运行 |

**SlotDirective(槽口指令) 变体全覆盖策略**（遵循 S-R01(槽口红线01) —— "所有变体必须被正确处理"）：

```rust
match action {
    // 正常放行——Final(最终) 或 None(无)
    LoopAction::Continue => SlotDirective::Continue,

    // Action(动作) 触发循环——跳回 THINK(思考) 阶段
    LoopAction::JumpToThink => SlotDirective::JumpTo(Phase::think()),

    // 超轮次——强制终止本轮
    LoopAction::ForceBreak => SlotDirective::BreakStep,
}

// 不使用的变体（不会出现在本模块的返回路径中，但必须在 switch(分支) 中有兜底）
// 如果未来条件变化导致变体出现，应有明确的处理策略：

// SlotDirective::BreakPhase       — 不适用。JumpTo(Phase) 已自带"跳出当前阶段"语义
// SlotDirective::RestartStep      — 不适用。react_loop 不做工具重试决策
// SlotDirective::AbortStep        — 不适用。react_loop 不做本轮标记错误
// SlotDirective::AbortPipeline    — 不适用。react_loop 不认为循环决策达到致命程度
```

**JumpTo(Phase)(跳转到(阶段)) 的正确使用**：遵循 Slot 接入协议 §5——`JumpTo(Phase)`(跳转到(阶段)) 是原生变体。本模块直接构造并返回此变体，**不需要** `request_jump()` + `BreakPhase`(跳出阶段) 的两段式方案。`request_jump()`(请求跳转()) 是 `SlotAccessPoint`(槽口访问点) 上的指令请求方法，用于在 `run()`(执行) 执行过程中向 Pipeline(管道) 发出指令请求；`JumpTo(Phase)`(跳转到(阶段)) 是 `run()`(执行) 的返回值，两者在语义上等价。在 `run()`(执行) 中同时调用 `request_jump()`(请求跳转()) 和返回 `BreakPhase`(跳出阶段) 会造成 Pipeline(管道) 重复处理跳转指令（详见 §7.1）。

### 1.5 SlotPlugin(槽口插件) 完整实现

```rust
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::slot::{SlotPlugin, SlotDirective};
use crate::core::access::SlotAccessPoint;
use crate::core::types::{error::PluginError, plugin::PluginInitContext};
use crate::core::phase::Phase;
use crate::core::thought::Thought;

pub struct ReActLoopSlot {
    orchestrator: Arc<RwLock<Orchestrator>>,
}

impl ReActLoopSlot {
    pub fn new() -> Self {
        Self {
            orchestrator: Arc::new(RwLock::new(Orchestrator::new())),
        }
    }
}

#[async_trait]
impl SlotPlugin for ReActLoopSlot {
    fn name(&self) -> &str {
        "react_loop"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut orch = self.orchestrator.write().await;

        // 注册组件
        orch.register(Box::new(TurnLimitComponent::new(
            ctx.config::<ReactLoopConfig>()
                .ok()
                .and_then(|c| c.max_turns)
                .unwrap_or(DEFAULT_MAX_TURNS),
        )))?;
        orch.register(Box::new(LoopDecisionComponent::new()))?;

        // 全量初始化（按 DAG(有向无环图) 拓扑序）
        // TurnLimitComponent(轮次限制组件) (priority=10) → LoopDecisionComponent(循环决策组件) (priority=20)
        orch.init_all().await.map_err(|e| PluginError::InitFailed(e.to_string()))?;

        Ok(())
    }

    async fn run(
        &mut self,
        ap_slot: &mut dyn SlotAccessPoint,
    ) -> Result<SlotDirective, PluginError> {
        // ========== 通道 A：通过 SlotAccessPoint(槽口访问点) 读取外部数据 ==========
        // 遵循 Slot 接入协议 §2：SlotAccessPoint(槽口访问点) 是外部数据的唯一入口
        let iteration = ap_slot.current_iteration();
        let thought_raw: Option<Thought> = ap_slot
            .read_context_raw("thought")
            .and_then(|any| any.downcast_ref::<Thought>())
            .cloned();

        // ========== 通道 B：通过 InternalAccessPoint(内部访问点) 访问内部组件 ==========
        // 遵循模块内部组件协议 §3：组件间通过 AccessPoint(访问点) 间接通信
        let orch = self.orchestrator.read().await;
        let ap_int = orch.access_point(); // Arc<RwLock<dyn AccessPoint>>
        let mut ap_guard = ap_int.write().await;

        // 将外部数据注入 InternalAccessPoint(内部访问点) 共享数据区
        // 供组件内部通过 ap.read::<T>("key") 读取
        ap_guard.write("current_iteration", iteration)
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        if let Some(ref t) = thought_raw {
            ap_guard.write("thought", t.clone())
                .map_err(|e| PluginError::Internal(e.to_string()))?;
        }

        // 获取 LoopDecisionComponent(循环决策组件) 句柄
        let handle = ap_guard.call("loop_decider")
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        let decider = handle.as_any()
            .downcast_ref::<dyn LoopDecisionService>()
            .ok_or_else(|| PluginError::Internal(
                "loop_decider: type mismatch(loop_decider：类型不匹配)".into()
            ))?;

        // 执行决策（组件内部通过 ap.call("turn_limiter") 访问兄弟组件）
        let action = decider.decide(&mut *ap_guard)
            .map_err(|e| PluginError::Internal(e.to_string()))?;

        drop(ap_guard);

        // ========== 将决策结果映射为 SlotDirective(槽口指令) ==========
        // 遵循 Slot 接入协议 §5
        match action {
            LoopAction::Continue => Ok(SlotDirective::Continue),
            LoopAction::JumpToThink => Ok(SlotDirective::JumpTo(Phase::think())),
            LoopAction::ForceBreak => Ok(SlotDirective::BreakStep),
        }
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let mut orch = self.orchestrator.write().await;
        orch.shutdown_all().await;
        Ok(())
    }
}
```

---

## 2. 内部架构总览（模块内部组件协议视角）

### 2.1 组件(Component) 一览

遵循模块内部组件协议 §1——模块内部每个功能单元统一实现 `Component`(组件) trait(特质)。

```
┌──────────────────────────────────────────────────────────────────┐
│  ReActLoopSlot (SlotPlugin(槽口插件) 入口)                        │
│                                                                   │
│  持有 Orchestrator(协调器) —— 不包含业务逻辑，只做编排              │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  Orchestrator(协调器)                                         │ │
│  │                                                               │ │
│  │  内部拥有:                                                    │ │
│  │  - components: Vec<Box<dyn Component>>  (DAG(有向无环图)拓扑排序)│ │
│  │  - access_point: Arc<RwLock<dyn AccessPoint>> (注入给组件的句柄)│ │
│  │                                                               │ │
│  │  公开方法:                                                    │ │
│  │  - register(component) → 注册 + 校验依赖 + 拓扑排序            │ │
│  │  - init_all() → 按 DAG(有向无环图) 序初始化全部组件            │ │
│  │  - access_point() → 返回 InternalAccessPoint(内部访问点) 引用   │ │
│  │  - process_all() → 按 DAG(有向无环图) 序执行全部组件的 process()│ │
│  │  - shutdown_all() → 逆序关闭全部组件                           │ │
│  └──────────────────────┬───────────────────────────────────────┘ │
│                         │                                         │
│       注入 Arc<RwLock<InternalAccessPointImpl>>                    │
│            (只有一个实例，Slot(槽口)和 Component(组件)共享)          │
│                         ▼                                         │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  InternalAccessPointImpl (dyn AccessPoint(动态访问点) 的具体实现)  │
│  │                                                               │ │
│  │  内部拥有:                                                    │ │
│  │  - components: HashMap<String, Box<dyn ComponentHandle>>      │ │
│  │    (Orchestrator(协调器) 在 register()(注册()) 时填充)         │ │
│  │  - data_share: HashMap<String, Box<dyn Any + Send>>           │ │
│  │    (Slot(槽口) 写入外部数据，Component(组件) 读取 / 兄弟组件间共享)  │ │
│  │  - config: Arc<ModuleConfig>                                  │ │
│  │  - logger: ModuleLogger                                       │ │
│  └──────────────────────┬───────────────────────────────────────┘ │
│                         │                                         │
│          Slot(槽口) 通过 ap.call("name") + downcast 访问组件         │
│                         ▼                                         │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │  Components(组件池)                                           │ │
│  │                                                               │ │
│  │  层级 1 (priority=10, 无依赖)                                 │ │
│  │  ┌───────────────────────────────────────────────────────┐   │ │
│  │  │  TurnLimitComponent(轮次限制组件)                        │   │ │
│  │  │    provides: &["turn_check"]                           │   │ │
│  │  │    requires: &[]                                       │   │ │
│  │  │    config_key: Some("react_loop")                      │   │ │
│  │  │                                                         │   │ │
│  │  │  业务 trait(特质): TurnLimitService                    │   │ │
│  │  │    fn is_exceeded(iteration) -> bool                   │   │ │
│  │  │    fn max_turns() -> usize                             │   │ │
│  │  │                                                         │   │ │
│  │  │  职责: 持有 max_turns(最大轮次) 配置，独立判断轮次是否超限 │   │ │
│  │  │  process(): no-op(无操作)（无定期维护任务）              │   │ │
│  │  └───────────────────────────────────────────────────────┘   │ │
│  │                                                               │ │
│  │  层级 2 (priority=20, 依赖层级 1)                             │ │
│  │  ┌───────────────────────────────────────────────────────┐   │ │
│  │  │  LoopDecisionComponent(循环决策组件)                     │   │ │
│  │  │    provides: &["loop_decision"]                        │   │ │
│  │  │    requires: &["turn_check"]  ← 真实调用兄弟组件        │   │ │
│  │  │    config_key: None                                    │   │ │
│  │  │                                                         │   │ │
│  │  │  业务 trait(特质): LoopDecisionService                 │   │ │
│  │  │    fn decide(ap) -> Result<LoopAction, ComponentError>  │   │ │
│  │  │                                                         │   │ │
│  │  │  内部逻辑:                                             │   │ │
│  │  │    1. ap.read("current_iteration") 读取 Slot(槽口) 写入的数据 │   │ │
│  │  │    2. ap.read("thought") 读取 Slot(槽口) 写入的数据     │   │ │
│  │  │    3. ap.call("turn_limiter") → downcast → TurnLimitService │   │ │
│  │  │    4. turn_limit.is_exceeded(iteration) 轮次判断       │   │ │
│  │  │    5. 根据轮次状态 + Thought(思考结果) 类型返回 LoopAction │   │ │
│  │  │                                                         │   │ │
│  │  │  职责: 综合轮次信息和 LLM(大模型) 输出，做出循环决策     │   │ │
│  │  │  process(): no-op(无操作)                               │   │ │
│  │  └───────────────────────────────────────────────────────┘   │ │
│  └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 组件依赖关系（DAG(有向无环图)）

遵循模块内部组件协议 §5——`provides`(提供) / `requires`(依赖) 声明，Orchestrator(协调器) 在 `register()`(注册()) 时自动校验。

```
          TurnLimitComponent(轮次限制组件)
          ┌──────────────────────────┐
          │ provides: ["turn_check"]  │
          │ requires: []              │
          │ priority: 10              │
          └──────────┬───────────────┘
                     │
                     │ ap.call("turn_limiter") 在运行时
                     │ LoopDecisionComponent(循环决策组件) 通过 InternalAccessPoint(内部访问点) 调用
                     ▼
          LoopDecisionComponent(循环决策组件)
          ┌──────────────────────────┐
          │ provides: ["loop_decision"] │
          │ requires: ["turn_check"]  │
          │ priority: 20              │
          └──────────────────────────┘
```

| 层级 | 组件 | priority(优先级) | provides(提供) | requires(依赖) | 层内并行 | 说明 |
|------|------|----------------|---------------|---------------|---------|------|
| 1 | TurnLimitComponent(轮次限制组件) | 10 | `turn_check` | — | — | 先初始化，负责配置读取和校验 |
| 2 | LoopDecisionComponent(循环决策组件) | 20 | `loop_decision` | `turn_check` | — | 后初始化，运行时通过 `ap.call("turn_limiter")` 访问层级 1 |

### 2.3 两条通道的协作模式

`ReActLoopSlot`(ReAct循环槽口) 在 `run()`(执行) 中同时操作两条独立的通道：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  run(&mut self, ap_slot: &mut dyn SlotAccessPoint)                       │
│                                                                          │
│  ──── 通道 A: SlotAccessPoint(槽口访问点) (来自 Pipeline(管道) 的参数) ────    │
│                                                                          │
│  let iteration = ap_slot.current_iteration();  // 读轮次                  │
│  let thought: Option<Thought> = ap_slot                                   │
│      .read_context_raw("thought")                                         │
│      .and_then(|v| v.downcast_ref::<Thought>())                          │
│      .cloned();                          // 读 Thought(思考结果)          │
│                                                                          │
│  ──── 桥接：Slot(槽口) 将外部数据写入 InternalAccessPoint(内部访问点) ───────    │
│                                                                          │
│  let ap_int = self.orchestrator.access_point();  // 获取内部通道          │
│  let mut guard = ap_int.write().await;                                    │
│  guard.write("current_iteration", iteration)?;  // 写入外部数据          │
│  guard.write("thought", thought)?;               // 写入外部数据          │
│                                                                          │
│  ──── 通道 B: InternalAccessPoint(内部访问点) (通过 Orchestrator(协调器)) ───  │
│                                                                          │
│  // 组件间通过 ap.call("name") + downcast 调用兄弟组件                     │
│  let handle = guard.call("loop_decider")?;                                │
│  let decider = handle.as_any().downcast_ref::<dyn LoopDecisionService>()?;│
│  decider.decide(&mut *guard)?;                                            │
│  // └→ 内部又调 guard.call("turn_limiter") → TurnLimitService            │
│                                                                          │
│  ──── 映射为 SlotDirective(槽口指令) ──────────────────────────────────────── │
│                                                                          │
│  match action {                                                          │
│      LoopAction::Continue       => SlotDirective::Continue,              │
│      LoopAction::JumpToThink    => SlotDirective::JumpTo(Phase::think()),│
│      LoopAction::ForceBreak     => SlotDirective::BreakStep,             │
│  }                                                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

**关键约束**（遵循模块内部组件协议 §3——"组件**无权自行构造或修改** AccessPoint(访问点)"）：

- Slot(槽口) 通过 `Orchestrator.access_point()`(协调器.access_point()) 获取内部通道，这是 Slot(槽口) 能访问 `InternalAccessPoint`(内部访问点) 的唯一合法途径
- Slot(槽口) 写入共享数据后，再由组件读取——**禁止** Slot(槽口) 直接调用组件的具体方法（应始终通过 `ap.call()` + `downcast`(向下转型)）
- 组件只看到 `dyn AccessPoint`(动态访问点 trait)，不知道数据是 Slot(槽口) 还是兄弟组件写入的——这是 InternalAccessPoint(内部访问点) 的设计宗旨

---

## 3. Component(组件) 详解

### 3.1 TurnLimitComponent(轮次限制组件)

#### 元数据声明

遵循模块内部组件协议 §5：

```rust
ComponentMeta {
    name: "turn_limiter",
    version: "0.1.0",
    priority: 10,
    provides: &["turn_check"],
    requires: &[],
    config_key: Some("react_loop"),
}
```

| 字段 | 值 | 说明 |
|------|---|------|
| `name` | `"turn_limiter"` | 全局唯一标识，其他组件和 Slot(槽口) 通过此名称 `ap.call("turn_limiter")` 获取 |
| `version` | `"0.1.0"` | 语义版本，替换时需保证 `provides`(提供) 不变或在更高级别 |
| `priority` | `10` | 本模块最高优先级，先初始化 |
| `provides` | `["turn_check"]` | 向模块内公开的能力名称 |
| `requires` | `[]` | 不依赖任何其他组件 |
| `config_key` | `Some("react_loop")` | 配置段键名，对应 `ModuleConfig`(模块配置) 中的 `react_loop` 段 |

#### 业务接口 trait(特质)

```rust
pub trait TurnLimitService: Send + Sync {
    /// 检查当前轮次是否超过最大轮次上限
    ///
    /// iteration(轮次) 从 0 开始计数。例如:
    /// - `is_exceeded(0)`(是否超过(0)) 当 `max_turns=1`(最大轮次=1) 时返回 false（第 1 轮）
    /// - `is_exceeded(1)`(是否超过(1)) 当 `max_turns=1`(最大轮次=1) 时返回 true（第 2 轮 = 超限）
    /// - `is_exceeded(9)`(是否超过(9)) 当 `max_turns=10`(最大轮次=10) 时返回 false（第 10 轮）
    /// - `is_exceeded(10)`(是否超过(10)) 当 `max_turns=10`(最大轮次=10) 时返回 true（第 11 轮 = 超限）
    fn is_exceeded(&self, iteration: usize) -> bool;

    /// 获取配置的最大轮次值
    /// 供 Slot(槽口) 日志 / 监控使用
    fn max_turns(&self) -> usize;
}
```

#### 完整实现

```rust
pub struct TurnLimitComponent {
    max_turns: usize,
}

impl TurnLimitComponent {
    pub fn new(max_turns: usize) -> Self {
        Self { max_turns }
    }
}

impl TurnLimitService for TurnLimitComponent {
    fn is_exceeded(&self, iteration: usize) -> bool {
        // 遵循跨平台与硬编码规范 §1 — 数字阈值从配置字段读取
        iteration >= self.max_turns
    }

    fn max_turns(&self) -> usize {
        self.max_turns
    }
}
```

#### 设计约束：`is_exceeded()` 是纯函数，不依赖 AccessPoint

`TurnLimitComponent`(轮次限制组件) 的 `is_exceeded()`(是否超过()) 是一个**纯函数**——它只依赖 `self.max_turns`(自身最大轮次)（构造时注入），不从 `AccessPoint`(访问点) 读取任何数据。

```rust
fn is_exceeded(&self, iteration: usize) -> bool {
    iteration >= self.max_turns  // 只读 self.max_turns，不碰 ap
}
```

这一约束意味着：
- `TurnLimitComponent`(轮次限制组件) 在 DAG(有向无环图) 中是**叶子节点**——它不调用任何兄弟组件
- 调用方（`LoopDecisionComponent`(循环决策组件) 或 Slot(槽口)）直接传递数据，不需要先通过 `AccessPoint`(访问点) 共享数据再读取——因为没有"调用 `turn_limiter` 读数据"这个步骤，只有"调用 `turn_limiter` 做判断"这个步骤
- 如果未来有组件需要访问 `AccessPoint`(访问点) 的数据（如从共享数据区读取 `session_id`(会话标识)），应新建组件或扩展 `TurnLimitComponent`(轮次限制组件)，而不是在 `is_exceeded()`(是否超过()) 内部引入 `AccessPoint`(访问点) 依赖

**为什么这样设计**：轮次判断的逻辑不依赖任何运行时上下文——只需要两个数字（当前值、上限值）。把这个判断做成纯函数，让测试零依赖，也让 `LoopDecisionComponent`(循环决策组件) 在调用它时不需要经过 AccessPoint(访问点) 的数据前置准备。

#### `init()` 校验规则

遵循 Slot 接入协议 §6——`init()` 失败则插件不加载（S-R02(槽口红线02)）：

| 输入字段 | 来源 | 必需 | 默认值 | 校验规则 | 失败后果 |
|---------|------|------|--------|---------|---------|
| `max_turns`(最大轮次) | `InitContext.config["react_loop"]["max_turns"]`(初始化上下文.配置["react_loop"]["max_turns"]) | 否 | `DEFAULT_MAX_TURNS`(默认最大轮次)（10） | `max_turns >= 1`，小于 1 则自动提升为 1 | 配置缺失不影响加载（使用默认值） |

```rust
const DEFAULT_MAX_TURNS: usize = 10;

impl Component for TurnLimitComponent {
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 如果配置段或字段不存在，使用默认值
        let configured = ctx.config
            .get("max_turns")
            .unwrap_or(DEFAULT_MAX_TURNS);
        // 边界处理：max_turns 至少为 1
        self.max_turns = if configured >= 1 { configured } else { 1 };
        Ok(())
    }
```

**边界情况**：

| `max_turns` 配置值 | 最终值 | 原因 |
|-------------------|--------|------|
| 10 | 10 | 正常 |
| 0 | 1 | 0 表示"不允许任何轮次"，但语义上至少应执行一轮 |
| 未配置 | `DEFAULT_MAX_TURNS`(默认最大轮次)（10） | 遵循跨平台与硬编码规范——数字阈值使用常量集中管理 |

#### `process()` 逻辑

```rust
    /// 无定期维护任务
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }
```

#### `shutdown()` 逻辑

```rust
    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
```

---

### 3.2 LoopDecisionComponent(循环决策组件)

#### 元数据声明

```rust
ComponentMeta {
    name: "loop_decider",
    version: "0.1.0",
    priority: 20,
    provides: &["loop_decision"],
    requires: &["turn_check"],  // 运行时通过 ap.call("turn_limiter") 真实调用
    config_key: None,
}
```

| 字段 | 值 | 说明 |
|------|---|------|
| `name` | `"loop_decider"` | 全局唯一标识，Slot(槽口) 通过此名称获取 |
| `priority` | `20` | 在 TurnLimitComponent(轮次限制组件) 之后初始化 |
| `requires` | `["turn_check"]` | 声明依赖 `TurnLimitComponent`(轮次限制组件) 的能力；Orchestrator(协调器) 在 `register()`(注册()) 时验证此依赖是否存在 |

#### 业务接口 trait(特质)

```rust
pub trait LoopDecisionService: Send + Sync {
    /// 根据当前轮次和 Thought(思考结果) 做出循环决策
    ///
    /// 内部通过 AccessPoint(访问点):
    ///   1. ap.read("current_iteration") — 读取 Slot(槽口) 写入的轮次数据
    ///   2. ap.read("thought") — 读取 Slot(槽口) 写入的思考结果
    ///   3. ap.call("turn_limiter") + downcast — 调用兄弟组件做轮次判断
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError>;
}

/// 循环决策结果——模块内部表达，与 SlotDirective(槽口指令) 一一映射
pub enum LoopAction {
    /// 正常继续 — 映射为 SlotDirective::Continue(槽口指令::继续)
    Continue,
    /// 跳回 THINK(思考) 阶段 — 映射为 SlotDirective::JumpTo(Phase::think())(槽口指令::跳转到(阶段::思考()))
    JumpToThink,
    /// 强制结束本轮 — 映射为 SlotDirective::BreakStep(槽口指令::跳出步骤)
    ForceBreak,
}
```

#### `decide()` 算法

```rust
impl LoopDecisionService for LoopDecisionComponent {
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError> {
        // 遵循模块内部组件协议 §3：通过 AccessPoint(访问点) 共享数据区读取
        // Slot(槽口) 在 run()(执行) 中已写入 "current_iteration"(当前轮次) 和 "thought"(思考结果)
        // iteration(轮次) 从 0 开始计数: iteration=5 表示第 6 轮；
        // is_exceeded(9) 当 max_turns=10 时返回 false, is_exceeded(10) 才返回 true
        let iteration = ap.read::<usize>("current_iteration")
            .copied()
            .unwrap_or(0);
        let thought: Option<&Thought> = ap.read::<Thought>("thought");

        // 遵循 C-R01(组件红线01)：call()(调用()) 后必须 downcast(向下转型)
        // 遵循 C-R02(组件红线02)：requires(依赖) 必须在代码中实际调用
        let handle = ap.call("turn_limiter")
            .map_err(|_| ComponentError::NotFound("turn_limiter".into()))?;
        let turn_limit = handle.as_any()
            .downcast_ref::<dyn TurnLimitService>()
            .ok_or_else(|| ComponentError::Internal(
                "turn_limiter: type mismatch(类型不匹配)".into()
            ))?;

        // ========== 决策逻辑（状态机） ==========
        let exceeded = turn_limit.is_exceeded(iteration);

        match (exceeded, thought) {
            // 1. 轮次超限 → 无论 Thought(思考结果) 类型，强制结束
            //    （此条件优先级最高，防止恶意无限循环）
            (true, _) => {
                tracing::warn!(
                    "[react_loop] max_turns reached: iteration={}, max_turns={}",
                    iteration,
                    turn_limit.max_turns(),
                );
                Ok(LoopAction::ForceBreak)
            }

            // 2. LLM(大模型) 返回 Action(动作) 且轮次未超限 → 跳回 THINK(思考)
            (false, Some(Thought::Action { .. })) => {
                tracing::debug!(
                    "[react_loop] Action detected, jumping back to THINK phase"
                );
                Ok(LoopAction::JumpToThink)
            }

            // 3. LLM(大模型) 返回 Final(最终) 或没有 Thought(思考结果) → 放行
            (false, Some(Thought::Final { .. })) | (false, None) => {
                tracing::trace!("[react_loop] Final/None, continuing");
                Ok(LoopAction::Continue)
            }
        }
    }
}
```

#### 算法流程图

```
decide(ap: &mut dyn AccessPoint)
  │
  ├── 读取 "current_iteration" 从共享数据区
  │     → iteration: usize
  │
  ├── 读取 "thought" 从共享数据区
  │     → thought: Option<&Thought>
  │
  ├── ap.call("turn_limiter")
  │     → downcast_ref::<dyn TurnLimitService>()
  │     → turn_limit
  │
  ├── exceeded = turn_limit.is_exceeded(iteration)
  │
  ├── (exceeded == true)
  │     └── LoopAction::ForceBreak(循环动作::强制跳出)
  │
  ├── (exceeded == false) && (thought == Action(动作))
  │     └── LoopAction::JumpToThink(循环动作::跳转思考)
  │
  └── (exceeded == false) && (thought == Final(最终) / None(无))
        └── LoopAction::Continue(循环动作::继续)
```

#### 边界情况处理

| 场景 | 条件 | 返回 | 原因 |
|------|------|------|------|
| 正常 Action(动作) 跳转 | `iteration=2, max_turns=10, Action`(iteration=2, max_turns=10, 动作) | `JumpToThink`(跳转思考) | LLM(大模型) 还有 8 轮空间 |
| 最后一轮 Action(动作) | `iteration=9, max_turns=10, Action`(iteration=9, max_turns=10, 动作) | `ForceBreak`(强制跳出) | 再跳就超了，触发保护（注意：iteration 是从 0 开始计数的，所以 iteration=9 时 is_exceeded(9) 返回 false，因为 9 < 10；is_exceeded(10) 才返回 true） |
| 临界值 | `iteration=0, max_turns=1` | `is_exceeded(0)` = false | 第一轮正常执行 |
| 临界值 | `iteration=1, max_turns=1` | `is_exceeded(1)` = true | 达到上限，强制结束 |
| 无 Thought(思考结果) | `thought=None` | `Continue`(继续) | 没有决策数据，不做任何跳转 |
| 超限后 Action(动作) | `iteration=10, max_turns=10` | `ForceBreak`(强制跳出) | 已经超过上限，不执行任何跳转 |

#### `init()` / `process()` / `shutdown()` 逻辑

```rust
impl Component for LoopDecisionComponent {
    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
        Ok(())
    }

    /// 无定期维护任务
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
```

---

## 4. Orchestrator(协调器) 编排逻辑

### 4.1 生命周期映射

遵循模块内部组件协议 §5.1——Orchestrator(协调器) 不包含任何业务代码，只做编排：

| SlotPlugin(槽口插件) 方法 | Orchestrator(协调器) 调用 | 编排顺序 |
|-------------------------|------------------------|---------|
| `init(ctx)` | `orch.init_all()` | [层级 1] TurnLimitComponent(轮次限制组件) (priority=10) → [层级 2] LoopDecisionComponent(循环决策组件) (priority=20) |
| `run(ap_slot)` | 通过 `orch.access_point()`(orch.access_point()) 获取 InternalAccessPoint(内部访问点)，写入外部数据后调用 `LoopDecisionService::decide()`(循环决策服务::decide()) | Slot(槽口) 编排：写数据 → 调组件 |
| — | `orch.process_all()`(orch.process_all())（由框架定时触发） | [层级 1] → [层级 2]；每个 Component(组件) 的 `process()`(处理) 为空操作 |
| `shutdown()` | `orch.shutdown_all()` | [层级 2] → [层级 1] 反向序关闭 |

### 4.2 `init_all()` 详细流程

```
Pipeline(管道) 启动 → PluginLoader(插件加载器) 加载所有 Slot(槽口)
  │
  └── llm_thinker_plugin.init(ctx: &PluginInitContext)
        │
        ├── 1. 读取配置
        │      ctx.config::<ReactLoopConfig>()
        │      → 如果存在 "react_loop" 配置段:
        │          let max_turns = config.max_turns // Option<usize>
        │      → 如果不存在: 使用默认值
        │          let max_turns = DEFAULT_MAX_TURNS  // 10
        │
        ├── 2. 构造 Orchestrator(协调器)
        │      let mut orch = Orchestrator::new()
        │
        ├── 3. 注册 TurnLimitComponent(轮次限制组件)
        │      orch.register(Box::new(TurnLimitComponent::new(max_turns)))
        │      → 校验: requires=[] 通过（无依赖）
        │      → 排序: priority=10, 作为层级 1
        │
        ├── 4. 注册 LoopDecisionComponent(循环决策组件)
        │      orch.register(Box::new(LoopDecisionComponent::new()))
        │      → 校验: requires=["turn_check"]
        │         在已注册组件中搜索 provides=["turn_check"]
        │         → 找到 TurnLimitComponent(轮次限制组件).provides = ["turn_check"]
        │         → 校验通过
        │      → 排序: priority=20, 依赖层级 1, 作为层级 2
        │
        ├── 5. 执行 init_all()
        │      orch.init_all().await
        │      → [层级 1, 串行] TurnLimitComponent.init(ctx)
        │      │   ├── 从 ctx.config 读取 "react_loop.max_turns"
        │      │   └── 校验: max_turns ≥ 1
        │      │       失败 → init() 返回 Err → Slot(槽口) 不加载（S-R02(槽口红线02)）
        │      │
        │      └── [层级 2, 串行] LoopDecisionComponent.init(ctx)
        │          └── 无操作，始终 Ok
        │
        └── 6. 保存 orch(协调器) 到 self.orchestrator
```

### 4.3 `run()` 完整流程

```
Pipeline(管道) 执行到 "loop"(循环) 阶段
  → 调用 react_loop_plugin.run(ap_slot: &mut dyn SlotAccessPoint)
  │
  ├── [A] 通过 SlotAccessPoint(槽口访问点) 读取外部数据
  │     ├── let iteration = ap_slot.current_iteration()
  │     │   → 来自 Pipeline(管道) 的迭代计数器
  │     │
  │     └── let thought_raw = ap_slot.read_context_raw("thought")
  │         → 来自上游 llm_thinker(大模型思考者) 写入的 StepContext(步骤上下文)
  │         → 类型: Option<&dyn Any>
  │         → downcast_ref::<Thought>() + cloned()
  │         → 输出: Option<Thought>
  │
  ├── [B] 获取 InternalAccessPoint(内部访问点)
  │     let orch = self.orchestrator.read().await
  │     let ap_int = orch.access_point()
  │     let mut ap_guard = ap_int.write().await
  │
  ├── [C] 将外部数据注入共享数据区
  │     ap_guard.write("current_iteration", iteration)?
  │     if let Some(t) = &thought_raw {
  │         ap_guard.write("thought", t.clone())?
  │     }
  │
  ├── [D] 获取 LoopDecisionComponent(循环决策组件) 句柄
  │     ap_guard.call("loop_decider")?
  │     → 返回 Box<dyn ComponentHandle>
  │     handle.as_any().downcast_ref::<dyn LoopDecisionService>()?
  │
  ├── [E] 执行决策
  │     decider.decide(&mut *ap_guard)?
  │     │
  │     │   └── [处理引擎 — 组件内部执行]
  │     │       ├── ap.read::<usize>("current_iteration")
  │     │       │   → 读取 Slot(槽口) 写入的轮次数据
  │     │       │
  │     │       ├── ap.read::<Thought>("thought")
  │     │       │   → 读取 Slot(槽口) 写入的思考结果
  │     │       │
  │     │       ├── ap.call("turn_limiter")
  │     │       │   → downcast_ref::<dyn TurnLimitService>()
  │     │       │   → turn_limit.is_exceeded(iteration)
  │     │       │
  │     │       └── 返回 LoopAction
  │     │
  │     └── action: LoopAction
  │
  └── [F] 映射为 SlotDirective(槽口指令)
        match action {
            LoopAction::Continue       ⇒ SlotDirective::Continue,
            LoopAction::JumpToThink    ⇒ SlotDirective::JumpTo(Phase::think()),
            LoopAction::ForceBreak     ⇒ SlotDirective::BreakStep,
        }
```

### 4.4 `process_all()`（定期维护）

遵循模块内部组件协议 §5.2——`process_all()`(全处理) 是 Orchestrator(协调器) 提供的定期维护入口：

```rust
impl Orchestrator {
    /// 按 DAG(有向无环图) 拓扑序执行所有 Component(组件) 的 process()(处理())
    /// 同层无依赖关系的组件并发执行，层间串行
    pub async fn process_all(&self) -> Result<(), ComponentError> {
        let mut ap = self.access_point.write().await;
        for group in &self.parallel_groups {
            let results = join_all(group.iter().map(|&idx| {
                let comp = &mut self.components[idx];
                async move { comp.process(&mut *ap).await }
            })).await;
            for result in results {
                match result? {
                    Processing::Continue => continue,
                    Processing::BreakChain => return Ok(()),
                    Processing::Restart => return self.process_all().await,
                    Processing::Warn { message } => {
                        tracing::warn!("[{}] {}", self.components[idx].name(), message);
                    }
                }
            }
        }
        Ok(())
    }
}
```

**触发时机**：`process_all()`(全处理) **不由** `run()`(执行) 调用。它由外部框架独立触发（如 Pipeline(管道) 在每轮 Step(步骤) 结束后、或定时器每 30 秒）。对于 `ReActLoopSlot`(ReAct循环槽口) 的当前实现，两个 Component(组件) 的 `process()`(处理) 均为 no-op(无操作)。未来的定期维护任务（如清理历史轮次记录）应在此方法中扩展。

### 4.5 `shutdown_all()` 详细流程

```
Pipeline(管道) 关闭 → PluginLoader(插件加载器) 销毁所有插件
  |
  └── react_loop_plugin.shutdown()
        |
        └── self.orchestrator.shutdown_all().await
              |
              ├── [逆序] LoopDecisionComponent.shutdown()
              |     └── 无操作
              |
              └── [逆序] TurnLimitComponent.shutdown()
                    └── 无操作
```

---

## 5. 跨平台与硬编码规范视角

### 5.1 配置值约束

遵循《跨平台与硬编码规范》§1——所有运行时从外部读取的值在业务逻辑中不得写死字面量：

| 类别 | 代码位置 | 规范要求（§1） | 合规证明 |
|------|---------|---------------|---------|
| **数字阈值** | `TurnLimitComponent.is_exceeded()`(轮次限制组件.is_exceeded()) 中的 `max_turns`(最大轮次) | 从 `InitContext.config`(初始化上下文.配置) 读取，不允许字面量 | `TurnLimitComponent.init()`(轮次限制组件.init()) 中通过 `ctx.config.get("max_turns")`(ctx.config.get("max_turns")) 读取 |
| **Fallback(回退) 默认值** | `max_turns`(最大轮次) 当配置缺失时 | 使用模块级 `const`(常量)，避免业务逻辑中写数字字面量 | `const DEFAULT_MAX_TURNS: usize = 10`(常量 默认最大轮次: usize = 10) 集中管理 |
| **Phase(阶段) 名称** | `SlotDirective::JumpTo(Phase::think())`(槽口指令::跳转到(阶段::思考())) 中的阶段名 | 使用核心定义的 `Phase`(阶段) 类型方法，不写字符串字面量 | 直接使用 `Phase::think()`(阶段::思考())，完整类型安全 |
| **User-Agent(用户代理)** | 本模块无 HTTP(超文本传输协议) 请求 | 不需要 | — |
| **文件路径** | 本模块无文件操作 | 不需要 | — |
| **平台指令** | 本模块无进程/命令执行 | 不需要 | — |

### 5.2 路径约束

遵循《跨平台与硬编码规范》§2——react_loop(循环) **不涉及**任何文件系统操作，以下规则均不适用：

- §2.1 禁止裸用 Unix-only(Unix专用) 路径 ✓（不涉及）
- §2.2 禁止裸用 `~` ✓（不涉及）
- §2.3 禁止相对路径依赖 CWD(当前工作目录) ✓（不涉及）
- §2.4 路径拼接 ✓（不涉及）
- §2.5 路径分隔符判断 ✓（不涉及）
- §2.6 文件扩展名判断 ✓（不涉及）
- §2.7 临时文件/目录 ✓（不涉及）
- §2.8 数据目录 ✓（不涉及）

### 5.3 测试规范

遵循《跨平台与硬编码规范》§3：

**单元测试范围**：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // TurnLimitComponent(轮次限制组件) 测试
    // ============================================

    #[test]
    fn test_turn_limit_not_exceeded() {
        // 规范 §1——数字阈值来自构造参数，非硬编码
        let component = TurnLimitComponent::new(5);
        assert!(!component.is_exceeded(0));
        assert!(!component.is_exceeded(4));
    }

    #[test]
    fn test_turn_limit_exceeded() {
        let component = TurnLimitComponent::new(5);
        assert!(component.is_exceeded(5));
        assert!(component.is_exceeded(100));
    }

    #[test]
    fn test_turn_limit_default() {
        let component = TurnLimitComponent::new(DEFAULT_MAX_TURNS);
        assert!(!component.is_exceeded(DEFAULT_MAX_TURNS - 1));
        assert!(component.is_exceeded(DEFAULT_MAX_TURNS));
    }

    #[test]
    fn test_turn_limit_boundary() {
        // 边界：max_turns=0 应被初始化为 1
        let component = TurnLimitComponent::new(1);
        assert!(!component.is_exceeded(0));
        assert!(component.is_exceeded(1));
    }

    // ============================================
    // LoopDecisionComponent(循环决策组件) 测试
    // ============================================

    // ============================================
    // MockAccessPoint(模拟访问点) — 完整的 AccessPoint(访问点) trait(特质) 实现
    // ============================================

    /// 供测试使用的 Mock(模拟) 访问点，模拟 InternalAccessPointImpl(内部访问点实现)
    /// - `read`/`write`(读取/写入) 操作内存中的 HashMap(哈希表)
    /// - `call`(调用) 返回通过 `register_mock_component`(注册模拟组件) 注册的组件
    /// - `config`/`metrics`/`log`(配置/度量/日志) 调用时 panic(恐慌)，因为这些方法在测试中不会被调用
    struct MockAccessPoint {
        data: HashMap<String, Box<dyn Any + Send>>,
        components: HashMap<String, Box<dyn ComponentHandle>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                data: HashMap::new(),
                components: HashMap::new(),
            }
        }

        /// 注册一个模拟组件，供 `call`(调用) 方法返回
        fn register_mock_component(&mut self, name: &str, component: Box<dyn ComponentHandle>) {
            self.components.insert(name.to_string(), component);
        }
    }

    impl AccessPoint for MockAccessPoint {
        fn read<T: 'static>(&self, key: &str) -> Option<&T> {
            self.data
                .get(key)
                .and_then(|v| v.downcast_ref::<T>())
        }

        fn write<T: 'static>(&mut self, key: &str, val: T) -> Result<(), ComponentError> {
            self.data.insert(key.to_string(), Box::new(val));
            Ok(())
        }

        fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError> {
            self.components
                .get(name)
                .map(|c| c.clone_box())
                .ok_or_else(|| ComponentError::NotFound(name.to_string()))
        }

        fn config(&self) -> &ModuleConfig {
            panic!("MockAccessPoint::config() called — not available in test context(测试上下文中不可用)")
        }

        fn metrics(&self) -> &MetricsHandle {
            panic!("MockAccessPoint::metrics() called — not available in test context(测试上下文中不可用)")
        }

        fn log(&self) -> &dyn ModuleLogger {
            panic!("MockAccessPoint::log() called — not available in test context(测试上下文中不可用)")
        }
    }

    #[test]
    fn test_decide_action_within_limit() {
        // 准备: iteration=3, max_turns=5, Thought::Action
        let mut ap = MockAccessPoint::new();
        ap.write("current_iteration", 3usize).unwrap();
        ap.write("thought", Thought::Action { ... }).unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::JumpToThink);
    }

    #[test]
    fn test_decide_action_exceeded() {
        // 准备: iteration=5, max_turns=5, Thought::Action
        let mut ap = MockAccessPoint::new();
        ap.write("current_iteration", 5usize).unwrap();
        ap.write("thought", Thought::Action { ... }).unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::ForceBreak);
    }

    #[test]
    fn test_decide_final_within_limit() {
        let mut ap = MockAccessPoint::new();
        ap.write("current_iteration", 2usize).unwrap();
        ap.write("thought", Thought::Final { ... }).unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::Continue);
    }

    #[test]
    fn test_decide_no_thought() {
        // Thought(思考结果) 不存在 = Continue(继续)
        let mut ap = MockAccessPoint::new();
        ap.write("current_iteration", 2usize).unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::Continue);
    }
}
```

**规范 §3.2——平台特定测试**：`#[cfg(target_os = "windows")]`(条件编译(目标系统 = "windows")) 不适用——react_loop(循环) 无平台相关代码。

**规范 §3.1——临时路径**：`std::env::temp_dir()`(std::env::temp_dir()) 不适用——react_loop(循环) 无文件操作。

**规范 §3.3——网络测试**：无外部 API(接口) 依赖，所有测试均为单元测试，无需 `#[ignore]`(忽略)。

### 5.4 新增插件自查清单

遵循《跨平台与硬编码规范》§4——提交代码前逐条确认：

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | 所有 URL(统一资源定位符) 端点来自配置或常量，非字面量写死 | ✅ 不涉及 |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ 不涉及 |
| 3 | 所有超时值来自配置或 `DEFAULT_*`(默认*) 常量 | ✅ `DEFAULT_MAX_TURNS`(默认最大轮次) |
| 4 | API(应用程序编程接口) 版本号定义为模块级 `const`(常量)，不散落 | ✅ `DEFAULT_MAX_TURNS`(默认最大轮次) |
| 5 | User-Agent(用户代理) 定义为 `const USER_AGENT`(常量 USER_AGENT) | ✅ 不涉及 |
| 6 | 文件路径通过 `dirs`(目录) + `PathBuf::join()`(路径缓冲::join()) 构建 | ✅ 不涉及 |
| 7 | 数字阈值默认 `None`(无) 或从配置读取 | ✅ `max_turns`(最大轮次) 从配置读取，`None`(无) 时使用 `DEFAULT_MAX_TURNS`(默认最大轮次) |
| 8 | 平台特定指令通过 `OsKind`(操作系统类型) 枚举分支 | ✅ 不涉及 |
| 9 | 测试中无 Unix-only(Unix专用) 路径，均用 `std::env::temp_dir()`(std::env::temp_dir()) | ✅ 不涉及 |
| 10 | `cargo build`(cargo 构建) + `cargo test`(cargo 测试) + `cargo clippy`(cargo clippy) 全部通过 | 待验证 |

---

## 6. 模块边界规范

### 6.1 `mod.rs` 暴露原则

遵循模块内部组件协议 §6.1——模块 `mod.rs` 只对外暴露入口、配置、错误类型，内部组件全部 `pub(crate)`(公有(模块))：

```rust
// ✅ 正确——只暴露三样东西
pub struct ReActLoopSlot;       // 对外 Slot(槽口) 入口
pub struct ReactLoopConfig;     // 配置
pub struct ReactLoopError;      // 错误类型

// 内部全部 pub(crate)(公有(模块))
pub(crate) mod orchestrator;
pub(crate) mod components;
```

```rust
// ❌ 禁止——将内部类型全部 pub use(公有使用)
pub use orchestrator::*;
pub use components::*;
```

### 6.2 依赖方向

遵循模块内部组件协议 §6.2：

```
┌─────────────────────────────┐
│  modules/react_loop/mod.rs   │
│  对外暴露:                   │
│    pub struct ReActLoopSlot   │
│    pub struct ReactLoopConfig │
│    pub struct ReactLoopError  │
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│  orchestrator.rs             │
│  Orchestrator(协调器)         │
│    - Vec<Box<dyn Component>> │
│    - Arc<RwLock<InternalAccessPointImpl>> │
│    - register()              │
│    - init_all()              │
│    - access_point()          │
│    - process_all()           │
│    - shutdown_all()          │
└──────────┬──────────────────┘
           │ 注入 Arc<RwLock<InternalAccessPointImpl>>
           ▼
┌─────────────────────────────┐
│  components/ 目录            │
│                             │
│  TurnLimitComponent          │
│    - impl Component          │
│    - impl TurnLimitService   │
│    → 只能看到 AccessPoint    │
│    → 不引用兄弟组件类型      │
│                             │
│  LoopDecisionComponent       │
│    - impl Component          │
│    - impl LoopDecisionService│
│    → 只能看到 AccessPoint    │
│    → 通过 ap.call() 间接调兄弟│
└─────────────────────────────┘
```

### 6.3 新增/替换 Component(组件) 标准流程

遵循模块内部组件协议 §10：

**新增 Component(组件)：**

| 步骤 | 做什么 | 涉及文件 | 协议依据 |
|------|--------|---------|---------|
| 1 | 在 `components/` 新建文件 | `components/rate_limiter.rs` | §10 步骤 1 |
| 2 | 实现 `Component` trait(特质) + `fn meta()`(元数据函数) | 同上 | §10 步骤 2 |
| 3 | 定义业务 trait(特质)（如 `RateLimitService`(速率限制服务)） | 同上 | §9.1 分层设计 |
| 4 | 在 `orchestrator.rs` 注册 | 加一行 `orch.register(Box::new(RateLimiter::new()))?;` | §10 步骤 3 |
| 5 | 运行 `cargo check`(cargo check) | — | §10 步骤 4 |

**替换现有 Component(组件)：**

| 步骤 | 做什么 | 协议依据 |
|------|--------|---------|
| 1 | 确认新旧组件的 `provides`(提供) 一致（否则依赖方报错） | §10 步骤 1 |
| 2 | 确认新旧组件的 `requires`(依赖) 是旧组件的子集 | §10 步骤 2 |
| 3 | 编写新 `impl Component`(实现组件)，替换原文件 | §10 步骤 3 |
| 4 | 若 `name`(名称) 不变，`orchestrator.rs`(orchestrator.rs) 无需修改 | §10 步骤 4 |
| 5 | 运行 `cargo check`(cargo check) | §10 步骤 5 |
| 6 | 运行单元测试，保证 `LoopAction`(循环动作) 返回值语义一致 | §10 步骤 6 |

---

## 7. 设计决策与约束

### 7.1 SlotAccessPoint(槽口访问点) 与 InternalAccessPoint(内部访问点) 的桥接策略

**问题**：`SlotAccessPoint`(槽口访问点)（来自 Pipeline(管道)）和 `InternalAccessPoint`(内部访问点)（来自 Orchestrator(协调器)）是两条独立的通道。组件只能看到 `InternalAccessPoint`(内部访问点)，但外部数据（`current_iteration`(当前轮次)、`thought`(思考结果)）只存在于 `SlotAccessPoint`(槽口访问点) 上。如何让组件访问到这些数据？

**禁止的方案**：让 `SlotAccessPoint`(槽口访问点) 和 `InternalAccessPoint`(内部访问点) "共享底层数据区"。**协议中没有这种设计**。`SlotAccessPoint`(槽口访问点) 和 `InternalAccessPoint`(内部访问点) 是两个独立的 trait(特质)，由不同的实现者实现，不保证任何共享行为。

**采纳的方案**（遵循模块内部组件协议 §3）：

```
Slot(槽口) 在 run() 的开始阶段充当"桥接器"(Bridge)：
  1. 从 SlotAccessPoint(槽口访问点) 读取外部数据
  2. 通过 AccessPoint::write()(访问点::write()) 将数据写入 InternalAccessPoint(内部访问点) 共享数据区
  3. 组件通过 AccessPoint::read()(访问点::read()) 从共享数据区读取
```

这是唯一符合两条通道协议约束的方案。桥接代码集中在 `ReActLoopSlot::run()`(ReAct循环槽口::run()) 的头部，不超过 10 行，职责清晰。

### 7.2 策略：直接使用 `SlotDirective::JumpTo(Phase)`(槽口指令::跳转到(阶段))

**问题**：在输出契约 §1.4 中，Action(动作) 场景的返回方式是 `SlotDirective::JumpTo(Phase::think())`(槽口指令::跳转到(阶段::思考()))，而不是 `request_jump("think")`(请求跳转("think")) + `SlotDirective::BreakPhase`(槽口指令::跳出阶段)。

**决策理由**：

| 方案 | 问题 |
|------|------|
| `request_jump("think")`(请求跳转("think")) + `BreakPhase`(跳出阶段) | 两段式发送跳转指令给 Pipeline(管道)。`request_jump()`(请求跳转()) 是 `SlotAccessPoint`(槽口访问点) 上的"请求"方法（在 `run()`(执行) 执行过程中向 Pipeline(管道) 发出指令）；而 `BreakPhase`(跳出阶段) 是 `SlotDirective`(槽口指令) 的一个变体，两者语义重叠。调用 `request_jump()`(请求跳转()) + 返回 `BreakPhase`(跳出阶段) 导致 Pipeline(管道) 可能重复处理跳转 |
| `SlotDirective::JumpTo(Phase::think())`(槽口指令::跳转到(阶段::思考())) | **符合协议 §5**，一次性明确告诉 Pipeline(管道)："跳转到 think(思考) 阶段"。`JumpTo(Phase)`(跳转到(阶段)) 是 `SlotDirective`(槽口指令) 的原生变体，Pipeline(管道) 保证正确处理 |

**结论**：直接使用 `SlotDirective::JumpTo(Phase::think())`(槽口指令::跳转到(阶段::思考()))。

### 7.3 策略：`LoopDecisionComponent.decide()`(循环决策组件.decide()) 的接口设计

**问题**：`decide()`(决策()) 的输入数据如何传递——通过参数还是通过 `AccessPoint`(访问点) 共享数据？

**决策**：两者结合——`iteration`(轮次) 和 `thought`(思考结果) 通过共享数据（`ap.read("key")`(ap.read("key"))）；兄弟组件通过 `ap.call("name")`(ap.call("name")) + `downcast`(向下转型)。

```rust
fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError>;
```

| 数据来源 | 获取方式 | 协议依据 |
|---------|---------|---------|
| 轮次（来自 SlotAccessPoint(槽口访问点)） | `ap.read::<usize>("current_iteration")`(ap.read::<usize>("current_iteration")) | 模块内部组件协议 §3，`AccessPoint`(访问点) 的 `read()`(读取()) 方法 |
| Thought(思考结果)（来自 SlotAccessPoint(槽口访问点)） | `ap.read::<Thought>("thought")`(ap.read::<Thought>("thought")) | 同上 |
| TurnLimitComponent(轮次限制组件) 句柄 | `ap.call("turn_limiter")`(ap.call("turn_limiter")) → `downcast_ref::<dyn TurnLimitService>()`(向下转型引用::<dyn TurnLimitService>()) | 模块内部组件协议 §2，`ComponentHandle`(组件句柄)；C-R01(组件红线01) |

### 7.4 策略：`process_all()`(全处理) 与 `run()`(执行) 的职责分离

遵循模块内部组件协议 §5.1——Orchestrator(协调器) 只做编排：

| 方法 | 调用者 | 用途 | 当前实现 |
|------|--------|------|---------|
| `process_all()`(全处理) | 外部框架（定时器 / Pipeline(管道) 事件） | 执行所有 Component(组件) 的 `process()`(处理()) 用于定期维护 | 两个 Component(组件) 的 `process()`(处理()) 均为 `Ok(Processing::Continue)`(Ok(处理::继续))——无定期任务 |
| `run()`(执行) | Pipeline(管道) / LOOP(循环) 阶段 | 执行业务逻辑——读取外部数据、编排内部组件、返回 `SlotDirective`(槽口指令) | 通过桥接模式调用 `LoopDecisionService::decide()`(循环决策服务::decide()) |

两个方法**互不重叠**：`run()`(执行) 不调用 `process_all()`(全处理)，`process_all()`(全处理) 不参与业务决策。这是模块内部组件协议 §5.1+§9.1 确立的分层原则——**Component(组件) 做生命周期 + 具体 trait(特质) 做业务接口**。

### 7.5 组件数量说明

遵循模块内部组件协议 §9.2——"组件数量少（每模块 3-7 个）"。本模块当前 2 个组件。

**为什么不拆更多**：

| 候选拆分 | 拒绝理由 |
|---------|---------|
| 将 `run()`(执行) 中的"读取 `SlotAccessPoint`(槽口访问点) 数据"拆为独立 Component(组件) | 组件只能通过 `InternalAccessPoint`(内部访问点) 通信，无法访问 `SlotAccessPoint`(槽口访问点)。读取 `SlotAccessPoint`(槽口访问点) 是 Slot(槽口) 的专属职责 |
| 将 `TurnLimitComponent`(轮次限制组件) 的 `is_exceeded()`(是否超过()) 逻辑内联到 `LoopDecisionComponent`(循环决策组件) | 违反职责分离——轮次配置管理是一个独立的关注点，可独立测试、独立替换 |
| 将 `LoopAction`(循环动作) → `SlotDirective`(槽口指令) 映射拆为独立组件 | 映射逻辑仅 3 行，且依赖 `SlotDirective`(槽口指令)（核心类型），不值得独立组件 |

**未来扩容方向**：

```
react_loop/components/
  ├── turn_limiter.rs       // 现有：轮次限制（P0(最高优先级)）
  ├── cost_limiter.rs       // 未来：费用限制（根据用户配置的每次工具调用预算）
  ├── token_budget.rs       // 未来：Token(令牌) 预算（根据模型 context_window(上下文窗口) 限制）
  └── loop_decider.rs       // 现有：综合决策（P0(最高优先级)）
```

每个新增 Component(组件) 按 §6.3 标准流程接入。

### 7.6 为什么不需要 Provider(提供商) 依赖？

Slot 接入协议 §2.2 的 Provider(提供商) 扩展机制——`provider_raw()`(provider_raw()) 用于获取外部 Service(服务) 注册的能力：

| 场景 | 当前判断依据 | 是否来自 Provider(提供商) |
|------|-------------|----------------------|
| 是否超轮次 | `current_iteration >= max_turns`(当前轮次 >= 最大轮次) | ❌ Pipeline(管道) 内建 |
| Thought(思考结果) 类型 | `Thought::Action`(思考结果::动作) / `Thought::Final`(思考结果::最终) | ❌ StepContext(步骤上下文) 内建 |
| 跳转目标 Phase(阶段) | `think`(思考) | ❌ Phase(阶段) 核心类型 |

如果未来需要外部数据（如"查一下当前对话已经花了多少钱"），则通过 `ap_slot.provider_raw("billing")`(ap_slot.provider_raw("billing")) → `downcast`(向下转型) 获取。运行时不可用（`provider_raw()`(provider_raw()) 返回 `None`(无)）时：**不阻止执行，跳过该检查，只记录 warn(警告) 日志**（遵循 Slot 接入协议 §7——"插件应优雅降级或报错"）。

### 7.7 决策状态机

`LoopDecisionComponent`(循环决策组件) 的 `decide()`(决策()) 本质是一个小型状态机：

```
状态: {exceeded(是否超过), thought_type(思考类型)}
       │
       ├── (true, *)               → ForceBreak(强制跳出) [吸收态(已终止)]
       │
       └── (false, Action(动作))    → JumpToThink(跳转思考) [转移态(需重入)]
       │
       └── (false, Final(最终))     → Continue(继续)       [吸收态(已结束)]
       │
       └── (false, None(无))       → Continue(继续)       [吸收态(已结束)]
```

| 当前状态转换 | 条件 | 新状态 | 含义 |
|-------------|------|--------|------|
| LOOP(循环) → FORCE_BREAK(强制跳出) | `exceeded == true`(是否超过 == true) | Step(步骤) 结束 | 防止 agent(代理) 无限工具循环 |
| LOOP(循环) → JUMP_BACK(跳回) | `exceeded == false && Action`(是否超过 == false && 动作) | THINK(思考) | 继续 ReAct(ReAct) 循环 |
| LOOP(循环) → CONTINUE(继续) | `exceeded == false && (Final(最终) \| None(无))`(是否超过 == false && (最终 | 无)) | 下一 Phase(阶段) | 正常结束 |

遵循红线6（状态机规范），所有状态转换显式定义在 `match`(匹配) 表达式中，无隐式状态穿越。

---

## 8. 红线对照表

| 编号 | 红线 | 内容 | 本模块执行情况 | 证明位置 |
|------|------|------|--------------|---------|
| **红线1** | 输入验证 | 所有外部输入在边界处校验 | `TurnLimitComponent.init()`(轮次限制组件.init()) 校验 `max_turns >= 1`，不满足则自动提升 | §3.1 init() |
| **红线2** | 错误隔离 | 每个组件独立处理自身错误 | 每个 Component(组件) 返回 `Result<_, ComponentError>`(结果<_, 组件错误>)，Slot(槽口) 统一转换为 `PluginError`(插件错误) | §1.5 完整实现 |
| **红线5** | 接口通信 | 模块之间只能通过接口通信 | 只依赖 `SlotPlugin`(槽口插件) trait(特质) + `SlotAccessPoint`(槽口访问点)；组件间通过 `InternalAccessPoint.call()`(内部访问点.call()) + `downcast`(向下转型) | §1.3、§2.3 |
| **红线6** | 状态机规范 | 所有状态转换走显式状态机 | `LoopDecisionComponent.decide()`(循环决策组件.decide()) 是一个完整覆盖 3 种状态 4 条路径的状态机 | §7.7 |
| **红线13** | 测试要求 | 每个 `pub fn` 必须有至少一个测试 | `TurnLimitService`(轮次限制服务) 的 2 个方法 + `LoopDecisionService`(循环决策服务) 的 `decide()`(决策()) 均有测试用例 | §5.3 |
| **红线18** | 函数长度 | 每个函数 ≤ 50 行 | `run()`(执行) ≈ 35 行；`decide()`(决策()) ≈ 25 行；`init()`(初始化()) ≈ 10 行 | §1.5、§3.2、§3.1 |
| **红线21** | 禁止 expect/unwrap(expect/unwrap) 在生产代码 | 只在测试中允许 | 所有代码使用 `?`(?) 或 `match`(匹配)，零 `unwrap()`(unwrap()) / `expect()`(expect()) | §1.5、§3.1、§3.2 |
| **S-R01(槽口红线01)** | 所有 `SlotDirective`(槽口指令) 变体必须被正确处理 | 7 个变体 | 3 个在 `run()`(执行) 的 `match`(匹配) 中覆盖，4 个未使用的在 §1.4 注释说明语义 | §1.4 |
| **S-R02(槽口红线02)** | `init()`(初始化()) 失败意味着插件不加载 | 失败后不允许退化运行 | `init_all()`(全初始化()) 返回 `Err`(错误) → `Slot::init()`(槽口::init()) 返回 `Err`(错误) → PluginLoader(插件加载器) 不加载 | §1.5、§4.2 |
| **S-R03(槽口红线03)** | `run()`(执行()) 中禁止持有跨次调用的可变状态 | 两次 `run()`(执行()) 间无隐式状态依赖 | 所有数据（`iteration`(轮次)、`thought`(思考结果)）在 `run()`(执行()) 内局部构造；组件中 `max_turns`(最大轮次) 在 `init()`(初始化()) 后只读不变 | §1.5、§3.1 |
| **C-R01(组件红线01)** | `AccessPoint::call()`(访问点::call()) 获取句柄后必须 downcast(向下转型) | 禁止将 `ComponentHandle`(组件句柄) 作为通信媒介 | `call("turn_limiter")`(call("turn_limiter")) 后立即 `downcast_ref::<dyn TurnLimitService>()`(downcast_ref::<dyn TurnLimitService>()) | §3.2 |
| **C-R02(组件红线02)** | `meta().requires`(元数据().requires) 声明必须真实可验证 | 声明依赖必须在代码中实际调用 | `LoopDecisionComponent`(循环决策组件) 声明 `requires: ["turn_check"]`(requires: ["turn_check"])，`decide()`(decide()) 中实际调用 `turn_limit.is_exceeded()`(turn_limit.is_exceeded()) | §3.2、§2.2 |
| **C-R03(组件红线03)** | `process()`(process()) 必须可重入 | 不应假设只被调用一次 | 两个 Component(组件) 的 `process()`(process()) 均为 no-op(无操作)，天然满足 | §3.1、§3.2 |

---

> 本文档严格遵循以下协议编写，每条设计决策均可追溯到协议具体条款：
>
> - **Slot 接入协议** — §1 SlotPlugin trait(槽口插件特质)、§2 SlotAccessPoint(槽口访问点)、§3 元数据声明、§4 权限枚举、§5 SlotDirective(槽口指令)、§6 生命周期、§9 红线
> - **模块内部组件协议** — §1 Component trait(组件特质)、§2 ComponentHandle(组件句柄)、§3 AccessPoint(访问点)、§4 Processing(处理结果)、§5 ComponentMeta(组件元数据)+Orchestrator(协调器)、§6 模块边界规范、§11 红线
> - **跨平台与硬编码规范** — §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单
>
> 与三份协议冲突时以协议原文为准。
