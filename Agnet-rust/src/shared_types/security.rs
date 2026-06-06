use serde::{Deserialize, Serialize};

/// Provider key 常量——audit_phase slot 通过此 key 查找 SecurityPolicyProvider
pub const PROVIDER_SECURITY: &str = "security";

/// 安全策略 Provider trait —— audit_phase slot 定义，SecurityService 实现
#[async_trait::async_trait]
pub trait SecurityPolicyProvider: Send + Sync + 'static {
    /// 评估工具调用是否安全
    async fn evaluate(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &AuditContext,
    ) -> Result<SecurityDecision, SecurityError>;
}

// 不再需要独立的 DynSecurityProvider——统一使用 shared_types::DynProvider<T>。
// 参见 protocol-shared_types契约协议.md §4。

/// 安全决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityDecision {
    /// 放行
    Allow,
    /// 放行但有警告
    AllowWithWarnings { warnings: Vec<AuditWarning> },
    /// 拦截
    Deny { reason: String },
    /// 需要用户确认
    RequireConfirmation { prompt: String },
}

/// 审计警告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditWarning {
    pub rule_name: String,
    pub severity: RiskSeverity,
    pub description: String,
    pub detail: String,
}

/// 风险级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// 安全评估错误
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("策略引擎故障: {0}")]
    EngineError(String),

    #[error("策略配置错误: {0}")]
    ConfigError(String),
}

/// 审计上下文——评估时传入的上下文信息
#[derive(Debug, Clone)]
pub struct AuditContext {
    pub session_id: String,
    pub phase_name: String,
}
