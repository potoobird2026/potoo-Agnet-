use crate::core::types::error::PluginError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemorySaverError {
    #[error("Memory Provider 未注册，无法持久化")]
    ProviderUnavailable,

    #[error("记忆写入超时（{timeout_secs} 秒）")]
    WriteTimeout { timeout_secs: u64 },

    #[error("记忆写入失败: {0}")]
    WriteError(String),

    #[error("向量索引更新失败: {0}")]
    VectorIndexError(String),

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),
}

impl From<MemorySaverError> for PluginError {
    fn from(e: MemorySaverError) -> Self {
        PluginError::Runtime(e.to_string())
    }
}
