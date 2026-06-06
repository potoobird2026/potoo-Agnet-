# ReActLoop Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 react-loop 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：react-loop（ReAct 循环决策器）
- **模块定位**：一个实现 `SlotPlugin` trait 的插件，对外仅通过 `SlotAccessPoint` 交互，在 Pipeline 的 `loop` 阶段根据当前迭代轮次和 LLM 输出做出"继续/跳转/跳出"决策
- **内部组件**：TurnLimitComponent（轮次限制组件）、LoopDecisionComponent（循环决策组件），共 2 个 Component
- **代码目录**：所有源码位于 `src/plugins/slots/react_loop/` 下
- **依赖项**：详见步骤 0
- **设计文档**：`docs/slots/react_loop/react_loop开发文档.md`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | — | 不涉及（react_loop 无 HTTP 请求） |
| 模型名 | — | 不涉及 |
| 超时秒数 | — | 不涉及 |
| API 版本号 | — | 不涉及 |
| User-Agent | — | 不涉及 |
| 文件路径 | — | 不涉及（react_loop 无文件操作） |
| 数字阈值 | `max_turns = 10` 写在业务逻辑中 | 从 `ReactLoopConfig.max_turns` 读取，默认值定义为 `DEFAULT_MAX_TURNS` 常量 |
| 阶段名称 | `"think"` 字符串字面量 | 使用 `Phase::think()` 核心类型方法 |
| 组件名称 | `"turn_limiter"` 字符串在业务逻辑中出现 | 在 `ComponentMeta.name` 中集中定义，通过 `ap.call("turn_limiter")` 使用（仅调用点一处） |
| 日志前缀 | `"[react_loop]"` 散落在各个文件中 | 定义为模块级常量 `LOG_PREFIX` |
| 配置键名 | `"max_turns"` 在读取配置时直接写字符串 | 定义为 `CFG_KEY_MAX_TURNS` 常量 |

---

## 项目目录结构

请手动创建以下目录结构（已有文件保留不动）：

```
src/plugins/
├── mod.rs                            # 已有 pub mod slots;
└── slots/
    ├── mod.rs                        # 需要添加 pub mod react_loop;
    └── react_loop/
        ├── mod.rs                    # 模块声明 + ReActLoopSlot 定义（SlotPlugin 实现）
        ├── types.rs                  # 新建：ReactLoopConfig、LoopAction、ReactLoopError 等公共类型
        ├── component.rs             # 新建：Component trait + ComponentMeta + ComponentHandle + AccessPoint + InitContext + ModuleConfig
        ├── orchestrator.rs          # 新建：Orchestrator + InternalAccessPointImpl
        └── components/              # 新建：组件实现目录
            ├── mod.rs               # pub mod 声明
            ├── turn_limiter.rs      # TurnLimitComponent + TurnLimitService
            └── loop_decider.rs      # LoopDecisionComponent + LoopDecisionService
```

模块声明链：

```
src/lib.rs                →  pub mod plugins;
src/plugins/mod.rs         →  pub mod slots;
src/plugins/slots/mod.rs   →  pub mod react_loop;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于设计文档。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与 `docs/slots/react_loop/react_loop开发文档.md` 完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - react_loop 只有 2 个组件（TurnLimitComponent、LoopDecisionComponent），不凭空生成第 3 个组件
   - react_loop 不涉及任何 HTTP 请求、文件操作、外部 API 调用
   - 不引入 reqwest、serde_json、uuid 等无关依赖

3. **零硬编码**：
   a. `max_turns` 阈值必须从 `ReactLoopConfig` 读取或使用常量 `DEFAULT_MAX_TURNS`，不允许在 `is_exceeded()` 中写数字字面量
   b. Phase 名称必须通过 `Phase::think()` 核心类型方法，不允许写 `"think"` 字符串
   c. 组件名称字符串（如 `"turn_limiter"`、`"loop_decider"`）仅在 `ComponentMeta.name` 中定义一次，其他位置通过 `ap.call()` 使用
   d. 日志前缀必须定义为 `const LOG_PREFIX: &str = "[react_loop]"` 常量
   e. 配置键名必须定义为 `const` 常量

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理完整**：所有可能失败的操作必须有错误处理，必须使用设计文档定义的错误类型，不得使用 `unwrap()`（测试除外）。测试中的 `unwrap()` 必须有注释说明"测试中安全"。

6. **一致性**：组件名称、方法名、字段名必须与设计文档完全一致，大小写敏感，单词拼写无误：
   - `TurnLimitComponent`（不是 `TurnLimit`、`TurnLimiter`）
   - `LoopDecisionComponent`（不是 `LoopDecider`、`DecisionComponent`）
   - `LoopAction`（不是 `LoopDecision`、`Action`)
   - `SlotDirective::JumpTo(Phase::think())`（不是 `BreakPhase` + `request_jump`）

7. **禁止额外依赖**：只能使用 `std`、`tokio`、`tracing`、`async-trait` 以及项目内部模块。**严禁**引入 `reqwest`、`serde`、`serde_json`、`uuid`、`futures`、`thiserror`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策）
   - 不允许写"做什么"的废话注释（如 `// 读取 iteration`）
   - 引用设计文档条款时用 `// 设计文档 §X.Y` 格式
   - 红线引用用 `// 遵循 C-R01` 格式

