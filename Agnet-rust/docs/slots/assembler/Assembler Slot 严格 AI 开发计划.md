# Assembler Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 Assembler 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：assembler（上下文组装器）
- **模块定位**：Pipeline CONTEXT 阶段 SlotPlugin，在 ToolRegistrySlot 之后、LlmThinkerSlot 之前运行。负责动态计算 Token 预算、从多方数据源收集上下文内容、组装 System Prompt，通过厂商输出适配器优化排版后写入 StepContext。无内部 Component 体系，5 个 ContextProvider 由 BlockCollector 按 priority 串行调用（不需要 Orchestrator）
- **内部结构**：组装引擎（BudgetCalculator / QuotaAllocator / BlockCollector / MessageBuilder）+ 5 个 ContextProvider + DocumentCompactor + RuleLlmSelector + 2 个 OutputAdapter。**注意**：assembler 没有 Orchestrator，没有 Component trait 体系，Provider 按 priority 排序后由 BlockCollector 串行调用
- **代码目录**：`src/plugins/slots/assembler/`
- **依赖项**：`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror`、`regex`（已有）
- **设计文档**：`docs/slots/assembler/ConversationAssembler-开发设计文档.md`
- **外部类型引用**（全部来自 `crate::shared_types`，Slot接入协议 §0/§2.2 红线：禁止引用 Service/Slot 内部类型）：
  - `DynProvider`、`Message`、`MessageRole`、`ContentBlock` → `crate::shared_types`
  - `PROVIDER_MEMORY`、`PROVIDER_TOOL` → `crate::shared_types`
  - `MemoryProvider` → `crate::shared_types::memory`
  - `ToolDefinition` → `crate::shared_types::tool`
  - `assembler::*` → `crate::shared_types::assembler`（ContextProvider trait、ContextBlock、ContextQuota、ProvidedContext、ProviderError、AssemblyReport、ProviderStat、AssemblyWarning、LlmOutputAdapter trait、CompactionConfig、RulePoolConfig、L3RulesConfig、RuleGroup、AssemblerConfig、ProviderSlotConfig）

---

## 协议合规红线清单

本模块受以下协议约束，违反任意一条将导致代码审查不通过：

| 协议 | 红线 | 本条模块必须遵守 |
|:-----|:-----|:----------------|
| Slot接入协议 §1 | 只能实现 `SlotPlugin` trait | AssemblerSlot 只 impl SlotPlugin，不 impl ServicePlugin。不实现旧版 `Slot` trait |
| Slot接入协议 §2 | 只能通过 `SlotAccessPoint` 通信 | 所有 `provider_raw()` / `read_context_raw()` / `messages()`，不碰 StepContext 内部字段 |
| Slot接入协议 §6 | 生命周期：`init → run → shutdown` | 三个方法全实现，init 只调一次，run 调多次，shutdown 只调一次 |
| Slot接入协议 S-R01 | 所有 `SlotDirective` 变体必须被正确处理 | Assembler 只返回 `Continue`——所有异常路径降级为 Continue + warn 日志 |
| Slot接入协议 S-R02 | `init()` 失败 = 不加载 | `init()` 返回 Err 后 `run()` 不被 Pipeline 调用 |
| Slot接入协议 S-R03 | `run()` 中禁止持有跨次调用的可变状态 | 所有 Provider 列表在 `init()` 中初始化，`run()` 中只读 |
| shared_types契约协议 T-R01 | Provider trait 禁止定义在插件内部 | `ContextProvider` trait 定义在 `shared_types/assembler/context.rs` |
| shared_types契约协议 K-R01 | 禁止 `provider_raw()` 使用裸字符串 | 全部使用 `PROVIDER_*` 常量 |
| shared_types契约协议 D-R01 | 禁止 `DynXxxProvider` | 统一使用 `DynProvider<T>` |
| 跨平台规范 §2 | 禁止硬编码路径 | 路径通过 `config.resolve_paths(data_dir)` 基于 `ctx.data_dir` 解析 |
| 跨平台规范 §2.3 | 禁止相对路径依赖 CWD | 模板路径基于 `data_dir.join()` 解析 |
| 跨平台规范 §2.4 | 路径拼接用 `join()` | 使用 `data_dir.join(&self.base_prompt_path)`，禁止 `format!("{}/{}", dir, file)` |

---

## 硬编码专项预防纲领

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 路径 | `"resources/templates/base_prompt.md"` 直接写死 | 从 `config.base_prompt_path` 读取，在 `resolve_paths()` 中基于 `data_dir` 解析 |
| Provider key | `provider_raw("memory")` | 使用 `PROVIDER_MEMORY` 常量 |
| context_window | `128_000` 写死在 BudgetCalculator 中 | 从 StepContext 中的 LlmConfig 读取（通过 `ap.read_context_raw("llm_config")`） |
| 策略名称 | `"balanced"` 写在 match 分支 | 从 `config.injection_policy` 读取 |
| 数字阈值 | `max_injection_tokens = 30000` | 从 `config.max_injection_tokens` 读取 |
| 超时秒数 | `Duration::from_millis(5000)` | 从 `config.rule_pool.selection_timeout_ms` 读取 |
| chars_per_token | `4.0` 散落在 compact 逻辑 | 从 `config.compaction.chars_per_token` 读取 |
| 日志前缀 | `"[assembler]"` 散落 | 定义为 `LOG_PREFIX` 常量 |
| 模板占位符 | `"{{rules}}"` 字符串散落 | 定义为 `RULES_PLACEHOLDER` 等常量（`const RULES_PLACEHOLDER: &str = "{{rules}}"`） |

---

## 项目目录结构

