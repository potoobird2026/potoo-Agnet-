# AuditPhase Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 audit-phase 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：audit-phase（安全审计槽口）
- **模块定位**：一个实现 `SlotPlugin` trait 的插件，对外仅通过 `SlotAccessPoint` 交互，在 Pipeline 的 `audit` 阶段对 LLM 产生的 Thought::Action 进行安全审查。职责单一，**不需要 Orchestrator 和内部 Component 体系**。
- **内部组件**：无（单 SlotPlugin，通过函数拆分文件，无 Component trait 体系）
- **代码目录**：所有源码位于 `src/plugins/slots/audit_phase/` 下
- **依赖项**：`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`regex`、`thiserror`
- **设计文档**：`docs/slots/audit_phase_/04_audit_phase_槽口开发文档.md`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | — | 不涉及（audit_phase 无 HTTP 请求） |
| 模型名 | — | 不涉及 |
| 超时秒数 | — | 不涉及（audit_phase 无网络调用） |
| API 版本号 | — | 不涉及 |
| User-Agent | — | 不涉及 |
| 文件路径 | 敏感规则中包含 `/etc/passwd` 等平台路径 | 敏感检测规则仅检测数据格式（API 密钥格式、密码格式、私钥格式），不包含任何平台路径 |
| 数字阈值 | `audit_log_capacity = 1000` 写在业务逻辑中 | 定义为 `DEFAULT_AUDIT_LOG_CAPACITY` 常量，从 `AuditPhaseConfig` 读取 |
| 字符串模板 | — | `format!()` 中的模板可接受（变量来自配置） |
| 平台指令 | — | 不涉及 |
| 工具名 | `"execute_command"` 写在风险评级逻辑中 | 从 `AuditPhaseConfig.high_risk_tools` 读取（默认值在 `default_high_risk_tools()` 中定义） |

---

## 项目目录结构

请手动创建以下目录结构（已有文件保留不动）：

```
src/plugins/
├── mod.rs                            # 已有 pub mod slots;
└── slots/
    ├── mod.rs                        # 需要添加 pub mod audit_phase;
    └── audit_phase/
        ├── mod.rs                    # 模块声明（组件协议 §6.1）
        ├── plugin.rs                 # 新建：AuditPhaseSlot 定义（SlotPlugin 实现）
        ├── config.rs                 # 新建：AuditPhaseConfig + RiskAction + RiskSeverity + SensitiveRuleConfig + 常量
        ├── types.rs                  # 新建：AuditResult / AuditEvent / AuditContext
        ├── security.rs               # 新建：SecurityPolicyProvider trait + SecurityDecision + SecurityError + AuditWarning
        └── error.rs                  # 新建：AuditPhaseError + Into<PluginError>
```

模块声明链：

```
src/lib.rs                →  pub mod plugins;
src/plugins/mod.rs         →  pub mod slots;
src/plugins/slots/mod.rs   →  pub mod audit_phase;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `04_audit_phase_槽口开发文档.md`。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与设计文档完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - audit_phase 没有内部 Component，不需要 Orchestrator，不实现 Component trait
   - audit_phase 不做工具执行、不做 LLM 请求、不做消息格式化
   - `run()` 只返回 `Continue` 或 `AbortStep`（S-R01），不返回其他变体

3. **零硬编码**：
   a. `DEFAULT_AUDIT_LOG_CAPACITY` 定义为 `const` 常量，`audit_log_capacity` 从配置读取
   b. 敏感检测规则中的正则模式仅检测数据格式，禁止包含任何平台路径字面量
   c. 高风险/中风险工具列表从配置 `default_high_risk_tools()` / `default_medium_risk_tools()` 读取
   d. `RiskAction::Block` 默认值在 `default_high_risk_action()` 中定义
   e. 日志前缀使用句点命名空间 `"audit_phase:"`，不允许裸字符串

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理完整**：
   - `init()` 中正则编译失败返回 `PluginError::InitFailed`（S-R02）
   - `run()` 中关键路径（audit_result，AbortStep 路径）写入失败必须传播 `Err`
   - `run()` 中非关键路径（audit_warnings、audit_log、Continue 路径的 audit_result）写入失败应记录 warn 并继续，不传播 `Err`
   - 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释说明"测试中安全"

