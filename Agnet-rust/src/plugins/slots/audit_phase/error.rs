use crate::core::types::error::PluginError;

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
    HighRiskBlocked { tool_name: String, reason: String },

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