```
src/
  shared_types/
    mod.rs                                        (改) — 新增 pub mod assembler;
    assembler/
      mod.rs                                      (NEW) — 重新导出所有子模块
      context.rs                                  (NEW) — ContextProvider trait + ContextBlock + ContextQuota + ProvidedContext + ProviderError
      report.rs                                   (NEW) — AssemblyReport + ProviderStat + AssemblyWarning
      adapter.rs                                  (NEW) — LlmOutputAdapter trait
      compaction.rs                               (NEW) — CompactionConfig
      rule_pool.rs                                (NEW) — RulePoolConfig + L3RulesConfig + RuleGroup
      config.rs                                   (NEW) — AssemblerConfig + ProviderSlotConfig

  plugins/slots/
    mod.rs                                        (改) — 新增 pub mod assembler;
    assembler/
      mod.rs                                      (NEW) — 模块入口，公开导出 AssemblerSlot + AssemblerConfig
      config.rs                                   (NEW) — 配置加载（serde_json from PluginInitContext，resolve_paths）
      slot.rs                                     (NEW) — AssemblerSlot（impl SlotPlugin）

      providers/
        mod.rs                                    (NEW) — 模块声明 + build_providers() 工厂函数
        system_prompt.rs                          (NEW) — SystemPromptProvider（pri=0）
        identity.rs                               (NEW) — IdentityProvider（pri=5）
        working_memory.rs                         (NEW) — WorkingMemoryProvider（pri=20）
        vector_memory.rs                          (NEW) — VectorMemoryProvider（pri=30）
        compression_summary.rs                    (NEW) — CompressionSummaryProvider（pri=10）

      assembly/
        mod.rs                                    (NEW) — 模块声明
        budget.rs                                 (NEW) — BudgetCalculator（纯函数）
        quota.rs                                  (NEW) — QuotaAllocator（纯函数）
        collector.rs                              (NEW) — BlockCollector
        builder.rs                                (NEW) — MessageBuilder

      compaction/
        mod.rs                                    (NEW) — 模块声明
        doc_compactor.rs                          (NEW) — DocumentCompactor

      rule_pool/
        mod.rs                                    (NEW) — 模块声明
        rule_llm_selector.rs                      (NEW) — RuleLlmSelector

      output_adapters/
        mod.rs                                    (NEW) — 模块声明
        anthropic.rs                              (NEW) — AnthropicOutputAdapter
        openai.rs                                 (NEW) — OpenAiOutputAdapter

resources/
  templates/
    base_prompt.md                                (NEW) — 基础 Prompt 模板骨架
    injection_layout.md                           (NEW) — 记忆内容注入排版模板
```

---

## AI 宪法

```
[宪法已生效，本次对话必须无条件遵守]

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，
   必须与 docs/slots/assembler/ConversationAssembler-开发设计文档.md 完全一致，不得自行增删改。

2. **零幻觉**：
   - Assembler 有且只有 5 个 ContextProvider（SystemPrompt/Identity/CompressionSummary/WorkingMemory/VectorMemory），不凭空生成第 6 个
   - Config 的 default 值必须与设计文档 §3.7 逐字段一致，比例数值必须精确
   - injection_order 有且只有 5 个元素（system_prompt/identity/compression_summary/working_memory/vector_memory）
   - QuotaAllocator 有且只有 5 种策略（balanced/memory_focused/token_efficient/identity_only/minimal）
   - 不可凭空生成第 6 种策略，不可修改百分比

3. **Provider::provide() 的第一个参数必须是 &dyn SlotAccessPoint**（非 &StepContext）。
   - 所有 Provider 通过 `ap.provider_raw()` / `ap.read_context_raw()` / `ap.messages()` 获取数据
   - 禁止在 Provider 中直接访问 StepContext 内部字段
   - 禁止在 Provider 中引入 ContractRegistry

4. **所有 provider_raw() 调用必须使用 PROVIDER_* 常量**，禁止裸字符串。
   - 正确：`ap.provider_raw(PROVIDER_MEMORY)`
   - 错误：`ap.provider_raw("memory")`

5. **所有路径通过 config.resolve_paths(data_dir) 解析**，禁止硬编码路径字符串。
   - 正确：`data_dir.join("templates/base_prompt.md")`
   - 错误：`"resources/templates/base_prompt.md"`

6. **禁止 todo!()、unimplemented!() 或空函数体**——每个函数必须完整实现。
   - 例外：L3 向量检索在 NoopEmbeddingModel 下返回空 Vec 是正常行为，不是占位符

7. **错误处理完整**：
   a. `init()` 配置解析失败 → `PluginError::Config`（S-R02）
   b. Provider::provide() 中 provider_raw() 返回 None → 返回 `ProvidedContext { blocks: vec![], tokens_used: 0 }`（降级，不报错）
   c. `write_context_raw()` 失败 → `tracing::warn!`，继续（不传播 Err）
   d. DocumentCompactor 压缩失败 → 返回 `compact()` 的原始文本（降级）
   e. RuleLlmSelector 调用 LLM 失败 → `fallback_enabled=true` 时返回全部规则，否则返回空规则组
   f. 模板文件不存在 → Provider 返回空（`silent_on_empty = true`）
   g. 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释 `// 测试中安全`

8. **run() 中禁止持有跨次可变状态**（S-R03）：
   - Provider 列表在 `init()` 中初始化，`run()` 中只读访问
   - RuleCache 使用 `RwLock` 内部可变性
   - BlockCollector 不缓存结果

9. **run() 所有路径都返回 SlotDirective::Continue**：
   - enabled=false → Continue
   - Provider 全部失败 → Continue
   - 超限紧急裁剪 → Continue
   - 不允许返回 BreakPhase、AbortStep、JumpTo 等

10. **日志规范**：
    - init 完成：`info!`（携带 enabled/enabled/debug/injection_policy）
    - enabled=false：`debug!`（优先级低，正常启动信息）
    - Provider 未注册（provider_raw 返回 None）：`debug!`（非 warn——某些 Provider 可能未配置，这是正常降级）
    - Provider 执行完成：`debug!`（携带 tokens_used）
    - 紧急裁剪触发：`warn!`
    - 模板文件不存在：`info!`（首次加载时，非重复）
    - 使用 `LOG_PREFIX` 常量统一前缀

11. **模块边界**（Slot接入协议 §0/§2.2 红线）：
    - 严禁引入工具注册、LLM 调用执行、记忆持久化等 assembler 职责以外的功能
    - 所有跨插件数据通过 `crate::shared_types` 中的契约类型交互
    - 严禁通过 `use crate::plugins::services::*` 或 `use crate::plugins::slots::*` 引用任何类型
    - MemoryService 内部类型（l1_identity::IdentitySection、l2_working::MemoryFile 等）对 assembler 完全不可见

12. **测试同时生成**：
    - 为每层生成单元测试
    - 使用 MockSlotAccessPoint + MockContextProvider，不依赖真实 Pipeline 或 Service
    - 测试名称体现测试意图（如 `test_budget_computation_with_large_context`）
    - 所有测试使用 `std::env::temp_dir()` 处理临时路径
    - 测试覆盖 enabled=false 路径（回归验证）

13. **注释规则**：
    - 只写"为什么"的注释，不写"做什么"的废话注释
    - 引用设计文档条款时用 `// 设计文档 §X.Y` 格式
    - 红线引用用 `// 遵循 S-R0X` 格式
    - 每个模块文件顶部有 `/*! 模块说明 */` 文档注释