6. **一致性**：方法名、字段名、枚举变体名必须与设计文档完全一致，大小写敏感。

7. **禁止额外依赖**：只能使用 `std`、`tokio`、`tracing`、`async-trait`、`serde`、`serde_json`、`regex`、`thiserror` 以及项目内部模块。严禁引入 `reqwest`、`uuid`、`chrono`、`tempfile`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策）
   - 不允许写"做什么"的废话注释（如 `// 读取 Thought`）
   - 引用设计文档条款时用 `// 设计文档 §X.Y` 格式
   - 红线引用用 `// 遵循 S-R0X` 格式

9. **测试同时生成**：
   - 为每个场景生成单元测试（设计文档 §9.1–§9.6）
   - 测试使用 `MockSlotAccessPoint`，不依赖真实 Pipeline
   - 测试名称体现测试意图（如 `test_low_risk_passes`）
   - 所有测试使用 `std::env::temp_dir()` 处理临时路径

10. **杜绝捷径**：
    - 不能因为职责单一就跳过 `shutdown()` 的实现——必须有完整生命周期
    - 不能省略 `record_audit_event()` 中的日志容量裁剪逻辑
    - 必须区分关键路径和非关键路径的错误处理策略

11. **模块边界**：
    - 严禁引入工具执行、LLM 调用、记忆管理等 audit_phase 职责以外的功能
    - `SecurityPolicyProvider` trait 定义在 audit_phase 中（消费方定义接口），实现由 SecurityService 提供
    - 不直接引用 `plugins/services/security` 的具体类型

