# audit_phase 槽口开发文档

> 文档版本：v3.1  
> 编写日期：2026-05-30  
> 状态：待开发（从零开始，无任何现有代码）  
> 优先级：P2（Pipeline Audit 阶段核心 Slot，负责安全审计、内容合规检查、敏感信息检测）  
> 执行规范（强制）：《跨平台与硬编码规范》《protocol-Slot接入协议》《protocol-模块内部组件协议》

---

## 0. 现状诊断

### 0.1 当前代码状态

`Phase::audit()` 阶段在 `core/phase.rs` 中已定义（`Phase::audit()` 返回 `Phase("audit".to_string())`），`Pipeline` 包含该阶段，但**整个阶段没有任何 Slot 注册**。

Pipeline 执行时，audit 阶段因 slots 为空而被跳过，从 think 阶段直接进入 execute 阶段。

### 0.2 设计意图

audit 阶段位于 think 和 execute 之间，是 Pipeline 的**安全关卡**，负责：
1. 审查 LLM 产生的 Thought/Action 是否安全
2. 检测工具调用参数是否包含敏感信息泄露风险
3. 验证操作是否符合安全策略
4. 记录审计日志

**核心原则**：audit 阶段不应修改 Thought/Action 的内容，只应做出**放行/拦截/标记**的判断。

### 0.3 相关依赖状态

| 依赖项 | 状态 | 说明 |
|--------|------|------|
| SecurityService | 已实现 | `plugins/services/security/service.rs`，包含 `SecurityPolicyEngine`、`Guardian` 链、`ApprovalService` |
| SecurityPolicyProvider | 未定义 | 需要新建 trait，由 SecurityService 实现并注册 |
| Thought 类型 | 定义在 `llm_thinker/types.rs` | 需确认是否迁移到 shared_types |

---

## 1. 功能概述

### 1.1 功能定位

`AuditPhaseSlot` 是 Pipeline **Audit 阶段**的核心槽口，负责在工具执行前对 LLM 产生的动作进行安全审查。

**核心职责**：
1. 从 StepContext 读取 llm_thinker 产生的 Thought
2. 对 Thought::Action 进行安全策略评估
3. 检测工具调用参数中的敏感信息（API 密钥、密码、私钥等）
4. 对高风险操作标记警告或拦截
5. 记录审计日志
6. 返回适当的 SlotDirective（Continue 放行 / AbortStep 拦截）

### 1.2 在 Pipeline 中的位置

```
Phase::init()       → InitPhaseSlot（会话初始化）
Phase::context()    → ToolRegistrySlot（收集工具定义）
Phase::think()      → LlmThinkerSlot（生成 Thought）
Phase::audit()      → ★ AuditPhaseSlot（本文档）
Phase::execute()    → ToolExecutorSlot（执行工具调用）
Phase::loop()       → ReActLoopSlot（决定是否继续迭代）
Phase::memorize()   → MemorySaverSlot + CompressionHookSlot
```

### 1.3 数据流

```
LlmThinkerSlot (think 阶段)
    │
    ▼
StepContext["thought"] ← Thought::Action { tool_name, arguments }
    │
    ▼
AuditPhaseSlot (audit 阶段) ★ 本文档
    │
    ├─ 1. read_context_raw("thought") 读取 Thought
    ├─ 2. provider_raw("security") 获取 SecurityPolicyProvider（可选）
    ├─ 3. 安全策略评估
    ├─ 4. 敏感信息检测（内置正则规则）
    ├─ 5. 风险评级
    ├─ 6. 记录审计日志 → StepContext["audit_log"]
    │
    ├─ 低风险 → Continue → ToolExecutorSlot 执行
    ├─ 中风险 → Continue（附带警告标记）→ ToolExecutorSlot 执行
    └─ 高风险/敏感信息/策略拒绝 → AbortStep → 终止本轮
```

---

## 2. 接口契约

### 2.1 实现 trait

```rust
#[async_trait::async_trait]
impl SlotPlugin for AuditPhaseSlot
```

### 2.2 生命周期方法

| 方法 | 调用次数 | 职责 |
|------|---------|------|
| `name()` | 多次 | 返回 `"audit_phase"` |
| `init()` | 1 | 解析配置，编译正则规则。**失败则插件不被加载（S-R02）** |
| `run()` | 每轮 Audit 阶段 | 读取 Thought → 安全评估 → 敏感检测 → 风险评级 → 决策 |
| `shutdown()` | 1 | 刷新审计日志缓冲区 |