9. **测试同时生成**：
   - 为每个 `pub fn` 生成单元测试（设计文档 §5.3、红线13）
   - 测试名称必须包含设计文档条款编号（如 `test_turn_limit_not_exceeded`、`test_decide_action_within_limit`）
   - 所有测试使用 `MockAccessPoint`，不依赖真实 Pipeline

10. **杜绝捷径**：
    - 不能因为两个组件的 `process()` 都是 no-op 就省略实现——必须完整写出，并注释 `// 设计文档 §3.1/§3.2 process(): no-op`
    - 不能跳过 `SlotAccessPoint` 和 `InternalAccessPoint` 的桥接步骤——严格按照设计文档 §1.5 的 `run()` 伪代码实现
    - 不能跳过 Orchestrator 的 `requires` 校验——`register()` 必须检查对应依赖是否存在

11. **模块边界**：
    - 严禁引入计费、Token 计数、模型选择、上下文裁剪等 react_loop 职责以外的功能
    - react_loop 不做工具调用、不做 LLM 请求、不做消息格式化——这些是 tool_executor 和 llm_thinker 的职责
    - `SlotDirective` 未使用的 4 个变体（BreakPhase、RestartStep、AbortStep、AbortPipeline）只注释说明为什么不使用，不要实现

12. **日志规范**：
    - 关键路径记录 `info`（如 init 完成）
    - 轮次超限记录 `warn`（携带 iteration 和 max_turns）
    - Action 跳转记录 `debug`
    - Final/Continue 记录 `trace`
    - 错误记录 `error`（携带错误信息）
    - 使用 `const LOG_PREFIX` 常量统一日志前缀
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架（手动操作）

**目标**：确保项目可编译，目录就绪。

**操作**：

1. 确认 Cargo.toml 中的依赖包含以下清单（react_loop 不需要额外依赖，仅使用已有依赖）：

```toml
# 以下依赖 crate 已存在，react_loop 不需要新增任何依赖：
# tokio = { version = "1", features = ["sync"] }
# tracing = "0.1"
# async-trait = "0.1"
```

2. 在 `src/plugins/slots/mod.rs` 中添加：
```rust
pub mod react_loop;
```

3. 在 `src/plugins/slots/react_loop/` 下创建目录结构（`plugin.rs` 已删除，全部代码在新文件中）：
   - 新建 `mod.rs`（空文件，后续步骤 5 填入 SlotPlugin 实现）
   - 新建 `types.rs`（空文件）
   - 新建 `component.rs`（空文件）
   - 新建 `orchestrator.rs`（空文件）
   - 新建 `components/mod.rs`（空文件）
   - 新建 `components/turn_limiter.rs`（空文件）
   - 新建 `components/loop_decider.rs`（空文件）

4. 确保 `cargo check` 通过（当前应有 warning 但无 error——空模块可能产生未使用警告，可接受）。

**验收标准**：
- `cargo check` 无 error
- 目录结构与上述一致
- `src/plugins/slots/mod.rs` 包含 `pub mod react_loop;`

---

### 步骤 1：生成所有公共数据类型

**目标**：生成 `types.rs`，包含 ReactLoopConfig、LoopAction、ReactLoopError 等基础类型，保证后续所有组件依赖无误。

**文件**：`src/plugins/slots/react_loop/types.rs`

**要求**：

1. **`ReactLoopConfig` 结构体**——遵循跨平台与硬编码规范 §1：

```rust
/// 设计文档 §5.1——数字阈值从配置读取
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ReactLoopConfig {
    pub max_turns: Option<usize>,
}
```

2. **`ReactLoopError` 枚举**——Slot 层统一错误类型：

```rust
/// 设计文档 §1.5——SlotPlugin 错误类型
#[derive(Debug)]
pub enum ReactLoopError {
    Config(String),
    Internal(String),
}
```

3. **`LoopAction` 枚举**——组件内部决策结果，与 `SlotDirective` ——映射：

```rust
/// 设计文档 §3.2——循环决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum LoopAction {
    Continue,
    JumpToThink,
    ForceBreak,
}
```

4. **`DEFAULT_MAX_TURNS` 常量**：

```rust
/// 设计文档 §5.1——数字阈值使用常量集中管理
pub const DEFAULT_MAX_TURNS: usize = 10;
```

5. **`LOG_PREFIX` 常量**：

```rust
/// 设计文档 AI 宪法 §3.d——日志前缀统一管理
pub const LOG_PREFIX: &str = "[react_loop]";
```

6. **配置键名常量**：

