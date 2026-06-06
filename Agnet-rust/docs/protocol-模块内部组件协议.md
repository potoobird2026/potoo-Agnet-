# 模块内部组件协议（Module Internal Component Protocol）

## 0. 本协议解决的问题

当前各模块（compression / memory / chronos / assembler）的内部子模块**各自为战**：

```
当前问题                          本协议的解决方案
─────────────                    ─────────────────
无内部 trait 抽象                 → 组件统一实现 Component trait
子模块直接跨引用兄弟模块            → 通过 InternalAccessPoint 间接通信
编排逻辑混在业务代码中              → 剥离给 Orchestrator
所有内部类型对外 pub use            → 只暴露 Orchestrator 和 Config
```

本协议将**外部协议（Slot/Service）的范式镜像到模块内部**，使每个模块成为有边界、自约束的子系统。

---

## 1. 组件单入口

模块内部每个功能单元统一实现 `Component`：

```rust
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    /// 组件标识
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError>;

    /// 核心处理逻辑
    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError>;

    /// 资源清理（只调用一次）
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}
```

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `init` | 1 | 校验配置、建立连接、分配资源 |
| `process` | 多次 | 执行业务逻辑，通过 AccessPoint 读写 |
| `shutdown` | 1 | 释放资源、持久化状态 |

### 对比当前散装接口

以 compression 模块为例，本协议强制后的效果：

```
当前写法（各自为战）              → 本协议（统一入口）
────────────────────             ────────────────────────
PidController::update(err, Δ)    → impl Component::process()
Scorer::score_message(msg)       → impl Component::process()
HierarchicalUCB::get_ucb(cat)    → impl Component::process()
FuzzyController::decide(metrics) → impl Component::process()
AnchorCalculator::get_window()   → impl Component::process()
Compressor::compress_range()     → impl Component::process()
FeedbackEngine::detect_loss()    → impl Component::process()
```

每个具体业务的差异体现在 `process()` 内部，对外接口统一、可互换、可测试。

---

## 2. 组件句柄（跨组件调用的桥梁）

兄弟组件之间通过 `ComponentHandle` 间接调用。调用者拿到句柄后，通过 `as_any()` 向下转型到具体类型接口，获得类型安全的方法。

```rust
/// 最小公共句柄——调用者通过 downcast 获得具体类型的接口
pub trait ComponentHandle: Send + Sync {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// 自动为所有 Component 实现 ComponentHandle
impl<T: Component + 'static> ComponentHandle for T {
    fn name(&self) -> &str { self.name() }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
```

---

## 3. 内部数据共享通道

组件之间、组件与模块基础设施之间，通过 `AccessPoint` 间接通信，**禁止直接引用兄弟组件的具体类型**。

```rust
pub trait AccessPoint: Send + Sync {
    // ── 数据读写（零成本 Any 传递，无序列化开销） ──
    fn read<T: 'static>(&self, key: &str) -> Option<&T>;
    fn write<T: 'static>(&mut self, key: &str, val: T) -> Result<(), ComponentError>;

    // ── 调用兄弟组件（按名称查找，返回后 downcast） ──
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;

    // ── 查询模块级状态 ──
    fn config(&self) -> &ModuleConfig;
    fn metrics(&self) -> &MetricsHandle;

    // ── 日志 ──
    fn log(&self) -> &dyn ModuleLogger;
}
```

> **设计取舍**：模块内部组件运行在同一进程同一地址空间，因此 `read`/`write` 使用 `Box<dyn Any>` 零成本传递，不做 Serde 序列化。跨模块（Slot/Service）通信才需要序列化。

### 3.1 对比当前跨引用写法

