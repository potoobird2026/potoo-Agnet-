# MemorySaver Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 memory-saver 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：memory-saver（记忆持久化槽口）
- **模块定位**：一个实现 `SlotPlugin` trait 的插件，对外仅通过 `SlotAccessPoint` 交互，在 Pipeline 的 `memorize` 阶段将对话上下文（消息、工具观察结果）持久化到 MemoryService。职责单一，**不需要 Orchestrator 和内部 Component 体系**。
- **内部组件**：无（单 SlotPlugin，通过函数拆分文件，无 Component trait 体系）
- **代码目录**：所有源码位于 `src/plugins/slots/memory_saver/` 下
- **依赖项**：`tokio`（需 `time` feature）、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror`
- **设计文档**：`docs/slots/memory_saver/02_memory_saver_槽口开发文档.md`
- **外部类型引用**：
  - `MemoryProvider` trait → `crate::shared_types::MemoryProvider`（定义在 shared_types）
  - `MemoryError` → `crate::shared_types::MemoryError`
  - `MemoryStats` → `crate::shared_types::MemoryStats`
  - `Message` → `crate::shared_types::Message`（或 `crate::core::types::Message`，取决于实际位置）
  - `Observation` → `crate::shared_types::Observation`
  - `ExperienceEntry` → `crate::shared_types::ExperienceEntry`
  - `Timestamp` → `crate::core::types::Timestamp`
- **MemoryService 配套修改**：`plugins/services/memory/service.rs` 需替换空 tuple Provider 为 `Arc::new(MemoryProviderImpl { ... })`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | — | 不涉及（memory_saver 无 HTTP 请求） |
| 模型名 | — | 不涉及 |
| 超时秒数 | `Duration::from_secs(10)` 写在 `tokio::time::timeout` 调用处 | 使用 `DEFAULT_MEMORY_WRITE_TIMEOUT_SECS` 常量，从 `MemorySaverConfig.write_timeout_secs` 读取 |
| API 版本号 | — | 不涉及 |
| User-Agent | — | 不涉及 |
| 文件路径 | — | 不涉及（文件操作由 MemoryService 内部处理） |
| 数字阈值 | `min_messages_for_experience = 5` 写在业务逻辑中 | 定义为 `DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE` 常量，从配置读取 |
| 日志前缀 | `"[memory_saver]"` 散落在各个文件中 | 定义为 `LOG_PREFIX` 模块级常量 |
| 配置键名 | `"memory_saver"` 写在配置读取处 | 定义为 `CFG_KEY` 常量 |

---

## 项目目录结构

请手动创建以下目录结构（已有文件保留不动）：

```
src/plugins/
├── mod.rs                            # 已有 pub mod slots;
└── slots/
    ├── mod.rs                        # 需要添加 pub mod memory_saver;
    └── memory_saver/
        ├── mod.rs                    # 模块声明（组件协议 §6.1）
        ├── plugin.rs                 # 新建：MemorySaverSlot 定义（SlotPlugin 实现）
        ├── config.rs                 # 新建：MemorySaverConfig + 常量
        ├── types.rs                  # 新建：MemoryPersistedMarker
        ├── provider.rs               # 新建：pub use shared_types 的 MemoryProvider re-export
        └── error.rs                  # 新建：MemorySaverError + Into<PluginError>
```

模块声明链：

```
src/lib.rs                →  pub mod plugins;
src/plugins/mod.rs         →  pub mod slots;
src/plugins/slots/mod.rs   →  pub mod memory_saver;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `02_memory_saver_槽口开发文档.md`。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与设计文档完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - memory_saver 没有内部 Component，不需要 Orchestrator，不实现 Component trait
   - memory_saver 不做安全审计、不做工具执行、不做 LLM 请求、不做会话初始化
   - `run()` 只返回 `Continue`（S-R01），不返回其他变体
   - `MemoryProvider` trait 定义在 `shared_types` 中，本模块的 `provider.rs` 只做 `pub use crate::shared_types::MemoryProvider` 的 re-export，**不定义自己的 MemoryProvider trait**
   - `provider.rs` 同时 re-export MemoryError、MemoryStats、ExperienceEntry
   - 没有 `batch_size` 字段——设计文档 §11 明确标注"无 batch_size 字段"
   - `write_timeout_secs` 是 `u64` 类型，不是 `usize`（因为 Duration::from_secs 接收 u64）
   - `tokio::spawn` 返回的 JoinHandle 必须故意丢弃（设计文档 §4.1 步骤 5/6 的 fire-and-forget 注释）