```rust
/// 设计文档 §3.1——配置键名集中管理，禁止散落字面量
pub const CFG_KEY_MAX_TURNS: &str = "max_turns";
```

**验收标准**：
- `cargo check` 通过
- `ReactLoopConfig` 字段与设计文档 §3.1 表格一致（仅一个字段：`max_turns: Option<usize>`）
- `LoopAction` 三个变体名称与设计文档 §3.2 完全一致（Continue、JumpToThink、ForceBreak）
- 所有数字默认值均为常量 `DEFAULT_MAX_TURNS`
- 没有引入不必要的依赖（如 serde——可通过已有机制反序列化）

---

### 步骤 2：生成 Component 框架和 Orchestrator

**目标**：建立组件生命周期管理骨架，为后续组件接入做准备。

**文件**：
- `src/plugins/slots/react_loop/component.rs` — Component trait + ComponentMeta + ComponentHandle + AccessPoint + InitContext + ModuleConfig
- `src/plugins/slots/react_loop/orchestrator.rs` — Orchestrator + InternalAccessPointImpl

**协议依据**：严格按 `docs/protocol-模块内部组件协议.md` 第 1-5 节定义

**`component.rs` 要求**：

1. **`ComponentMeta` 结构体**（协议 §5）：

```rust
pub struct ComponentMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub priority: u8,
    pub provides: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub config_key: Option<&'static str>,
}
```

2. **`Component` trait**（协议 §1），所有方法为 `async fn`：

```rust
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    fn meta(&self) -> &ComponentMeta;
    fn clone_box(&self) -> Box<dyn Component>;
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError>;
    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError>;
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}
```

**注意**：与 LLM Thinker 不同，react_loop 的 `process()` 签名使用 `&mut dyn AccessPoint` 而非 `&dyn AccessPoint`，因为 react_loop 只有 2 个组件且串行执行，不需要 `join_all` 并发——因此不需要 interior mutability。这是设计文档 §4.4 中采用串行 `process_all()` 的原因。

3. **`ComponentHandle` trait**（协议 §2）：

```rust
pub trait ComponentHandle: Send + Sync {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn ComponentHandle>;
}
```

**`clone_box()` 的 blanket impl**——安全方案：`Component::clone_box()` 直接返回 `Box<dyn ComponentHandle>`，消除所有 unsafe 转换：

```rust
pub trait Component: Send + Sync {
    fn meta(&self) -> &ComponentMeta;
    /// 返回 Box<dyn ComponentHandle> 而非 Box<dyn Component>，
    /// 从而在 ComponentHandle blanket impl 中无需 unsafe transmute
    fn clone_box(&self) -> Box<dyn ComponentHandle>;
    // ...
}

impl<T: Component + 'static> ComponentHandle for T {
    fn name(&self) -> &str { self.meta().name }
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Component::clone_box(self)  // 安全：Component 的实现者自己构造 Box<dyn ComponentHandle>
    }
}
```

这样每个具体 Component（如 `TurnLimitComponent`）实现 `clone_box()` 时直接 `Box::new(self.clone())` 或 `Box::new(Self { ... })`，返回 `Box<dyn ComponentHandle>`。因 `ComponentHandle` 的 blanket impl 要求 `T: Component + 'static`，此转换类型安全，无 unsafe。

4. **`AccessPoint` trait**（协议 §3）：

```rust
pub trait AccessPoint: Send + Sync {
    /// 读取共享数据，返回类型擦除的引用，调用方通过 downcast_ref::<T>() 提取具体类型
    fn read_any(&self, key: &str) -> Option<&dyn Any>;
    /// 写入共享数据，接收 Box<dyn Any + Send> 实现类型擦除
    fn write_any(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), ComponentError>;
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;
    fn config(&self) -> &ModuleConfig;
    fn metrics(&self) -> &MetricsHandle;
    fn log(&self) -> &dyn ModuleLogger;
}
```

**必须使用 `read_any`/`write_any`（而非泛型 `read<T>`/`write<T>`）的原因**：泛型方法是 non-object-safe，无法在 `&mut dyn AccessPoint` 上调用。步骤 4 中 `decide()` 的签名是 `fn decide(&self, ap: &mut dyn AccessPoint)`，内部需要调用 `ap.read_any("current_iteration").and_then(|v| v.downcast_ref::<usize>()).copied()`，这要求 AccessPoint 是 object-safe。

5. **`ModuleConfig` 结构体**——强类型包装，避免通用 KV 存储：

react_loop 只有一个配置字段 `max_turns`，不需要通用 JSON KV 存储。直接从 `PluginInitContext` 反序列化 `ReactLoopConfig` 后包装：

```rust
#[derive(Debug, Clone)]
pub struct ModuleConfig {
    max_turns: usize,
}

impl ModuleConfig {
    /// 读取最大轮次，避免 ctx.config.max_turns 的嵌套写法
    pub fn max_turns(&self) -> usize {
        self.max_turns
    }
}
```

