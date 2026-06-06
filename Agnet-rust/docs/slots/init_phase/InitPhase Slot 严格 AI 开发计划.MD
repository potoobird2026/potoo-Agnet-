# InitPhase Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 init-phase 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：init-phase（会话初始化槽口）
- **模块定位**：一个实现 `SlotPlugin` trait 的插件，对外仅通过 `SlotAccessPoint` 交互，在 Pipeline 的 `init` 阶段执行会话检测、记忆加载、系统提示词组装、上下文预检。职责单一，**不需要 Orchestrator 和内部 Component 体系**。
- **内部组件**：无（单 SlotPlugin，通过函数拆分文件，无 Component trait 体系）
- **代码目录**：所有源码位于 `src/plugins/slots/init_phase/` 下
- **依赖项**：`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror`
- **设计文档**：`docs/slots/init_phase/03_init_phase_槽口开发文档.md`
- **外部类型引用**（Slot接入协议 §0/§2.2：仅通过 shared_types 契约类型交互，**禁止引用 Service 内部类型**）：
  - `IdentitySection` → `crate::shared_types::IdentitySection`（契约类型，MemoryService 实现方负责将内部类型映射至此）
  - `MemoryFileEntry` → `crate::shared_types::MemoryFileEntry`（契约类型，同上）
  - `MemoryProvider` trait → `crate::shared_types::MemoryProvider`（或由 `memory_saver/provider.rs` re-export，需扩展 `load_identity`/`load_working_memory`/`is_new_session` 方法）
  - `Timestamp` → `crate::core::types::Timestamp`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | — | 不涉及（init_phase 无 HTTP 请求） |
| 模型名 | — | 不涉及 |
| 超时秒数 | — | 不涉及（init_phase 无网络调用） |
| API 版本号 | — | 不涉及 |
| User-Agent | — | 不涉及 |
| 文件路径 | — | 不涉及（文件操作由 MemoryService 处理） |
| 数字阈值 | `working_memory_limit = 10` 写在业务逻辑中 | 定义为 `DEFAULT_WORKING_MEMORY_LIMIT` 常量，从 `InitPhaseConfig` 读取 |
| 字符串模板 | 系统提示词模板散落在业务代码中 | 设计文档 §4.4 中集中定义默认模板字符串 |
| 日志前缀 | `"init_phase:"` 散落在各个文件中 | 统一使用 `"init_phase:"` 前缀（字符串模板可接受） |

---

## 项目目录结构

请手动创建以下目录结构（已有文件保留不动）：

```
src/plugins/
├── mod.rs                            # 已有 pub mod slots;
└── slots/
    ├── mod.rs                        # 需要添加 pub mod init_phase;
    └── init_phase/
        ├── mod.rs                    # 模块声明（组件协议 §6.1）
        ├── plugin.rs                 # 新建：InitPhaseSlot 定义（SlotPlugin 实现）
        ├── config.rs                 # 新建：InitPhaseConfig + 常量
        ├── types.rs                  # 新建：SessionMeta
        └── error.rs                  # 新建：InitPhaseError + Into<PluginError>
```

模块声明链：

