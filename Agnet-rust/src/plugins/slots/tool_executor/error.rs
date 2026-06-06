use crate::core::types::error::PluginError;

#[derive(Debug, thiserror::Error)]
pub enum ToolExecutorError {
    #[error("StepContext 中无 Thought")]
    NoThought,

    #[error("安全策略拒绝: {reason}")]
    SecurityDenied { reason: String },

    #[error("用户拒绝或确认超时")]
    UserRejected,

    #[error("工具未找到: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("工具执行超时: {tool_name}，限制 {timeout_secs} 秒")]
    Timeout {
        tool_name: String,
        timeout_secs: u64,
    },

    #[error("工具执行错误: {tool_name}，原因: {reason}")]
    ExecutionError { tool_name: String, reason: String },

    #[error("熔断器打开: {tool_name}")]
    CircuitBroken { tool_name: String },

    #[error("工具 Provider 未注册")]
    ProviderUnavailable,

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("内部组件错误: {0}")]
    ComponentError(String),
}

impl From<ToolExecutorError> for PluginError {
    fn from(e: ToolExecutorError) -> Self {
        match e {
            ToolExecutorError::NoThought => PluginError::Internal(e.to_string()),
            ToolExecutorError::SecurityDenied { .. } => PluginError::PermissionDenied {
                required: "tool_execute".into(),
            },
            ToolExecutorError::ProviderUnavailable => PluginError::Internal(e.to_string()),
            ToolExecutorError::ConfigError(msg) => PluginError::Config(msg),
            _ => PluginError::Internal(e.to_string()),
        }
    }
}