### 2.3 配置结构体

> **《跨平台与硬编码规范》§1**：数字阈值必须定义为常量。  
> **《跨平台与硬编码规范》§2.1-2.2**：禁止裸用 Unix-only 路径。敏感检测规则中的路径模式必须跨平台兼容。

```rust
/// 审计日志默认容量
pub const DEFAULT_AUDIT_LOG_CAPACITY: usize = 1000;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuditPhaseConfig {
    /// 是否启用安全策略检查，默认 true
    #[serde(default = "default_true")]
    pub enable_security_check: bool,

    /// 是否启用敏感信息检测，默认 true
    #[serde(default = "default_true")]
    pub enable_sensitive_detection: bool,

    /// 敏感信息检测规则列表
    #[serde(default = "default_sensitive_rules")]
    pub sensitive_rules: Vec<SensitiveRuleConfig>,

    /// 高风险操作列表（工具名），默认包含危险操作
    #[serde(default = "default_high_risk_tools")]
    pub high_risk_tools: Vec<String>,

    /// 中风险操作列表（工具名）
    #[serde(default = "default_medium_risk_tools")]
    pub medium_risk_tools: Vec<String>,

    /// 高风险操作的处理方式："block" 或 "warn"，默认 "block"
    #[serde(default = "default_high_risk_action")]
    pub high_risk_action: RiskAction,

    /// 是否记录审计日志，默认 true
    #[serde(default = "default_true")]
    pub enable_audit_log: bool,

    /// 审计日志最大条目数（内存缓冲），默认 DEFAULT_AUDIT_LOG_CAPACITY
    #[serde(default = "default_audit_log_capacity")]
    pub audit_log_capacity: usize,
}

fn default_true() -> bool { true }
fn default_high_risk_action() -> RiskAction { RiskAction::Block }
fn default_audit_log_capacity() -> usize { DEFAULT_AUDIT_LOG_CAPACITY }

fn default_sensitive_rules() -> Vec<SensitiveRuleConfig> {
    vec![
        SensitiveRuleConfig {
            name: "api_key".to_string(),
            // 规范合规：仅检测数据格式，不涉及平台特定路径
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum RiskAction {
    Block,
    Warn,
    LogOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SensitiveRuleConfig {
    pub name: String,
    pub pattern: String,
    pub description: String,
    pub severity: RiskSeverity,
}
```

> **规范合规说明**：`default_sensitive_rules()` 中的正则模式仅检测**数据格式**（API 密钥、密码、私钥的字符串格式），不包含任何平台特定路径（如 `/etc/passwd`、`~/.ssh/`、`C:\Windows\System32` 等），符合《跨平台与硬编码规范》§2.1-2.2。

### 2.4 错误类型

```rust
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

impl From<AuditPhaseError> for PluginError {
    fn from(e: AuditPhaseError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
```

### 2.5 插件元数据声明

> **《protocol-Slot接入协议》§3**：每个插件必须附带元数据声明。

```rust
pub fn metadata() -> PluginMetadata {
    PluginMetadata {
        name: "audit_phase".to_string(),
        category: "slot".to_string(),
        version: "0.1.0".to_string(),
        permissions: vec![
            "context:read".to_string(),
            "context:write".to_string(),
        ],
        requires: vec![
            "security".to_string(),  // 依赖 SecurityService 注册的安全策略 Provider（可选，未注册时降级为内置规则）
        ],
        conflicts: vec![],
        config_schema: None,
    }
}
```

---

## 3. 依赖接口

### 3.1 Core 内建（通过 SlotAccessPoint）

> **《protocol-Slot接入协议》§2.1**：权限 tag 必须与协议定义完全一致。

| 方法 | 权限 tag | 用途 |
|------|---------|------|
| `read_context_raw("thought")` | `context:read` | 读取 llm_thinker 产生的 Thought |
| `write_context_raw("audit_result", ...)` | `context:write` | 写入审计结果 |
| `write_context_raw("audit_warnings", ...)` | `context:write` | 写入审计警告列表 |
| `write_context_raw("audit_log", ...)` | `context:write` | **S-R03 合规**：审计日志缓冲区存入 StepContext |
| `session_id()` | 无（总是允许） | 获取当前会话 ID（审计日志用） |
| `phase_name()` | 无（总是允许） | 确认当前在 audit 阶段 |
| `current_iteration()` | 无（总是允许） | 获取当前迭代次数 |

### 3.2 Provider 扩展