12. **日志规范**：
    - init 完成记录 `info`（携带编译的正则规则数）
    - 安全策略拒绝记录 `warn`
    - 高风险拦截记录 `warn`
    - 敏感信息检测记录 `warn`
    - 审计通过记录 `info`
    - 非关键路径降级记录 `warn`
    - 使用 `"audit_phase:"` 统一日志前缀
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
regex = "1"                               # ★ 新增：敏感信息检测正则
thiserror = "1"                            # ★ 新增：错误类型派生
```

2. 创建 `src/plugins/slots/audit_phase/` 目录及以下空文件：

```
src/plugins/slots/audit_phase/
├── mod.rs           （空文件）
├── plugin.rs        （空文件）
├── config.rs        （空文件）
├── types.rs         （空文件）
├── security.rs      （空文件）
└── error.rs         （空文件）
```

3. 在 `src/plugins/slots/mod.rs` 中添加：

```rust
pub mod audit_phase;
```

4. 确保 `cargo check` 通过（空模块可能产生未使用警告，可接受）。

**验收标准**：
- `cargo check` 无 error
- 目录结构与上述一致
- `src/plugins/slots/mod.rs` 包含 `pub mod audit_phase;`
- Cargo.toml 新增了 `regex` 和 `thiserror` 依赖

---

### 步骤 1：生成所有公共数据类型

**目标**：生成 `config.rs`、`error.rs`、`types.rs`、`security.rs`，包含 AuditPhaseConfig、AuditPhaseError、AuditResult、AuditEvent、SecurityPolicyProvider trait 等基础类型。

#### 1a：config.rs

**文件**：`src/plugins/slots/audit_phase/config.rs`

**要求**：

1. **常量**（跨平台规范 §1-7）：

```rust
/// 设计文档 §2.3——数字阈值使用常量集中管理
pub const DEFAULT_AUDIT_LOG_CAPACITY: usize = 1000;
```

2. **`AuditPhaseConfig` 结构体**：

```rust
/// 设计文档 §2.3
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuditPhaseConfig {
    #[serde(default = "default_true")]
    pub enable_security_check: bool,

    #[serde(default = "default_true")]
    pub enable_sensitive_detection: bool,

    #[serde(default = "default_sensitive_rules")]
    pub sensitive_rules: Vec<SensitiveRuleConfig>,

    #[serde(default = "default_high_risk_tools")]
    pub high_risk_tools: Vec<String>,

    #[serde(default = "default_medium_risk_tools")]
    pub medium_risk_tools: Vec<String>,

    #[serde(default = "default_high_risk_action")]
    pub high_risk_action: RiskAction,

    #[serde(default = "default_true")]
    pub enable_audit_log: bool,

    #[serde(default = "default_audit_log_capacity")]
    pub audit_log_capacity: usize,
}
```

3. **默认值函数**——严格按设计文档 §2.3 表格：

```rust
fn default_true() -> bool { true }
fn default_high_risk_action() -> RiskAction { RiskAction::Block }
fn default_audit_log_capacity() -> usize { DEFAULT_AUDIT_LOG_CAPACITY }
```

4. **`default_sensitive_rules()` 函数**：

```rust
fn default_sensitive_rules() -> Vec<SensitiveRuleConfig> {
    vec![
        SensitiveRuleConfig {
            name: "api_key".to_string(),
            pattern: r"(?i)(api[_-]?key|apikey|access[_-]?key|secret[_-]?key)\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{16,}['"]?".to_string(),
            description: "API 密钥泄露".to_string(),
            severity: RiskSeverity::High,
        },
        SensitiveRuleConfig {
            name: "password".to_string(),
            pattern: r"(?i)(password|passwd|pwd)\s*[:=]\s*['"]?[^\s'"]{8,}['"]?".to_string(),
            description: "密码泄露".to_string(),
            severity: RiskSeverity::High,
        },
        SensitiveRuleConfig {
            name: "private_key".to_string(),
            pattern: r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----".to_string(),
            description: "私钥泄露".to_string(),
            severity: RiskSeverity::Critical,
        },
    ]
}
```

5. **`default_high_risk_tools()` 和 `default_medium_risk_tools()`**：

```rust
fn default_high_risk_tools() -> Vec<String> {
    vec![
        "execute_command".to_string(),
        "write_file".to_string(),
        "delete_file".to_string(),
        "http_request".to_string(),
    ]
}

fn default_medium_risk_tools() -> Vec<String> {
    vec![
        "read_file".to_string(),
        "list_directory".to_string(),
        "search_web".to_string(),
    ]
}
```

6. **`RiskAction`、`RiskSeverity`、`SensitiveRuleConfig` 枚举/结构体**：

```rust
/// 设计文档 §2.3——风险处理方式
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum RiskAction {
    Block,
    Warn,
    LogOnly,
}

/// 设计文档 §2.3——风险严重级别
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// 设计文档 §2.3——敏感信息检测规则
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SensitiveRuleConfig {
    pub name: String,
    pub pattern: String,
    pub description: String,
    pub severity: RiskSeverity,
}
```

**验收标准**：
- `cargo check` 通过
- 所有默认值函数与设计文档完全一致
- `DEFAULT_AUDIT_LOG_CAPACITY` 为模块级 `const`
- 敏感检测规则中不包含任何平台路径字面量
- 序列化/反序列化测试覆盖

---

#### 1b：error.rs

**文件**：`src/plugins/slots/audit_phase/error.rs`

**要求**：

```rust
/// 设计文档 §2.4——AuditPhase 错误类型
#[derive(Debug, thiserror::Error)]
pub enum AuditPhaseError {
    #[error("StepContext 中无 Thought，跳过审计")]
    NoThought,

    #[error("Thought 为 Final 类型，跳过审计")]
    ThoughtIsFinal,

    #[error("安全策略引擎不可用: {0}")]
    SecurityEngineError(String),