```
src/lib.rs                →  pub mod plugins;
src/plugins/mod.rs         →  pub mod slots;
src/plugins/slots/mod.rs   →  pub mod init_phase;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `03_init_phase_槽口开发文档.md`。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与设计文档完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - init_phase 没有内部 Component，不需要 Orchestrator，不实现 Component trait
   - init_phase 不做工具注册、不做 LLM 请求、不做记忆持久化、不做安全审计
   - `run()` 只返回 `Continue`（S-R01），不返回其他变体——所有异常路径都降级为 Continue + warn 日志
   - init_phase 不 panic、不 unwrap——所有写 context 失败都记录 warn 并继续

3. **零硬编码**：
   a. `DEFAULT_WORKING_MEMORY_LIMIT` 和 `DEFAULT_MAX_MESSAGES_PRECHECK` 定义为 `const` 常量
   b. 系统提示词默认模板在 `assemble_system_prompt()` 中定义，不散落
   c. 日志前缀使用句点命名空间 `"init_phase:"`，不允许裸字符串
   d. `working_memory_limit` 和 `max_messages_precheck` 从配置读取

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理完整**：
   - `init()` 中 `working_memory_limit == 0` 或 `max_messages_precheck == 0` 返回 `PluginError::InitFailed`（S-R02）
   - `run()` 中所有 `write_context_raw` 失败：记录 `warn`，继续执行（不传播 `Err`）
   - `provider_raw("memory")` 返回 `None`：记录 `warn`，跳过初始化，返回 `Continue`
   - `is_new_session()` 超时或失败：默认 `true`（新会话安全假设）
   - 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释说明"测试中安全"

6. **一致性**：方法名、字段名、枚举变体名必须与设计文档完全一致，大小写敏感。

7. **禁止额外依赖**：只能使用 `std`、`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror` 以及项目内部模块。严禁引入 `reqwest`、`uuid`、`chrono`、`tempfile`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策）
   - 不允许写"做什么"的废话注释（如 `// 读取 session_id`）
   - 引用设计文档条款时用 `// 设计文档 §X.Y` 格式
   - 红线引用用 `// 遵循 S-R0X` 格式

9. **测试同时生成**：
   - 为每个场景生成单元测试（设计文档 §9.1–§9.6）
   - 测试使用 `MockSlotAccessPoint` + `MockMemoryProvider`，不依赖真实 Pipeline 或 MemoryService
   - 测试名称体现测试意图（如 `test_new_session_initialization`）
   - 所有测试使用 `std::env::temp_dir()` 处理临时路径

10. **杜绝捷径**：
    - 不能因为所有路径都返回 `Continue` 就省略降级日志——每条降级路径都必须有 `tracing::warn!`
    - 不能省略 `is_new_session()` 的 `unwrap_or(true)`——超时场景必须安全降级
    - `shutdown()` 必须有完整的生命周期实现

11. **模块边界**（Slot接入协议 §0/§2.2 红线）：
    - 严禁引入工具注册、LLM 调用、安全审计等 init_phase 职责以外的功能
    - `MemoryProvider` trait 定义在 `crate::shared_types` 中（或由 `memory_saver/provider.rs` re-export），init_phase 只通过 `provider_raw("memory")` + downcast 使用
    - **严禁**通过 `use crate::plugins::services::memory::*` 引用任何类型——所有数据通过 `crate::shared_types` 中的契约类型交互（`shared_types::IdentitySection`、`shared_types::MemoryFileEntry`）
    - MemoryService 内部类型（`l1_identity::IdentitySection`、`l2_working::MemoryFile` 等）对 init_phase **完全不可见**

12. **日志规范**：
    - init 完成记录 `info`（携带配置摘要）
    - Provider 未注册记录 `warn`
    - 身份记忆加载成功记录 `debug`，失败记录 `warn`
    - 工作记忆加载成功记录 `debug`，失败记录 `warn`
    - 新会话无身份记忆记录 `info`
    - 上下文窗口接近上限记录 `warn`
    - 所有 `write_context_raw` 降级记录 `warn`
    - 使用 `"init_phase:"` 统一日志前缀
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架（手动操作）

**目标**：确保项目可编译，目录就绪，依赖齐全，模块注册到 `slots/mod.rs`。

**操作**：