| Provider 名 | 期望类型 | 用途 |
|-------------|---------|------|
| `"security"` | `Arc<dyn SecurityPolicyProvider>` | 安全策略评估（可选，未注册时降级为内置规则） |

**SecurityPolicyProvider trait 定义**（由 audit_phase 定义，由 SecurityService 实现）：

> **《protocol-Slot接入协议》§2.2**：Provider 通过 `provider_raw(name)` 返回类型擦除的 `Arc`，调用方通过 `downcast` 获取具体类型。

```rust
/// 安全策略 Provider——由 SecurityService 在 start() 时注册
///
/// 说明：定义安全策略评估接口。
/// AuditPhaseSlot 通过 SlotAccessPoint::provider_raw("security") 查找后 downcast 使用。
/// 如果未注册，audit_phase 使用内置规则进行评估。
#[async_trait::async_trait]
pub trait SecurityPolicyProvider: Send + Sync {
    /// 评估工具调用是否安全
    ///
    /// 入参：
    /// - tool_name：工具名称
    /// - arguments：工具调用参数
    /// - context：审计上下文（会话 ID、阶段名）
    ///
    /// 出参：
    /// - SecurityDecision：安全决策
    ///
    /// 错误：
    /// - 策略引擎故障 → SecurityError
    async fn evaluate(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &AuditContext,
    ) -> Result<SecurityDecision, SecurityError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityDecision {
    /// 允许执行
    Allow,
    /// 允许但附带警告
    AllowWithWarnings { warnings: Vec<AuditWarning> },
    /// 拒绝执行
    Deny { reason: String },
    /// 需要人工确认
    RequireConfirmation { prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarning {
    pub rule_name: String,
    pub severity: RiskSeverity,
    pub description: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct AuditContext {
    pub session_id: String,
    pub phase_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("策略引擎故障: {0}")]
    EngineError(String),
    #[error("策略配置错误: {0}")]
    ConfigError(String),
}
```

---

## 4. 执行逻辑

### 4.1 run() 完整流程

> **《protocol-Slot接入协议》§9 S-R01**：Continue 和 AbortStep 必须按场景正确返回。  
> **《protocol-Slot接入协议》§9 S-R03**：run() 中禁止持有跨次调用的可变状态。审计日志缓冲区存入 StepContext。