    #[error("敏感信息检测错误: {0}")]
    SensitiveDetectionError(String),

    #[error("高风险操作被拦截: {tool_name}, 原因: {reason}")]
    HighRiskBlocked {
        tool_name: String,
        reason: String,
    },

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("正则编译失败: {rule_name}, 模式: {pattern}, 原因: {reason}")]
    RegexCompileError {
        rule_name: String,
        pattern: String,
        reason: String,
    },
}

/// 设计文档 §2.4——自动转换为 PluginError
impl From<AuditPhaseError> for PluginError {
    fn from(e: AuditPhaseError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
```

**验收标准**：
- `cargo check` 通过
- 7 个变体名称与设计文档 §2.4 完全一致
- `Into<PluginError>` 实现存在且正确
- 没有引入额外依赖

---

#### 1c：types.rs

**文件**：`src/plugins/slots/audit_phase/types.rs`

**要求**：

1. **`AuditResult` 结构体**：

```rust
/// 设计文档 §5.1——审计结果，写入 StepContext["audit_result"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub passed: bool,
    pub reason: String,
}
```

2. **`AuditEvent` 结构体**：

```rust
/// 设计文档 §5.2——审计事件，存入 StepContext["audit_log"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: Timestamp,
    pub session_id: String,
    pub tool_name: String,
    pub result: String,   // "passed" / "blocked" / "denied" / "sensitive_detected"
    pub reason: String,
    pub risk_level: RiskSeverity,
}
```

3. **`AuditContext` 结构体**：

```rust
/// 设计文档 §3.2——审计上下文
#[derive(Debug, Clone)]
pub struct AuditContext {
    pub session_id: String,
    pub phase_name: String,
}
```

**`Timestamp` 类型来源**：使用 `crate::core::types::Timestamp`。

**验收标准**：
- `cargo check` 通过
- 所有字段与设计文档 §5.1–§5.3 完全一致
- `AuditEvent.result` 字段类型为 `String`（非枚举），值范围在文档中约定但不做运行时校验

---

#### 1d：security.rs

**文件**：`src/plugins/slots/audit_phase/security.rs`

**要求**：

1. **`SecurityPolicyProvider` trait**：

```rust
/// 设计文档 §3.2——安全策略 Provider 接口
/// 由 SecurityService 在 start() 时注册，AuditPhaseSlot 通过
/// SlotAccessPoint::provider_raw("security") 查找后 downcast 使用。
/// 如果未注册，audit_phase 使用内置规则进行评估。
#[async_trait::async_trait]
pub trait SecurityPolicyProvider: Send + Sync {
    async fn evaluate(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &AuditContext,
    ) -> Result<SecurityDecision, SecurityError>;
}
```

2. **`SecurityDecision` 枚举**：

```rust
/// 设计文档 §3.2——安全决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityDecision {
    Allow,
    AllowWithWarnings { warnings: Vec<AuditWarning> },
    Deny { reason: String },
    RequireConfirmation { prompt: String },
}
```

3. **`AuditWarning` 结构体**：

```rust
/// 设计文档 §3.2——审计警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarning {
    pub rule_name: String,
    pub severity: RiskSeverity,
    pub description: String,
    pub detail: String,
}
```

4. **`SecurityError` 枚举**：

```rust
/// 设计文档 §3.2——安全策略错误
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("策略引擎故障: {0}")]
    EngineError(String),
    #[error("策略配置错误: {0}")]
    ConfigError(String),
}
```

**验收标准**：
- `cargo check` 通过
- `SecurityPolicyProvider` 的方法签名与设计文档 §3.2 完全一致
- `SecurityDecision` 的四个变体与文档一致
- `SecurityError` 的两个变体与文档一致
- trait 使用 `#[async_trait]` 标记
- 没有引用任何 `plugins/services/security` 的具体类型

---