14. **禁止引入额外依赖**：只能使用 `std`、`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`thiserror`、`regex` 以及项目内部模块。严禁引入 `reqwest`、`uuid`、`chrono`、`tempfile`、`handlebars` 等。

15. **职责分离**：
    - DocumentCompactor 只做文本压缩，不做 Token 预算计算
    - BudgetCalculator 只做预算计算，不做配额分配
    - QuotaAllocator 只做配额分配，不调用 Provider
    - BlockCollector 只做收集，不做消息拼装
    - MessageBuilder 只做拼装，不做适配
    - SystemPromptProvider 调用 RuleLlmSelector，但 RuleLlmSelector 本身是独立模块
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**目标**：确保项目可编译，目录就绪，依赖齐全，模块注册到 `slots/mod.rs`。

**操作**：

1. 确认 Cargo.toml 包含以下依赖（无需新增——tokio/tracing/serde/serde_json/async-trait/thiserror/regex 均已存在）：

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "fs"] }    # sync 用于 RwLock，fs 用于模板文件读取
tracing = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "1"
regex = "1"
```

2. 创建 `src/shared_types/assembler/` 目录（7 个空文件）：
```
src/shared_types/assembler/
├── mod.rs
├── context.rs
├── report.rs
├── adapter.rs
├── compaction.rs
├── rule_pool.rs
└── config.rs
```

3. 在 `src/shared_types/mod.rs` 中添加：`pub mod assembler;`

4. 创建 `src/plugins/slots/assembler/` 目录及子目录：
```
src/plugins/slots/assembler/
├── mod.rs
├── config.rs
├── slot.rs
├── providers/
│   └── mod.rs
├── assembly/
│   └── mod.rs
├── compaction/
│   └── mod.rs
├── rule_pool/
│   └── mod.rs
└── output_adapters/
    └── mod.rs
```

5. 在 `src/plugins/slots/mod.rs` 中添加：`pub mod assembler;`

**验收标准**：
- [ ] `cargo check` 无 error（空模块可能产生 `unused` warning，可接受）
- [ ] 目录结构与上述一致
- [ ] `src/shared_types/mod.rs` 包含 `pub mod assembler;`
- [ ] `src/plugins/slots/mod.rs` 包含 `pub mod assembler;`

---

### 步骤 1：shared_types 契约层

**文件**：`src/shared_types/assembler/` 下 7 个文件

#### 1a：`shared_types/assembler/mod.rs`

```rust
pub mod config;
pub mod context;
pub mod report;
pub mod adapter;
pub mod compaction;
pub mod rule_pool;

pub use config::*;
pub use context::*;
pub use report::*;
pub use adapter::*;
pub use compaction::*;
pub use rule_pool::*;
```

#### 1b：`shared_types/assembler/context.rs`

包含设计文档 §3.2 定义的类型：

```rust
use async_trait::async_trait;

// ── 跨插件数据结构 ──

/// 单个内容块（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ContextBlock {
    pub section_title: String,
    pub content: String,
    pub source: String,
    pub token_count: usize,
}

/// 提供者返回的完整内容（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ProvidedContext {
    pub blocks: Vec<ContextBlock>,
    pub tokens_used: usize,
}

/// 上下文配额（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ContextQuota {
    pub max_tokens: usize,
    pub max_items: usize,
    pub max_chars_per_item: usize,
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,
}

impl Default for ContextQuota { /* ... */ }

// ── Provider 错误 ──

/// 内容提供者错误（设计文档 §3.2）
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("内容缺失: {0}")]
    Missing(String),
    #[error("配额超限: used={used}, max={max}")]
    QuotaExceeded { used: usize, max: usize },
    #[error("内部错误: {0}")]
    Internal(String),
}

// ── Provider trait （遵循 shared_types契约协议 T-R01）──

/// 内容提供者 trait——定义在 shared_types 中，不归属于 Assembler 或任何 Provider 实现方
/// （设计文档 §3.2，shared_types契约协议 T-R01）
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> u8;
    fn allow_truncation(&self) -> bool { true }
    fn silent_on_empty(&self) -> bool { true }
    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize;

    // 注意：参数 ap: &dyn SlotAccessPoint（非 &StepContext），遵循 Slot接入协议 §2
    async fn provide(
        &self,
        ap: &dyn crate::core::access::SlotAccessPoint,
        quota: &ContextQuota,
        config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError>;
}
```

**详细要求**：
- `ContextQuota` 的默认值：`max_tokens: 0, max_items: 5, max_chars_per_item: 0, min_guaranteed_tokens: 0, allow_compaction: true`
- `ContextProvider` 的 `provide()` 第一个参数为 `&dyn crate::core::access::SlotAccessPoint`（注意这里是 `SlotAccessPoint` 的实际 trait 路径，不是 `StepContext`）
- `ContextProvider::silent_on_empty()` 默认返回 `true`——空数据不报错
- 禁止添加 `Provide` 方法额外参数（如 `ctx: &StepContext`）

#### 1c：`shared_types/assembler/report.rs`

```rust
use std::time::Duration;

/// 组装报告（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct AssemblyReport {
    pub request_id: String,
    pub session_id: String,
    pub context_window: usize,
    pub total_available: usize,
    pub history_tokens: usize,
    pub injection_budget: usize,
    pub final_total_tokens: usize,
    pub selected_policy: String,
    pub provider_stats: Vec<ProviderStat>,
    pub rules_group: String,
    pub adapter_used: Option<String>,
    pub truncation_applied: bool,
    pub warnings: Vec<AssemblyWarning>,
    pub assembly_duration: Duration,
}

/// Provider 执行统计（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct ProviderStat {
    pub name: String,
    pub priority: u8,
    pub tokens_used: usize,
    pub blocks_count: usize,
    pub success: bool,
    pub error: Option<String>,
}

/// 组装警告（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct AssemblyWarning {
    pub code: String,
    pub message: String,
}
```

**详细要求**：
- `request_id` 使用 `Uuid` 生成——但 `uuid` 不是本模块依赖！处理方式：`request_id: String` 不作为必需字段，设为 `String::new()` 或 `"assembler-{timestamp}"` 格式。**或者**：在主流程中用 `std::time::SystemTime` 生成一个简单 ID。**禁止引入 `uuid` 依赖**
- `assembly_duration: Duration`——来自 `run()` 中的 `start.elapsed()`

#### 1d：`shared_types/assembler/adapter.rs`

```rust
/// 厂商输出适配契约（设计文档 §3.4，shared_types契约协议 T-R01）
#[async_trait]
pub trait LlmOutputAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    fn adapt_system_prompt(&self, text: &str, context_window: usize) -> String { text.to_string() }
    fn adapt_context_block(&self, section_title: &str, content: &str) -> String {
        format!("{}\n\n{}", section_title, content)
    }
    fn recommended_rule_count(&self, context_window: usize) -> usize { usize::MAX }
}
```