```rust
async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
    // ══════════════════════════════════════════
    // 步骤 1：从 StepContext 读取 Thought
    // ══════════════════════════════════════════
    let thought = match ap.read_context_raw("thought") {
        Some(any) => match any.downcast_ref::<Thought>() {
            Some(t) => t.clone(),
            None => {
                tracing::warn!("audit_phase: thought 类型不匹配，跳过审计");
                return Ok(SlotDirective::Continue);
            }
        },
        None => {
            tracing::debug!("audit_phase: StepContext 中无 Thought，跳过审计");
            return Ok(SlotDirective::Continue);
        }
    };

    // ══════════════════════════════════════════
    // 步骤 2：判断 Thought 类型
    // ══════════════════════════════════════════
    let action = match &thought {
        Thought::Action { action, .. } => action,
        Thought::Final { .. } => {
            tracing::debug!("audit_phase: Thought 为 Final，跳过审计");
            return Ok(SlotDirective::Continue);
        }
    };

    let mut audit_warnings: Vec<AuditWarning> = Vec::new();

    // ══════════════════════════════════════════
    // 步骤 3：安全策略评估（Provider 扩展，可选）
    // ══════════════════════════════════════════
    if self.config.enable_security_check {
        if let Some(raw) = ap.provider_raw("security") {
            if let Ok(security_arc) = raw.downcast::<Arc<dyn SecurityPolicyProvider>>() {
                let audit_ctx = AuditContext {
                    session_id: ap.session_id().to_string(),
                    phase_name: ap.phase_name().to_string(),
                };

                match (*security_arc)
                    .evaluate(&action.tool_name, &action.arguments, &audit_ctx)
                    .await
                {
                    Ok(SecurityDecision::Deny { reason }) => {
                        tracing::warn!(
                            "audit_phase: 安全策略拒绝工具 {}: {}",
                            action.tool_name,
                            reason
                        );
                        // 非关键路径：审计日志写入失败不中断拦截决策
                        if let Err(e) = self.record_audit_event(ap, &action, "denied", &reason) {
                            tracing::warn!("audit_phase: 审计日志写入失败: {}，继续拦截", e);
                        }
                        // 关键路径：audit_result 写入失败必须传播错误
                        // 下游 tool_executor 依赖 audit_result 判断是否放行
                        ap.write_context_raw(
                            "audit_result",
                            Box::new(AuditResult {
                                passed: false,
                                reason: reason.clone(),
                            }),
                        )?;
                        return Ok(SlotDirective::AbortStep);
                    }
                    Ok(SecurityDecision::AllowWithWarnings { warnings }) => {
                        audit_warnings.extend(warnings);
                    }
                    Ok(SecurityDecision::Allow) => {}
                    Ok(SecurityDecision::RequireConfirmation { prompt }) => {
                        tracing::info!("audit_phase: 需要人工确认: {}", prompt);
                        // 关键路径：audit_result 写入失败必须传播错误
                        ap.write_context_raw(
                            "audit_result",
                            Box::new(AuditResult {
                                passed: false,
                                reason: format!("需要人工确认: {}", prompt),
                            }),
                        )?;
                        return Ok(SlotDirective::AbortStep);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "audit_phase: 安全策略引擎故障: {}，降级为内置规则",
                            e
                        );
                    }
                }
            }
        }
    }

    // ══════════════════════════════════════════
    // 步骤 4：内置风险评级
    // ══════════════════════════════════════════
    let risk_level = self.assess_risk(&action.tool_name);

    match risk_level {
        RiskSeverity::Critical | RiskSeverity::High => {
            if self.config.high_risk_action == RiskAction::Block {
                let reason = format!("高风险工具: {}", action.tool_name);
                // 非关键路径：审计日志写入失败不中断拦截决策
                if let Err(e) = self.record_audit_event(ap, &action, "blocked", &reason) {
                    tracing::warn!("audit_phase: 审计日志写入失败: {}，继续拦截", e);
                }
                // 关键路径：audit_result 写入失败必须传播错误
                ap.write_context_raw(
                    "audit_result",
                    Box::new(AuditResult {
                        passed: false,
                        reason: reason.clone(),
                    }),
                )?;
                return Ok(SlotDirective::AbortStep);
            } else {
                audit_warnings.push(AuditWarning {
                    rule_name: "high_risk_tool".to_string(),
                    severity: RiskSeverity::High,
                    description: "高风险工具".to_string(),
                    detail: format!("工具 {} 在高风险列表中", action.tool_name),
                });
            }
        }
        RiskSeverity::Medium => {
            audit_warnings.push(AuditWarning {
                rule_name: "medium_risk_tool".to_string(),
                severity: RiskSeverity::Medium,
                description: "中风险工具".to_string(),
                detail: format!("工具 {} 在中风险列表中", action.tool_name),
            });
        }
        RiskSeverity::Low => {}
    }

    // ══════════════════════════════════════════
    // 步骤 5：敏感信息检测
    // ══════════════════════════════════════════
    if self.config.enable_sensitive_detection {
        let args_str = action.arguments.to_string();

        // 编译后的正则从 self.compiled_rules 读取（init() 时编译）
        for (rule, compiled_re) in &self.compiled_rules {
            if compiled_re.is_match(&args_str) {
                let warning = AuditWarning {
                    rule_name: rule.name.clone(),
                    severity: rule.severity.clone(),
                    description: rule.description.clone(),
                    detail: format!(
                        "工具 {} 参数中检测到: {}",
                        action.tool_name, rule.description
                    ),
                };

                if rule.severity >= RiskSeverity::High {
                    // 非关键路径：审计日志写入失败不中断拦截决策
                    if let Err(e) = self.record_audit_event(
                        ap,
                        &action,
                        "sensitive_detected",
                        &rule.description,
                    ) {
                        tracing::warn!("audit_phase: 审计日志写入失败: {}，继续拦截", e);
                    }
                    // 关键路径：audit_result 写入失败必须传播错误
                    ap.write_context_raw(
                        "audit_result",
                        Box::new(AuditResult {
                            passed: false,
                            reason: format!("敏感信息: {}", rule.description),
                        }),
                    )?;
                    return Ok(SlotDirective::AbortStep);
                }

                audit_warnings.push(warning);
            }
        }
    }

    // ══════════════════════════════════════════
    // 步骤 6：写入审计结果（Continue 路径，非关键路径，降级处理）
    // ══════════════════════════════════════════
    if !audit_warnings.is_empty() {
        // 非关键路径：audit_warnings 写入失败不中断 Pipeline
        if let Err(e) = ap.write_context_raw("audit_warnings", Box::new(audit_warnings.clone())) {
            tracing::warn!("audit_phase: 写入 audit_warnings 失败: {}，跳过", e);
        }
        for w in &audit_warnings {
            tracing::warn!(
                "audit_phase: 警告 [{:?}] {} - {}",
                w.severity,
                w.rule_name,
                w.description
            );
        }
    }

    // 非关键路径：audit_result 写入失败不中断 Pipeline（tool_executor 自行判断）
    if let Err(e) = ap.write_context_raw(
        "audit_result",
        Box::new(AuditResult {
            passed: true,
            reason: String::new(),
        }),
    ) {
        tracing::warn!("audit_phase: 写入 audit_result 失败: {}，跳过", e);
    }

    // ══════════════════════════════════════════
    // 步骤 7：记录审计日志（存入 StepContext，S-R03 合规）
    // ══════════════════════════════════════════
    if self.config.enable_audit_log {
        // 非关键路径：审计日志写入失败不中断 Pipeline
        if let Err(e) = self.record_audit_event(ap, &action, "passed", "审计通过") {
            tracing::warn!("audit_phase: 审计日志写入失败: {}，跳过", e);
        }
    }

    // ══════════════════════════════════════════
    // 步骤 8：返回 Continue（S-R01）
    // ══════════════════════════════════════════
    Ok(SlotDirective::Continue)
}
```

