# Core 设计文档

> 依据重构后的代码状态更新（2026-05-28）。
> 对齐协议：`protocol-Slot接入协议.md`、`protocol-Service集成协议.md`、`protocol-模块内部组件协议.md`

---

## 一、定位

Core 是 aagnet 微内核的最底层基础设施，定义整个框架的**扩展接口**、**执行引擎**和**会话状态管理**。

**核心三件事**：
1. **定义扩展接口**：`SlotPlugin`、`ServicePlugin`、`Component`、`Phase`
2. **提供执行引擎**：`Pipeline`、`AgentRuntime`
3. **管理会话状态**：`StepContext`、`SessionState`

**关键设计决策**：
- Core 不定义任何业务接口（如记忆查询、工具调用、事件订阅）
- 业务能力通过 **Provider 扩展机制** 由 Service 注册、Slot 按需查找
- 业务类型（`Thought`/`Action`/`Observation` 等）全部迁出至 `plugins/` 和 `shared_types/`
- Core 只做三件事：定义插件框架、路由 Provider 查找、提供模块内部组件协议

**红线约束**：
- Core **禁止依赖** `plugins/` 下的任何具体类型
- 所有公开类型必须实现 `Clone + Debug + Send + Sync`
- Core 对外仅通过 `access::SlotAccessPoint` 和 `access::ServiceAccessPoint` 暴露
- Core **不为 Provider 鉴权**——Provider 级访问控制由 Provider 自身负责

**不做的**：
- ❌ 不包含任何业务逻辑（思考、工具调用、记忆等全部在 plugins/ 中实现）
- ❌ 不直接与文件系统、网络、LLM 交互
- ❌ 不感知任何插件内部结构
- ❌ 不定义 `MemoryProvider`、`ToolProvider` 等业务接口
- ❌ 不提供任务调度、事件总线、定时任务等能力

---

## 二、文件结构

```
src/
├── core/
│   ├── mod.rs                 # 模块声明 + 核心 API 重导出（仅基础设施类型）
│   ├── context.rs             # StepContext / AgentHandle / StepInput
│   ├── phase.rs               # Phase 阶段标识符
│   ├── pipeline.rs            # Pipeline 执行引擎
│   ├── runtime.rs             # AgentRuntime 主循环 + SharedMessageStore + SessionState
│   ├── service.rs             # ServicePlugin 接口 + ServiceSignal
│   ├── slot.rs                # SlotPlugin 接口 + SlotDirective
│   ├── component.rs           # Component trait + InternalAccessPoint（模块内部协议）
│   ├── access/
│   │   └── mod.rs             # SlotAccessPoint / ServiceAccessPoint + ProviderRegistry
│   └── types/
│       ├── mod.rs             # 基础设施类型（Timestamp / Version / CancellationToken）
│       ├── error.rs           # 错误类型体系（PluginError / AgentError）
│       ├── persistence.rs     # 持久化命令（PersistenceCommand / PersistenceAck）
│       └── plugin.rs          # PluginInitContext / PluginMetadata / AgentConfig / RunMode
│
├── shared_types/              # 跨插件共享数据契约（core 和 plugins 共同依赖）
│   ├── mod.rs
│   ├── message.rs             # Message / MessageRole / ContentBlock / ToolCall
│   └── step_response.rs       # StepResponse（通用变体，无 ReAct 语义）
│
├── plugins/                   # 插件层（所有业务逻辑）
│   └── slots/
│       └── llm_thinker/
│           └── types.rs       # Thought / Action / Observation / ActionResult / Turn（ReAct 类型）
│
└── infra/                     # 基础设施
    ├── config/                # 配置加载
    └── metadata/
        └── descriptor.rs      # ComponentDescriptor / DescriptorKind（组件描述符）
```

> **业务类型已迁出 core**：`Message`/`ContentBlock` → `shared_types/`，`Thought`/`Action`/`Observation` → `plugins/slots/llm_thinker/types.rs`，`ComponentDescriptor` → `infra/metadata/`，`ToolDefinition` → `plugins/slots/tool_registry/types.rs`。

---

## 三、Core 目录文件详细说明

### 3.1 文件总览表