**设计理由**：若使用 `ModuleConfig { config: ReactLoopConfig }`，则 `init()` 中的读取代码变为 `ctx.config.config.max_turns.unwrap_or(DEFAULT_MAX_TURNS)`，出现 `config.config` 嵌套。改为 `ModuleConfig` 直接暴露 `max_turns()` 方法，调用方写 `ctx.config.max_turns()` 即可。

6. **`InitContext` 结构体**：

```rust
pub struct InitContext {
    pub config: ModuleConfig,
}
```

7. **`ComponentError` 枚举**：

```rust
pub enum ComponentError {
    Config(String),
    Internal(String),
    NotFound(String),
}
```

8. **`Processing` 枚举**（协议 §4）：

```rust
pub enum Processing {
    Continue,
    BreakChain,
    Restart,
    Warn { message: String },
}
```

9. **`MetricsHandle` 结构体**——占位实现，react_loop 当前不收集指标：

```rust
#[derive(Clone)]
pub struct MetricsHandle;
```

10. **`ModuleLogger` trait**——简化版，react_loop 直接使用 `tracing`：

```rust
pub trait ModuleLogger: Send + Sync {
    fn log(&self, level: log::Level, message: &str);
}
```

**注**：如果项目中没有 `log::Level`，可以使用 tracing 替代。**优先使用 tracing 直接输出，不额外封装**。

**`orchestrator.rs` 要求**（协议 §5.2）：

```rust
pub struct Orchestrator {
    components: Vec<Box<dyn Component>>,
    parallel_groups: Vec<Vec<usize>>,
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ModuleConfig,
}
```

`Orchestrator` 公开方法：

| 方法 | 签名 | 职责 |
|------|------|------|
| `new()` | `pub fn new() -> Self` | 创建空 Orchestrator |
| `register()` | `pub fn register(&mut self, component: Box<dyn Component>) -> Result<(), ComponentError>` | 注册组件，校验 requires/provides，拓扑排序 |
| `init_all()` | `pub async fn init_all(&mut self) -> Result<(), ComponentError>` | 按拓扑序调用每个组件的 `init()` |
| `access_point()` | `pub fn access_point(&self) -> Arc<RwLock<InternalAccessPointImpl>>` | 返回内部访问点引用 |
| `process_all()` | `pub async fn process_all(&mut self) -> Result<(), ComponentError>` | 按 DAG 序执行全部 `process()` |
| `shutdown_all()` | `pub async fn shutdown_all(&mut self)` | 逆序调用全部 `shutdown()` |

1. **`register()` 详细要求**：

   - 插入组件到 `components` 列表
   - 记录当前已注册组件的 `name` 和 `provides` 到索引集合
   - 校验新组件的 `requires` 是否被已有组件的 `provides` 覆盖：若存在未覆盖的依赖，返回 `ComponentError::NotFound(name)`
   - 校验组件 `name` 是否已存在：若重复，返回 `ComponentError::Config("duplicate component name: {name}")`
   - 重新计算拓扑排序后的 `parallel_groups`：
     - 每个组件的 dependency 集合 = 其 `requires` 列表
     - 进行 DAG 分层：第 0 层为无依赖的组件，第 n 层为依赖至少一个第 n-1 层组件（且不依赖更高层）的组件
     - react_loop 已知只有 2 层，但算法必须通用
   - **未实现环检测**：当前 2 组件的 DAG 静态已知无环

2. **`init_all()` 详细要求**：

   ```
   for group in parallel_groups:
       for idx in group (串行):
           component = &mut self.components[idx]
           component.init(&InitContext { config: self.config.clone() }).await?
   ```
   串行执行的原因是：react_loop 的组件数量少（2 个）且无并发初始化需求。

3. **`process_all()` 详细要求**：

   ```
   for group in parallel_groups:
       for idx in group (串行):
           let result = self.components[idx].process(&mut ap).await?
           match result:
               Continue => continue
               BreakChain => return
               Restart => 重新执行 process_all()
               Warn { message } => tracing::warn!(...)
   ```

4. **`shutdown_all()` 详细要求**：

   ```
   for idx in reversed(flat order):
       self.components[idx].shutdown().await.ok();
   ```

**`InternalAccessPointImpl` 结构体**（协议 §3.2）：

```rust
pub(crate) struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    data_share: HashMap<String, Box<dyn Any + Send>>,
    config: ModuleConfig,
}
```

**`AccessPoint` trait 实现**——使用 `Box<dyn Any + Send>` 存储（`+ Send` 确保跨线程安全），通过 `read_any` 返回 `&dyn Any` 引用：