### 步骤 2：实现 AuditPhaseSlot（plugin.rs）

**目标**：实现 `SlotPlugin` trait，包含完整生命周期和核心审计逻辑。

**文件**：`src/plugins/slots/audit_phase/plugin.rs`

**要求**：

1. **`AuditPhaseSlot` 结构体**——遵循 S-R03：

```rust
/// 设计文档 §5.3——Slot 结构体
/// S-R03 合规：compiled_rules 在 init() 时一次性编译，run() 中只读
pub struct AuditPhaseSlot {
    config: Option<AuditPhaseConfig>,
    /// 编译后的正则规则（init() 时编译，run() 中只读不修改）
    compiled_rules: Vec<(SensitiveRuleConfig, regex::Regex)>,
}
```

2. **`new()` 构造函数**：

```rust
impl AuditPhaseSlot {
    pub fn new() -> Self {
        Self {
            config: None,
            compiled_rules: Vec::new(),
        }
    }
}
```

3. **`name()` 方法**——返回 `"audit_phase"`

4. **`init()` 方法**——严格按设计文档 §4.2：

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    // 配置解析 → 正则预编译
    // 失败则返回 PluginError::InitFailed（S-R02）
}
```

5. **`run()` 方法**——严格按设计文档 §4.1 的 8 段式流程：

   - [步骤 1] 从 StepContext 读取 Thought
   - [步骤 2] 判断 Thought 类型（Action vs Final）
   - [步骤 3] 安全策略评估（Provider 扩展，可选）
   - [步骤 4] 内置风险评级
   - [步骤 5] 敏感信息检测
   - [步骤 6] 写入审计结果（Continue 路径）
   - [步骤 7] 记录审计日志
   - [步骤 8] 返回 Continue

6. **`shutdown()` 方法**——设计文档 §4.3：

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    tracing::info!("audit_phase: shutdown 完成");
    Ok(())
}
```

7. **`assess_risk()` 辅助方法**——设计文档 §4.4：

```rust
fn assess_risk(&self, tool_name: &str) -> RiskSeverity {
    // 从 config.high_risk_tools / config.medium_risk_tools 判断
}
```

8. **`record_audit_event()` 辅助方法**——设计文档 §4.4（S-R03 合规）：

```rust
/// S-R03 合规：审计日志从 StepContext 读取，追加后写回
fn record_audit_event(
    &self,
    ap: &mut dyn SlotAccessPoint,
    action: &Action,
    result: &str,
    reason: &str,
) -> Result<(), PluginError> {
    // 构造 AuditEvent
    // 从 StepContext["audit_log"] 读取现有日志缓冲区
    // 追加后写回
    // 超出容量时裁剪
}
```

9. **`Action` 类型来源**：`Action` 是 `Thought::Action` 内部的字段，需要先通过 `downcast_ref::<Thought>()` 获取 Thought 后再匹配。`Thought` 类型定义在 `crate::plugins::slots::llm_thinker::types::Thought`（临时路径，后续迁移到 shared_types）。

**关键路径 vs 非关键路径错误处理**：

| 路径 | 操作 | 失败后果 |
|------|------|---------|
| 关键路径（AbortStep 时） | write_context_raw("audit_result") | 传播 `Err`——tool_executor 依赖此结果 |
| 非关键路径（Continue 时） | write_context_raw("audit_result") | 记录 warn，不传播 |
| 非关键路径 | write_context_raw("audit_warnings") | 记录 warn，不传播 |
| 非关键路径 | write_context_raw("audit_log") | 记录 warn，不传播 |
| 非关键路径（AbortStep 时） | record_audit_event() | 记录 warn，继续返回 AbortStep |

**验收标准**：
- `cargo check` 通过
- `name()` 返回 `"audit_phase"`
- `init()` 使用 `serde_json::from_value::<AuditPhaseConfig>()` 解析配置
- `init()` 中正则编译失败返回 `Err`（S-R02）
- `run()` 中关键路径/非关键路径的错误处理符合设计文档 §4.1 的注释标注
- `run()` 函数长度在合理范围内
- 未使用 `unwrap()` 或 `expect()`