**注意**：`#[async_trait]` 虽然当前方法都是同步的，但加上以允许未来的异步适配器实现。

#### 1e：`shared_types/assembler/compaction.rs`

```rust
/// DocumentCompactor 配置（设计文档 §3.5）
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub chars_per_token: f64,
    pub preserve_unique_entities: bool,
    pub min_sentences_for_compaction: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self { chars_per_token: 4.0, preserve_unique_entities: true, min_sentences_for_compaction: 3 }
    }
}
```

**详细要求**：
- 字段名、类型、默认值与设计文档 §3.5 完全一致
- `chars_per_token: f64`——非 usize
- `preserve_unique_entities: bool`——非 `String`

#### 1f：`shared_types/assembler/rule_pool.rs`

```rust
/// 规则池配置（设计文档 §3.6）
#[derive(Debug, Clone)]
pub struct RulePoolConfig {
    pub enabled: bool,
    pub llm_name: String,           // "secondary" / "primary"
    pub rules_file: String,         // 空路径 = 不加载文件规则
    pub selection_timeout_ms: u64,
    pub fallback_enabled: bool,
    pub l3_rules: L3RulesConfig,
}

/// L3 规则来源配置（设计文档 §3.6）
#[derive(Debug, Clone)]
pub struct L3RulesConfig {
    pub enabled: bool,
    pub max_items: usize,
    pub query_template: String,     // "{user_text} 行业经验教训"
}

impl Default for RulePoolConfig { /* ... */ }

/// 规则组（设计文档 §3.6）
#[derive(Debug, Clone)]
pub struct RuleGroup {
    pub name: String,       // "code+general"
    pub rules: Vec<String>, // 精选后的规则条目列表
}

impl RuleGroup {
    pub fn empty() -> Self { Self { name: "empty".into(), rules: vec![] } }
}
```

**详细要求**：
- `RulePoolConfig` 的默认值与设计文档 §3.6 逐字段一致
- `L3RulesConfig` 默认 `enabled: false`（默认关闭，不影响输出）
- `query_template` 默认 `"{user_text} 行业经验教训".into()`
- `RuleGroup::empty()` 返回 `name: "empty".into()`

#### 1g：`shared_types/assembler/config.rs`

**AssemblerConfig** 严格按设计文档 §3.7 定义，包含：

- `enabled: bool`（默认 false）
- `debug: bool`（默认 false）
- `response_reserve_ratio: f64`（0.2）
- `history_budget_ratio: f64`（0.7）
- `min_recent_messages: usize`（4）
- `max_injection_tokens: usize`（30000）
- `minimum_context_size: usize`（1000）
- `injection_policy: String`（"balanced"）
- `disabled_providers: Vec<String>`（空）
- `providers: HashMap<String, ProviderSlotConfig>`（包含 4 个默认 Provider 配置）
- `injection_order: Vec<String>`（5 个元素：system_prompt / identity / compression_summary / working_memory / vector_memory）
- `compaction: CompactionConfig`（默认）
- `rule_pool: RulePoolConfig`（默认）
- `output_adapter_enabled: bool`（true）
- `base_prompt_path: String`（"templates/base_prompt.md"）
- `injection_layout_path: String`（"templates/injection_layout.md"）

**ProviderSlotConfig**：

```rust
#[derive(Debug, Clone)]
pub struct ProviderSlotConfig {
    pub enabled: bool,
    pub max_tokens: usize,
    pub max_items: usize,
    pub max_chars_per_item: usize,
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,
    pub allow_truncation: bool,
}
```

**详细要求**：
- `providers` 的默认 HashMap 必须有 4 个 key（identity / working_memory / vector_memory / compression_summary），各自的 `ProviderSlotConfig` 与设计文档 §3.7 完全一致
- `identity` 的 `allow_compaction: false, allow_truncation: false`——永不裁剪身份
- `injection_order` 的 5 个元素顺序必须与设计文档一致
- 所有比例数值（0.2、0.7）必须精确

#### 1h：`shared_types/mod.rs` 修改

```rust
// 在现有 pub mod 声明后添加
pub mod assembler;
```

**验收标准**：
- [ ] `cargo check` 通过，零 error（warning 可接受）
- [ ] `ContextProvider` trait 定义在 `shared_types/assembler/context.rs` 中，不在模块内部
- [ ] `ContextProvider::provide()` 参数为 `&dyn SlotAccessPoint`，非 `&StepContext`
- [ ] 所有 struct derive `Debug + Clone`
- [ ] `AssemblerConfig` 的 `Default` impl 与设计文档 §3.7 逐字段一致
- [ ] `CompactionConfig` 的 `chars_per_token: f64`（非 usize）
- [ ] `RulePoolConfig` 的 `l3_rules.query_template` 默认值正确
- [ ] 没有使用 `core::contract` 路径
- [ ] 没有引入 `uuid`、`handlebars`、`chrono` 等额外依赖

**严格禁止**：
- ❌ 不要在 shared_types 中放任何实现代码（只有类型定义 + trait 声明 + Default impl）
- ❌ 不要在 ContextProvider::provide() 的参数中使用 StepContext
- ❌ 不要引入 ContractRegistry 或类似全局注册表
- ❌ 不要在 shared_types 中放文件 IO 代码（模板读取在 plugins/slots/assembler 中做）

---

### 步骤 2：plugins/slots/assembler/config.rs + mod.rs

#### 2a：`plugins/slots/assembler/config.rs`

```rust
/// 日志前缀（跨平台规范 §1.7——常量集中管理）
pub const LOG_PREFIX: &str = "assembler:";

/// 模板占位符常量（防止散落字符串）
pub const RULES_PLACEHOLDER: &str = "{{rules}}";
pub const ENV_INFO_PLACEHOLDER: &str = "{{env_info}}";
pub const IDENTITY_PLACEHOLDER: &str = "{{identity}}";
pub const COMPRESSION_SUMMARY_PLACEHOLDER: &str = "{{compression_summary}}";
pub const WORKING_MEMORY_PLACEHOLDER: &str = "{{working_memory}}";
pub const VECTOR_MEMORY_PLACEHOLDER: &str = "{{vector_memory}}";
```

**`resolve_paths` 方法**（遵循跨平台规范 §2.3/§2.4）：