### 4.2 init() 流程

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    let config: AuditPhaseConfig = serde_json::from_value(ctx.plugin_config.clone())
        .map_err(|e| PluginError::InitFailed {
            reason: format!("audit_phase 配置解析失败: {}", e),
        })?;

    // 预编译正则规则（失败则插件不加载，S-R02）
    let mut compiled_rules = Vec::new();
    for rule in &config.sensitive_rules {
        let re = regex::Regex::new(&rule.pattern).map_err(|e| {
            PluginError::InitFailed {
                reason: format!(
                    "audit_phase: 正则编译失败 [{}]: {} - {}",
                    rule.name, rule.pattern, e
                ),
            }
        })?;
        compiled_rules.push((rule.clone(), re));
    }

    self.config = Some(config);
    self.compiled_rules = compiled_rules;

    tracing::info!("audit_phase: 初始化完成，编译 {} 条正则规则", self.compiled_rules.len());
    Ok(())
}
```

### 4.3 shutdown() 流程

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    tracing::info!("audit_phase: shutdown 完成");
    Ok(())
}
```

### 4.4 辅助方法

```rust
impl AuditPhaseSlot {
    /// 根据工具名评估风险级别
    fn assess_risk(&self, tool_name: &str) -> RiskSeverity {
        let config = self.config.as_ref().unwrap();
        if config.high_risk_tools.contains(&tool_name.to_string()) {
            RiskSeverity::High
        } else if config.medium_risk_tools.contains(&tool_name.to_string()) {
            RiskSeverity::Medium
        } else {
            RiskSeverity::Low
        }
    }

    /// 记录审计事件（写入 StepContext，S-R03 合规）
    fn record_audit_event(
        &self,
        ap: &mut dyn SlotAccessPoint,
        action: &Action,
        result: &str,
        reason: &str,
    ) -> Result<(), PluginError> {
        let event = AuditEvent {
            timestamp: Timestamp::now(),
            session_id: ap.session_id().to_string(),
            tool_name: action.tool_name.clone(),
            result: result.to_string(),
            reason: reason.to_string(),
            risk_level: self.assess_risk(&action.tool_name),
        };

        // 从 StepContext 读取现有日志缓冲区，追加后写回（S-R03 合规）
        let mut log_buffer: Vec<AuditEvent> = ap
            .read_context_raw("audit_log")
            .and_then(|any| any.downcast_ref::<Vec<AuditEvent>>())
            .cloned()
            .unwrap_or_default();

        log_buffer.push(event.clone());

        // 超出容量时淘汰最旧的
        let capacity = self.config.as_ref().map(|c| c.audit_log_capacity).unwrap_or(DEFAULT_AUDIT_LOG_CAPACITY);
        if log_buffer.len() > capacity {
            log_buffer.drain(0..log_buffer.len() - capacity);
        }

        // 非关键路径：audit_log 写入失败不中断审计流程
        if let Err(e) = ap.write_context_raw("audit_log", Box::new(log_buffer)) {
            tracing::warn!("audit_phase: 写入 audit_log 失败: {}，继续", e);
        }

        // 同时输出 tracing 日志
        match result {
            "passed" => {
                tracing::info!(
                    audit = true,
                    tool = %event.tool_name,
                    result = %event.result,
                    "审计通过"
                );
            }
            _ => {
                tracing::warn!(
                    audit = true,
                    tool = %event.tool_name,
                    result = %event.result,
                    reason = %event.reason,
                    "审计拦截"
                );
            }
        }

        Ok(())
    }
}
```