---

### 步骤 3：实现 mod.rs（模块入口）

**目标**：按协议 §6.1 编写模块入口，对外只暴露必要类型，内部全部 `pub(crate)`。

**文件**：`src/plugins/slots/audit_phase/mod.rs`

**要求**：

```rust
// ============================================
// 模块：audit_phase 槽口
//
// 模块职责：
// 在 Pipeline Audit 阶段对 LLM 产生的工具调用动作进行安全审查
//
// 模块边界：
// - 本模块负责：安全策略评估、敏感信息检测、风险评级、审计日志
// - 本模块不负责：工具执行（ToolExecutorSlot）、LLM 思考（LlmThinkerSlot）
//
// 依赖 Provider：
// - "security"（可选，由 SecurityService 注册，未注册时使用内置规则）
//
// 被依赖模块：
// - tool_executor 读取本模块写入的 audit_result 和 audit_warnings
//
// 协议合规：
// - S-R01：Continue（放行）/ AbortStep（拦截）按场景正确返回
// - S-R03：审计日志缓冲区存入 StepContext
// - C-R03：run() 可重入
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod security;
pub mod types;

pub use config::{AuditPhaseConfig, RiskAction, RiskSeverity, SensitiveRuleConfig};
pub use plugin::AuditPhaseSlot;
pub(crate) use error::AuditPhaseError;
```

**验收标准**：
- `cargo check` 通过
- 对外暴露：`AuditPhaseSlot` + `AuditPhaseConfig` + `RiskAction` + `RiskSeverity` + `SensitiveRuleConfig`
- 对外隐藏：`types`、`security`、`error` 内部的实现细节
- 模块头注释包含职责边界和协议合规声明

---

### 步骤 4：注册到 Pipeline

**目标**：将 audit_phase 注册到 Pipeline 的 audit 阶段。

**涉及文件**：

1. `src/plugins/slots/mod.rs` —— 已有 `pub mod audit_phase;`（步骤 0 已完成）

2. **`main.rs`**（或其他 Pipeline 构建代码）：

```rust
pipeline.add_slot(
    Phase::audit(),
    Box::new(AuditPhaseSlot::new()),
);
```

**注意**：当前 `AuditPhaseConfig` 使用全部默认值，不强制要求用户配置。如果 AgentRuntime 支持插件配置注入，应在 `init()` 之前设置 config。

**验收标准**：
- `cargo check` 通过
- `Phase::audit()` 阶段已注册 `AuditPhaseSlot`

---

### 步骤 5：集成测试

**目标**：验证 AuditPhaseSlot 从 init 到 run 再到 shutdown 的完整流程。

**文件**：在 `plugin.rs` 末尾添加 `#[cfg(test)] mod tests { ... }` 块。

**MockSlotAccessPoint 实现**：

```rust
struct MockSlotAccessPoint {
    thought: Option<Thought>,
    session_id: String,
    phase_name: String,
    context_data: HashMap<String, Box<dyn Any + Send>>,
}

impl SlotAccessPoint for MockSlotAccessPoint {
    fn session_id(&self) -> &str { &self.session_id }
    fn phase_name(&self) -> &str { &self.phase_name }
    fn read_context_raw(&self, key: &str) -> Option<&dyn Any> {
        self.context_data.get(key).map(|b| b.as_ref() as &dyn Any)
    }
    fn write_context_raw(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), PluginError> {
        self.context_data.insert(key.to_string(), val);
        Ok(())
    }
    // 其他方法返回默认值
}
```

**测试场景**（设计文档 §9.1–§9.6）：