```rust
impl AssemblerConfig {
    /// 基于 data_dir 解析模板路径（跨平台规范 §2.3：不依赖 CWD；§2.4：使用 join()）
    pub fn resolve_paths(&mut self, data_dir: &std::path::Path) {
        if !self.base_prompt_path.is_empty() {
            self.base_prompt_path = data_dir.join(&self.base_prompt_path)
                .to_string_lossy().to_string();
        }
        if !self.injection_layout_path.is_empty() {
            self.injection_layout_path = data_dir.join(&self.injection_layout_path)
                .to_string_lossy().to_string();
        }
    }
}
```

**注意**：`resolve_paths` 使用 `data_dir` 参数（来自 `PluginInitContext.data_dir`），禁止使用 `std::env::current_dir()`。

#### 2b：`plugins/slots/assembler/mod.rs`

```rust
pub mod config;
pub mod slot;
mod providers;
mod assembly;
mod compaction;
mod rule_pool;
mod output_adapters;

pub use config::AssemblerConfig;
pub use slot::AssemblerSlot;
```

**注意**：只公开导出 `AssemblerSlot` 和 `AssemblerConfig`。内部模块（providers/assembly/compaction/rule_pool/output_adapters）使用 `mod`（私有）。

**验收标准**：
- [ ] `cargo check` 通过
- [ ] `LOG_PREFIX` 定义为模块级常量
- [ ] 6 个模板占位符常量定义为 `const`
- [ ] `resolve_paths()` 使用 `data_dir.join()`，非 `format!()`
- [ ] `mod.rs` 只公开导出 `AssemblerSlot` + `AssemblerConfig`

---

### 步骤 3：DocumentCompactor

**文件**：`src/plugins/slots/assembler/compaction/doc_compactor.rs`

**要求**：

```rust
use crate::shared_types::assembler::CompactionConfig;

/// 轻量、临时、只读文档压缩器（设计文档 §6）
///
/// 不污染原文件，不调 LLM，只做文本级压缩。
pub struct DocumentCompactor {
    config: CompactionConfig,
}

impl DocumentCompactor {
    pub fn new(config: CompactionConfig) -> Self { Self { config } }

    /// 压缩文本到 max_tokens 以内
    ///
    /// preserve_entities = true 时优先保留含独有实体的句子。
    /// 无法压缩到目标大小时返回原始文本（降级，不截断）。
    pub fn compact(&self, text: &str, max_tokens: usize, preserve_entities: bool) -> String {
        if text.is_empty() { return String::new(); }

        let max_chars = (max_tokens as f64 * self.config.chars_per_token) as usize;
        if text.len() <= max_chars { return text.to_string(); }

        let sentences = self.split_sentences(text);
        if sentences.len() < self.config.min_sentences_for_compaction {
            return text.to_string(); // 句子太少，不压缩
        }

        // 1. 提取独有实体（如 API_KEY_XXXX 等）
        let unique_entities = if preserve_entities && self.config.preserve_unique_entities {
            self.extract_unique_entities(text)
        } else {
            Vec::new()
        };

        // 2. 逐句评分
        let mut scored: Vec<(usize, &str, f64)> = sentences.iter().enumerate()
            .map(|(i, s)| (i, s.as_str(), self.score_sentence(s, &unique_entities)))
            .collect();

        // 3. 标记 must_keep（含独有实体的句子）
        let must_keep: std::collections::HashSet<usize> = if preserve_entities && self.config.preserve_unique_entities {
            scored.iter()
                .filter(|(_, s, _)| unique_entities.iter().any(|e| s.contains(e)))
                .map(|(i, _, _)| *i)
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        // 4. 按分数降序排列（must_keep 优先）
        scored.sort_by(|a, b| {
            let a_keep = must_keep.contains(&a.0);
            let b_keep = must_keep.contains(&b.0);
            b_keep.cmp(&a_keep).then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
        });

        // 5. 取 top 句直到满配额
        let mut result = String::new();
        let mut selected_indices: Vec<usize> = Vec::new();
        for (idx, sentence, _) in &scored {
            let would_add = if result.is_empty() { sentence.len() } else { result.len() + 1 + sentence.len() };
            if would_add <= max_chars || must_keep.contains(idx) {
                if !result.is_empty() { result.push('\n'); }
                result.push_str(sentence);
                selected_indices.push(*idx);
            }
        }

        // 6. 如果有 must_keep 未加入（配额不够），按原顺序重排
        if !must_keep.is_empty() && selected_indices.len() < sentences.len() {
            selected_indices.sort();
            let mut reordered = String::new();
            for i in selected_indices {
                let s = &sentences[i];
                if !reordered.is_empty() { reordered.push('\n'); }
                reordered.push_str(s);
            }
            // 如果重排后超了，用原始 text 截断到 max_chars
            if reordered.len() > max_chars {
                return text.chars().take(max_chars).collect();
            }
            return reordered;
        }

        result
    }

    /// 中英文分句（设计文档 §6.1）
    fn split_sentences(&self, text: &str) -> Vec<String> { /* ... */ }

    /// 提取独有实体（大写+数字组合，如 API_KEY_SK_XXXX）
    fn extract_unique_entities(&self, text: &str) -> Vec<String> { /* ... */ }

    /// 句子评分（字数 + 关键词密度）
    fn score_sentence(&self, sentence: &str, entities: &[String]) -> f64 { /* ... */ }
}
```

**详细要求**：
- `split_sentences`：用 `。！？；！？.!?;\n` 作为分隔符，同时支持中英文
- `extract_unique_entities`：正则匹配 `[A-Z][A-Z_0-9]{4,}` 模式（如 `API_KEY_SK_XXXX`、`CONFIG_PATH`）
- `score_sentence`：`sentence.len() as f64 * (1.0 + 0.1 * entities.iter().filter(|e| sentence.contains(*e)).count() as f64)`
- 当 `preserve_unique_entities=true` 时，含独有实体的句子标记为 must_keep，优先保留
- 当无法压缩到目标大小时返回原始文本（**降级，不截断**）

**验收标准**：
- [ ] `cargo check` 通过
- [ ] `compact()` 完整实现，无 `todo!()`
- [ ] 分句支持中英文
- [ ] 实体提取使用 `regex` crate
- [ ] `preserve_entities=true` 时保留含独有实体的句子
- [ ] 无法压缩时降级返回原文
- [ ] `#[cfg(test)]` 模块包含单元测试：`test_compact_truncation`、`test_compact_entity_preservation`、`test_compact_empty_input`

---

### 步骤 4：RuleLlmSelector

**文件**：`src/plugins/slots/assembler/rule_pool/rule_llm_selector.rs`

**要求**：

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::{RulePoolConfig, RuleGroup, L3RulesConfig};

/// LLM 驱动的规则选择器（设计文档 §8）
pub struct RuleLlmSelector {
    config: RulePoolConfig,
    cache: RwLock<Option<(String, RuleGroup)>>,  // (user_text, selected_group)
}