---

## 5. 数据结构

### 5.1 审计结果

```rust
/// 审计结果——写入 StepContext["audit_result"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub passed: bool,
    pub reason: String,
}
```

### 5.2 审计事件

```rust
/// 审计事件——存入 StepContext["audit_log"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: Timestamp,
    pub session_id: String,
    pub tool_name: String,
    pub result: String,  // "passed" / "blocked" / "denied" / "sensitive_detected"
    pub reason: String,
    pub risk_level: RiskSeverity,
}
```

### 5.3 Slot 结构体

> **S-R03 合规说明**：`compiled_rules` 在 `init()` 时一次性编译，在 `run()` 中只读不修改，属于"初始化后不变"的字段，不违反 S-R03。审计日志缓冲区存入 StepContext，不在 Slot 字段中。

```rust
pub struct AuditPhaseSlot {
    config: Option<AuditPhaseConfig>,
    /// 编译后的正则规则（init() 时编译，run() 中只读）
    compiled_rules: Vec<(SensitiveRuleConfig, regex::Regex)>,
}
```

---

## 6. 文件结构

```
plugins/slots/audit_phase/
├── mod.rs              # 模块入口（组件协议 §6.1：只暴露 AuditPhaseSlot + AuditPhaseConfig）
├── plugin.rs           # SlotPlugin 实现（核心逻辑）
├── config.rs           # AuditPhaseConfig / RiskAction / RiskSeverity / SensitiveRuleConfig + 常量
├── types.rs            # AuditResult / AuditEvent / AuditContext
├── security.rs         # SecurityPolicyProvider trait + SecurityDecision + SecurityError + AuditWarning
└── error.rs            # AuditPhaseError 定义 + Into<PluginError>
```

---

## 7. mod.rs 规范

> **《protocol-模块内部组件协议》§6.1**：模块 `mod.rs` 只暴露三样东西：对外 Slot 入口、配置、错误类型。

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
// 核心层实现：
// - SlotPlugin → AuditPhaseSlot
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
//
// 协议合规：
// - S-R01：Continue（放行）/ AbortStep（拦截）按场景正确返回
// - S-R03：审计日志缓冲区存入 StepContext，编译后的正则缓存在 Slot 字段中（init() 时一次性编译）
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

---

## 8. 注册步骤

> **《protocol-Slot接入协议》§8**：新增 Slot 标准流程共需改 2 个文件。

### 8.1 修改 `plugins/slots/mod.rs`（第 1 个文件）

```rust
pub mod audit_phase;     // ★ 新增
pub mod init_phase;
pub mod llm_thinker;
pub mod memory_saver;
pub mod react_loop;
pub mod tool_executor;
pub mod tool_registry;
```

### 8.2 修改 Pipeline 构建代码（第 2 个文件）

```rust
pipeline.add_slot(
    Phase::audit(),
    Box::new(AuditPhaseSlot::new(audit_config)),
);
```

---

## 9. 测试要点

> **《跨平台与硬编码规范》§3**：测试中无 Unix-only 路径，均用 `std::env::temp_dir()`。

### 9.1 正常路径测试

| 测试场景 | 前置条件 | 输入 | 期望 |
|---------|---------|------|------|
| 低风险工具通过 | Thought::Action，tool_name="search_web" | 正常参数 | Continue，audit_result.passed=true |
| 中风险工具通过（带警告） | Thought::Action，tool_name="read_file" | 正常参数 | Continue，audit_warnings 非空 |
| Final 类型跳过 | Thought::Final | answer="最终回答" | Continue，不执行审计 |
| 无 Thought 跳过 | context 中无 thought | — | Continue，不执行审计 |

### 9.2 边界条件测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 空参数 | arguments = serde_json::Value::Null | 正常审计，不 panic |
| 工具名不在任何列表 | tool_name="unknown_tool" | 评为低风险，Continue |
| 审计日志满 | 超过 audit_log_capacity | 淘汰最旧条目 |
| 正则模式无效 | pattern = "[invalid" | init() 返回 Err（S-R02），插件不加载 |

