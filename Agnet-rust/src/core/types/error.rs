use thiserror::Error;

/// 插件统一错误类型——Slot 和 Service 统一使用此错误
#[derive(Debug, Clone, Error)]
pub enum PluginError {
    #[error("初始化失败: {0}")]
    InitFailed(String),

    #[error("运行时错误: {0}")]
    Runtime(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("权限拒绝: 需要 {required} 权限")]
    PermissionDenied { required: String },

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("关闭错误: {0}")]
    Shutdown(String),

    #[error("名称重复: {0}")]
    DuplicateName(String),

    #[error("依赖未满足: {0}")]
    DependencyNotFound(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl PluginError {
    pub fn init_failed(msg: impl Into<String>) -> Self {
        Self::InitFailed(msg.into())
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// Agent 顶层错误
#[derive(Debug, Clone, Error)]
pub enum AgentError {
    #[error("插件执行失败: plugin={plugin_name}, {message}")]
    PluginFailed {
        plugin_name: String,
        message: String,
    },

    #[error("管道已终止: {reason}")]
    PipelineAborted { reason: String },

    #[error("会话错误: {message}")]
    SessionError { message: String },

    #[error("运行时正在关闭")]
    RuntimeShuttingDown,

    #[error("内部错误: {message}")]
    Internal { message: String },
}

impl From<PluginError> for AgentError {
    fn from(e: PluginError) -> Self {
        AgentError::Internal {
            message: e.to_string(),
        }
    }
}

impl AgentError {
    pub fn plugin_failed(plugin_name: impl Into<String>, err: PluginError) -> Self {
        AgentError::PluginFailed {
            plugin_name: plugin_name.into(),
            message: err.to_string(),
        }
    }
}