| 序号 | 文件名 | 类别 | 公开类型数 | 核心作用 |
|------|--------|------|-----------|---------|
| 1 | `mod.rs` | 入口 | 15+ | 模块声明 + 核心基础设施 API 重导出 |
| 2 | `context.rs` | 状态 | 3 | 步骤执行上下文、运行时句柄、步骤输入 |
| 3 | `phase.rs` | 类型 | 1 | 透明的阶段标识符 |
| 4 | `pipeline.rs` | 引擎 | 1 | 阶段编排引擎，顺序执行 Slot |
| 5 | `runtime.rs` | 引擎 | 4 | Agent 主循环 + SharedMessageStore + SessionState + 配置注入 |
| 6 | `service.rs` | 接口 | 2 | ServicePlugin trait + ServiceSignal |
| 7 | `slot.rs` | 接口 | 3 | SlotPlugin trait + SlotDirective + SlotEntry |
| 8 | `component.rs` | 接口 | 6 | Component trait + InternalAccessPoint + Processing（模块内部协议） |
| 9 | `access/mod.rs` | 协议 | 3 | SlotAccessPoint / ServiceAccessPoint + ProviderRegistry |
| 10 | `types/mod.rs` | 类型 | 3 | 基础设施类型（Timestamp / Version / CancellationToken） |
| 11 | `types/error.rs` | 类型 | 2 | 错误类型体系 |
| 12 | `types/persistence.rs` | 类型 | 2 | 持久化命令 |
| 13 | `types/plugin.rs` | 类型 | 4 | 插件初始化上下文、元数据、配置、运行模式 |

### 3.2 各文件详细说明

---

#### 文件 1：`mod.rs` —— 模块声明 + 重导出

| 属性 | 值 |
|------|-----|
| **职责** | 声明所有子模块，统一重导出核心基础设施 API |
| **公开命名空间** | `aagnet::core::` |
| **依赖** | `shared_types`（Message 等跨插件类型） |
| **注意** | 不再重导出业务类型（Message/Thought/Action 等已迁至 shared_types / plugins） |

**重导出清单**：

| 分类 | 类型 |
|------|------|
| 扩展接口 | `SlotPlugin`、`SlotDirective`、`ServicePlugin`、`ServiceSignal` |
| 接入协议 | `SlotAccessPoint`、`ServiceAccessPoint`、`ProviderRegistry` |
| 执行引擎 | `Pipeline`、`AgentRuntime` |
| 阶段 | `Phase` |
| 上下文 | `StepContext`、`StepInput`、`AgentHandle` |
| 插件基础设施 | `PluginInitContext`、`PluginMetadata`、`AgentConfig`、`RunMode`、`PluginError`、`AgentError` |
| 基础设施类型 | `Timestamp`、`Version`、`CancellationToken` |

---

#### 文件 2：`context.rs` —— 步骤上下文

| 属性 | 值 |
|------|-----|
| **核心类型** | `StepContext`、`AgentHandle`、`StepInput` |
| **职责** | 1. `StepContext`：存储单步执行的全部状态（消息列表、当前轮次、通用上下文数据、步骤结果）<br>2. `AgentHandle`：对外暴露的运行时句柄，通过 mpsc 通道触发 step<br>3. `StepInput`：封装步骤输入（session_id、消息内容、响应通道） |
| **关键设计** | - `StepContext` 同时实现了 `SlotAccessPoint`，作为 Slot 与核心的受控通道<br>- 持有 `Arc<ProviderRegistry>` 引用，实现 `provider_raw()` 查找<br>- `pending_directive` 使用 `Cell` 实现内部可变性<br>- ReAct 专属字段（thought/action/observation）已移除，改用 `step_result: Option<Box<dyn Any + Send>>` 通用承载<br>- `data` 字段按 String key 索引（`set_context`/`get_context`），替代旧 `set_data`/`get_data`<br>- 内置权限校验（`check_permission`），基于 `PluginMetadata.permissions` |
| **生命周期** | 每次 Step 新建一个 `StepContext`，Pipeline 执行完成后写回 shared_store |

---

#### 文件 3：`phase.rs` —— 阶段标识符

| 属性 | 值 |
|------|-----|
| **核心类型** | `Phase` |
| **职责** | 透明的阶段标识符字符串包装，核心不做任何语义假设 |
| **预定义的常量阶段** | `init`、`context`、`think`、`audit`、`execute`、`loop`、`memorize` |
| **关键特性** | 实现了 `Serialize`、`Deserialize`、`Hash`、`Eq`，可用作 HashMap 的 key<br>支持任意自定义阶段名称 |
| **设计意图** | 将阶段的语义完全交由 plugins/ 定义，core 只提供编排容器 |