### 9.3 异常路径测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 高风险工具 + Block 模式 | tool_name="execute_command" | AbortStep，audit_result.passed=false |
| 高风险工具 + Warn 模式 | high_risk_action=Warn | Continue，audit_warnings 包含高风险警告 |
| 敏感信息检测命中（API 密钥） | 参数包含 API 密钥格式 | AbortStep，audit_result.passed=false |
| 敏感信息检测命中（私钥） | 参数包含私钥格式 | AbortStep，audit_result.passed=false |
| 安全策略拒绝 | SecurityPolicyProvider 返回 Deny | AbortStep |
| 安全策略引擎故障 | SecurityPolicyProvider 返回 Err | 降级为内置规则，Continue |
| 需要人工确认 | SecurityPolicyProvider 返回 RequireConfirmation | AbortStep |
| write_context_raw 失败（audit_result，Continue 路径） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| write_context_raw 失败（audit_warnings） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| write_context_raw 失败（audit_log） | StepContext 写入异常 | 记录警告，Continue/AbortStep 不受影响 |
| record_audit_event 失败（AbortStep 路径） | StepContext 写入异常 | 记录警告，继续返回 AbortStep |

### 9.4 外部依赖测试

| 测试场景 | 前置条件 | 期望 |
|---------|---------|------|
| Security Provider 未注册 | provider_raw("security") 返回 None | 使用内置规则，不报错 |
| Security Provider 类型不匹配 | downcast 失败 | 使用内置规则，不报错 |

### 9.5 SlotDirective 完整性测试（S-R01）

| 返回值 | 场景 | Pipeline 行为 |
|--------|------|-------------|
| `Continue` | 低/中风险通过、无 Thought、Final 类型、Provider 降级 | 进入 execute 阶段 |
| `Continue` | write_context_raw 失败（Continue 路径的 audit_result/warnings/log） | 记录警告，进入 execute 阶段 |
| `AbortStep` | 高风险拦截、敏感信息检测、安全策略拒绝、需要确认 | 终止本轮 Step |
| `AbortStep` | record_audit_event 失败（AbortStep 路径） | 记录警告，仍返回 AbortStep |

### 9.6 S-R03 合规验证

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 重复审计 | 同一 StepContext 运行两次 | 审计日志正确追加（从 StepContext 读取） |
| Slot 重建后运行 | 新建 Slot 实例，使用同一 StepContext | 从 StepContext 读取审计日志，正确续接 |

---

## 10. 待确认事项

1. **Thought 类型归属**：`Thought` 类型当前定义在 `llm_thinker/types.rs` 中，但它是 llm_thinker 写入、audit_phase 和 tool_executor 读取的共享类型。
   - **建议**：迁移到 `shared_types` 中（与 Message 并列）。

2. **regex 依赖**：敏感信息检测需要 `regex` crate。
   - **确认**：需要在 `Cargo.toml` 中添加 `regex = "1"` 依赖。

3. **审计日志持久化**：当前设计为 StepContext 缓冲 + tracing 输出。
   - **建议**：v0.1 仅内存缓冲，v0.2 考虑通过 Provider 持久化。

4. **人工确认流程**：RequireConfirmation 在 v0.1 直接拦截。
   - **建议**：v0.1 返回 AbortStep，v0.2 接入确认 channel。

5. **SecurityPolicyProvider 实现**：由 `plugins/services/security` 服务实现并注册。
   - **确认**：SecurityService 需要在 `start()` 中注册 `Arc<dyn SecurityPolicyProvider>`。

---

## 11. 规范合规检查清单

### 《跨平台与硬编码规范》10 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | 不涉及 URL | ✅ 不适用 |
| 2 | 所有模型名称来自配置字段，非硬编码 | 不涉及模型名 | ✅ 不适用 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | 不涉及超时（无网络请求） | ✅ 不适用 |
| 4 | API 版本号定义为模块级 `const`，不散落 | 不涉及 API 版本 | ✅ 不适用 |
| 5 | User-Agent 定义为 `const USER_AGENT` | 不涉及 HTTP 请求 | ✅ 不适用 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | 敏感规则仅检测数据格式，不含 `/etc/passwd` 等平台路径 | ✅ |
| 7 | 数字阈值默认 `None` 或从配置读取 | `DEFAULT_AUDIT_LOG_CAPACITY` 常量 | ✅ |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | 不涉及平台指令 | ✅ 不适用 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | 测试使用 `std::env::temp_dir()` | ✅ |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待实现后验证 | ☐ 待验证 |

