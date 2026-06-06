use crate::core::types::error::PluginError;

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

impl From<InitPhaseError> for PluginError {
    fn from(e: InitPhaseError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