3. **零硬编码**：
   a. `DEFAULT_MEMORY_WRITE_TIMEOUT_SECS` 和 `DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE` 定义为 `const` 常量
   b. `LOG_PREFIX` 定义为模块级常量 `const LOG_PREFIX: &str = "memory_saver:"`
   c. `CFG_KEY` 定义为 `const CFG_KEY: &str = "memory_saver"`（用于 `ctx.plugin_config.get("memory_saver")`）
   d. `Duration::from_secs(self.config.write_timeout_secs)` 从配置读取，不在 timeout 调用处写数字字面量

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理完整**——特别注意 memory_saver 有两种不同的错误处理策略（设计文档 §4.1）：
   a. **关键路径（StepContext 写入）**：`write_context_raw("last_persisted_count")`、`write_context_raw("last_indexed_count")`、`write_context_raw("memory_persisted")` 失败必须传播 `Err`（返回 `PluginError::Runtime`）
   b. **非关键路径（MemoryProvider 操作）**：`persist_messages`、`persist_observation`、`trigger_vector_index`、`extract_experiences` 失败一律降级——记录 error/warn 日志，返回 Continue
   c. `tokio::time::timeout` 超时：记录 warn，不传播 Err
   d. `provider_raw("memory")` 返回 None 或 downcast 失败：记录 warn，返回 Continue
   e. 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释说明

6. **一致性**：方法名、字段名、枚举变体名必须与设计文档完全一致，大小写敏感。

7. **禁止额外依赖**：只能使用 `std`、`tokio`（需 `time` feature）、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror` 以及项目内部模块。严禁引入 `reqwest`、`uuid`、`chrono`、`tempfile`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策）
   - 不允许写"做什么"的废话注释（如 `// 读取 session_id`）
   - 引用设计文档条款时用 `// 设计文档 §X.Y` 格式
   - 红线引用用 `// 遵循 S-R0X` 格式
   - `tokio::spawn` 必须带 fire-and-forget 注释

9. **测试同时生成**：
   - 为每个场景生成单元测试（设计文档 §9.1–§9.4）
   - 测试使用 `MockSlotAccessPoint` + `MockMemoryProvider`，不依赖真实 Pipeline 或 MemoryService
   - 测试名称体现测试意图（如 `test_messages_persisted_incrementally`）
   - 所有测试使用 `std::env::temp_dir()` 处理临时路径

10. **杜绝捷径**：
    - 不能因为 `persist_messages` 失败会被降级就省略降级日志——每条降级路径都必须有 `tracing::warn!` 或 `tracing::error!`
    - 不能省略 `skip(last_persisted_count)` 增量逻辑——必须实现增量持久化
    - `tokio::spawn` 必须正确处理闭包捕获（clone provider/session_id/messages），不产生生命周期问题
    - `shutdown()` 必须有完整的生命周期实现

11. **模块边界**：
    - 严禁引入会话初始化、工具注册、LLM 调用等 memory_saver 职责以外的功能
    - `MemoryProvider` 通过 `shared_types` 引用，不反向定义
    - 不直接引用 `plugins/services/memory` 的具体类型（通过 shared_types 间接）
    - `Observation` 类型引用 `crate::shared_types::Observation`

12. **日志规范**：
    - init 完成记录 `info`（携带配置摘要）
    - Provider 未注册记录 `warn`
    - Provider 类型不匹配记录 `warn`
    - 消息持久化成功记录 `debug`（携带条数和累计数）
    - 消息持久化失败记录 `error`
    - 消息持久化超时记录 `warn`
    - 观察结果持久化成功记录 `debug`
    - 观察结果持久化失败记录 `error`
    - 观察结果持久化超时记录 `warn`
    - 向量索引更新失败记录 `error`
    - 经验提取成功记录 `info`（携带条数）
    - 经验提取失败记录 `error`
    - 使用 `LOG_PREFIX` 常量统一日志前缀