```
当前写法（直接跨引用具体类型）                         本协议（通过 AccessPoint + downcast）
────────────────────────────                         ───────────────────────────────────────
// memory/l2_working/slot.rs:19                       // 不 import L3 任何具体类型
use l3_vector::manager::VectorStoreManager;           let handle = ap.call("l3")?;
                                                      let l3 = handle.as_any()
                                                          .downcast_ref::<VectorSearchComponent>()?;
                                                      l3.search(query).await?;

// experience_extract/service.rs                       // 不 import L2 任何具体类型
use l2_working::WorkingMemoryManager;                 let handle = ap.call("working_memory")?;
                                                      let wm = handle.as_any()
                                                          .downcast_ref::<WorkingMemoryComponent>()?;
                                                      wm.write(entry).await?;

// assembler/compaction/doc_compactor.rs               // 不 import compression 任何具体类型
use compression::EntityExtractor;                     let handle = ap.call("entity_extractor")?;
                                                      let ee = handle.as_any()
                                                          .downcast_ref::<ExtractorComponent>()?;
                                                      ee.extract(text).await?;
```

> **注意**：`downcast_ref` 要求调用方知道目标组件的具体 trait 类型。这是有意为之——组件之间如果有通信需求，必然意味着知道对方的能力接口。关键是不直接引用对方的具体实现 struct，只引用接口 trait。

### 3.2 AccessPoint 实现者的职责

```rust
struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    data_share: HashMap<String, Box<dyn Any + Send>>,
    module_config: Arc<ModuleConfig>,
    logger: ModuleLogger,
}

impl AccessPoint for InternalAccessPointImpl {
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError> {
        self.components.get(name)
            .cloned()
            .ok_or_else(|| ComponentError::NotFound(name.to_string()))
    }

    fn read<T: 'static>(&self, key: &str) -> Option<&T> {
        self.data_share.get(key)
            .and_then(|v| v.downcast_ref::<T>())
    }

    fn write<T: 'static>(&mut self, key: &str, val: T) -> Result<(), ComponentError> {
        self.data_share.insert(key.to_string(), Box::new(val));
        Ok(())
    }
}
```

> **关键约束**：`InternalAccessPointImpl` 由 Orchestrator 统一构造并注入，组件**无权自行构造或修改** AccessPoint。

---

## 4. 处理结果

替代当前零散的返回值：

```rust
pub enum Processing {
    /// 正常完成，继续串行链的下一个组件
    Continue,
    /// 阻断串行链，跳过剩余组件
    BreakChain,
    /// 重启流程（等价于重新 process）
    Restart,
    /// 标记错误，但不断链（仅记录日志，不产生副作用）
    Warn { message: String },
}
```

> `Warn` 仅用于日志记录，不会向后续组件传递任何数据。如果某个组件的 warn 需要影响后面的流程，应通过 `AccessPoint::write()` 写入共享数据区。

---

## 5. 组件元数据声明

每个组件附带一份声明，Orchestrator 启动时据此编排：

```rust
pub struct ComponentMeta {
    pub name: &'static str,            // 组件名
    pub version: &'static str,         // 语义版本
    pub priority: u8,                  // 优先级（越小越先执行）
    pub provides: &'static [&'static str],   // 提供的能力列表
    pub requires: &'static [&'static str],   // 依赖的能力列表
    pub config_key: Option<&'static str>,     // 对应配置段键名
}
```

### 声明示例

```rust
impl PidController {
    pub fn meta() -> ComponentMeta {
        ComponentMeta {
            name: "pid_controller",
            version: "0.1.0",
            priority: 10,
            provides: &["compression_intensity"],
            requires: &["token_counter"],
            config_key: Some("compression.pid"),
        }
    }
}
```

Orchestrator 在加载时按 `requires` / `provides` 做依赖检查，循环依赖报错。

---

## 5. Orchestrator（模块协调器）

### 5.1 定位

Orchestrator **不包含任何业务代码**，只做编排。它取代当前散落在各模块 `service.rs` / `slot.rs` 中的编排逻辑。

```
Service 入口（对外）             →  Orchestrator（对内）     →  Components（业务）
CompressionService::run()       →  CompressionOrch::run()  →  PidController::process()
CliChannel::run()               →  (不需要，功能单一)      →  Scorer::process()
AssemblerSlot::run()            →  (已内建 provider 循环)   →  UCB::process()
```

### 5.2 核心职责（且仅做这些）