```rust
impl AccessPoint for InternalAccessPointImpl {
    fn read_any(&self, key: &str) -> Option<&dyn Any> {
        self.data_share.get(key).map(|b| b.as_ref() as &dyn Any)
    }

    fn write_any(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), ComponentError> {
        self.data_share.insert(key.to_string(), val);
        Ok(())
    }

    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError> {
        self.components.get(name)
            .map(|c| c.clone_box())
            .ok_or_else(|| ComponentError::NotFound(name.to_string()))
    }

    fn config(&self) -> &ModuleConfig { &self.config }
    fn metrics(&self) -> &MetricsHandle { unimplemented!("react_loop 不收集指标") }
    fn log(&self) -> &dyn ModuleLogger { unimplemented!("react_loop 直接使用 tracing") }
}
```

**注意**：`read_any` 使用 `b.as_ref() as &dyn Any` 而非 `b.downcast_ref::<T>()`。虽然 `b: &Box<dyn Any + Send>` 可通过 `Box` 解引用拿到 `dyn Any`，但转换为 `&dyn Any` 时发生自动解引用 + 重新装箱（fat pointer）。等效于 `&**b as &dyn Any`。

**验收标准**：
- `cargo check` 通过
- 测试创建一个虚拟 Component（实现 Component trait，元数据自定义），加入 Orchestrator，验证 init_all / process_all / shutdown_all 顺序
- 测试验证 Processing::BreakChain 能正确阻断后续组件
- 测试验证 `register()` 在 `requires` 未满足时返回 `ComponentError::NotFound`
- 测试验证 `register()` 在组件名称重复时返回 `ComponentError::Config`
- 没有硬编码组件名称字符串（测试除外）

---

### 步骤 3：实现 TurnLimitComponent

**文件**：`src/plugins/slots/react_loop/components/turn_limiter.rs`

**要求**：

1. **`TurnLimitService` trait 定义**——严格按设计文档 §3.1：

```rust
pub trait TurnLimitService: Send + Sync {
    /// 设计文档 §3.1——iteration 从 0 开始计数
    fn is_exceeded(&self, iteration: usize) -> bool;
    fn max_turns(&self) -> usize;
}
```

2. **`TurnLimitComponent` 结构体**：

```rust
pub struct TurnLimitComponent {
    max_turns: usize,
}
```

3. **`new()` 构造函数**——接收 `max_turns: usize`：

```rust
pub fn new(max_turns: usize) -> Self
```

4. **`TurnLimitService` 实现**——`is_exceeded()` 是纯函数，不依赖 AccessPoint（设计文档 §3.1 设计约束）：

```rust
impl TurnLimitService for TurnLimitComponent {
    fn is_exceeded(&self, iteration: usize) -> bool {
        iteration >= self.max_turns
    }

    fn max_turns(&self) -> usize {
        self.max_turns
    }
}
```

5. **`Component` trait 实现**：

| 方法 | 实现要求 |
|------|---------|
| `meta()` | 返回 `ComponentMeta { name: "turn_limiter", version: "0.1.0", priority: 10, provides: &["turn_check"], requires: &[], config_key: Some("react_loop") }` |
| `clone_box()` | `Box::new(Self { max_turns: self.max_turns })` |
| `init()` | 从 `ctx.config` 的 `max_turns` 字段读取配置，若 < 1 则提升为 1；若缺失则使用 `DEFAULT_MAX_TURNS` |
| `process()` | 返回 `Ok(Processing::Continue)` 并注释 `// 设计文档 §3.1——no-op` |
| `shutdown()` | 返回 `Ok(())` |

6. **`init()` 详细校验逻辑**（设计文档 §3.1 表格）：

```rust
// 设计文档 §3.1——配置读取 + 边界校验
// CFG_KEY_MAX_TURNS 常量保障非硬编码
let configured = ctx.config.get(CFG_KEY_MAX_TURNS)
    .unwrap_or(DEFAULT_MAX_TURNS);
self.max_turns = if configured >= 1 { configured } else { 1 };
```

**注意**：`ctx.config` 的类型是 `ModuleConfig`，而 `ModuleConfig` 在步骤 2 中定义。`get()` 方法如何实现？如果 `ModuleConfig` 中的 `config` 是 `ReactLoopConfig`，则直接访问 `config.max_turns.unwrap_or(DEFAULT_MAX_TURNS)`。设计文档 §3.1 表格中的伪代码 `ctx.config.get("max_turns")` 只是示意写法。

**实际实现**——取决于步骤 2 中 `ModuleConfig` 的具体设计：
- 方案 A：`ModuleConfig { config: ReactLoopConfig }` — 直接访问 `ctx.config.config.max_turns`
- 方案 B：`ModuleConfig { raw: HashMap<String, Value> }` — 通过 `ctx.config.raw.get("max_turns")` 访问

**优先方案 A**（类型安全，无额外依赖）。

7. **`as_any()` / `as_any_mut()` 实现**——通过 blanket impl 自动生成。

**验收标准**：
- `cargo check` 通过
- 所有测试通过（设计文档 §5.3 的 4 个测试用例）
- 测试验证 `max_turns=0` 被提升为 1
- 测试验证跨平台规范——数字阈值来自构造参数，非硬编码
- `is_exceeded()` 没有 AccessPoint 依赖（验证函数签名不接收 `ap` 参数）