---

#### 文件 4：`pipeline.rs` —— 管道执行引擎

| 属性 | 值 |
|------|-----|
| **核心类型** | `Pipeline` |
| **职责** | 按注册顺序遍历阶段和 Slot，处理 SlotDirective 流程控制指令 |
| **关键方法** | - `add_phase` / `insert_phase_before` / `insert_phase_after` / `remove_phase`：阶段编排<br>- `add_slot` / `register`：Slot 注册<br>- `run`：主执行循环<br>- `validate`：校验 Pipeline 完整性（阶段非空、每阶段至少一个 Slot） |
| **返回类型** | `Result<StepResponse, AgentError>`——`StepResponse` 为通用变体（`Completed`/`Interrupted`/`RestartRequested`/`LimitReached`），不含 ReAct 语义 |
| **设计要点** | - 使用 `HashMap<Phase, Vec<SlotEntry>>` 存储 Slot，按注册顺序执行<br>- 后向跳转（JumpTo）使用 `max_backward_jumps` 计数器防死循环（默认 10 次）<br>- 所有执行步骤均记录时间戳和耗时<br>- 支持 `with_recommended_phases()` 快速创建含 7 个推荐阶段的 Pipeline |

---

#### 文件 5：`runtime.rs` —— Agent 运行时

| 属性 | 值 |
|------|-----|
| **核心类型** | `AgentRuntime`、`SessionState`、`SharedMessageStore` |
| **职责** | 1. 管理多个会话的生命周期<br>2. 运行主循环，接收 `StepInput`，驱动 Pipeline 执行<br>3. 维护消息上下文窗口（保留最近 10 条 + System 消息，超限裁剪最早的非 System 消息）<br>4. 桥接 `SharedMessageStore` 与持久化通道（CAS 写防丢失）<br>5. 持有 `ProviderRegistry` 引用，供 Service 注册、Slot 查找 |
| **关键方法** | - `new_with_config`：带配置创建（推荐方式，由 main.rs 从 TOML 注入）<br>- `run`：异步主循环<br>- `step`：直接单步执行<br>- `create_service_access_point`：使用运行时持有的 `config` 创建 ServiceAccessPoint |
| **关键设计** | - `SharedMessageStore` 从 types/ 迁入 runtime.rs（它是运行时组件，不是纯类型）<br>- `AgentConfig` 不再硬编码默认值，由 `new_with_config` 注入<br>- 消息超限裁剪策略：保留 System 消息 + 最近 10 条，裁剪最早的非 System 消息 |

##### ProviderRegistry 设计

```rust
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self;

    /// 注册 Provider（T 必须 'static 以便 downcast）
    pub fn register<T: Send + Sync + 'static>(&self, name: &str, provider: Arc<T>);

    /// 按名称和类型查找 Provider
    pub fn get<T: Send + Sync + 'static>(&self, name: &str) -> Option<Arc<T>>;

    /// 按名称查找原始 Provider（类型擦除，用于 trait 对象安全）
    pub fn get_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>>;

    /// 反注册 Provider
    pub fn unregister(&self, name: &str);
}
```

Core **不定义任何 Provider 接口**，只提供按名称查找和类型向下转型的机制。
`register`/`get` 要求 `T: 'static` 以满足 `downcast` 约束。
`get_raw` 用于 `SlotAccessPoint::provider_raw()` 的 trait 对象安全实现。

---

#### 文件 6：`service.rs` —— 服务插件接口

| 属性 | 值 |
|------|-----|
| **核心类型** | `ServicePlugin` trait、`ServiceSignal` enum |
| **职责** | 定义后台服务的接口规范——独立于 Pipeline 运行的服务，通过 `register_provider` 暴露业务能力 |
| **接口方法** | `name()`、`init(&mut self, ctx: &PluginInitContext)`、`start(&mut self, ap: ServiceAccessPoint)`、`handle_signal(&mut self, signal)`、`stop(&mut self)`、`shutdown(&mut self)` |
| **信号类型** | `GracefulShutdown`、`ImmediateShutdown`、`ConfigReload`、`HealthCheck`、`Suspend`、`Resume` |
| **设计要点** | - 所有方法接收 `&mut self`，允许修改内部状态<br>- `start()` 返回 `Result<(), PluginError>`，可报告启动失败<br>- 通过 `ServiceAccessPoint::register_provider()` 注册业务能力<br>- `shutdown()` 只调用一次，用于反注册 Provider 和释放资源 |