```rust
pub struct Orchestrator<C: Component> {
    /// 按拓扑序排列的组件列表（依赖先，被依赖后）
    components: Vec<Box<dyn Component>>,
    /// 可并行执行的组件分组（同组内无依赖关系）
    parallel_groups: Vec<Vec<usize>>,
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ModuleConfig,
}

impl<C> Orchestrator<C> {
    /// 注册一个组件（自动拓扑排序+并行分组）
    pub fn register(&mut self, component: Box<dyn Component>) -> Result<(), ComponentError> {
        // 1. 校验 requires 是否满足（基于 provides）
        // 2. 检测循环依赖
        // 3. 拓扑排序，同层无依赖的组件归入同一 parallel_group
        // 4. 存储到 components
    }

    /// 全量初始化
    pub async fn init_all(&mut self) -> Result<(), ComponentError> {
        for c in &mut self.components {
            c.init(&InitContext::new(self.config.clone())).await?;
        }
    }

    /// 全量 process（同层并行，层间串行）
    ///
    /// 执行策略：
    /// - 按 parallel_groups 分层
    /// - 同组内无依赖关系，并发执行
    /// - 组间按拓扑序串行等待
    pub async fn process_all(&mut self, ap: &mut dyn AccessPoint) -> Result<(), ComponentError> {
        let mut ap = self.access_point.write().await;
        for group in &self.parallel_groups.clone() {
            let results: Vec<_> = futures::future::join_all(
                group.iter().map(|&idx| {
                    let comp = &mut self.components[idx];
                    async move { (idx, comp.process(&mut *ap).await) }
                })
            ).await;

            for (idx, result) in results {
                match result? {
                    Processing::Continue => continue,
                    Processing::BreakChain => return Ok(()),
                    Processing::Restart => return self.process_all(ap).await,
                    Processing::Warn { message } => {
                        tracing::warn!("[{}] {}", self.components[idx].name(), message);
                    }
                }
            }
        }
        Ok(())
    }

    /// 串行版（用于组件间有隐式顺序依赖的场景）
    pub async fn process_all_serial(&mut self, ap: &mut dyn AccessPoint) -> Result<(), ComponentError> {
        let mut ap = self.access_point.write().await;
        for c in &mut self.components {
            match c.process(&mut *ap).await? {
                Processing::Continue => continue,
                Processing::BreakChain => break,
                Processing::Restart => return self.process_all_serial(ap).await,
                Processing::Warn { message } => {
                    tracing::warn!("[{}] {}", c.name(), message);
                }
            }
        }
        Ok(())
    }

    /// 全量销毁
    pub async fn shutdown_all(&mut self) {
        for c in self.components.iter_mut().rev() {
            c.shutdown().await.ok();
        }
    }
}
```

> Orchestrator 默认使用串行模式（`process_all_serial`），适用于大多数模块。压缩引擎等高性能场景可选择 DAG 并行模式（`process_all`），通过 `register` 时自动构建依赖图。

### 5.3 使用示例（以 compression 为例）

```rust
let mut orch = Orchestrator::<CompressionComponent>::new(config);
orch.register(Box::new(TokenCounter::new()))?;
orch.register(Box::new(PidController::new()))?;
orch.register(Box::new(Scorer::new()))?;
orch.register(Box::new(HierarchicalUCB::new()))?;
orch.register(Box::new(Compressor::new(llm_contract)))?;
orch.register(Box::new(FeedbackEngine::new()))?;
// 依赖校验通过，按 priority 排序

// ---- 运行 ----
orch.init_all().await?;
orch.process_all(&mut ctx_access_point).await?;
orch.shutdown_all().await;
```

---

## 6. 模块边界规范

### 6.1 模块 `mod.rs` 只暴露三样东西

```rust
// ✅ 正确做法
pub struct CompressionService;     // 对外 Service 入口
pub struct CompressionConfig;      // 配置
// 内部所有 Component / AccessPoint / Orchestrator 全部 pub(crate)
```

```rust
// ❌ 当前做法（全部 pub use 裸奔）
pub use algorithms::*;     // 所有算法对外暴露
pub use engine::*;         // 所有引擎对外暴露
pub use storage::*;        // 所有存储对外暴露
```

