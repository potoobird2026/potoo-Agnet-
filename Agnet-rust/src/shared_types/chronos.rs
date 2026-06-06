//! Chronos 调度服务跨插件契约
//!
//! 协议依据：protocol-shared_types契约协议.md §2-§3
//!
//! Provider key：PROVIDER_CHRONOS
//! Provider trait：ChronosContract
//! 服务方：ChronosServicePlugin 实现
//! 消费方：未来 Slot/Service 通过 provider_raw(PROVIDER_CHRONOS) 调用

use async_trait::async_trait;

/// Provider key 常量（K-R01）
pub const PROVIDER_CHRONOS: &str = "chronos";

/// Chronos 调度服务契约（T-R01）
/// 服务方：ChronosServicePlugin
/// 消费方：通过 provider_raw(PROVIDER_CHRONOS) 调用
#[async_trait]
pub trait ChronosContract: Send + Sync {
    /// 获取调度服务状态快照
    async fn status(&self) -> ChronosStatus;

    /// 暂停调度（不接受新任务执行）
    async fn suspend(&self) -> Result<(), ChronosError>;

    /// 恢复调度
    async fn resume(&self) -> Result<(), ChronosError>;
}

/// Chronos 状态快照
#[derive(Debug, Clone)]
pub struct ChronosStatus {
    pub running: bool,
    pub suspended: bool,
    pub pending_tasks: usize,
}

/// Chronos 错误类型
#[derive(Debug)]
pub enum ChronosError {
    NotInitialized,
    Internal(String),
}