```rust
#[async_trait]
pub trait ServicePlugin: Send + Sync {
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 启动（传入受控访问句柄，在此注册 Provider）
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;

    /// 处理运行时信号
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>;

    /// 停止服务（暂停，不销毁）
    async fn stop(&mut self) -> Result<(), PluginError>;

    /// 销毁（只调用一次）
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

---

#### 文件 7：`slot.rs` —— 槽口插件接口

| 属性 | 值 |
|------|-----|
| **核心类型** | `SlotPlugin` trait、`SlotDirective` enum、`SlotEntry` struct |
| **职责** | 定义管道内处理单元的接口规范——所有业务功能以 SlotPlugin 形式接入 Pipeline |
| **接口方法** | `name()`、`init(&mut self, ctx: &PluginInitContext)`、`run(&mut self, ap: &mut dyn SlotAccessPoint)`、`shutdown(&mut self)` |
| **执行指令** | `Continue`、`BreakPhase`、`BreakStep`、`RestartStep`、`AbortStep`、`AbortPipeline`、`JumpTo(Phase)` |
| **设计要点** | - 完整的生命周期管理：init → run (多次) → shutdown<br>- `init` 中校验配置、通过 `PluginInitContext` 获取依赖信息<br>- `run` 通过 `SlotAccessPoint` 与核心交互，通过 `provider_raw(name)` + `downcast` 获取业务能力<br>- `phase()` 移出 trait，改为在 Pipeline 注册时通过 `add_slot(phase, slot)` 参数传入，存入 `SlotEntry` |

```rust
#[async_trait]
pub trait SlotPlugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 每次 Phase 触发时调用
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;

    /// 清理（只调用一次）
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

---

#### 文件 8：`access/mod.rs` —— 接入协议

| 属性 | 值 |
|------|-----|
| **核心类型** | `SlotAccessPoint` trait、`ServiceAccessPoint` struct、`ProviderRegistry` struct、`ServiceAccessImpl` trait |
| **职责** | 定义插件与核心交互的**受控通道**——插件不能直接访问 StepContext、AgentRuntime 等内部结构 |

##### SlotAccessPoint

```rust
pub trait SlotAccessPoint {
    // ── Core 内建 ──
    fn messages(&self) -> &[Message];
    fn session_id(&self) -> &str;
    fn phase_name(&self) -> &str;
    fn current_iteration(&self) -> usize;
    fn write_observation(&mut self, obs: Box<dyn Any + Send>) -> Result<(), PluginError>;

    /// 写入上下文数据（类型擦除，调用方自行装箱）
    fn write_context_raw(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), PluginError>;

    /// 读取上下文数据（类型擦除，调用方自行向下转型）
    fn read_context_raw(&self, key: &str) -> Option<&dyn Any>;

    fn request_jump(&self, phase: &str) -> Result<(), PluginError>;
    fn request_abort(&self) -> Result<(), PluginError>;

    // ── Provider 扩展：获取其他 Service 注册的能力 ──

    /// 按名称查找业务 Provider（返回类型擦除的 Arc，调用方通过 downcast 获取具体类型）
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>>;
}
```

> **技术说明**：`SlotAccessPoint` 作为 `dyn` trait 使用时必须保证对象安全，因此所有方法不能有泛型参数。`write_context_raw`/`read_context_raw`/`provider_raw` 均采用类型擦除方案——调用方自行装箱/向下转型。

| 方法 | 权限 tag | 说明 |
|------|---------|------|
| `messages()` | `messages:read` | 读取当前会话对话历史 |
| `session_id()` | 无 | 当前 Session ID |
| `phase_name()` | 无 | 当前 Phase 名称 |
| `current_iteration()` | 无 | 当前迭代次数 |
| `write_observation()` | `observation:write` | 写入工具观察结果 |
| `write_context_raw()` | `context:write` | 写入上下文数据 |
| `read_context_raw()` | `context:read` | 读取上下文数据 |
| `request_jump()` | `phase:jump` | 请求跳转到指定 Phase |
| `request_abort()` | `phase:abort` | 请求终止当前 Pipeline |
| `provider_raw()` | 无 | 按名称查找业务 Provider |

