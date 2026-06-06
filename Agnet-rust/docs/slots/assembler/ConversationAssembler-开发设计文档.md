# ConversationAssembler — aagnet 完整开发设计文档（协议适配版）

> 版本：v2.1-protocol-adapted
> 基於：原始 v2.1 设计文档，按以下协议执行合规适配：
> - `protocol-shared_types契约协议.md`（红线 K-R01/T-R01/D-R01）
> - `protocol-Slot接入协议.md`（红线 S-R01/S-R02/S-R03）
> - `protocol-模块内部组件协议.md`（红线 C-R01/C-R02）
> - `跨平台与硬编码规范.md`（红线 P-R01/P-R02/P-R03）
> - `开发插件完整流程.md`（红线 1-6）
>
> 适配变更：见附录 C「协议合规适配记录」

---

## 目录

1. [设计总纲](#一设计总纲)
2. [目录结构](#二目录结构)
3. [shared_types 契约层](#三shared_types-契约层)
4. [ContextProvider 接口](#四contextprovider-接口)
5. [Provider 实现](#五provider-实现)
6. [DocumentCompactor 文档压缩器](#六documentcompactor-文档压缩器)
7. [组装引擎](#七组装引擎)
8. [规则池系统（LLM 驱动 + L3 双来源）](#八规则池系统llm-驱动--l3-双来源)
9. [LlmOutputAdapter — 厂商输出适配契约](#九llmoutputadapter--厂商输出适配契约)
10. [AssemblerConfig 完整配置](#十assemblerconfig-完整配置)
11. [AssemblerSlot 实现](#十一assemblerslot-实现)
12. [模板说明](#十二模板说明)
13. [AssemblyReport 可观测性](#十三assemblyreport-可观测性)
14. [与现有系统共存策略](#十四与现有系统共存策略)
15. [未来扩展接口](#十五未来扩展接口)
16. [实施计划](#十六实施计划)
17. [因果链预演](#十七因果链预演)
18. [附录](#附录)

---

## 一、设计总纲

### 1.1 定位

ConversationAssembler 是 aagnet 的**上下文组装器**，作为 SlotPlugin 在 Pipeline 的 CONTEXT 阶段运行。负责：

1. **动态预算计算**：根据模型 `context_window` 实时计算可用 token
2. **配额分配**：按策略模板将预算分配到各内容提供者
3. **内容收集**：从多数据源收集身份/记忆/摘要/规则
4. **智能截断**：配额不足时用 `DocumentCompactor` 临时压缩（保护关键实体）
5. **规则优选**：用 LLM 从规则池 + L3 经验库中精选适用的规则组
6. **厂商适配**：最终输出前按厂商最佳实践调整排版（XML 标签 vs 简洁标题）
7. **消息拼装**：按优先级和模板拼装最终 System 消息

### 1.2 协议合规声明

| 协议 | 遵守方式 |
|:-----|:---------|
| `protocol-Slot接入协议.md` | 实现 `SlotPlugin` trait，通过 `SlotAccessPoint` 通信，生命周期 `init → run → shutdown` |
| `protocol-shared_types契约协议.md` | 所有跨插件类型（trait、struct、常量）定义在 `shared_types` 中 |
| `protocol-模块内部组件协议.md` | 内部子组件（Provider、Compactor 等）实现 `Component` trait，通过 `InternalAccessPoint` 通信 |
| `跨平台与硬编码规范.md` | 所有路径通过配置 + `dirs` crate 解析，无裸路径、无裸字符串 key |

### 1.3 核心原则

| 原则 | 含义 |
|------|------|
| **配置驱动** | 所有策略、比例、阈值通过配置定义，代码只执行配置 |
| **协议隔离** | 所有跨插件数据访问走 `SlotAccessPoint::provider_raw()` + `downcast`，不直接 import 任何 plugin 模块 |
| **可插拔** | Provider 只需要实现 `ContextProvider` trait，注册到 Assembler 即可 |
| **不破坏现有** | 不修改任何已有模块。Assembler 作为新增 Slot 并行运行 |
| **可观测** | 每次组装生成 `AssemblyReport`，记录预算/配额/各 Provider 消耗 |
| **安全兜底** | 超限紧急裁剪按优先级从低到高移除，身份永远不裁剪 |
| **失败退化** | 规则池 LLM 调用失败 → 空规则组；厂商适配器缺失 → 原始输出，不影响正常组装 |

### 1.4 不在职责范围内

```
不直接执行上下文压缩（由 CompressionService 负责）
不直接操作记忆库（由 Provider 通过 SlotAccessPoint::provider_raw() 调用 Manager）
不触发压缩（只读取压缩结果）
```

### 1.5 整体架构

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Pipeline CONTEXT 阶段                                                       │
│                                                                               │
│  [ToolRegistrySlot] → [AssemblerSlot] → [LlmThinkerSlot]                      │
│                           │                                                   │
│                           ▼                                                   │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │  AssemblerSlot (SlotPlugin)                                               │ │
│  │  ┌──────────────────────────────────────────────────────────────────────┐ ││
│  │  │  配置层（全部可配置，零硬编码）                                        │ ││
│  │  │  AssemblerConfig (config.toml → PluginInitContext.plugin_config)      │ ││
│  │  └──────────────────────────────────────────────────────────────────────┘ ││
│  │  ┌──────────────────────────────────────────────────────────────────────┐ ││
│  │  │  数据获取层（通过 SlotAccessPoint 获取）                               │ ││
│  │  │  • provider_raw(PROVIDER_MEMORY)  → MemoryProvider                    │ ││
│  │  │  • provider_raw(PROVIDER_TOOL)    → ToolProvider (读工具 token)       │ ││
│  │  │  • read_context_raw("identity")   → IdentitySection                   │ ││
│  │  │  • read_context_raw("working_memory") → Vec<MemoryFileEntry>          │ ││
│  │  │  • messages()                     → 历史消息                          │ ││
│  │  └──────────────────────────────────────────────────────────────────────┘ ││
│  │  ┌──────────────────────────────────────────────────────────────────────┐ ││
│  │  │  组装引擎（内部 Component，通过 InternalAccessPoint 通信）             │ ││
│  │  │                                                                       │ ││
│  │  │  BudgetCalculator  → 计算三层预算                                     │ ││
│  │  │  RuleLlmSelector   → LLM 精选规则组（文件 + L3 双来源）                 │ ││
│  │  │  QuotaAllocator    → 按策略模板分配配额到各 Provider                   │ ││
│  │  │  BlockCollector    → 按优先级依次调用 Provider 收集内容块               │ ││
│  │  │  MessageBuilder    → 拼装 System 消息 + 环境注入 + 拼接历史             │ ││
│  │  │  OutputAdapter     → 厂商排版调整（XML / Markdown / 精简）              │ ││
│  │  │  AssemblyReport    → 生成完整组装报告                                  │ ││
│  │  └──────────────────────────────────────────────────────────────────────┘ ││
│  │  ┌──────────────────────────────────────────────────────────────────────┐ ││
│  │  │  ContextProvider 实现（内部 Component）                                │ ││
│  │  │                                                                       │ ││
│  │  │  SystemPromptProvider        (pri=0)  → 规则 + 基础模板 + 环境信息      │ ││
│  │  │  IdentityProvider            (pri=5)  → 身份内容                       │ ││
│  │  │  CompressionSummaryProvider  (pri=10) → 压缩摘要（User 角色伪装）       │ ││
│  │  │  WorkingMemoryProvider       (pri=20) → L2 工作记忆                    │ ││
│  │  │  VectorMemoryProvider        (pri=30) → L3 向量检索                    │ ││
│  │  └──────────────────────────────────────────────────────────────────────┘ ││
│  └──────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 二、目录结构

Assembler 是 SlotPlugin，故放在 `plugins/slots/assembler/`（非 `services/`）。所有跨插件契约放在 `shared_types/assembler/`。内部组件按模块内部组件协议组织。

```
src/
  shared_types/
    mod.rs                          (改)  — 新增 pub mod assembler
    assembler/
      mod.rs                        (NEW) — 重新导出所有公共类型
      context.rs                    (NEW) — ContextProvider trait + ContextBlock + ContextQuota + ProviderError
      report.rs                     (NEW) — AssemblyReport + ProviderStat + AssemblyWarning
      adapter.rs                    (NEW) — LlmOutputAdapter trait
      compaction.rs                 (NEW) — CompactionConfig
      rule_pool.rs                  (NEW) — RulePoolConfig + L3RulesConfig + RuleGroup
      config.rs                     (NEW) — AssemblerConfig + ProviderSlotConfig

  plugins/slots/assembler/
    mod.rs                          (NEW) — 模块声明 + 公开导出
    config.rs                       (NEW) — 配置加载（serde_json from PluginInitContext）
    slot.rs                         (NEW) — AssemblerSlot (impl SlotPlugin)

    providers/
      mod.rs                        (NEW) — ContextProvider trait 的模块内部组织
      system_prompt.rs              (NEW) — SystemPromptProvider
      identity.rs                   (NEW) — IdentityProvider
      working_memory.rs             (NEW) — WorkingMemoryProvider
      vector_memory.rs              (NEW) — VectorMemoryProvider
      compression_summary.rs        (NEW) — CompressionSummaryProvider

    assembly/
      mod.rs                        (NEW) — 模块声明
      budget.rs                     (NEW) — BudgetCalculator
      quota.rs                      (NEW) — QuotaAllocator
      collector.rs                  (NEW) — BlockCollector
      builder.rs                    (NEW) — MessageBuilder

    compaction/
      mod.rs                        (NEW) — 模块声明
      doc_compactor.rs              (NEW) — DocumentCompactor

    rule_pool/
      mod.rs                        (NEW) — 模块声明
      rule_llm_selector.rs          (NEW) — RuleLlmSelector（LLM 选择规则组）

    output_adapters/
      mod.rs                        (NEW) — 模块声明
      anthropic.rs                  (NEW) — AnthropicOutputAdapter
      openai.rs                     (NEW) — OpenAiOutputAdapter

resources/
  templates/
    rules.md                        (NEW) — 用户可编辑的规则文件（可选，不存在不影响）
    base_prompt.md                  (NEW) — 基础 Prompt 模板骨架
    injection_layout.md             (NEW) — 记忆内容注入排版模板
```

**总计：28 个新文件，2 个现有改动（`shared_types/mod.rs` + `plugins/slots/mod.rs`），0 个现有文件破坏。**

### 目录结构合规说明

| 原设计位置 | 适配后位置 | 原因（协议条款） |
|:-----------|:-----------|:----------------|
| `plugins/services/assembler/` | `plugins/slots/assembler/` | Assembler 是 SlotPlugin，放入 `slots/`（Slot接入协议 §1） |
| `core/contract/*.rs` | `shared_types/assembler/*.rs` | 跨插件契约必须放 shared_types（shared_types契约协议 T-R01） |
| `core/contract/mod.rs` 加槽位 | 不修改 core | Slot 通过 `SlotAccessPoint::provider_raw()` 获取数据，不需要 ContractRegistry |

### 与原设计的架构差异

**原设计**引入 `ContractRegistry` — 一个新的全局注册表，将所有数据访问抽象为 Contract trait。这违反了"Slot 只通过 `SlotAccessPoint` 通信"的协议原则（Slot接入协议 §2），且与现有的 `ProviderRegistry` 功能重叠。

**适配方案**：去掉 `ContractRegistry`，AssemblerSlot 通过 `SlotAccessPoint` 已有的方法获取所有数据：
- `ap.provider_raw(PROVIDER_MEMORY)` → `downcast::<DynProvider<dyn MemoryProvider>>()` 获取记忆
- `ap.provider_raw(PROVIDER_TOOL)` → 获取工具列表用于 token 预算
- `ap.read_context_raw("identity")` → 获取身份内容（已在 InitPhaseSlot 中写入）
- `ap.read_context_raw("working_memory")` → 获取工作记忆（已在 InitPhaseSlot 中写入）
- `ap.provider_raw(PROVIDER_SECURITY)` → `downcast` 获取 SecurityPolicyProvider 用于规则（可选）
- `ap.messages()` → 获取历史消息列表

---

## 三、shared_types 契约层

### 3.1 模块结构

```rust
// src/shared_types/assembler/mod.rs
pub mod context;
pub mod report;
pub mod adapter;
pub mod compaction;
pub mod rule_pool;
pub mod config;

pub use context::*;
pub use report::*;
pub use adapter::*;
pub use compaction::*;
pub use rule_pool::*;
pub use config::*;
```

### 3.2 ContextProvider trait 与关联类型

```rust
// src/shared_types/assembler/context.rs

use async_trait::async_trait;

// ── Provider 返回的内容 ──

/// 单个内容块
#[derive(Debug, Clone)]
pub struct ContextBlock {
    pub section_title: String,   // "## Working Memory"
    pub content: String,         // 实际注入文本
    pub source: String,          // 来源标识（日志用）
    pub token_count: usize,
}

/// 提供者返回的完整内容
#[derive(Debug, Clone)]
pub struct ProvidedContext {
    pub blocks: Vec<ContextBlock>,
    pub tokens_used: usize,
}

// ── Provider 配额 ──

/// 上下文配额（由 QuotaAllocator 计算，传给每个 Provider）
#[derive(Debug, Clone)]
pub struct ContextQuota {
    pub max_tokens: usize,           // 0 = 禁止注入
    pub max_items: usize,
    pub max_chars_per_item: usize,   // 0 = 不限制
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,      // DocumentCompactor 临时压缩
}

impl Default for ContextQuota {
    fn default() -> Self {
        Self { max_tokens: 0, max_items: 5, max_chars_per_item: 0, min_guaranteed_tokens: 0, allow_compaction: true }
    }
}

// ── Provider 错误 ──

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("内容缺失: {0}")]
    Missing(String),
    #[error("配额超限: used={used}, max={max}")]
    QuotaExceeded { used: usize, max: usize },
    #[error("内部错误: {0}")]
    Internal(String),
}

// ── Provider trait ──

/// 内容提供者 trait
///
/// 定义在 shared_types 中（shared_types契约协议 T-R01），
/// 不归属于 Assembler 或任何 Provider 实现方。
/// 实现方：Assembler 内部的 5 个 Provider
/// 调用方：AssemblerSlot 的 BlockCollector
///
/// 注意：provide 的参数是 &dyn SlotAccessPoint（非 StepContext），
/// 遵守 Slot接入协议 §2——Slot 只能通过 SlotAccessPoint 通信。
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> u8;
    fn allow_truncation(&self) -> bool { true }
    fn silent_on_empty(&self) -> bool { true }
    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize;

    async fn provide(
        &self,
        ap: &dyn crate::core::access::SlotAccessPoint,
        quota: &ContextQuota,
        config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError>;
}
```

### 3.3 AssemblyReport

```rust
// src/shared_types/assembler/report.rs

use std::time::Duration;

/// 单次组装的完整报告
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
    pub rules_group: String,         // "code+general" / "empty"
    pub adapter_used: Option<String>, // "anthropic" / "openai" / None
    pub truncation_applied: bool,
    pub warnings: Vec<AssemblyWarning>,
    pub assembly_duration: Duration,
}

/// Provider 执行统计
#[derive(Debug, Clone)]
pub struct ProviderStat {
    pub name: String,
    pub priority: u8,
    pub tokens_used: usize,
    pub blocks_count: usize,
    pub success: bool,
    pub error: Option<String>,
}

/// 组装警告
#[derive(Debug, Clone)]
pub struct AssemblyWarning {
    pub code: String,     // "PROVIDER_FAILED" / "TRUNCATION_APPLIED" / "QUOTA_EXCEEDED"
    pub message: String,
}
```

### 3.4 LlmOutputAdapter

```rust
// src/shared_types/assembler/adapter.rs

use async_trait::async_trait;

/// 厂商输出适配契约
///
/// 定义在 shared_types 中（shared_types契约协议 T-R01），
/// 实现方放在各自模块（anthropic.rs / openai.rs）。
/// 不注册此契约 → 跳过适配，直接输出原始组装结果，零影响。
#[async_trait]
pub trait LlmOutputAdapter: Send + Sync {
    /// 厂商名称
    fn provider_name(&self) -> &str;

    /// 调整 System Prompt 整体排版（默认实现不改变）
    fn adapt_system_prompt(&self, text: &str, context_window: usize) -> String {
        text.to_string()
    }

    /// 调整单个记忆注入块的排版
    fn adapt_context_block(&self, section_title: &str, content: &str) -> String {
        format!("{}\n\n{}", section_title, content)
    }

    /// 建议保留的规则数量（小窗口精简用，usize::MAX = 不精简）
    fn recommended_rule_count(&self, context_window: usize) -> usize {
        usize::MAX
    }
}
```

### 3.5 CompactionConfig

```rust
// src/shared_types/assembler/compaction.rs

/// DocumentCompactor 配置
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub chars_per_token: f64,               // 4.0
    pub preserve_unique_entities: bool,      // true
    pub min_sentences_for_compaction: usize, // 3
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self { chars_per_token: 4.0, preserve_unique_entities: true, min_sentences_for_compaction: 3 }
    }
}
```

### 3.6 RulePoolConfig

```rust
// src/shared_types/assembler/rule_pool.rs

/// 规则池配置
#[derive(Debug, Clone)]
pub struct RulePoolConfig {
    /// 是否启用规则池（默认 false，不影响正常组装）
    pub enabled: bool,
    /// LLM 选择器使用的 LLM 名称
    pub llm_name: String,                    // "secondary" / "primary"
    /// 规则文件路径（空路径 = 不加载文件规则）
    pub rules_file: String,
    /// LLM 选择超时（毫秒）
    pub selection_timeout_ms: u64,
    /// 是否在 LLM 失败时回退到全部规则
    pub fallback_enabled: bool,
    /// L3 规则来源配置
    pub l3_rules: L3RulesConfig,
}

/// L3 向量库规则配置
#[derive(Debug, Clone)]
pub struct L3RulesConfig {
    pub enabled: bool,
    pub max_items: usize,
    pub query_template: String,
}

impl Default for RulePoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            llm_name: "secondary".into(),
            rules_file: String::new(),
            selection_timeout_ms: 5000,
            fallback_enabled: false,
            l3_rules: L3RulesConfig {
                enabled: false,
                max_items: 3,
                query_template: "{user_text} 行业经验教训".into(),
            },
        }
    }
}

/// 规则组
#[derive(Debug, Clone)]
pub struct RuleGroup {
    pub name: String,       // "code+general"
    pub rules: Vec<String>,
}

impl RuleGroup {
    pub fn empty() -> Self { Self { name: "empty".into(), rules: vec![] } }
}
```

### 3.7 AssemblerConfig

```rust
// src/shared_types/assembler/config.rs

use std::collections::HashMap;
use super::compaction::CompactionConfig;
use super::rule_pool::RulePoolConfig;

/// ConversationAssembler 完整配置
#[derive(Debug, Clone)]
pub struct AssemblerConfig {
    // ── 基础开关 ──
    pub enabled: bool,
    pub debug: bool,

    // ── 预算 ──
    pub response_reserve_ratio: f64,       // 0.2
    pub history_budget_ratio: f64,         // 0.7
    pub min_recent_messages: usize,        // 4
    pub max_injection_tokens: usize,       // 30000（安全上限）
    pub minimum_context_size: usize,       // 1000

    // ── 策略 ──
    pub injection_policy: String,          // "balanced" | "memory_focused" | ...
    pub disabled_providers: Vec<String>,

    // ── Provider ──
    pub providers: HashMap<String, ProviderSlotConfig>,
    pub injection_order: Vec<String>,

    // ── 子模块 ──
    pub compaction: CompactionConfig,
    pub rule_pool: RulePoolConfig,
    pub output_adapter_enabled: bool,

    // ── 模板路径（通过 env + dirs 解析，跨平台规范 P-R01） ──
    pub base_prompt_path: String,          // 相对 data_dir
    pub injection_layout_path: String,
}

/// Provider 独立配置
#[derive(Debug, Clone)]
pub struct ProviderSlotConfig {
    pub enabled: bool,
    pub max_tokens: usize,
    pub max_items: usize,
    pub max_chars_per_item: usize,      // 0 = 不限制，超出触发 DocumentCompactor
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,
    pub allow_truncation: bool,
}

impl Default for ProviderSlotConfig {
    fn default() -> Self {
        Self { enabled: true, max_tokens: 3000, max_items: 10, max_chars_per_item: 2000,
               min_guaranteed_tokens: 0, allow_compaction: true, allow_truncation: true }
    }
}

impl Default for AssemblerConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        providers.insert("identity".into(), ProviderSlotConfig {
            max_tokens: 2000, max_items: 1, max_chars_per_item: 0,
            min_guaranteed_tokens: 500, allow_compaction: false, allow_truncation: false,
            ..Default::default()
        });
        providers.insert("working_memory".into(), ProviderSlotConfig {
            max_tokens: 10000, max_items: 10, max_chars_per_item: 2000,
            min_guaranteed_tokens: 500, allow_compaction: true, allow_truncation: true,
            ..Default::default()
        });
        providers.insert("vector_memory".into(), ProviderSlotConfig {
            max_tokens: 8000, max_items: 5, max_chars_per_item: 1000,
            min_guaranteed_tokens: 0, allow_compaction: true, allow_truncation: true,
            ..Default::default()
        });
        providers.insert("compression_summary".into(), ProviderSlotConfig {
            max_tokens: 5000, max_items: 1, max_chars_per_item: 0,
            min_guaranteed_tokens: 0, allow_compaction: true, allow_truncation: true,
            ..Default::default()
        });
        Self {
            enabled: false, debug: false,
            response_reserve_ratio: 0.2, history_budget_ratio: 0.7,
            min_recent_messages: 4, max_injection_tokens: 30000,
            minimum_context_size: 1000,
            injection_policy: "balanced".into(), disabled_providers: vec![],
            providers, injection_order: vec![
                "system_prompt".into(), "identity".into(), "compression_summary".into(),
                "working_memory".into(), "vector_memory".into(),
            ],
            compaction: CompactionConfig::default(),
            rule_pool: RulePoolConfig::default(),
            output_adapter_enabled: true,
            base_prompt_path: "templates/base_prompt.md".into(),
            injection_layout_path: "templates/injection_layout.md".into(),
        }
    }
}
```

### 3.8 合规说明

| 原设计位置 | 适配后位置 | 协议依据 |
|:-----------|:-----------|:---------|
| `core/contract/session_messages.rs` | 不存在（通过 `ap.messages()` 替代） | Slot接入协议 §2 |
| `core/contract/identity_context.rs` | 不存在（通过 `ap.read_context_raw("identity")` 替代） | Slot接入协议 §2 |
| `core/contract/compression_summary.rs` | 不存在（通过 `ap.provider_raw("compression")` 替代） | Slot接入协议 §2 |
| `core/contract/llm_output_adapter.rs` | `shared_types/assembler/adapter.rs` | shared_types契约协议 T-R01 |
| `core/contract/mod.rs` 加槽位 | 不修改 | 不引入新全局注册表 |

**为什么去掉 ContractRegistry？** 原设计的 ContractRegistry 与现有的 ProviderRegistry 功能完全重叠。现有 Slot 通过 `provider_raw()` + `downcast` 获取所有能力，Assembler 也应遵循同一模式。引入第二个全局注册表会增加维护成本且违反"单一入口"原则。

---

## 四、ContextProvider 接口

见 §3.2 —— trait 定义在 `shared_types/assembler/context.rs` 中。

---

## 五、Provider 实现

（内容同原设计 §5，但做以下适配：）

| 变更点 | 原设计 | 适配后 |
|:-------|:-------|:-------|
| `provide()` 参数 | `ctx: &StepContext` | `ap: &dyn SlotAccessPoint`（Slot接入协议 §2） |
| 数据来源 | ContractRegistry.get_xxx() | `ap.provider_raw()` / `ap.read_context_raw()` / `ap.messages()` |
| 类型系统 | 直接 Arc<dyn Contract> | `DynProvider<dyn Trait>`（shared_types契约协议 D-R01） |

### 5.1 SystemPromptProvider（pri=0）

```rust
pub struct SystemPromptProvider {
    rule_selector: RuleLlmSelector,
    base_template: String,
    injection_template: String,
}

#[async_trait]
impl ContextProvider for SystemPromptProvider {
    fn name(&self) -> &str { "system_prompt" }
    fn priority(&self) -> u8 { 0 }
    fn allow_truncation(&self) -> bool { false }
    fn silent_on_empty(&self) -> bool { false }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError>
    {
        // 1. LLM 精选规则（通过 RuleLlmSelector）
        let rules = self.rule_selector.select(ap).await;

        // 2. 渲染基础模板
        let mut base = self.base_template.clone();
        let rules_text = rules.rules.iter().map(|r| format!("- {}", r)).collect::<Vec<_>>().join("\n");
        base = base.replace("{{rules}}", &rules_text);

        // 3. 环境信息（通过 SlotAccessPoint 提供的数据构建）
        let env_info = self.build_env_info(ap);
        base = base.replace("{{env_info}}", &env_info);

        // 4. 注入布局模板
        let layout = self.injection_template.clone();
        let content = format!("{}\n\n{}", base, layout);

        let tokens = (content.len() as f64 / 4.0).ceil() as usize;
        Ok(ProvidedContext {
            blocks: vec![ContextBlock {
                section_title: "## System".into(),
                content, source: "system_prompt".into(),
                token_count: tokens.min(quota.max_tokens),
            }],
            tokens_used: tokens.min(quota.max_tokens),
        })
    }
}

impl SystemPromptProvider {
    fn build_env_info(&self, ap: &dyn SlotAccessPoint) -> String {
        // 从 SlotAccessPoint 获取环境信息
        // 遵守跨平台规范 §2：使用 std::env / dirs crate，无裸路径
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let platform = if cfg!(target_os = "windows") { "Windows" }
                       else if cfg!(target_os = "macos") { "macOS" }
                       else { "Linux" };
        format!("工作目录: {}\n平台: {}\n会话: {}", cwd, platform, ap.session_id())
    }
}
```

### 5.2 IdentityProvider（pri=5）

通过 `ap.read_context_raw("identity")` 读取（InitPhaseSlot 已写入）。不可裁剪、不可压缩。

### 5.3 CompressionSummaryProvider（pri=10）

通过 `ap.provider_raw("compression")` 获取压缩摘要（若当前注册了 CompressionProvider）。目前 compression 注册的是 `Arc::new(())`，所以此 Provider 会返回空——与原设计一致（CompressionSummaryContract 未注册时返回 None）。

### 5.4 WorkingMemoryProvider（pri=20）

通过 `ap.read_context_raw("working_memory")` 读取（InitPhaseSlot 已写入）。内容超出 `max_chars_per_item` 时调用 DocumentCompactor 临时压缩。

### 5.5 VectorMemoryProvider（pri=30）

通过 `ap.provider_raw(PROVIDER_MEMORY)` 获取 MemoryProvider，调用其向量检索方法（目前 NoopEmbeddingModel，返回空）。

---

## 六、DocumentCompactor 文档压缩器

（同原设计 §6，无需协议适配。）

---

## 七、组装引擎

### 7.1 BudgetCalculator

```rust
pub fn compute(
    messages: &[Message],
    tools_tokens: usize,
    config: &AssemblerConfig,
) -> Budget {
    // context_window 从 LlmThinkerSlot 写入 StepContext 的配置中读取
    // 通过 ap.read_context_raw("llm_config") 获取
    let context_window = 128_000; // 默认，从 config 中读取
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

（7.2-7.5 同原设计，适配点同上：所有 `StepContext` 引用改为 `&dyn SlotAccessPoint`。）

---

## 八、规则池系统

（同原设计 §8，适配变更：）

| 变更点 | 原设计 | 适配后 |
|:-------|:-------|:-------|
| 数据来源 | `ContractRegistry.get_llm()` | `ap.provider_raw("llm")` 或自有 LLM Provider |
| L3 检索 | `ContractRegistry.get_l3_vector()` | `ap.provider_raw(PROVIDER_MEMORY)` → downcast 后调用向量检索 |

---

## 九、LlmOutputAdapter

（同原设计 §9，trait 定义移至 `shared_types/assembler/adapter.rs`，见 §3.4。）

---

## 十、AssemblerConfig

见 §3.7。

---

## 十一、AssemblerSlot 实现

```rust
// src/plugins/slots/assembler/slot.rs

use std::sync::Arc;
use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::assembler::*;

use super::providers::*;
use super::assembly::*;
use super::compaction::DocumentCompactor;
use super::rule_pool::RuleLlmSelector;

pub struct AssemblerSlot {
    config: AssemblerConfig,
    providers: Vec<Arc<dyn ContextProvider>>,
    rule_selector: Option<RuleLlmSelector>,
}

impl AssemblerSlot {
    pub fn new() -> Self {
        Self { config: AssemblerConfig::default(), providers: vec![], rule_selector: None }
    }
}

#[async_trait]
impl SlotPlugin for AssemblerSlot {
    fn name(&self) -> &str { "assembler" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 1. 加载配置（遵守跨平台规范 §2：路径通过 config.resolve_paths() 解析）
        let mut config: AssemblerConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("assembler 配置解析: {}", e)))?;

        // 2. 构建 Provider 列表
        let compactor = DocumentCompactor::new(config.compaction.clone());
        let rule_selector = if config.rule_pool.enabled {
            Some(RuleLlmSelector::new(config.rule_pool.clone()))
        } else {
            None
        };
        let providers = Self::build_providers(&config, &compactor, &rule_selector);

        self.config = config;
        self.providers = providers;
        self.rule_selector = rule_selector;
        tracing::info!("assembler: 初始化完成 (enabled={})", self.config.enabled);
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        if !self.config.enabled {
            return Ok(SlotDirective::Continue);
        }

        let start = std::time::Instant::now();

        // Phase 1: 读取历史消息
        let history_messages = ap.messages().to_vec();
        let history_tokens: usize = history_messages.iter().map(|m| m.estimate_tokens()).sum();

        // Phase 2: 估算工具 token
        let tools_tokens = ap.read_context_raw("tools")
            .and_then(|any| any.downcast_ref::<Vec<crate::shared_types::ToolDefinition>>())
            .map(|tools| tools.len() * 50) // 粗略估算
            .unwrap_or(0);

        // Phase 3: 预算计算
        let context_window = 128_000; // 应从 LlmConfig 中读取（通过 read_context_raw）
        let budget = BudgetCalculator::compute(&history_messages, tools_tokens, &self.config);
        let injection_budget = budget.total_available
            .saturating_sub(history_tokens)
            .min(self.config.max_injection_tokens);

        // Phase 4: 配额分配
        let quotas = QuotaAllocator::allocate(injection_budget, &self.config.injection_policy, &self.config);

        // Phase 5: 内容收集
        let (blocks, provider_stats, warnings) =
            BlockCollector::collect(&self.providers, ap, &quotas, &self.config.providers).await;

        // Phase 6: 消息拼装
        let mut messages = MessageBuilder::build(&blocks, &history_messages, &self.config);

        // Phase 7: 厂商输出适配
        // 适配器通过 provider_raw 查找（需要 LlmOutputAdapter 注册为 Provider）
        if self.config.output_adapter_enabled {
            if let Some(raw) = ap.provider_raw("llm_output_adapter") {
                if let Ok(adapter) = raw.downcast::<DynProvider<dyn LlmOutputAdapter>>() {
                    let rule_count = adapter.0.recommended_rule_count(context_window);
                    // ... 适配逻辑
                }
            }
        }

        // Phase 8: 安全检查
        let total: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        if total > context_window {
            MessageBuilder::emergency_truncate(&mut messages, context_window);
        }

        // Phase 9: 写入 StepContext
        ap.write_context_raw("assembler_messages", Box::new(messages))?;

        if self.config.debug {
            let report = AssemblyReport {
                assembly_duration: start.elapsed(),
                // ... 填充其他字段
            };
            tracing::info!("[assembler] {}", serde_json::to_string(&report).unwrap_or_default());
        }

        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("assembler: shutdown");
        Ok(())
    }
}
```

### 11.1 生命周期合规

| 阶段 | 方法 | 合规（Slot接入协议 §6） |
|:-----|:-----|:----------------------|
| 1 | `init()` — 加载配置、构建 Provider | ✅ S-R02：失败不加载 |
| 2..N | `run()` — 每次 CONTEXT 阶段执行 | ✅ S-R03：不持有跨次可变状态 |
| 1 | `shutdown()` — 资源清理 | ✅ |

---

## 十二、模板说明

（同原设计 §12。路径适配：所有路径通过 `config.resolve_paths()` 基于 `ctx.data_dir` 解析，遵守跨平台规范 §2。）

### 12.1 模板路径解析

```rust
// 在 AssemblerConfig 中：
impl AssemblerConfig {
    pub fn resolve_paths(&mut self, data_dir: &std::path::Path) {
        self.base_prompt_path = data_dir.join(&self.base_prompt_path)
            .to_string_lossy().to_string();
        self.injection_layout_path = data_dir.join(&self.injection_layout_path)
            .to_string_lossy().to_string();
    }
}
```

---

## 十三、AssemblyReport

见 §3.3。

---

## 十四、与现有系统共存策略

```
main.rs:
  if assembler_config.enabled:
    pipeline.add_slot(Phase::context(), Box::new(AssemblerSlot::new()))
    // InitPhaseSlot 中禁用 assemble_system_prompt（通过配置）
  else:
    // 旧路径完全不变
    // InitPhaseSlot 照常 assemble_system_prompt()
```

| 场景 | 行为 |
|:-----|:------|
| `AssemblerConfig.enabled = false` | 旧 Slot 照常工作，Assembler 不注册，零影响 |
| `enabled = true` | Assembler 运行，负责完整的 System Prompt 组装 |
| Assembler 编译失败 | 旧路径不受影响（Assembler 是独立模块） |

---

## 十五、未来扩展接口

（同原设计 §15。）

---

## 十六、实施计划

### 阶段 1：shared_types 契约层（7 文件）

```
1.1 shared_types/assembler/mod.rs           — 模块声明
1.2 shared_types/assembler/context.rs        — ContextProvider + ContextBlock + ContextQuota + ProviderError
1.3 shared_types/assembler/report.rs         — AssemblyReport + ProviderStat + AssemblyWarning
1.4 shared_types/assembler/adapter.rs        — LlmOutputAdapter
1.5 shared_types/assembler/compaction.rs     — CompactionConfig
1.6 shared_types/assembler/rule_pool.rs      — RulePoolConfig + L3RulesConfig + RuleGroup
1.7 shared_types/assembler/config.rs         — AssemblerConfig + ProviderSlotConfig
1.8 shared_types/mod.rs                      — 新增 pub mod assembler
```
**验证**：`cargo check` — 新增类型不影响现有代码

### 阶段 2：配置加载 + 文档压缩器（4 文件）

```
2.1 plugins/slots/assembler/mod.rs
2.2 plugins/slots/assembler/config.rs        — 配置加载 reslove_paths
2.3 plugins/slots/assembler/compaction/mod.rs
2.4 plugins/slots/assembler/compaction/doc_compactor.rs
```

### 阶段 3：规则池（2 文件）

```
3.1 plugins/slots/assembler/rule_pool/mod.rs
3.2 plugins/slots/assembler/rule_pool/rule_llm_selector.rs
```

### 阶段 4：Provider 实现（7 文件）

```
4.1 plugins/slots/assembler/providers/mod.rs
4.2 plugins/slots/assembler/providers/system_prompt.rs
4.3 plugins/slots/assembler/providers/identity.rs
4.4 plugins/slots/assembler/providers/compression_summary.rs
4.5 plugins/slots/assembler/providers/working_memory.rs
4.6 plugins/slots/assembler/providers/vector_memory.rs
4.7 plugins/slots/assembler/output_adapters/mod.rs
4.8 plugins/slots/assembler/output_adapters/anthropic.rs
4.9 plugins/slots/assembler/output_adapters/openai.rs
```

### 阶段 5：组装引擎（5 文件）

```
5.1 plugins/slots/assembler/assembly/mod.rs
5.2 plugins/slots/assembler/assembly/budget.rs
5.3 plugins/slots/assembler/assembly/quota.rs
5.4 plugins/slots/assembler/assembly/collector.rs
5.5 plugins/slots/assembler/assembly/builder.rs
```

### 阶段 6：Slot + 模板 + 接线（5 文件）

```
6.1 plugins/slots/assembler/slot.rs
6.2 plugins/slots/mod.rs                    — 新增 pub mod assembler
6.3 resources/templates/base_prompt.md
6.4 resources/templates/injection_layout.md
6.5 resources/templates/rules.md
```

### 阶段 7：main.rs 接线 + 全量验证

```
7.1 main.rs — AssemblerSlot 注册
7.2 cargo fmt --check
7.3 cargo clippy --all-targets -- -D warnings
7.4 cargo test
7.5 enabled = false → 旧路径不变（回归）
```

---

## 十七、因果链预演

（同原设计 §17，适配后场景不变。）

---

## 附录

### 附录 A：文件清单

```
NEW: 28 个文件
MODIFY: 2 个文件（shared_types/mod.rs + plugins/slots/mod.rs）
EXISTING: 0 个文件修改
总新增代码量估算：~2800 lines Rust + ~100 lines Markdown 模板
```

### 附录 B：与原设计的关键差异

| 维度 | 原设计 | 适配后 | 原因 |
|:-----|:-------|:-------|:-----|
| 模块位置 | `plugins/services/assembler/` | `plugins/slots/assembler/` | SlotPlugin 应放 slots/，非 services/ |
| 契约位置 | `core/contract/*.rs` | `shared_types/assembler/*.rs` | 跨插件类型必须放 shared_types（T-R01） |
| 数据获取 | ContractRegistry | `SlotAccessPoint.provider_raw()` + `read_context_raw()` | Slot 唯一通信通道（Slot接入协议 §2） |
| Provider trait 位置 | assembler 内部 | shared_types | 跨插件 trait 禁止定义在插件内部（T-R01） |
| 路径 | 硬编码字符串 | `resolve_paths()` + `data_dir` | 跨平台规范 §2 |
| Slot trait | 旧版 `Slot`（`box_clone`） | `SlotPlugin`（`init/run/shutdown`） | 当前框架标准 |
| 参数类型 | `&StepContext` | `&dyn SlotAccessPoint` | Slot 只通过 AccessPoint 通信 |

### 附录 C：协议合规适配记录

| 原设计问题 | 违反协议 | 适配操作 |
|:-----------|:---------|:---------|
| `ContractRegistry` 引入第二个全局注册表 | Slot接入协议 §2（绕过 SlotAccessPoint） | 删除 ContractRegistry，改用 provider_raw() |
| ContextProvider 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/context.rs |
| LlmOutputAdapter 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/adapter.rs |
| CompactionConfig 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/compaction.rs |
| AssemblerConfig 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/config.rs |
| RulePoolConfig 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/rule_pool.rs |
| AssemblyReport 定义在 assembler 内部 | shared_types契约协议 T-R01 | 移至 shared_types/assembler/report.rs |
| 路径硬编码 `"resources/templates/*"` | 跨平台规范 §2.3 | 改为 config + data_dir 解析 |
| 模块放 `services/` 目录 | Slot接入协议 §1 | 移至 `slots/` 目录 |
| `provide(&self, ctx: &StepContext)` | Slot接入协议 §2 | 改为 `ap: &dyn SlotAccessPoint` |
| 使用旧版 `Slot` trait | Slot接入协议 §1 | 改为 `SlotPlugin` trait |