### 6.2 依赖方向

```
┌──────────────┐
│  模块 mod.rs  │  （对外暴露的公共 API）
└──────┬───────┘
       │
       ▼
┌──────────────────────────────┐
│  Orchestrator                │
│  ├─ ProcessAll()             │
│  └─ 持有 Vec<Box<dyn Component>>  │
└──────────────────────────────┘
       │ 注入 InternalAccessPoint
       ▼
┌──────────────────────────────┐
│  Components                  │
│  ├─ PidController            │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ Scorer                   │──→ 依赖 AccessPoint，不依赖兄弟
│  ├─ HierarchicalUCB          │──→ 依赖 AccessPoint，不依赖兄弟
│  └─ ...                      │
└──────────────────────────────┘
```

- ✅ 组件只能看到 `AccessPoint`，不看到兄弟组件的具体类型
- ✅ 组件之间零直接引用
- ✅ 替换一个组件不影响其他组件

---

## 7. 迁移策略（从当前状态到本协议）

### 三步走

**第一步：给每个模块定义 `Component` trait + 元数据**
```
compression → CompressionComponent
memory      → MemoryComponent (L1/L2/L3 统一)
chronos     → ChronosComponent
```

**第二步：将现有业务逐步包装成 `impl Component`**
```
PidController → impl CompressionComponent
  原 update() 移入 process() 内部

VectorStoreManager → impl MemoryComponent (L3 部分)
  原直接调兄弟模块 → 改为 ap.call("...") + downcast

WorkingMemoryManager → impl MemoryComponent (L2 部分)
  原直接调兄弟模块 → 改为 ap.call("...") + downcast
```

**第三步：抽取 Orchestrator**
```
CompressionService::run() 中的编排逻辑 → CompressionOrch
原先 940 行的 service.rs：
  ├─ 编排逻辑 → 移入 Orchestrator
  ├─ 业务逻辑 → 移入各 Component
  └─ 对外接口（Service trait）→ 保留 50 行
```

---

## 8. 与外部协议的关系

```
外部协议（已存在）             内部协议（本文档）
─────────────────            ─────────────────
SlotPlugin / ServicePlugin   Component
SlotAccessPoint / ServiceAccessPoint  InternalAccessPoint
元数据 YAML                   ComponentMeta 结构体
Pipeline / AgentRuntime      Orchestrator
插件市场                     模块内组件注册
解决：模块↔框架解耦           解决：子模块↔子模块解耦
```

两者是**同一范式的两个层级**，一个管外部隔离，一个管内部约束。叠加后整条依赖链为：

```
AgentRuntime
  └─ Pipeline
       └─ SlotPlugin (via SlotAccessPoint)
            └─ AssemblerSlot
                 └─ Orchestrator (via InternalAccessPoint)
                      ├─ SystemPromptProvider Component
                      ├─ IdentityProvider Component
                      ├─ WorkingMemoryProvider Component
                      └─ VectorMemoryProvider Component
```

每一层都通过"单入口 trait + 受控访问通道"约束，不存在越级直接引用。

---

## 9. 设计决策与约束

### 9.1 `Component::process()` 为何如此泛化？

`Component` trait 的 `process()` 签名是极简的——不包含任何业务语义。这是有意为之：

- **单一性**：模块初始化、执行、销毁三个生命周期阶段整齐划一
- **可编排性**：Orchestrator 不需要知道具体业务，只是按序调用
- **可替换性**：任何组件只要实现了 `Component`，就可以插拔

如果组件需要暴露类型安全的方法（例如 `VectorSearchComponent` 有 `search()`），通过 `ComponentHandle::as_any()` 向下转型获得。这是 **Component 做生命周期，具体 trait 做业务接口** 的分层设计。