##### ServiceAccessPoint

```rust
#[derive(Clone)]
pub struct ServiceAccessPoint { ... }

impl ServiceAccessPoint {
    pub fn get_config(&self) -> AgentConfig;
    pub fn log(&self, level: &str, message: &str);

    /// 将本服务的 Provider 注册到运行时
    pub fn register_provider<T: Send + Sync + 'static>(
        &self,
        name: &str,
        provider: Arc<T>,
    );

    /// 反注册 Provider（shutdown 时调用）
    pub fn unregister_provider(&self, name: &str);
}
```

| 方法 | 说明 |
|------|------|
| `get_config()` | 读取 Agent 配置（从运行时注入的真实配置，非硬编码默认值） |
| `log()` | 向框架日志系统写入日志 |
| `register_provider()` | 注册业务 Provider |
| `unregister_provider()` | 反注册 Provider（Service shutdown 时调用） |

**设计要点**：
- `SlotAccessPoint` 由 `StepContext` 实现，但插件只见 AccessPoint 不见 Context
- `ServiceAccessPoint` 使用 `Arc<dyn ServiceAccessImpl>` 实现 Clone，可在多个任务间共享
- `register_provider` 将 Provider 存入 `ProviderRegistry`，`provider_raw()` 从中查找
- Core 不做 Provider 级鉴权——鉴权由 Provider 接口自行设计

---

#### 文件 9：`types/mod.rs` —— 基础设施类型

| 属性 | 值 |
|------|-----|
| **包含的类型** | `Timestamp`、`Version`、`CancellationToken`（仅 3 个基础设施类型） |
| **设计原则** | - 消除对 `chrono`、`semver`、`tokio-util` 的依赖，自实现轻量替代<br>- 所有类型实现 `Clone + Debug + Send + Sync`<br>- 业务类型（Message/Thought/Action/ToolDefinition 等）已全部迁出 |

**关键类型详解**：

| 类型 | 用途 | 关键设计 |
|------|------|---------|
| `Timestamp` | 毫秒级 Unix 时间戳 | 自实现 RFC3339/紧凑格式格式化，不依赖 chrono |
| `Version` | 语义化版本号 | 自实现 Parse/Display，不依赖 semver crate |
| `CancellationToken` | 轻量取消令牌 | 基于 AtomicBool，不依赖 tokio-util |

#### 文件 10：`component.rs` —— 模块内部组件协议

| 属性 | 值 |
|------|-----|
| **核心类型** | `Component` trait、`Processing` enum、`InternalAccessPoint` trait、`ComponentHandle` trait、`ComponentError` enum、`ComponentMeta` struct |
| **职责** | 将 Slot/Service 的外部协议范式镜像到模块内部，使模块内功能单元有统一的接口和隔离边界 |
| **设计要点** | - `Component` 三阶段生命周期：`init` → `process`（多次）→ `shutdown`<br>- `InternalAccessPoint` 禁止直接引用兄弟组件的具体类型，通过 `call(name)` + `downcast` 间接调用<br>- `Processing` 枚举控制流程（Continue/BreakChain/Restart/Warn）<br>- 与 `protocol-模块内部组件协议.md` 对齐 |

> **迁移说明**：原 `types/data_contract.rs`（`ComponentDescriptor`/`DescriptorKind` 等）已迁至 `infra/metadata/descriptor.rs`，不属于 core。

---

#### 文件 11：`types/error.rs` —— 错误类型体系

| 属性 | 值 |
|------|-----|
| **包含的类型** | `PluginError`、`AgentError` |
| **设计原则** | - 统一错误类型，消除冗余<br>- `SlotError` 和 `ServiceError` 合并入 `PluginError`，统一插件级错误处理 |

**核心变更**：
- ❌ 废弃 `SlotError`（7 个变体）——统一使用 `PluginError`
- ❌ 废弃 `ServiceError`（2 个变体）——统一使用 `PluginError`

**错误类型详解**：

| 类型 | 变体数 | 用途 | 关键变体 |
|------|--------|------|---------|
| `PluginError` | 10 | 插件统一错误 | `InitFailed`、`Runtime`、`Config`、`PermissionDenied`、`NotFound`、`Timeout`、`Shutdown`、`DuplicateName`、`DependencyNotFound`、`Internal` |
| `AgentError` | 5 | Agent 顶层错误 | `PluginFailed`、`PipelineAborted`、`SessionError`、`RuntimeShuttingDown`、`Internal` |