---

### 步骤 4：实现 LoopDecisionComponent

**文件**：`src/plugins/slots/react_loop/components/loop_decider.rs`

**要求**：

1. **`LoopDecisionService` trait 定义**——严格按设计文档 §3.2：

```rust
pub trait LoopDecisionService: Send + Sync {
    /// 设计文档 §3.2——通过 AccessPoint 读取共享数据和调用兄弟组件
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError>;
}
```

2. **`LoopDecisionComponent` 结构体**——无状态组件：

```rust
pub struct LoopDecisionComponent;
```

3. **`new()` 构造函数**：

```rust
pub fn new() -> Self
```

4. **`LoopDecisionService` 实现**——严格按设计文档 §3.2 算法实现 3 路决策状态机：

```rust
impl LoopDecisionService for LoopDecisionComponent {
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError> {
        // 设计文档 §3.2——从共享数据区读取
        let iteration = ap.read_any("current_iteration")
            .and_then(|v| v.downcast_ref::<usize>())
            .copied()
            .unwrap_or(0);
        let thought: Option<&Thought> = ap.read_any("thought")
            .and_then(|v| v.downcast_ref::<Thought>());

        // 遵循 C-R01：call() 后必须 downcast
        // 遵循 C-R02：requires 必须在代码中实际调用
        let handle = ap.call("turn_limiter")
            .map_err(|_| ComponentError::NotFound("turn_limiter".into()))?;
        let turn_limit = handle.as_any()
            .downcast_ref::<dyn TurnLimitService>()
            .ok_or_else(|| ComponentError::Internal(
                "turn_limiter: type mismatch".into()
            ))?;

        // 设计文档 §7.7——决策状态机
        let exceeded = turn_limit.is_exceeded(iteration);

        match (exceeded, thought) {
            // 状态 1：轮次超限 → 强制跳出（最高优先级）
            (true, _) => {
                tracing::warn!(
                    "{} max_turns reached: iteration={}, max_turns={}",
                    LOG_PREFIX, iteration, turn_limit.max_turns(),
                );
                Ok(LoopAction::ForceBreak)
            }
            // 状态 2：Action 且未超限 → 跳回 THINK
            (false, Some(Thought::Action { .. })) => {
                tracing::debug!("{} Action detected, jumping back to THINK", LOG_PREFIX);
                Ok(LoopAction::JumpToThink)
            }
            // 状态 3：Final 或 None → 放行
            (false, Some(Thought::Final { .. })) | (false, None) => {
                tracing::trace!("{} Final/None, continuing", LOG_PREFIX);
                Ok(LoopAction::Continue)
            }
        }
    }
}
```

5. **`Component` trait 实现**：

| 方法 | 实现要求 |
|------|---------|
| `meta()` | 返回 `ComponentMeta { name: "loop_decider", version: "0.1.0", priority: 20, provides: &["loop_decision"], requires: &["turn_check"], config_key: None }` |
| `clone_box()` | `Box::new(Self)` |
| `init()` | 返回 `Ok(())`——无状态组件，无需初始化 |
| `process()` | 返回 `Ok(Processing::Continue)`——设计文档 §3.2——no-op |
| `shutdown()` | 返回 `Ok(())`——无资源需释放 |

6. **`Thought` 类型来源**：

`Thought` 定义在 `src/core/types/mod.rs`，通过以下方式导入：
```rust
use crate::core::types::Thought;
```

**不允许**重新定义 `Thought`——必须使用核心类型。

**验收标准**：
- `cargo check` 通过
- 所有测试通过（设计文档 §5.3 的 4 个测试用例 + 边界情况）
- 测试验证 3 种状态的所有 4 条路径（Action 未超限 → JumpToThink、Action 超限 → ForceBreak、Final → Continue、None → Continue）
- 测试验证边界条件：iteration=9, max_turns=10 → JumpToThink; iteration=10, max_turns=10 → ForceBreak
- `decide()` 函数长度 ≤ 35 行（红线18——每个函数 ≤ 50 行）
- 未使用 `unwrap()` 或 `expect()`（红线21——仅在测试中允许）

---

### 步骤 5：实现 ReActLoopSlot（在 mod.rs 中）

**目标**：在 `mod.rs` 中实现 `SlotPlugin` trait，串联 Orchestrator（步骤 2）+ 组件（步骤 3-4），完成 Slot 入口。

**文件**：
- `src/plugins/slots/react_loop/mod.rs` — 模块声明 + SlotPlugin 实现

**遵循协议 §6.1**——`mod.rs` 只对外暴露三样东西，内部全部 `pub(crate)`：

```rust
// 对外暴露：入口、配置、错误类型
pub struct ReActLoopSlot;
pub struct ReactLoopConfig;
pub struct ReactLoopError;

// 内部组件全部 pub(crate)
pub(crate) mod orchestrator;
pub(crate) mod components;
```