```rust
// 组件的业务接口单独定义，不塞进 Component
#[async_trait::async_trait]
pub trait VectorSearch: Send + Sync {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemoryItem>>;
}

// 具体组件同时实现 Component（生命周期）和 VectorSearch（业务）
pub struct VectorSearchComponent { /* ... */ }

#[async_trait::async_trait]
impl Component for VectorSearchComponent {
    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing> {
        // 由 Orchestrator 驱动，做定期维护任务
        Ok(Processing::Continue)
    }
}

#[async_trait::async_trait]
impl VectorSearch for VectorSearchComponent {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<MemoryItem>> {
        // 业务逻辑，由兄弟组件通过 call() + downcast 调用
    }
}

// 调用方
let handle = ap.call("vector_search")?;
let vs = handle.as_any().downcast_ref::<dyn VectorSearch>()?;
let results = vs.search("query", 10).await?;
```

### 9.2 为什么没有权限控制？

外部协议（Slot/Service）有 `permissions` 列表 + 运行时审计，但**模块内部协议不做权限检查**。

理由：
- **信任边界不同**：模块内部组件属于同一开发团队，由同一个 Orchestrator 管理，不需要防御性权限控制
- **性能**：高频调用路径（如 compression 每个消息都要过 scorer）避免运行时权限检查的开销
- **简化**：组件数量少（每模块 3-7 个），依赖关系在注册时由 `requires`/`provides` 静态校验

> 如果未来支持从外部加载第三方组件到模块内部，再引入 `permissions` 机制不迟。

### 9.3 组件注册是编译期静态的

`ComponentMeta` 使用 `&'static str`，意味着组件列表在编译时确定。v0.x 版本暂不支持从 YAML 或动态库热加载组件。

未来扩展方向：
- 基于 `requires`/`provides` 的能力发现（组件市场）
- 从动态库 `.so`/`.dll` 加载组件
- 运行时插件热替换

---

## 10. 新增/替换 Component 标准流程

### 在已有 Service 内新增 Component

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 新建组件文件 | `components/my_algo.rs` |
| 2 | 实现 `Component` trait + `fn meta()` | 同上 |
| 3 | 在 `components/orchestrator.rs` 注册 | 加一行 `orch.register(Box::new(MyAlgo::new()))?;` |
| 4 | 运行 `cargo check` | — |

**共需改 2 个文件**：新建组件文件 + 修改 `orchestrator.rs`

### 替换现有 Component

| 步骤 | 做什么 |
|------|--------|
| 1 | 确认新旧组件的 `ComponentMeta.provides` 一致（否则依赖方报错） |
| 2 | 确认新旧组件的 `ComponentMeta.requires` 是旧组件的子集 |
| 3 | 编写新 `impl Component`，替换原文件 |
| 4 | 若 `name` 不变，`orchestrator.rs` 无需修改 |
| 5 | 运行 `cargo check` |
| 6 | 运行单元测试（保证 `Processing` 返回值语义一致） |

**原则**：替换 Component 时 `orchestrator.rs` 通常不需修改，因为注册名不变。

---

## 11. 协议特有红线

以下是 Component 必须遵守的协议级红线，违反即违规：

| 编号 | 红线 | 说明 |
|------|------|------|
| C-R01 | **`AccessPoint::call()` 获取句柄后必须 downcast** | 拿到 `ComponentHandle` 后必须通过 `as_any()` 向下转型到具体业务 trait。禁止将 `ComponentHandle` 本身作为通信媒介传递数据。 |
| C-R02 | **`meta().requires` 声明必须真实可验证** | 声明依赖某个能力（如 `compression_intensity`）的组件必须在代码中实际调用该能力。虚假声明导致 Orchestrator 依赖检查通过但运行时缺失提供者。 |
| C-R03 | **`process()` 必须可重入** | 同一组件可能被 Orchestrator 多次调用 `process()`（每轮 Step 或每个定时周期）。组件不应假设 `process()` 只被调用一次，也不应在两次 `process()` 之间保留隐式状态。 |
| C-R04 | **模块入口必须触发已注册组件** | `ServicePlugin::start()` 中的主循环或 `SlotPlugin::run()` 必须实际调用已注册组件的 `process()`/业务方法。仅构造组件并注册到 Orchestrator 但不在主执行路径中触发，视为未完成集成。Component 字段定义在 struct 中但不在任何非测试方法中被读取，同属违反。 |