1. 确认 Cargo.toml 包含以下依赖：

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "fs"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
async-trait = "0.1"
thiserror = "1"                            # ★ 新增：错误类型派生
```

2. 创建 `src/plugins/slots/init_phase/` 目录及以下空文件：

```
src/plugins/slots/init_phase/
├── mod.rs           （空文件）
├── plugin.rs        （空文件）
├── config.rs        （空文件）
├── types.rs         （空文件）
└── error.rs         （空文件）
```

3. 在 `src/plugins/slots/mod.rs` 中添加：

```rust
pub mod init_phase;      // ★ 新增
```

4. 确保 `cargo check` 通过（空模块可能产生未使用警告，可接受）。

**验收标准**：
- `cargo check` 无 error
- 目录结构与上述一致
- `src/plugins/slots/mod.rs` 包含 `pub mod init_phase;`
- Cargo.toml 新增了 `thiserror` 依赖

---

### 步骤 1：生成所有公共数据类型

**目标**：生成 `config.rs`、`error.rs`、`types.rs`，包含 InitPhaseConfig、InitPhaseError、SessionMeta 等基础类型。

#### 1a：config.rs

**文件**：`src/plugins/slots/init_phase/config.rs`

**要求**：

1. **常量**（跨平台规范 §1-7）：

```rust
/// 设计文档 §2.3——工作记忆默认加载条数
pub const DEFAULT_WORKING_MEMORY_LIMIT: usize = 10;
/// 设计文档 §2.3——上下文窗口预检默认最大消息数
pub const DEFAULT_MAX_MESSAGES_PRECHECK: usize = 100;
```

2. **`InitPhaseConfig` 结构体**——严格按设计文档 §2.3：

```rust
/// 设计文档 §2.3——Init 阶段配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitPhaseConfig {
    /// 是否加载身份记忆（L1），默认 true
    #[serde(default = "default_true")]
    pub load_identity: bool,

    /// 是否加载工作记忆（L2），默认 true
    #[serde(default = "default_true")]
    pub load_working_memory: bool,

    /// 工作记忆加载条数，默认 DEFAULT_WORKING_MEMORY_LIMIT
    #[serde(default = "default_working_memory_limit")]
    pub working_memory_limit: usize,

    /// 是否组装系统提示词，默认 true
    #[serde(default = "default_true")]
    pub assemble_system_prompt: bool,

    /// 系统提示词模板（可选，为空则使用默认模板）
    #[serde(default)]
    pub system_prompt_template: Option<String>,

    /// 上下文窗口预检最大消息数，默认 DEFAULT_MAX_MESSAGES_PRECHECK
    #[serde(default = "default_max_messages")]
    pub max_messages_precheck: usize,
}
```

3. **默认值函数**：

```rust
fn default_true() -> bool { true }
fn default_working_memory_limit() -> usize { DEFAULT_WORKING_MEMORY_LIMIT }
fn default_max_messages() -> usize { DEFAULT_MAX_MESSAGES_PRECHECK }
```

**验收标准**：
- `cargo check` 通过
- 所有字段与设计文档 §2.3 完全一致
- `DEFAULT_WORKING_MEMORY_LIMIT` 和 `DEFAULT_MAX_MESSAGES_PRECHECK` 为模块级 `const`
- 序列化/反序列化测试覆盖

---

#### 1b：error.rs

**文件**：`src/plugins/slots/init_phase/error.rs`

**要求**：

```rust
/// 设计文档 §2.4——InitPhase 错误类型
#[derive(Debug, thiserror::Error)]
pub enum InitPhaseError {
    #[error("Memory Provider 未注册")]
    MemoryProviderUnavailable,

    #[error("身份记忆加载失败: {0}")]
    IdentityLoadError(String),

    #[error("工作记忆加载失败: {0}")]
    WorkingMemoryLoadError(String),

    #[error("上下文超限: {count} > {limit}")]
    ContextOverflow { count: usize, limit: usize },

    #[error("配置解析错误: {0}")]
    ConfigError(String),
}