**`ReActLoopSlot` 结构体**（直接定义在 `mod.rs` 中）：

```rust
pub struct ReActLoopSlot {
    orchestrator: Arc<RwLock<Orchestrator>>,
}
```

**`SlotPlugin` trait 实现**——设计文档 §1.5 完整伪代码：

**`init()` 方法**：

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    let mut orch = self.orchestrator.write().await;

    // 步骤 1：读取配置
    let max_turns = ctx.config::<ReactLoopConfig>()
        .ok()
        .and_then(|c| c.max_turns)
        .unwrap_or(DEFAULT_MAX_TURNS);

    // 步骤 2：注册 TurnLimitComponent
    orch.register(Box::new(TurnLimitComponent::new(max_turns)))?;

    // 步骤 3：注册 LoopDecisionComponent
    orch.register(Box::new(LoopDecisionComponent::new()))?;

    // 步骤 4：全量初始化（按 DAG 拓扑序）
    orch.init_all().await
        .map_err(|e| PluginError::InitFailed(e.to_string()))
}
```

**`run()` 方法**——严格按设计文档 §1.5 的 6 段式流程：

```rust
async fn run(
    &mut self,
    ap_slot: &mut dyn SlotAccessPoint,
) -> Result<SlotDirective, PluginError> {
    // [A] 通过 SlotAccessPoint 读取外部数据
    let iteration = ap_slot.current_iteration();
    let thought_raw: Option<Thought> = ap_slot
        .read_context_raw("thought")
        .and_then(|any| any.downcast_ref::<Thought>())
        .cloned();

    // [B] 获取 InternalAccessPoint
    let orch = self.orchestrator.read().await;
    let ap_int = orch.access_point();
    let mut ap_guard = ap_int.write().await;

    // [C] 将外部数据注入共享数据区（通过 write_any 类型擦除）
    ap_guard.write_any("current_iteration", Box::new(iteration))?;
    if let Some(ref t) = thought_raw {
        ap_guard.write_any("thought", Box::new(t.clone()))?;
    }

    // [D] 获取 LoopDecisionComponent 句柄
    let handle = ap_guard.call("loop_decider")?;
    let decider = handle.as_any()
        .downcast_ref::<dyn LoopDecisionService>()
        .ok_or_else(|| PluginError::Internal(
            "loop_decider: type mismatch".into()
        ))?;

    // [E] 执行决策
    let action = decider.decide(&mut *ap_guard)?;
    drop(ap_guard);
    drop(orch);

    // [F] 映射为 SlotDirective
    match action {
        LoopAction::Continue => Ok(SlotDirective::Continue),
        LoopAction::JumpToThink => Ok(SlotDirective::JumpTo(Phase::think())),
        LoopAction::ForceBreak => Ok(SlotDirective::BreakStep),
    }
}
```

**`shutdown()` 方法**：

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    let mut orch = self.orchestrator.write().await;
    orch.shutdown_all().await;
    Ok(())
}
```

**错误处理策略**（设计文档 §1.4）：

| 错误来源 | 处理方式 | 协议依据 |
|---------|---------|---------|
| `register()` 失败 | 返回 `PluginError::InitFailed` → 插件不加载 | S-R02 |
| `init_all()` 组件初始化失败 | 返回 `PluginError::InitFailed` → 插件不加载 | S-R02 |
| `call("loop_decider")` 未找到 | 返回 `PluginError::Internal` → 设计错误（测试中暴露） | 红线2 |
| `decide()` 运行时错误 | 返回 `PluginError::Internal` → 向上传播 | 红线2 |
| `write_any` 到共享数据区失败 | 返回 `PluginError::Internal` | 红线2 |

**`run()` 中不进行错误降级**：react_loop 的决策如果出错，不应静默放行——这可能导致 Pipeline 无限循环。错误向上传播，由 Pipeline 的异常处理机制（如 abort step）处理。

**验收标准**：
- `cargo check` 通过
- `name()` 返回 `"react_loop"`
- `init()` 使用 `PluginInitContext.config::<ReactLoopConfig>()` 读取配置（而非直接操作原始 JSON）
- `run()` 函数长度 ≤ 50 行（红线18）
- 所有变量命名与设计文档一致（`iteration`、`thought_raw`、`ap_slot`、`ap_guard`、`action`）
- `String::from("...")` 字符串字面量全部为常量或 `&'static str`
- 模块声明链完整：`src/lib.rs → pub mod plugins; → src/plugins/mod.rs → pub mod slots; → src/plugins/slots/mod.rs → pub mod react_loop;`（指向 `react_loop/mod.rs`）

---

### 步骤 6：集成测试

**目标**：验证 ReActLoopSlot 从 init 到 run 再到 shutdown 的完整流程。

**文件**：`src/plugins/slots/react_loop/tests/`（可选，或直接在 `mod.rs` 中加 `#[cfg(test)]`）