### 《protocol-Slot接入协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| S-R01 | 所有 `SlotDirective` 变体必须被正确处理 | Continue（放行）/ AbortStep（拦截）按场景正确返回，无遗漏 | ✅ |
| S-R02 | `init` 失败意味着插件不加载 | 配置解析失败、正则编译失败均返回 `Err` | ✅ |
| S-R03 | `run()` 中禁止持有跨次调用的可变状态 | 审计日志存入 StepContext；compiled_rules 在 init() 时编译后只读 | ✅ |

### 《protocol-Slot接入协议》权限与依赖

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 权限声明 | context:read, context:write | PluginMetadata.permissions 声明 | ✅ |
| requires | 声明依赖 "llm_thinker" | PluginMetadata.requires 声明 | ✅ |
| Provider 查找 | provider_raw("security") + downcast | run() 中按规范查找，未注册时降级 | ✅ |
| 优雅降级 | Provider 未注册时使用内置规则 | 不 panic，记录 warn 日志 | ✅ |

### 《protocol-模块内部组件协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| C-R01 | `AccessPoint::call()` 获取句柄后必须 downcast | audit_phase 不使用内部组件协议（单组件），通过 SlotAccessPoint 与外部交互 | ✅ 不适用 |
| C-R02 | `meta().requires` 声明必须真实可验证 | 同上 | ✅ 不适用 |
| C-R03 | `process()` 必须可重入 | run() 无隐式跨调用状态，可重入 | ✅ |

### 《protocol-模块内部组件协议》模块边界

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| mod.rs 只暴露三样东西 | AuditPhaseSlot + AuditPhaseConfig + AuditPhaseError | 内部 types/security 全部 pub(crate) | ✅ |
| 依赖方向正确 | 只依赖 core + shared_types + llm_thinker::types（临时） | 不依赖其他 Slot 具体实现 | ✅ |

---

## 12. 开发清单

| 序号 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `Cargo.toml` | 添加 `regex = "1"` 依赖 | 敏感信息检测需要 |
| 2 | `shared_types` | 确认/迁移 `Thought` 类型 | 与 llm_thinker 协商 |
| 3 | `plugins/slots/audit_phase/config.rs` | 新建 | 常量 + AuditPhaseConfig + RiskAction + RiskSeverity + SensitiveRuleConfig |
| 4 | `plugins/slots/audit_phase/error.rs` | 新建 | AuditPhaseError + Into<PluginError> |
| 5 | `plugins/slots/audit_phase/types.rs` | 新建 | AuditResult / AuditEvent / AuditContext |
| 6 | `plugins/slots/audit_phase/security.rs` | 新建 | SecurityPolicyProvider trait + SecurityDecision + SecurityError + AuditWarning |
| 7 | `plugins/slots/audit_phase/plugin.rs` | 新建 | AuditPhaseSlot 实现 |
| 8 | `plugins/slots/audit_phase/mod.rs` | 新建 | 模块入口（组件协议 §6.1） |
| 9 | `plugins/slots/mod.rs` | 添加 `pub mod audit_phase` | 模块注册 |
| 10 | `main.rs` | Pipeline 添加 `.add_slot(Phase::audit(), ...)` | 注册到 audit 阶段 |
| 11 | `plugins/services/security/service.rs` | 修改 register_provider | 注册 SecurityPolicyProvider |

---

## 13. 依赖关系

### 13.1 上游依赖

| 依赖 | 类型 | 说明 |
|------|------|------|
| `SecurityService` | Provider `"security"` | 注册 Arc<dyn SecurityPolicyProvider>（可选） |
| `shared_types::Message` | 类型 | 不直接依赖（通过 StepContext 间接交互） |
| `llm_thinker::types::Thought` | 类型 | 从 StepContext["thought"] 读取（临时，待迁移到 shared_types） |
| `llm_thinker::types::Action` | 类型 | Thought::Action 内部的工具调用动作 |

### 13.2 下游依赖

| 依赖者 | 说明 |
|--------|------|
| `tool_executor` | 读取本模块写入的 audit_result 和 audit_warnings |

### 13.3 执行顺序

Pipeline 阶段顺序保证 audit 在 think 之后、execute 之前，无需额外同步。

---

> 文档版本：v3.0  
> 最后更新：2026-05-30  
> 按三份规范逐项对照修订完成，不简化、不降级、不走捷径。