/// 设计文档 §2.4——自动转换为 PluginError
impl From<InitPhaseError> for PluginError {
    fn from(e: InitPhaseError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
```

**验收标准**：
- `cargo check` 通过
- 5 个变体名称与设计文档 §2.4 完全一致
- `Into<PluginError>` 实现存在且正确

---

#### 1c：types.rs

**文件**：`src/plugins/slots/init_phase/types.rs`

**要求**：

```rust
/// 设计文档 §5.1——会话元数据，写入 StepContext["session_meta"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub is_new: bool,
    pub initialized_at: Timestamp,
}
```

**`Timestamp` 类型来源**：使用 `crate::core::types::Timestamp`。

**验收标准**：
- `cargo check` 通过
- 所有字段与设计文档 §5.1 完全一致
- `Timestamp` 从核心类型引用，不重复定义

---

### 步骤 2：实现 InitPhaseSlot（plugin.rs）

**目标**：实现 `SlotPlugin` trait，包含完整生命周期和 6 段式 run() 流程。

**文件**：`src/plugins/slots/init_phase/plugin.rs`

**要求**：

1. **`InitPhaseSlot` 结构体**：

```rust
/// 设计文档 §6——Slot 结构体
/// S-R03 合规：config 在 init() 时设置，run() 中只读
pub struct InitPhaseSlot {
    config: Option<InitPhaseConfig>,
}
```

2. **`new()` 构造函数**：

```rust
impl InitPhaseSlot {
    pub fn new() -> Self {
        Self { config: None }
    }
}
```

3. **`name()` 方法**——返回 `"init_phase"`

4. **`init()` 方法**——严格按设计文档 §4.2：

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    let config: InitPhaseConfig = serde_json::from_value(ctx.plugin_config.clone())
        .map_err(|e| PluginError::Config(format!("init_phase 配置解析失败: {}", e)))?;

    // 设计文档 §4.2——边界校验（S-R02：失败则插件不加载）
    if config.working_memory_limit == 0 {
        return Err(PluginError::Config(
            "init_phase: working_memory_limit 不能为 0".into(),
        ));
    }
    if config.max_messages_precheck == 0 {
        return Err(PluginError::Config(
            "init_phase: max_messages_precheck 不能为 0".into(),
        ));
    }

    self.config = Some(config);
    tracing::info!("init_phase: 初始化完成");
    Ok(())
}
```

5. **`run()` 方法**——严格按设计文档 §4.1 的 7 段式流程：

```
步骤 1：获取 Memory Provider（降级：未注册 → warn + Continue）
步骤 2：检测会话类型（降级：失败 → 默认新会话）
步骤 3：加载身份记忆 L1（降级：失败 → warn + 继续）
步骤 4：加载工作记忆 L2（降级：失败 → warn + 继续）
步骤 5：组装系统提示词（降级：写入失败 → warn + 继续）
步骤 6：上下文窗口预检（超限 → warn + 继续）
步骤 7：返回 Continue
```

**关键路径 vs 非关键路径错误处理**（设计文档 §4.1 的降级策略）：

| 路径 | 操作 | 失败后果 |
|------|------|---------|
| 步骤 2 | write_context_raw("session_meta") | 记录 warn，不中断 |
| 步骤 3 | write_context_raw("identity") | 记录 warn，不中断 |
| 步骤 4 | write_context_raw("working_memory") | 记录 warn，不中断 |
| 步骤 5 | write_context_raw("system_prompt") | 记录 warn，不中断 |
| 步骤 6 | messages.len() > max_messages_precheck | 记录 warn，不中断 |
| 所有路径 | 最终返回 | Continue |

6. **`shutdown()` 方法**——设计文档 §4.3：

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    tracing::info!("init_phase: shutdown 完成");
    Ok(())
}
```

7. **`assemble_system_prompt()` 辅助方法**——设计文档 §4.4：

```rust
impl InitPhaseSlot {
    /// 设计文档 §4.4——系统提示词组装
    /// 遵循 Slot接入协议 §2.2：identity 类型来自 shared_types，非 Service 内部类型
    fn assemble_system_prompt(
        &self,
        session_id: &str,
        identity: Option<crate::shared_types::IdentitySection>,
        is_new_session: bool,
    ) -> String {
        let mut prompt = String::new();

        if let Some(template) = &self.config.system_prompt_template {
            prompt.push_str(template);
        } else {
            prompt.push_str("You are a helpful AI agent.\n\n");
        }

        if let Some(id) = identity {
            prompt.push_str("## Identity Context\n");
            prompt.push_str(&id.content);
            prompt.push('\n');
        }

        if is_new_session {
            prompt.push_str("\n## Session Info\nThis is a new session.\n");
        }

        prompt
    }
}
```

**重要依赖说明**（Slot接入协议 §0/§2.2 红线）：

- `MemoryProvider` trait 应定义在 `crate::shared_types` 中（或由 `memory_saver/provider.rs` 从 shared_types re-export）。InitPhaseSlot **不重复定义**任何 trait。

- 如果 `MemoryProvider` 尚未包含 `load_identity()`、`load_working_memory()`、`is_new_session()` 这三个方法，**必须先扩展 MemoryProvider trait** 再实现 InitPhaseSlot。

- **契约类型来源（全部来自 `crate::shared_types`，禁止引用 Service 内部路径）**：
  - `IdentitySection` → `crate::shared_types::IdentitySection`
  - `MemoryFileEntry` → `crate::shared_types::MemoryFileEntry`
  - `MemoryProvider` → `crate::shared_types::MemoryProvider`（或从 memory_saver re-export）

- **MemoryService 内部类型对 InitPhaseSlot 完全不可见**：
  - `plugins/services/memory/l1_identity::IdentitySection` — Service 内部，不引用
  - `plugins/services/memory/l2_working::MemoryFile` — Service 内部，不引用

- MemoryService 实现 `MemoryProvider` trait 时，负责将内部类型映射为 `shared_types` 中的契约类型再返回。

**`MemoryProvider` 类型擦除处理**——`provider_raw("memory")` 返回 `Option<Arc<dyn Any + Send + Sync>>`：

```rust
let memory_provider = match ap.provider_raw("memory") {
    Some(raw) => match raw.downcast::<Arc<dyn MemoryProvider>>() {
        Ok(arc) => (*arc).clone(),
        Err(_) => {
            tracing::warn!("init_phase: Memory Provider 类型不匹配，跳过初始化");
            return Ok(SlotDirective::Continue);
        }
    },
    None => {
        tracing::warn!("init_phase: Memory Provider 未注册，跳过初始化");
        return Ok(SlotDirective::Continue);
    }
};
```

**`is_new_session()` 降级逻辑**——设计文档 §4.1 步骤 2：

```rust
let is_new_session = memory_provider
    .is_new_session(&session_id)
    .await
    .unwrap_or(true);  // 超时/错误时默认新会话