---

## 12. 集成验证（Integration Verification）

### 12.1 问题背景

当前多个模块（compression、chronos、memory、llm）存在同一模式：**内部组件全部实现并注册到 struct/Orchestrator，但模块对外接口（`run_loop`/`execute`/`chat`）从未调用它们。** 导致 12 个压缩组件、9 个调度组件、3 个 LLM 组件全部空转。

根本原因：协议 §5 要求将组件注册到 Orchestrator，§10 要求在 `orchestrator.rs` 注册，但**没有任何地方要求验证"注册后是否实际被调用"**。

### 12.2 集成检查清单

以下检查在模块开发完成后、PR 合并前必须逐项执行：

```
□ 1. 模块对外入口是否调用了内部组件？
   检查方法：
   - ServicePlugin → start() 中的 run_loop / tokio::spawn 循环体
   - SlotPlugin → run() 方法体
   如果循环体只有状态切换或日志，没有调用任何已注册组件的业务方法，判定未完成。

□ 2. 每个 struct 字段是否至少在一个非测试方法中被读取？
   检查方法：
   cargo clippy --lib 2>&1 | Select-String "field.*is never read"
   或：
   rg "field `\w+` is never read"
   所有 field is never read 警告必须逐条确认：
   - 是预留字段 → 加 #[allow(dead_code)] 并注释用途
   - 是遗漏集成 → 补上调用

□ 3. 所有 execute()/process()/业务方法不是占位符？
   检查方法：
   rg "暂为占位|TODO.*executed\s*\+=\s*|executed\s*\+=\s*1" src/
   搜索结果为 0。

□ 4. 所有 Arc::new(()) 占位符已替换？
   检查方法：
   rg "Arc::new\(\(\)\)" src/plugins/services/*/service.rs
   所有注册到 ProviderRegistry 的值必须是真实实现，不允许空元组占位。

□ 5. Orchestrator 的 process_all() 是否被主循环调用？
   检查方法：
   - 如果模块使用 Orchestrator，在 run_loop 或主处理函数中搜索 orch.process_all 或 process_all_serial
   - 如果模块不使用 Orchestrator（如 MemoryService 直接调用子服务），验证每个子服务字段被读取

□ 6. SlotPlugin::run() 或 ServicePlugin::start() 是否完整
   检查方法：
   - 确保所有读取的 Provider 值被使用（非仅 unwrap/log）
   - 确保所有写入 StepContext/ServiceAccessPoint 的数据符合设计文档预期
```

### 12.3 验证命令一键脚本

```bash
# 1. 检查未读字段（最直接的"零件没接上"信号）
cargo clippy --lib 2>&1 | Select-String "field.*is never read"

# 2. 检查占位符
rg "暂为占位|executed\s*\+=\s*1" src/
rg "Arc::new\(\(\)\)" src/plugins/services/

# 3. 检查已注册 Provider 的实现完整性
rg "register_provider\(" src/plugins/services/ | rg "Arc::new" 
# 这条检查注册的都是真实实现，不是空元组
```

### 12.4 修复案例

| 场景 | 错误代码（仅注册未调用） | 修复代码 |
|------|------------------------|---------|
| Compression: 12 组件空转 | `run_loop` 中只做 `state` 切换，不调 `compressor.compress()` | `run_loop` 中调 `orchestrator.process_all()` |
| Chronos: ExecuteTool 占位 | `ActionType::ExecuteTool => { tracing::info!("暂为占位"); executed += 1; }` | 调 `self.tool_provider.execute(name, args).await` |
| Memory: 子服务字段未读 | `struct MemoryInner { dream: ..., experience_extract: ..., feedback: ... }` → 各方法中未使用 | `persist_observation()` 中调 `feedback.on_observation()` |
| LlmService: 组件未构造 | `struct LlmServiceRef { client, invoker }` → `chat()` 中不用 | `chat()` 中构造 `ErrorClassifier` 并用于错误处理 |