```rust
impl From<PluginError> for AgentError { ... }
```

---

#### 文件 12：`types/persistence.rs` —— 持久化命令

| 属性 | 值 |
|------|-----|
| **核心类型** | `PersistenceCommand`、`PersistenceAck` |
| **职责** | 定义运行时与持久化工作进程之间的通信协议。运行时通过 mpsc 通道发送 `PersistenceCommand`，持久化工作进程处理后返回 `PersistenceAck` |
| **关键设计** | - `PersistenceCommand::SaveSession` 携带可选 ACK 通道（`oneshot::Sender`），调用方可等待确认<br>- `PersistenceCommand::Shutdown` 用于优雅关闭持久化进程 |

```rust
pub enum PersistenceCommand {
    SaveSession {
        session_id: String,
        messages: Vec<Message>,
        ack_tx: Option<oneshot::Sender<PersistenceAck>>,
    },
    Shutdown,
}

pub enum PersistenceAck {
    Ok { message_count: usize },
    Failed { reason: String, timestamp: Timestamp },
}
```

---

#### 文件 13：`types/plugin.rs` —— 插件基础设施

| 属性 | 值 |
|------|-----|
| **核心类型** | `PluginInitContext`、`PluginMetadata`、`AgentConfig` |
| **职责** | 定义插件初始化所需的环境信息、元数据声明格式、Agent 配置结构 |

##### PluginInitContext

```rust
/// 插件初始化上下文——每个插件在 init() 时收到
pub struct PluginInitContext {
    pub plugin_name: String,       // 插件名称
    pub plugin_config: Value,       // 插件专属配置段（JSON）
    pub agent_config: AgentConfig,  // Agent 全局配置
    pub data_dir: PathBuf,          // 插件数据目录
}
```

| 字段 | 说明 |
|------|------|
| `plugin_name` | 当前插件名称，与 YAML 元数据中的 `name` 一致 |
| `plugin_config` | 该插件的专属配置段（由 PluginLoader 从 TOML 解析并注入） |
| `agent_config` | Agent 全局配置（工作空间、agent_id 等） |
| `data_dir` | 插件可用的私有数据目录 |

##### PluginMetadata

```rust
/// 插件元数据声明——每个插件在 YAML 中声明，PluginLoader 读取校验
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub permissions: Vec<String>,   // Slot 用：声明需要 core 内建的哪些权限
    pub provides: Vec<String>,      // Service 用：声明注册哪些 Provider
    pub requires: Vec<String>,      // 依赖的其他插件/Provider
    pub conflicts: Vec<String>,
}
```

##### AgentConfig

```rust
pub struct AgentConfig {
    pub agent_id: String,
    pub workspace: PathBuf,
    pub log_level: String,
    pub data_dir: PathBuf,
}
```

---

## 四、架构分层图