**建议方案**：在 `mod.rs` 中添加 `#[cfg(test)] mod tests { ... }` 块（集成测试与 SlotPlugin 实现在一起，可访问 `super::*`）。

**测试场景**：

1. **`test_init_ok_with_default_config`** — 无配置时使用默认值，init 成功
2. **`test_init_ok_with_custom_max_turns`** — 提供 `max_turns=5`，init 成功
3. **`test_run_action_within_limit`** — `iteration=2, max_turns=5, Action` → 返回 `JumpTo(Phase::think())`
4. **`test_run_action_exceeded`** — `iteration=5, max_turns=5, Action` → 返回 `BreakStep`
5. **`test_run_final_within_limit`** — `iteration=2, max_turns=5, Final` → 返回 `Continue`
6. **`test_run_no_thought`** — `iteration=2, max_turns=5`（无 Thought）→ 返回 `Continue`
7. **`test_shutdown_ok`** — shutdown 正常执行，不 panic
8. **`test_invalid_max_turns_is_clamped`** — `max_turns=0` 被提升为 1
9. **`test_run_cycle_behavior`** — 模拟 3 轮 Action 后在 limit 到达时 BreakStep

**MockSlotAccessPoint 实现要求**——匹配 `src/core/access/mod.rs:118` 的实际 trait 签名，所有方法必须实现：

```rust
struct MockSlotAccessPoint {
    iteration: usize,
    thought: Option<Thought>,
}

impl SlotAccessPoint for MockSlotAccessPoint {
    fn messages(&self) -> &[Message] { &[] }
    fn session_id(&self) -> &str { "mock-session" }
    fn phase_name(&self) -> &str { "mock-phase" }
    fn current_iteration(&self) -> usize { self.iteration }
    fn write_observation(&mut self, _obs: Observation) -> Result<(), PluginError> { Ok(()) }
    fn write_context_raw(&mut self, _key: &str, _val: Box<dyn Any + Send>) -> Result<(), PluginError> { Ok(()) }
    fn read_context_raw(&self, _key: &str) -> Option<&dyn Any> {
        self.thought.as_ref().map(|t| t as &dyn Any)
    }
    fn request_jump(&self, _phase: &str) -> Result<(), PluginError> { Ok(()) }
    fn request_abort(&self) -> Result<(), PluginError> { Ok(()) }
    fn provider_raw(&self, _name: &str) -> Option<Arc<dyn Any + Send + Sync>> { None }
}
```

**验收标准**：
- `cargo test` 所有测试通过
- 每个测试覆盖一个明确场景，断言具体返回值
- 测试名称体现测试意图（如 `test_run_action_within_limit`）
- 无 `#[ignore]` 测试

---

### 步骤 7：终态自检与硬编码审计

1. **全量编译与测试**：

```bash
cargo test --all
cargo clippy -- -D warnings
cargo fmt --check
```

2. **硬编码扫描**：

```bash
# 检查 max_turns 是否硬编码（应在 is_exceeded 中出现字面量数字）
rg '(>=|is_exceeded)\s*[0-9]+' src/plugins/slots/react_loop/
# 检查 Phase 名称是否使用字符串字面量
rg '"think"|"loop"|"final"' src/plugins/slots/react_loop/
# 检查组件名称是否在业务逻辑中硬编码（应出现在 ComponentMeta 中）
rg '"turn_limiter"|"loop_decider"' src/plugins/slots/react_loop/ --type rust
# 检查日志前缀是否符合规范
rg '"(?!\[react_loop\])' src/plugins/slots/react_loop/ --type rust
```

3. **合理性检查清单**：

| 检查项 | 预期 |
|--------|------|
| `is_exceeded()` 是纯函数吗？ | 是 —— 只依赖 `self.max_turns` |
| `decide()` 是否同时使用了两个通道的数据？ | 是 —— A 通道数据（iteration/thought）+ B 通道调用（turn_limiter） |
| `JumpTo(Phase::think())` 是否直接返回（而非两段式）？ | 是 |
| 所有 `ComponentMeta.name` 是否唯一？ | 是 —— `"turn_limiter"` 和 `"loop_decider"` |
| `run()` 是否在 50 行以内？ | 是 |
| `slot/mod.rs` 是否包含 `pub mod react_loop;`？ | 是 |
| `mod.rs` 是否只暴露了 3 样东西？ | 是 —— ReActLoopSlot、ReactLoopConfig、ReactLoopError |
| `requires` 是否在 `register()` 中校验？ | 是 |
| 是否使用了任何额外依赖？ | 否 |

4. **合规性扫描**：

```bash
# 查找 panic/unwrap/expect（测试文件除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/react_loop/ --type rust --glob '!*test*'
# 查找 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/react_loop/ --type rust --glob '!*test*'
```

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
- 如果某步骤产生多余的 warning，必须先解决再进入下一步
- 如果发现设计文档有疏漏，记录到设计文档的 TODO 中，不在代码中猜测