13. **配置读取方式**：`ctx.plugin_config.get("memory_saver")` 嵌套路径（设计文档 §4.2），不是直接 `serde_json::from_value(ctx.plugin_config.clone())`。这是与 audit_phase/init_phase 的关键差异。

14. **PluginError 变体**：init 失败使用 `PluginError::Config`（不是 `InitFailed`），运行时 context 写入失败使用 `PluginError::Runtime`（不是 `Internal`）。严格按设计文档 §4.1/§4.2。
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架（手动操作）

**目标**：确保项目可编译，目录就绪，依赖齐全，模块注册到 `slots/mod.rs`。

**操作**：

1. 确认 Cargo.toml 包含以下依赖——特别注意 tokio 需要 `time` feature：

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "fs", "time"] }  # ★ time 是新增 feature
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
async-trait = "0.1"
thiserror = "1"
```

2. 确认 `shared_types` 中已定义 `MemoryProvider` trait（含 `persist_messages`、`persist_observation`、`trigger_vector_index`、`extract_experiences`、`stats` 五个方法）、`MemoryError`、`MemoryStats`、`ExperienceEntry`。

3. 创建 `src/plugins/slots/memory_saver/` 目录及以下空文件：

```
src/plugins/slots/memory_saver/
├── mod.rs           （空文件）
├── plugin.rs        （空文件）
├── config.rs        （空文件）
├── types.rs         （空文件）
├── provider.rs      （空文件）
└── error.rs         （空文件）
```

4. 在 `src/plugins/slots/mod.rs` 中添加：

```rust
pub mod memory_saver;    // ★ 新增
```

5. 确保 `cargo check` 通过（空模块可能产生未使用警告，可接受）。

**验收标准**：
- `cargo check` 无 error
- 目录结构与上述一致
- `src/plugins/slots/mod.rs` 包含 `pub mod memory_saver;`
- Cargo.toml 中 tokio 新增 `time` feature
- `shared_types` 中已定义 MemoryProvider trait

---

### 步骤 1：生成所有公共数据类型

**目标**：生成 `config.rs`、`error.rs`、`types.rs`、`provider.rs`，包含 MemorySaverConfig、MemorySaverError、MemoryPersistedMarker 等基础类型，以及 shared_types 的 re-export。

#### 1a：config.rs

**文件**：`src/plugins/slots/memory_saver/config.rs`

**要求**：

1. **常量**（跨平台规范 §1-3/§1-7）：

```rust
/// 设计文档 §2.3——记忆写入默认超时秒数
pub const DEFAULT_MEMORY_WRITE_TIMEOUT_SECS: u64 = 10;
/// 设计文档 §2.3——经验提取最小消息数默认值
pub const DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE: usize = 5;
```

2. **日志前缀常量**：

```rust
/// 设计文档 AI 宪法 §3.c——日志前缀统一管理
pub const LOG_PREFIX: &str = "memory_saver:";
```

3. **配置键名常量**：

```rust
/// 设计文档 §4.2——配置键名集中管理
pub const CFG_KEY: &str = "memory_saver";
```

4. **`MemorySaverConfig` 结构体**——严格按设计文档 §2.3，**无 `batch_size` 字段**：

```rust
/// 设计文档 §2.3——记忆持久化配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySaverConfig {
    /// 是否自动提取并存储用户消息，默认 true
    #[serde(default = "default_true")]
    pub persist_user_messages: bool,

    /// 是否自动存储工具调用结果，默认 true
    #[serde(default = "default_true")]
    pub persist_observations: bool,

    /// 是否触发向量索引更新，默认 true
    #[serde(default = "default_true")]
    pub update_vector_index: bool,

    /// 是否启用经验提取，默认 false
    #[serde(default)]
    pub enable_experience_extract: bool,

    /// 经验提取最小消息数，默认 DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE
    #[serde(default = "default_min_messages_for_experience")]
    pub min_messages_for_experience: usize,

    /// 写入超时秒数，默认 DEFAULT_MEMORY_WRITE_TIMEOUT_SECS
    #[serde(default = "default_write_timeout_secs")]
    pub write_timeout_secs: u64,
}
```

5. **默认值函数**：

```rust
fn default_true() -> bool { true }
fn default_min_messages_for_experience() -> usize { DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE }
fn default_write_timeout_secs() -> u64 { DEFAULT_MEMORY_WRITE_TIMEOUT_SECS }
```

**验收标准**：
- `cargo check` 通过
- 所有字段与设计文档 §2.3 完全一致
- `write_timeout_secs` 类型为 `u64`（不是 `usize`）
- 没有 `batch_size` 字段
- 三个 `DEFAULT_*` 常量为模块级 `const`
- `LOG_PREFIX` 和 `CFG_KEY` 为模块级 `const`

---

#### 1b：error.rs

**文件**：`src/plugins/slots/memory_saver/error.rs`

**要求**：

```rust
/// 设计文档 §2.4——MemorySaver 错误类型
#[derive(Debug, thiserror::Error)]
pub enum MemorySaverError {
    #[error("Memory Provider 未注册，无法持久化")]
    ProviderUnavailable,