```
┌──────────────────────────────────────────────────────────────────┐
│                     plugins/ (业务实现)                            │
│                                                                    │
│  ┌───────────────────────┐     ┌──────────────────────────────┐   │
│  │     Slot 插件          │     │      Service 插件             │   │
│  │  ┌─────────────────┐  │     │  ┌────────────────────────┐  │   │
│  │  │ impl SlotPlugin  │  │     │  │ impl ServicePlugin    │  │   │
│  │  │   run(ap) {      │  │     │  │   start(ap) {         │  │   │
│  │  │     let tool =   │  │     │  │     ap.register_      │  │   │
│  │  │     ap.provider_raw│  │     │  │     provider("tool",  │  │   │
│  │  │     <dyn Tool>   │  │     │  │     Arc::new(...));   │  │   │
│  │  │     ("tool");    │  │     │  │   }                   │  │   │
│  │  │   }              │  │     │  └────────────────────────┘  │   │
│  │  └─────────────────┘  │     └──────────────────────────────┘   │
│  └───────────────────────┘     └──────────────────────────────┘   │
├──────────────────────────────────────────────────────────────────┤
│                      core/ (微核)                                 │
│                                                                    │
│  ┌──────────────────────┐    ┌────────────────────────────────┐   │
│  │   access::            │    │   Pipeline                    │   │
│  │  SlotAccessPoint {    │    │   Phase[0] → Slot[0..N]      │   │
│  │    messages()         │    │   Phase[1] → Slot[0..N]      │   │
│  │    write_observation()│    │   根据 SlotDirective 跳转      │   │
│  │    request_jump()     │    └──────────────┬─────────────────┘   │
│  │    provider_raw(n) ──┼─── ProviderReg    │                    │
│  │  }                   │    ┌──────────────▼─────────────────┐   │
│  │  ServiceAccessPoint { │    │   AgentRuntime                 │   │
│  │    get_config()       │    │   - 主循环（mpsc 接收）        │   │
│  │    register_provider ─┼───►│   - 会话管理                   │   │
│  │    log()              │    │   - shared_store 桥接          │   │
│  │  }                    │    │   - 持久化通知                  │   │
│  └──────────────────────┘    └────────────────────────────────┘   │
│                                                                    │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │  ProviderRegistry (Arc 共享)                                │  │
│  │  "memory" → Arc<dyn MemoryProvider>                         │  │
│  │  "tool"   → Arc<dyn ToolProvider>                           │  │
│  │  "event"  → Arc<dyn EventProvider>                          │  │
│  │  "task"   → Arc<dyn TaskProvider>                           │  │
│  │  Core 不定义上述接口——由注册方自行定义                        │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                    │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │  types/ (数据类型层)                                        │  │
│  │  Timestamp / Message / Thought / Action / Observation       │  │
│  │  StepResponse / ToolDefinition / ComponentDescriptor        │  │
│  │  PluginError / AgentError / PluginInitContext / AgentConfig  │  │
│  └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

---

## 五、数据流

### 5.1 主数据流（Pipeline 执行）

```
用户输入 / 外部事件
       │
       ▼
  StepInput ──→ AgentRuntime.run()
                     │
                     │ 1. 从 shared_store 读取历史消息
                     │ 2. 追加用户消息
                     │ 3. 创建 StepContext（注入 Arc<ProviderRegistry>）
                     ▼
                Pipeline.run(ctx)
                     │
                     │ 遍历 Phase[0..N]
                     │   遍历 Slot[0..N]
                     │       执行 slot.run(&mut ctx)
                     │          ├─ 内建方法：messages() / write_observation() 等
                     │          └─ 扩展：provider_raw("tool") + downcast 查找
                     │       获取 SlotDirective
                     │          ┌─ Continue    → 继续
                     │          ├─ BreakPhase  → 跳出当前阶段
                     │          ├─ BreakStep   → 返回结果
                     │          ├─ RestartStep → 重试
                     │          ├─ AbortStep   → 报错
                     │          ├─ AbortPipeline→ 终止
                     │          └─ JumpTo      → 跳到指定阶段
                     ▼
                StepResponse
                     │
                     │ 4. 写回 shared_store
                     │ 5. 异步持久化（可选）
                     │ 6. 发送结果（若请求了响应通道）
                     ▼
               返回给调用方 / 继续下一轮
```

### 5.2 Provider 注册与查找流

```
Service 启动时：
  ServicePlugin::start(ap)
    → ap.register_provider("memory", Arc::new(L2Provider))
    → ap.register_provider("vector", Arc::new(VectorProvider))
    → ProviderRegistry 存储： "memory" → Arc<dyn L2Provider>
                            "vector" → Arc<dyn VectorProvider>

Slot 执行时：
  SlotPlugin::run(ap)
    → let mem = ap.provider_raw("memory") → downcast::<dyn L2Provider>()
    → match mem {
        Some(p) => p.read("/user/prefs"),
        None => fallback()  // Provider 未注册，优雅降级
      }
```

---

## 六、边界与依赖规则

| 方向 | 规则 |
|------|------|
| **core → 外部** | core 禁止依赖 `plugins/` 下的任何类型 |
| **plugins → core** | 插件只能通过 `SlotAccessPoint` / `ServiceAccessPoint` 与核心交互 |
| **plugins ↔ plugins** | 通过 Provider 机制间接调用，不直接引用对方具体类型 |
| **core → infra** | core 不直接引用 infra 模块；通过 `PersistenceCommand` 通道松耦合通信 |
| **types → 外部** | `types/` 下的类型不依赖任何业务 crate |
| **公开类型要求** | 所有公开类型必须实现 `Clone + Debug + Send + Sync` |
| **Provider 接口** | Core 不定义、不感知任何 Provider 接口——由注册方自行定义 |

---