impl RuleLlmSelector {
    pub fn new(config: RulePoolConfig) -> Self { /* ... */ }

    /// 选择规则组（设计文档 §8.3）
    pub async fn select(&self, ap: &dyn SlotAccessPoint) -> RuleGroup {
        if !self.config.enabled { return RuleGroup::empty(); }

        // 1. 获取用户消息
        let user_text = self.get_user_text(ap);
        if user_text.is_empty() { return RuleGroup::empty(); }

        // 2. 检查缓存
        { let cache = self.cache.read().await; /* 相同 user_text 复用 */ }

        // 3. 加载文件规则
        let file_rules = self.load_file_rules().await;

        // 4. 加载 L3 规则（若启用）
        let l3_rules = if self.config.l3_rules.enabled {
            self.load_l3_rules(ap).await
        } else { vec![] };

        // 5. 两个来源都为空 → 返回空
        if file_rules.is_empty() && l3_rules.is_empty() { return RuleGroup::empty(); }

        // 6. 调用 LLM 选择
        let result = self.select_with_llm(&user_text, &file_rules, &l3_rules).await;

        // 7. 更新缓存
        *self.cache.write().await = Some((user_text.to_string(), result.clone()));

        result
    }

    /// 获取用户最新消息（从 ap.messages() 中找最后一条 User 消息）
    fn get_user_text(&self, ap: &dyn SlotAccessPoint) -> String { /* ... */ }

    /// 从规则文件加载规则（不存在时返回空 Vec）
    async fn load_file_rules(&self) -> Vec<(String, String)> { /* ... */ }

    /// 从 L3 向量库加载经验规则（provider_raw(PROVIDER_MEMORY) → downcast → search）
    async fn load_l3_rules(&self, ap: &dyn SlotAccessPoint) -> Vec<String> { /* ... */ }

    /// 调用 LLM 选择规则（LLM 失败时根据 fallback_enabled 决定行为）
    async fn select_with_llm(&self, user_text: &str, file_rules: &[(String, String)], l3_rules: &[String]) -> RuleGroup { /* ... */ }
}
```

**详细要求**：
- `get_user_text`：从 `ap.messages()` 中逆序遍历，找到第一条 `role == MessageRole::User` 的消息，返回其文本内容。如果无 User 消息返回空字符串
- `load_file_rules`：读取 `self.config.rules_file` 路径的文件，解析 `## group: xxx` 格式。文件不存在或路径为空时返回空 Vec。**路径已在 resolve_paths 中解析为绝对路径**
- `load_l3_rules`：通过 `ap.provider_raw(PROVIDER_MEMORY)` 获取 MemoryProvider，调用 `search_memory()` 方法。Provider 未注册→返回空；搜索失败→返回空
- `select_with_llm`：构建 selection prompt，通过 `ap.provider_raw("llm")` 获得 LLM Provider 调用。LLM 不可用时 `fallback_enabled=true` 返回合并规则，否则返回空。**注意**：当前没有 LLM Provider 以 `"llm"` 为 key 注册——所以此方法在实际运行中将始终走 fallback 或空路径。这是预期的（规则池默认关闭 `enabled=false`）
- 缓存 key 为 `user_text`（String），value 为 `RuleGroup`。缓存不过期

**验收标准**：
- [ ] `cargo check` 通过
- [ ] `select()` 完整实现，无 `todo!()`
- [ ] 缓存使用 `RwLock` 内部可变性（S-R03 合规）
- [ ] 文件规则解析支持 `## group: xxx` / `## industry: xxx` / `## project_type: xxx` 格式
- [ ] L3 规则检索在 Provider 未注册时返回空 Vec（不 panic、不报错）
- [ ] LLM 不可用时根据 `fallback_enabled` 决定行为
- [ ] `enabled=false` 时直接返回 `RuleGroup::empty()`

---

### 步骤 5：ContextProvider 实现（5 个 Provider）

**文件**：`src/plugins/slots/assembler/providers/` 下 6 个文件

#### 5a：`providers/mod.rs`

```rust
mod system_prompt;
mod identity;
mod working_memory;
mod vector_memory;
mod compression_summary;

use std::sync::Arc;
use crate::shared_types::assembler::{ContextProvider, AssemblerConfig};
use super::compaction::DocumentCompactor;
use super::rule_pool::RuleLlmSelector;

/// 构建 Provider 列表（按 injection_order 排序，跳过 disabled）（设计文档 §11）
pub fn build_providers(
    config: &AssemblerConfig,
    compactor: &DocumentCompactor,
    rule_selector: &Option<RuleLlmSelector>,
) -> Vec<Arc<dyn ContextProvider>> {
    // ... 按 config.injection_order 顺序构建，跳过 config.disabled_providers 中的 provider
}
```

#### 5b-5f：5 个 Provider

**SystemPromptProvider（pri=0）**：
- 从模板文件 `base_prompt.md` 读取内容（不存在时使用默认模板字符串）
- 调用 `RuleLlmSelector::select()` 获取规则
- 替换 `{{rules}}` 占位符
- 构建环境信息（工作目录、平台、日期），替换 `{{env_info}}`
- 拼接 `injection_layout.md` 模板
- `allow_truncation() → false`（不允许裁剪 system prompt）

**IdentityProvider（pri=5）**：
- 通过 `ap.read_context_raw("identity")` 获取 `IdentitySection`
- 无数据时返回空
- `allow_truncation() → false`（身份不可裁剪）
- `silent_on_empty() → true`

**CompressionSummaryProvider（pri=10）**：
- 通过 `ap.provider_raw("compression")` 获取压缩摘要（当前注册的是 `Arc::new(())`，所以总是返回空——设计预期，未来替换）
- 超配额时调用 `DocumentCompactor::compact()` 压缩
- `silent_on_empty() → true`

**WorkingMemoryProvider（pri=20）**：
- 通过 `ap.read_context_raw("working_memory")` 获取 `Vec<MemoryFileEntry>`
- 按更新时间排序，取最近 `quota.max_items` 个（设计文档 §5.4）
- 单个条目超 `max_chars_per_item` 时调用 `DocumentCompactor::compact()`
- `allow_compaction: true`

**VectorMemoryProvider（pri=30）**：
- 通过 `ap.provider_raw(PROVIDER_MEMORY)` 获取 `DynProvider<dyn MemoryProvider>`
- 调用 `provider.search_memory(query, limit)` 检索
- 无数据时返回空
- `silent_on_empty() → true`