    #[error("记忆写入超时（{timeout_secs} 秒）")]
    WriteTimeout { timeout_secs: u64 },

    #[error("记忆写入失败: {source}")]
    WriteError { source: String },

    #[error("向量索引更新失败: {source}")]
    VectorIndexError { source: String },

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),
}

/// 自动转换为 PluginError——所有运行时错误映射为 Internal
impl From<MemorySaverError> for PluginError {
    fn from(e: MemorySaverError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
```

**验收标准**：
- `cargo check` 通过
- 6 个变体名称与设计文档 §2.4 完全一致
- `WriteTimeout` 变体有 `timeout_secs: u64` 字段
- `WriteError` 变体有 `source: String` 字段
- `VectorIndexError` 变体有 `source: String` 字段

---

#### 1c：types.rs

**文件**：`src/plugins/slots/memory_saver/types.rs`

**要求**：

```rust
use crate::core::types::Timestamp;

/// 设计文档 §5.1——持久化完成标记，写入 StepContext["memory_persisted"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPersistedMarker {
    pub session_id: String,
    pub persisted_count: usize,
    pub timestamp: Timestamp,
}
```

**验收标准**：
- `cargo check` 通过
- 所有字段与设计文档 §5.1 完全一致
- `Timestamp` 从核心类型引用，不重复定义

---

#### 1d：provider.rs

**文件**：`src/plugins/slots/memory_saver/provider.rs`

**要求**：

```rust
/// 设计文档 §3.2——MemoryProvider trait 定义在 shared_types 中
/// 本文件仅做 re-export，不定义新类型
/// 遵循 §0.2 设计原则：MemoryService 从 shared_types 引用 MemoryProvider，不反向依赖 Slot

pub use crate::shared_types::{MemoryProvider, MemoryError, MemoryStats, ExperienceEntry};
```

**验收标准**：
- `cargo check` 通过
- 仅 re-export，无新类型定义
- 引用路径为 `crate::shared_types`（不是本地定义）
- 文件不含 `use crate::plugins::services::memory::*` 之类的反向引用

---

### 步骤 2：实现 MemorySaverSlot（plugin.rs）

**目标**：实现 `SlotPlugin` trait，包含完整生命周期和 8 段式 run() 流程。

**文件**：`src/plugins/slots/memory_saver/plugin.rs`

**要求**：

1. **`MemorySaverSlot` 结构体**——S-R03 合规：所有跨 run() 状态存入 StepContext，不在 Slot 字段中：

```rust
/// 设计文档 §6——Slot 结构体
/// S-R03 合规：last_persisted_count / last_indexed_count 存入 StepContext，
/// 不在 MemorySaverSlot 的字段中
pub struct MemorySaverSlot {
    config: MemorySaverConfig,
}
```

2. **`new()` 构造函数**——接收默认配置（init 时覆盖）：

```rust
impl MemorySaverSlot {
    pub fn new() -> Self {
        Self {
            config: MemorySaverConfig {
                persist_user_messages: true,
                persist_observations: true,
                update_vector_index: true,
                enable_experience_extract: false,
                min_messages_for_experience: DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE,
                write_timeout_secs: DEFAULT_MEMORY_WRITE_TIMEOUT_SECS,
            },
        }
    }
}
```

3. **`name()` 方法**——返回 `"memory_saver"`

4. **`init()` 方法**——严格按设计文档 §4.2，使用嵌套配置路径：

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    // 设计文档 §4.2——从嵌套路径读取配置（不是直接反序列化整个 ctx.plugin_config）
    // CFG_KEY 常量保障非硬编码
    let config_value = ctx
        .plugin_config
        .get(CFG_KEY)
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    self.config = serde_json::from_value(config_value)
        .map_err(|e| PluginError::Config(
            format!("{} 解析 memory_saver 配置失败: {}", LOG_PREFIX, e)
        ))?;

    tracing::info!(
        "{} 初始化完成: persist_user_messages={}, persist_observations={}, update_vector_index={}",
        LOG_PREFIX,
        self.config.persist_user_messages,
        self.config.persist_observations,
        self.config.update_vector_index
    );

    Ok(())
}
```

5. **`run()` 方法**——严格按设计文档 §4.1 的 8 段式流程：

```
步骤 1：获取 Memory Provider（降级：未注册/类型不匹配 → warn + Continue）
步骤 2：从 StepContext 读取 last_persisted_count 和 last_indexed_count
步骤 3：持久化用户消息（增量：skip(last_persisted_count)）
步骤 4：持久化工具观察结果
步骤 5：异步触发向量索引更新（fire-and-forget tokio::spawn）
步骤 6：异步经验提取（fire-and-forget tokio::spawn）
步骤 7：写入 memory_persisted 完成标记
步骤 8：返回 Continue
```

**关键错误处理策略**（设计文档 §4.1——两种不同的错误处理）：

| 路径 | 操作 | 失败后果 |
|------|------|---------|
| 关键路径 | `write_context_raw("last_persisted_count", ...)` | 传播 `Err(PluginError::Runtime(...))` |
| 关键路径 | `write_context_raw("last_indexed_count", ...)` | 传播 `Err(PluginError::Runtime(...))` |
| 关键路径 | `write_context_raw("memory_persisted", ...)` | 传播 `Err(PluginError::Runtime(...))` |
| 非关键路径 | `memory_provider.persist_messages()` | 记录 error，继续 |
| 非关键路径 | `memory_provider.persist_messages()` 超时 | 记录 warn，继续 |
| 非关键路径 | `memory_provider.persist_observation()` | 记录 error，继续 |
| 非关键路径 | `memory_provider.persist_observation()` 超时 | 记录 warn，继续 |
| 非关键路径 | `memory_provider.trigger_vector_index()` | fire-and-forget error 日志 |
| 非关键路径 | `memory_provider.extract_experiences()` | fire-and-forget error 日志 |
| 非关键路径 | `provider_raw("memory")` 返回 None | 记录 warn，Continue |
| 非关键路径 | downcast 失败 | 记录 warn，Continue |

**在代码中区分的具体写法**（设计文档 §4.1 步骤 3 的精确模式）：

```rust
// ★ 关键路径：progress marker 写入失败必须传播 Err
ap.write_context_raw("last_persisted_count", Box::new(new_count))
    .map_err(|e| PluginError::Runtime(
        format!("{} 写入持久化进度失败: {}", LOG_PREFIX, e)
    ))?;

// 非关键路径：memory provider 写入失败只记录日志
match tokio::time::timeout(
    Duration::from_secs(self.config.write_timeout_secs),
    memory_provider.persist_messages(&session_id, &new_messages),
)
.await
{
    Ok(Ok(())) => { /* 成功 */ }
    Ok(Err(e)) => {
        tracing::error!("{} 消息持久化失败: {}", LOG_PREFIX, e);
    }
    Err(_) => {
        tracing::warn!(
            "{} 消息持久化超时（{} 秒）",
            LOG_PREFIX, self.config.write_timeout_secs
        );
    }
}
```

**`tokio::spawn` fire-and-forget 模式**（设计文档 §4.1 步骤 5/6）：

```rust
// ★ fire-and-forget：JoinHandle 被故意丢弃
tokio::spawn(async move {
    if let Err(e) = provider_clone
        .trigger_vector_index(&session_id_clone, &new_messages)
        .await
    {
        tracing::error!("{} 向量索引更新失败: {}", LOG_PREFIX, e);
    }
});
// JoinHandle 故意丢弃——向量索引不阻塞 Pipeline
```

6. **`shutdown()` 方法**——设计文档 §4.3：

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    // 设计文档 §4.3——无内部缓冲区需要刷新
    // 每次 run() 直接将数据推送给 MemoryProvider，不缓存本地
    tracing::info!("{} 关闭，刷新缓冲区", LOG_PREFIX);
    Ok(())
}
```

**验收标准**：
- `cargo check` 通过
- `name()` 返回 `"memory_saver"`
- `init()` 使用 `ctx.plugin_config.get(CFG_KEY)` 嵌套路径
- `init()` 配置解析失败返回 `Err(PluginError::Config(...))`（S-R02）
- `run()` 中 StepContext 写入传播 Err，MemoryProvider 操作降级
- `run()` 中增量逻辑使用 `skip(last_persisted_count)` 和 `skip(last_indexed_count)`
- `tokio::spawn` 正确捕获 clone 变量
- 未使用 `unwrap()` 或 `expect()`

---

### 步骤 3：实现 mod.rs（模块入口）

**目标**：按协议 §6.1 编写模块入口，对外只暴露必要类型，内部全部 `pub(crate)`。

**文件**：`src/plugins/slots/memory_saver/mod.rs`

**要求**：

```rust
// ============================================
// 模块：memory_saver 槽口
//
// 模块职责：
// 在 Pipeline Memorize 阶段将对话上下文持久化到记忆系统
//
// 模块边界：
// - 本模块负责：消息持久化、观察结果存储、向量索引触发、经验提取
// - 本模块不负责：记忆检索（由 llm_thinker 通过 Provider 读取）、
//                 记忆压缩（由 compression 服务处理）
//
// 依赖 Provider：
// - "memory"（由 MemoryService 注册，提供 MemoryProvider trait）
//   注意：MemoryProvider trait 定义在 shared_types 中，不在本模块定义
//   本模块 provider.rs 只做 re-export
//
// 被依赖模块：
// - compression_hook 在同一 Memorize 阶段运行，依赖本模块完成持久化
//
// 核心层实现：
// - SlotPlugin → MemorySaverSlot（无状态，无内部组件）
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
// Provider 接口：见 provider.rs（re-export from shared_types）
//
// 协议合规：
// - S-R03 合规：持久化进度（last_persisted_count）存入 StepContext，不在 Slot 字段中
// - 组件协议 §0：本槽口无子模块，不需要 Orchestrator/Component/AccessPoint
// - 组件协议 §6：mod.rs 只暴露 MemorySaverSlot + MemorySaverConfig
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod provider;
pub mod types;