| # | 测试名称 | 场景 | 断言 |
|:-:|---------|------|------|
| 1 | `test_low_risk_passes` | tool_name="search_web"，正常参数 | Continue，audit_result.passed=true |
| 2 | `test_medium_risk_warns` | tool_name="read_file" | Continue，audit_warnings 非空 |
| 3 | `test_final_skips_audit` | Thought::Final | Continue，不执行审计 |
| 4 | `test_no_thought_skips` | 无 Thought | Continue |
| 5 | `test_high_risk_blocked` | tool_name="execute_command" | AbortStep |
| 6 | `test_high_risk_warn_mode` | high_risk_action=Warn，tool_name="execute_command" | Continue，warnings 包含高风险 |
| 7 | `test_sensitive_api_key` | 参数含 API 密钥 | AbortStep |
| 8 | `test_sensitive_private_key` | 参数含私钥 | AbortStep |
| 9 | `test_security_deny` | Provider 返回 Deny | AbortStep |
| 10 | `test_security_engine_fallback` | Provider 返回 Err | 降级为内置规则，Continue |
| 11 | `test_provider_unavailable` | provider_raw("security") 返回 None | 使用内置规则，Continue |
| 12 | `test_regex_compile_invalid` | pattern="[invalid" | init() 返回 Err |
| 13 | `test_audit_log_append` | 两次 run() | 审计日志累积追加 |
| 14 | `test_empty_args` | arguments = Null | 正常审计，不 panic |
| 15 | `test_unknown_tool` | tool_name="unknown" | 评为 Low，Continue |
| 16 | `test_audit_log_capacity` | 超过 capacity | 淘汰最旧条目 |
| 17 | `test_require_confirmation` | Provider 返回 RequireConfirmation | AbortStep |

**验收标准**：
- `cargo test` 所有测试通过
- 每个测试覆盖一个明确场景，断言具体返回值
- 测试名称体现测试意图
- 无 `#[ignore]` 测试
- `MockSlotAccessPoint` 实现了 `SlotAccessPoint` trait 的所有方法

---

### 步骤 6：终态自检与硬编码审计

1. **全量编译与测试**：

```bash
cargo test
cargo clippy -- -D warnings
```

2. **硬编码扫描**：

```bash
# 检查 DEFAULT_AUDIT_LOG_CAPACITY 是否正确定义
rg '1000' src/plugins/slots/audit_phase/ --type rust
# 检查敏感规则中无平台路径
rg '/etc/|~/.ssh|C:\\\\Windows' src/plugins/slots/audit_phase/
# 检查日志前缀统一
rg '"audit_phase:' src/plugins/slots/audit_phase/ --type rust
# 检查无 panic/unwrap（测试除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/audit_phase/ --type rust --glob '!*test*'
# 检查无 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/audit_phase/ --type rust --glob '!*test*'
```

3. **合规检查清单**——逐项对照设计文档 §11 的三张检查表：

**跨平台规范 10 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| 1 | 无硬编码 URL/端点 | ✅ |
| 2 | 无硬编码模型名 | ✅ |
| 3 | 无硬编码超时值 | ✅ |
| 4 | 无硬编码 API 版本 | ✅ |
| 5 | 无硬编码 User-Agent | ✅ |
| 6 | 敏感规则不含平台路径 | ✅ |
| 7 | `DEFAULT_AUDIT_LOG_CAPACITY` 常量 | ✅ |
| 8 | 无平台指令假设 | ✅ |
| 9 | 测试使用 `std::env::temp_dir()` | ✅ |
| 10 | `cargo test` + `cargo clippy` 通过 | ☐ 待验证 |

**Slot 协议红线 3 项**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| S-R01 | Continue + AbortStep 正确处理 | ✅ |
| S-R02 | init 失败返回 Err | ✅ |
| S-R03 | run() 无跨次可变状态 | ✅ |

**模块边界检查**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| mod.rs 只暴露 3 样东西 | AuditPhaseSlot + AuditPhaseConfig + 关联类型 | ✅ |
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