**验收标准**：
- [ ] `cargo check` 通过
- [ ] 5 个 Provider 的 `priority()` 值与设计文档一致（0/5/10/20/30）
- [ ] 所有 `provide()` 参数使用 `&dyn SlotAccessPoint`
- [ ] 所有数据获取走 `SlotAccessPoint`，无 `ContractRegistry`、无 `use crate::plugins::services::*`
- [ ] 全部使用 `PROVIDER_*` 常量，无裸字符串
- [ ] SystemPromptProvider 的模板文件不存在时使用默认字符串（非 panic）
- [ ] 每个 Provider 的 `#[cfg(test)]` 包含至少一个单元测试

---

### 步骤 6：组装引擎

**文件**：`src/plugins/slots/assembler/assembly/` 下 5 个文件

#### 6a：`assembly/budget.rs`

```rust
/// 三层预算计算（设计文档 §7.1）
pub struct Budget {
    pub context_window: usize,
    pub system_overhead: usize,
    pub tools_tokens: usize,
    pub response_reserve: usize,
    pub total_available: usize,
    pub history_budget: usize,
}

/// BudgetCalculator——纯函数，无状态（设计文档 §7.1）
pub fn compute_budget(
    context_window: usize,
    tools_tokens: usize,
    history_tokens: usize,
    config: &AssemblerConfig,
) -> Budget {
    let system_overhead = 500;
    let response_reserve = (context_window as f64 * config.response_reserve_ratio) as usize;
    let total_available = context_window
        .saturating_sub(system_overhead)
        .saturating_sub(tools_tokens)
        .saturating_sub(response_reserve);
    let history_budget = (total_available as f64 * config.history_budget_ratio) as usize;
    Budget { context_window, system_overhead, tools_tokens, response_reserve, total_available, history_budget }
}
```

**详细要求**：
- `compute_budget` 是纯函数（无 self，无副作用）
- 使用 `saturating_sub` 防止整数下溢
- `response_reserve_ratio` 和 `history_budget_ratio` 从 config 读取

#### 6b：`assembly/quota.rs`

```rust
/// 5 种策略模板的百分比分配（设计文档 §7.3）
pub fn allocate_quotas(
    injection_budget: usize,
    policy: &str,
    config: &AssemblerConfig,
) -> HashMap<String, ContextQuota> {
    let ratios: HashMap<&str, f64> = match policy {
        "balanced" => vec![("identity", 0.1), ("compression_summary", 0.15),
                           ("working_memory", 0.40), ("vector_memory", 0.30)]
            .into_iter().collect(),
        "memory_focused" => vec![("identity", 0.05), ("compression_summary", 0.10),
                                 ("working_memory", 0.55), ("vector_memory", 0.25)]
            .into_iter().collect(),
        "token_efficient" => vec![("identity", 0.15), ("compression_summary", 0.15),
                                  ("working_memory", 0.30), ("vector_memory", 0.15)]
            .into_iter().collect(),
        "identity_only" => vec![("identity", 0.90)].into_iter().collect(),
        "minimal" => HashMap::new(), // 全部给响应保留，不注入任何 Provider
        _ => return allocate_quotas(injection_budget, "balanced", config), // 未知策略回退 balanced
    };

    // 按比例分配，同时受 ProviderSlotConfig.max_tokens 限制
    let mut quotas = HashMap::new();
    for (name, ratio) in &ratios {
        let provider_config = config.providers.get(*name);
        let computed = (injection_budget as f64 * ratio) as usize;
        let max_allowed = provider_config.map(|c| c.max_tokens).unwrap_or(usize::MAX);
        let max_tokens = computed.min(max_allowed);
        quotas.insert(name.to_string(), ContextQuota {
            max_tokens,
            // ... 从 provider_config 读取其他字段
            ..Default::default()
        });
    }
    quotas
}
```

#### 6c：`assembly/collector.rs`

按 `priority()` 排序 Provider，串行调用 `provide()`，累计 token 超配额时停止。单个 Provider 失败不阻塞后续（记录 warning 后 continue）。

#### 6d：`assembly/builder.rs`

将收集到的 `ContextBlock` 拼接为 `Vec<Message>`，追加历史消息。紧急裁剪：从前往后移除低优先级 Provider 的 block。

**验收标准**：
- [ ] `cargo check` 通过
- [ ] `compute_budget` 使用 `saturating_sub`
- [ ] `allocate_quotas` 有 5 种策略，未知策略回退 balanced
- [ ] `collect` 累计 token 超配额时停止
- [ ] `build` 紧急裁剪不裁剪身份
- [ ] 全部为纯函数或结构体方法（无全局状态）

---

### 步骤 7：OutputAdapter（2 个厂商适配器）

**文件**：`src/plugins/slots/assembler/output_adapters/anthropic.rs` + `openai.rs`

#### AnthropicOutputAdapter

```rust
pub struct AnthropicOutputAdapter;

impl LlmOutputAdapter for AnthropicOutputAdapter {
    fn provider_name(&self) -> &str { "anthropic" }

    fn adapt_system_prompt(&self, text: &str, _cw: usize) -> String {
        // XML 结构分隔：用 <section> 标签包裹段落
        let mut result = String::from("<system_prompt>\n");
        result.push_str(text);
        result.push_str("\n</system_prompt>");
        result
    }

    fn adapt_context_block(&self, title: &str, content: &str) -> String {
        format!("<context_block>\n<title>{}</title>\n{}\n</context_block>", title, content)
    }

    fn recommended_rule_count(&self, _cw: usize) -> usize { usize::MAX }
}
```

#### OpenAiOutputAdapter

```rust
pub struct OpenAiOutputAdapter;

impl LlmOutputAdapter for OpenAiOutputAdapter {
    fn provider_name(&self) -> &str { "openai" }

    fn adapt_system_prompt(&self, text: &str, _cw: usize) -> String { text.to_string() }

    fn adapt_context_block(&self, title: &str, content: &str) -> String {
        format!("{}\n{}", title, content)
    }

    fn recommended_rule_count(&self, cw: usize) -> usize {
        if cw < 16000 { 3 } else if cw < 64000 { 5 } else { usize::MAX }
    }
}
```

**验收标准**：
- [ ] `cargo check` 通过
- [ ] 两个 adapter 实现 `LlmOutputAdapter` trait（来自 shared_types）
- [ ] Anthropic 使用 XML 标签，OpenAI 使用简洁 Markdown
- [ ] `recommended_rule_count` 阈值：16000/64000

---

### 步骤 8：AssemblerSlot

**文件**：`src/plugins/slots/assembler/slot.rs`

**要求**：