pub use config::MemorySaverConfig;
pub use plugin::MemorySaverSlot;
pub(crate) use error::MemorySaverError;
```

**验收标准**：
- `cargo check` 通过
- 对外暴露：`MemorySaverSlot` + `MemorySaverConfig`
- `provider` 模块对外可见（供 MemoryService 引用——但实际 MemoryService 应从 shared_types 引用，不反向依赖）
- 模块头注释包含职责边界和协议合规声明

---

### 步骤 4：注册到 Pipeline

**目标**：将 memory_saver 注册到 Pipeline 的 memorize 阶段，在 compression_hook 之前注册。

**涉及文件**：

1. `src/plugins/slots/mod.rs` —— 已有 `pub mod memory_saver;`（步骤 0 已完成）

2. **`main.rs`**（或其他 Pipeline 构建代码）：

```rust
pipeline
    .add_slot(Phase::memorize(), Box::new(MemorySaverSlot::new()))
    .add_slot(Phase::memorize(), Box::new(compression_hook_slot));
```

**执行顺序**：memory_saver 在 compression_hook **之前**注册，确保持久化完成后再触发压缩。

**验收标准**：
- `cargo check` 通过
- `Phase::memorize()` 阶段已注册 `MemorySaverSlot`
- memory_saver 在 compression_hook 之前注册

---

### 步骤 5：集成测试

**目标**：验证 MemorySaverSlot 从 init 到 run 再到 shutdown 的完整流程。

**文件**：在 `plugin.rs` 末尾添加 `#[cfg(test)] mod tests { ... }` 块。