```

**验收标准**：
- `cargo check` 通过
- `name()` 返回 `"init_phase"`
- `init()` 正确解析配置，边界校验返回 Err（S-R02）
- `run()` 中所有 `write_context_raw` 失败降级，不传播 Err
- `run()` 中所有降级路径有 `tracing::warn!`
- `run()` 函数与设计文档 §4.1 伪代码段数一致
- 未使用 `unwrap()` 或 `expect()`

---

### 步骤 3：实现 mod.rs（模块入口）

**目标**：按协议 §6.1 编写模块入口，对外只暴露必要类型，内部全部 `pub(crate)`。

**文件**：`src/plugins/slots/init_phase/mod.rs`

**要求**：

```rust
// ============================================
// 模块：init_phase 槽口
//
// 模块职责：
// 在 Pipeline Init 阶段执行会话初始化、身份加载、工作记忆恢复
//
// 模块边界：
// - 本模块负责：会话检测、记忆加载、系统提示词组装、上下文预检
// - 本模块不负责：工具注册（ToolRegistrySlot）、LLM 思考（LlmThinkerSlot）、
//                 工具执行（ToolExecutorSlot）、记忆持久化（MemorySaverSlot）
//
// 依赖 Provider：
// - "memory"（由 MemoryService 注册，提供 MemoryProvider trait）
//
// 被依赖模块：
// - llm_thinker 读取本模块写入的 system_prompt 和 identity
//
// 核心层实现：
// - SlotPlugin → InitPhaseSlot
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
//
// 协议合规：
// - S-R03 合规：无跨 run() 的可变状态
// - C-R03 合规：run() 可重入
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod types;