```rust
pub struct AssemblerSlot {
    config: AssemblerConfig,
    providers: Vec<Arc<dyn ContextProvider>>,
    rule_selector: Option<RuleLlmSelector>,
}

impl AssemblerSlot {
    pub fn new() -> Self { Self { config: AssemblerConfig::default(), providers: vec![], rule_selector: None } }
}

#[async_trait]
impl SlotPlugin for AssemblerSlot {
    fn name(&self) -> &str { "assembler" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 1. 加载配置（设计文档 §11）
        let mut config: AssemblerConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("assembler 配置解析失败: {}", e)))?;
        // 遵循 S-R02：init 失败 = 不加载

        // 2. 解析路径（跨平台规范 §2.3）
        config.resolve_paths(&ctx.data_dir);

        // 3. 构建组件
        let compactor = DocumentCompactor::new(config.compaction.clone());
        let rule_selector = if config.rule_pool.enabled {
            Some(RuleLlmSelector::new(config.rule_pool.clone()))
        } else { None };
        let providers = build_providers(&config, &compactor, &rule_selector);

        self.config = config;
        self.providers = providers;
        self.rule_selector = rule_selector;
        tracing::info!("{} 初始化完成 (enabled={})", LOG_PREFIX, self.config.enabled);
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        if !self.config.enabled {
            tracing::debug!("{} disabled, skipping", LOG_PREFIX);
            return Ok(SlotDirective::Continue);
        }

        let start = std::time::Instant::now();

        // Phase 1: 读取历史消息
        let history_messages = ap.messages().to_vec();
        let history_tokens: usize = history_messages.iter().map(|m| m.estimate_tokens()).sum();

        // Phase 2: 估算工具 token
        let tools_tokens = /* 从 context 读取 ToolDefinition 列表，估算 token */ 0;

        // Phase 3: 获取 context_window（从 LlmConfig）
        let context_window = /* 从 read_context_raw("llm_config") 获取 */ 128_000;

        // Phase 4: 预算计算（设计文档 §7.1）
        let budget = compute_budget(context_window, tools_tokens, history_tokens, &self.config);
        let injection_budget = budget.total_available
            .saturating_sub(history_tokens)
            .min(self.config.max_injection_tokens);

        // Phase 5: 配额分配（设计文档 §7.3）
        let quotas = allocate_quotas(injection_budget, &self.config.injection_policy, &self.config);

        // Phase 6: 内容收集（设计文档 §7.4）
        let (blocks, provider_stats, warnings) =
            BlockCollector::collect(&self.providers, ap, &quotas, &self.config.providers).await;

        // Phase 7: 消息拼装（设计文档 §7.5）
        let messages = MessageBuilder::build(&blocks, &history_messages, &self.config);

        // Phase 8: 厂商适配
        // ...

        // Phase 9: 安全检查 + 紧急裁剪
        // ...

        // Phase 10: 写入 context
        ap.write_context_raw("assembler_messages", Box::new(messages))
            .unwrap_or_else(|e| tracing::warn!("{} 写入 assembler_messages 失败: {}", LOG_PREFIX, e));
        // 遵循 S-R01：写入失败降级为 warn + Continue

        tracing::debug!("{} 组装完成 ({:?})", LOG_PREFIX, start.elapsed());
        Ok(SlotDirective::Continue) // S-R01：所有路径返回 Continue
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("{} shutdown", LOG_PREFIX);
        Ok(())
    }
}
```

**验收标准**：
- [ ] `cargo check` 通过
- [ ] 实现 `SlotPlugin` trait（非旧版 `Slot`），3 个方法全部实现
- [ ] `init()` 失败时返回 `PluginError::Config`（S-R02）
- [ ] `run()` 中 `enabled=false` 时立即返回 `Continue`
- [ ] `run()` 所有路径返回 `Continue`（S-R01）
- [ ] 所有 `write_context_raw` 失败降级（不传播 Err）
- [ ] 无 `unwrap()`（测试除外）

---

### 步骤 9：模板文件

**文件**：`resources/templates/base_prompt.md`

```markdown
You are aagnet, an AI agent.

{{rules}}

<env>
Working directory: {{work_dir}}
Platform: {{platform}}
Today's date: {{today}}
</env>
```

**文件**：`resources/templates/injection_layout.md`

```markdown
## Agent Identity
{{identity}}

## Conversation Context
{{compression_summary}}

## Working Memory
{{working_memory}}

## Related Knowledge
{{vector_memory}}
```

---

### 步骤 10：main.rs 接线

**操作**：

1. 在 `src/main.rs` 中，在 `ToolRegistrySlot` 注册之后、`LlmThinkerSlot` 注册之前添加：

```rust
// 注册 AssemblerSlot（CONTEXT 阶段，ToolRegistrySlot 之后）
runtime.register_slot(
    Phase::context(),
    Box::new(AssemblerSlot::new()),
    &PluginInitContext::new(
        "assembler",
        plugins_config.get("assembler").cloned().unwrap_or(serde_json::json!({})),
        AgentConfig::default(),
        PathBuf::from("./data/assembler"),
    ),
).await.map_err(|e| format!("AssemblerSlot.init() 失败: {e}"))?;
```

2. 添加 import：
```rust
use aagnet::plugins::slots::assembler::AssemblerSlot;
```

---

### 步骤 11：全量验证

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

**验收标准**：
- [ ] `cargo check` 零 error
- [ ] `cargo clippy` 零 warning
- [ ] `cargo test` 全部通过
- [ ] 代码中搜索 `todo!()` 或 `unimplemented!()` → 零结果
- [ ] 代码中搜索 `provider_raw("`（裸字符串）→ 零结果（全部使用 PROVIDER_* 常量）
- [ ] 代码中搜索 `.unwrap()` → 仅出现在 `#[cfg(test)]` 中
- [ ] `AssemblerConfig.enabled = false` 时 Pipeline 行为不变（手动验证）

---

## 依赖关系图

```
步骤 0 (环境与骨架)         ← 无依赖
   ↓
步骤 1 (shared_types)       ← 依赖步骤 0
   ↓
步骤 2 (config + mod.rs)    ← 依赖步骤 1
   ↓
┌───┬───┬───┬───┬───┬───┐
 3   4   5   6   7   8     ← 可并行（依赖步骤 1+2）
 │   │   │   │   │   │
 doc rule prov eng adapter 模板
 └───┴───┴───┴───┴───┘
   ↓
步骤 9 (AssemblerSlot)     ← 依赖步骤 2-8（全部）
   ↓
步骤 10 (main.rs 接线)     ← 依赖步骤 9
   ↓
步骤 11 (全量验证)
```

**注意**：步骤 3-8 可并行开发（各自依赖步骤 1+2），但步骤 9 必须等 3-8 全部完成后才能开始。