**MockSlotAccessPoint 实现**：

```rust
struct MockSlotAccessPoint {
    session_id: String,
    messages: Vec<Message>,
    context_data: HashMap<String, Box<dyn Any + Send>>,
    memory_provider: Option<Arc<dyn MemoryProvider>>,
}
```

**MockMemoryProvider 实现**（设计文档 §9.5）：

```rust
struct MockMemoryProvider {
    persisted_messages: Arc<RwLock<Vec<Message>>>,
    persisted_observations: Arc<RwLock<Vec<Observation>>>,
    vector_index_called: Arc<AtomicBool>,
    extract_called: Arc<AtomicBool>,
}

#[async_trait]
impl MemoryProvider for MockMemoryProvider {
    async fn persist_messages(&self, _session_id: &str, messages: &[Message]) -> Result<(), MemoryError> {
        self.persisted_messages.write().await.extend(messages.to_vec());
        Ok(())
    }
    async fn persist_observation(&self, _session_id: &str, observation: &Observation) -> Result<(), MemoryError> {
        self.persisted_observations.write().await.push(observation.clone());
        Ok(())
    }
    async fn trigger_vector_index(&self, _session_id: &str, _messages: &[Message]) -> Result<(), MemoryError> {
        self.vector_index_called.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn extract_experiences(&self, _session_id: &str, _messages: &[Message]) -> Result<Vec<ExperienceEntry>, MemoryError> {
        self.extract_called.store(true, Ordering::SeqCst);
        Ok(vec![])
    }
    async fn stats(&self) -> Result<MemoryStats, MemoryError> {
        Ok(MemoryStats { l1_count: 0, l2_count: 0, l3_count: 0, experience_count: 0 })
    }
}
```