pub use config::InitPhaseConfig;
pub use plugin::InitPhaseSlot;
pub(crate) use error::InitPhaseError;
```

**验收标准**：
- `cargo check` 通过
- 对外暴露：`InitPhaseSlot` + `InitPhaseConfig`
- 对外隐藏：`types`、`error` 内部的实现细节
- 模块头注释包含职责边界和协议合规声明

---

### 步骤 4：注册到 Pipeline

**目标**：将 init_phase 注册到 Pipeline 的 init 阶段。

**涉及文件**：

1. `src/plugins/slots/mod.rs` —— 已有 `pub mod init_phase;`（步骤 0 已完成）

2. **`main.rs`**（或其他 Pipeline 构建代码）：

```rust
pipeline.add_slot(
    Phase::init(),
    Box::new(InitPhaseSlot::new()),
);
```

**注意**：当前 `InitPhaseConfig` 使用全部默认值，不强制要求用户配置。如果 AgentRuntime 支持插件配置注入，应在 `init()` 之前设置 config。

**验收标准**：
- `cargo check` 通过
- `Phase::init()` 阶段已注册 `InitPhaseSlot`

---

### 步骤 5：集成测试

**目标**：验证 InitPhaseSlot 从 init 到 run 再到 shutdown 的完整流程。

**文件**：在 `plugin.rs` 末尾添加 `#[cfg(test)] mod tests { ... }` 块。

**MockSlotAccessPoint 实现**：

```rust
struct MockSlotAccessPoint {
    session_id: String,
    phase_name: String,
    iteration: usize,
    messages: Vec<Message>,
    context_data: HashMap<String, Box<dyn Any + Send>>,
    memory_provider: Option<Arc<dyn MemoryProvider>>,
}
```

**MockMemoryProvider 实现**（使用 shared_types 契约类型）：

```rust
struct MockMemoryProvider {
    is_new: bool,
    identity: Option<crate::shared_types::IdentitySection>,
    working_memories: Vec<crate::shared_types::MemoryFileEntry>,
}

#[async_trait::async_trait]
impl MemoryProvider for MockMemoryProvider {
    async fn is_new_session(&self, _session_id: &str) -> Result<bool, MemoryError> {
        Ok(self.is_new)
    }
    async fn load_identity(&self, _session_id: &str) -> Result<crate::shared_types::IdentitySection, MemoryError> {
        self.identity.clone().ok_or(MemoryError::NotFound)
    }
    async fn load_working_memory(&self, _session_id: &str, _limit: usize) -> Result<Vec<crate::shared_types::MemoryFileEntry>, MemoryError> {
        Ok(self.working_memories.clone())
    }
    // ... 其他 MemoryProvider 方法 no-op 实现
}
```

**测试场景**（设计文档 §9.1–§9.6）：