**测试场景**（设计文档 §9.1–§9.4）：

| # | 测试名称 | 场景 | 断言 |
|:-:|---------|------|------|
| T-N01 | `test_messages_persisted` | 3 条消息，Provider 已注册 | persist_messages 被调用，last_persisted_count=3，Continue |
| T-N02 | `test_observation_persisted` | context 中有 observation | persist_observation 被调用，Continue |
| T-N03 | `test_vector_index_triggered` | 有新消息，Provider 已注册 | trigger_vector_index 被调用（异步），last_indexed_count 更新 |
| T-N04 | `test_full_flow` | messages + observation 均存在 | 所有步骤完成，memory_persisted 标记存在，Continue |
| T-B01 | `test_no_new_messages` | messages 为空（无增量） | persist_messages **不**被调用，Continue |
| T-B02 | `test_no_observation` | context 中无 observation key | persist_observation 被跳过，Continue |
| T-B03 | `test_experience_not_extracted_below_min` | messages.len() < min_messages_for_experience | extract_experiences **不**被调用 |
| T-B04 | `test_all_config_disabled` | persist_user_messages=false, persist_observations=false, update_vector_index=false | 所有写入被跳过，Continue |
| T-B05 | `test_incremental_persistence` | 已有 last_persisted_count=2，3 条消息 | 仅 skip 前 2 条，持久化 1 条新消息 |
| T-E01 | `test_provider_not_registered` | provider_raw("memory") 返回 None | 跳过持久化，Continue |
| T-E02 | `test_provider_type_mismatch` | provider_raw 返回错误类型 | 跳过持久化，Continue |
| T-E03 | `test_persist_timeout` | persist_messages 超时（通过超短 timeout 模拟） | 记录 warn，Continue |
| T-E04 | `test_persist_error` | persist_messages 返回 Err | 记录 error，Continue |
| T-E05 | `test_init_config_error` | 配置格式错误 | init() 返回 Err |
| T-I01 | `test_repeated_run_idempotent` | 同一 StepContext 运行两次 | 第二次不重复写入（last_persisted_count 去重） |
| T-I02 | `test_new_slot_resume` | 新建 Slot 实例，使用同一 StepContext | 从 StepContext 读取进度，正确续接 |
| T-I03 | `test_shutdown_ok` | shutdown 正常执行 | 不 panic |

**验收标准**：
- `cargo test` 所有测试通过
- 每个测试覆盖一个明确场景，断言具体返回值
- 测试名称体现测试意图（如 `test_messages_persisted`）
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
# 检查常量正确定义
rg 'DEFAULT_MEMORY_WRITE_TIMEOUT_SECS|DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE' src/plugins/slots/memory_saver/
# 检查日志前缀统一
rg '"memory_saver:' src/plugins/slots/memory_saver/ --type rust
# 检查配置键名为 CFG_KEY 常量
rg '"memory_saver"' src/plugins/slots/memory_saver/ --type rust
# 检查无 panic/unwrap（测试除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/memory_saver/ --type rust --glob '!*test*'
# 检查无 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/memory_saver/ --type rust --glob '!*test*'
# 检查 write_context_raw 传播 Err 的关键路径
rg 'write_context_raw.*last_persisted_count' src/plugins/slots/memory_saver/ --type rust
rg 'write_context_raw.*last_indexed_count' src/plugins/slots/memory_saver/ --type rust
rg 'write_context_raw.*memory_persisted' src/plugins/slots/memory_saver/ --type rust
# 检查 tokio::spawn 在 fire-and-forget 模式
rg 'tokio::spawn' src/plugins/slots/memory_saver/ --type rust
```

3. **合规检查清单**（设计文档 §10）：

**跨平台规范 10 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| 1 | 无硬编码 URL/端点 | ✅ 不适用 |
| 2 | 无硬编码模型名 | ✅ 不适用 |
| 3 | `DEFAULT_MEMORY_WRITE_TIMEOUT_SECS` 常量 + serde default | ✅ |
| 4 | 无硬编码 API 版本 | ✅ 不适用 |
| 5 | 无硬编码 User-Agent | ✅ 不适用 |
| 6 | 无文件路径操作 | ✅ |
| 7 | `DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE` 常量 + serde default | ✅ |
| 8 | 无平台指令假设 | ✅ 不适用 |
| 9 | 测试使用 Mock MemoryProvider，无文件 I/O | ✅ |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 通过 | ☐ 待验证 |

**Slot 协议红线 3 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| S-R01 | 所有路径返回 `Continue` | ✅ |
| S-R02 | init 失败返回 Err（配置解析错误） | ✅ |
| S-R03 | 持久化进度存入 StepContext，Slot 无跨 run() 可变状态 | ✅ |

**模块边界检查**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| mod.rs 只暴露 MemorySaverSlot + MemorySaverConfig | ✅ |
| MemoryProvider trait 在 shared_types（不在本模块定义） | ✅ |
| 不引用其他 Slot 具体类型 | ✅ |

---

## MemoryService 配套修改（外部依赖）

memory_saver 的正常工作依赖 MemoryService 注册真正的 MemoryProvider（而非当前的空 tuple）。开发 memory_saver 时需同步修改以下文件：

**文件**：`src/plugins/services/memory/service.rs`

**修改内容**：
- 将 `ap.register_provider("memory", Arc::new(()) as Arc<dyn Any + Send + Sync>)` 替换为 `Arc::new(MemoryProviderImpl { ... })`
- `MemoryProviderImpl` 实现 `MemoryProvider` trait（定义在 `crate::shared_types`）
- 实现 5 个方法：`persist_messages`、`persist_observation`、`trigger_vector_index`、`extract_experiences`、`stats`

**依赖方向**：MemoryService 从 `shared_types` 引用 `MemoryProvider`，不反向使用 `crate::plugins::slots::memory_saver::provider`。

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
- 如果某步骤产生多余的 warning，必须先解决再进入下一步
- 如果发现设计文档有疏漏，记录到设计文档的 TODO 中，不在代码中猜测