| # | 测试名称 | 场景 | 断言 |
|:-:|---------|------|------|
| 1 | `test_new_session_initialization` | MemoryProvider 已注册，无历史 | session_meta.is_new=true，Continue |
| 2 | `test_old_session_recovery` | 有历史身份和工作记忆 | is_new=false，加载 identity + working_memory，Continue |
| 3 | `test_system_prompt_assembly_with_identity` | identity 已加载 | system_prompt 包含身份内容和开头模板 |
| 4 | `test_system_prompt_custom_template` | system_prompt_template 设为 "Custom: " | 提示词以 "Custom: " 开头 |
| 5 | `test_full_flow_all_providers` | 所有 Provider 已注册 | 所有步骤完成，Continue |
| 6 | `test_memory_provider_not_registered` | provider_raw("memory") 返回 None | 跳过初始化，Continue，warn 日志 |
| 7 | `test_memory_provider_type_mismatch` | provider_raw 返回错误类型 | 跳过初始化，Continue |
| 8 | `test_new_session_identity_missing` | is_new=true，load_identity 返回 Err | info 日志，Continue，identity 未写入 |
| 9 | `test_old_session_identity_failure` | is_new=false，load_identity 返回 Err | warn 日志，Continue |
| 10 | `test_working_memory_load_failure` | load_working_memory 返回 Err | warn 日志，Continue |
| 11 | `test_context_precheck_warning` | 消息数 > max_messages_precheck | warn 日志，Continue |
| 12 | `test_write_context_identity_failure` | write_context_raw("identity") 失败 | warn 日志，Continue |
| 13 | `test_write_context_working_memory_failure` | write_context_raw("working_memory") 失败 | warn 日志，Continue |
| 14 | `test_write_context_session_meta_failure` | write_context_raw("session_meta") 失败 | warn 日志，Continue |
| 15 | `test_working_memory_limit_zero` | working_memory_limit=0 | init() 返回 Err |
| 16 | `test_max_messages_precheck_zero` | max_messages_precheck=0 | init() 返回 Err |
| 17 | `test_config_parse_error` | plugin_config 格式错误 | init() 返回 Err |
| 18 | `test_config_default_values` | plugin_config 为空 JSON | 使用所有默认值，init 成功 |
| 19 | `test_is_new_session_timeout` | is_new_session 超时/失败 | 默认为新会话，Continue |
| 20 | `test_s_r03_compliance_repeated_run` | 同一 StepContext 运行两次 | 无状态污染 |
| 21 | `test_shutdown_completes` | shutdown 正常执行 | 不 panic |

**验收标准**：
- `cargo test` 所有测试通过
- 每个测试覆盖一个明确场景，断言具体返回值
- 测试名称体现测试意图
- 无 `#[ignore]` 测试

---

### 步骤 6：终态自检与硬编码审计

1. **全量编译与测试**：

```bash
cargo test
cargo clippy -- -D warnings
```

2. **硬编码扫描**：

```bash
# 检查数字阈值是否正确定义
rg 'DEFAULT_WORKING_MEMORY_LIMIT|DEFAULT_MAX_MESSAGES_PRECHECK' src/plugins/slots/init_phase/
# 检查日志前缀统一
rg '"init_phase:' src/plugins/slots/init_phase/ --type rust
# 检查无 panic/unwrap（测试除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/init_phase/ --type rust --glob '!*test*'
# 检查无 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/init_phase/ --type rust --glob '!*test*'
```

3. **合规检查清单**——逐项对照设计文档 §11 的三张检查表：

**跨平台规范 10 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| 1 | 无硬编码 URL/端点 | ✅ 不适用 |
| 2 | 无硬编码模型名 | ✅ 不适用 |
| 3 | 无硬编码超时值 | ✅ 不适用 |
| 4 | 无硬编码 API 版本 | ✅ 不适用 |
| 5 | 无硬编码 User-Agent | ✅ 不适用 |
| 6 | 无硬编码文件路径 | ✅ 不适用 |
| 7 | `DEFAULT_WORKING_MEMORY_LIMIT` + `DEFAULT_MAX_MESSAGES_PRECHECK` 常量 | ✅ |
| 8 | 无平台指令假设 | ✅ 不适用 |
| 9 | 测试使用 `std::env::temp_dir()` | ✅ |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 通过 | ☐ 待验证 |

**Slot 协议红线 3 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| S-R01 | 所有路径返回 `Continue` | ✅ |
| S-R02 | init 失败返回 Err（配置解析/limit=0） | ✅ |
| S-R03 | run() 无跨次可变状态 | ✅ |

**模块边界检查**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| mod.rs 只暴露 2 样东西 | InitPhaseSlot + InitPhaseConfig | ✅ |
| 不引用其他 Slot 具体类型 | ✅ | ✅ |

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
- 如果某步骤产生多余的 warning，必须先解决再进入下一步
- 如果发现设计文档有疏漏，记录到设计文档的 TODO 中，不在代码中猜测
