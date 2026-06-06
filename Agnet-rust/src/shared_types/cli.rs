//! CLI 通道服务跨插件契约
//!
//! 协议依据：protocol-shared_types契约协议.md §2-§3
//!
//! Provider key：PROVIDER_CLI_CHANNEL
//! Provider trait：CliProvider
//! 服务方：CliChannel 实现
//! 消费方：未来 Slot/Service 通过 provider_raw(PROVIDER_CLI_CHANNEL) 调用

use async_trait::async_trait;

/// Provider key 常量（K-R01）
pub const PROVIDER_CLI_CHANNEL: &str = "cli_channel";

/// CLI 通道服务契约（T-R01）
/// 服务方：CliChannel
/// 消费方：通过 provider_raw(PROVIDER_CLI_CHANNEL) 调用
#[async_trait]
pub trait CliProvider: Send + Sync {
    /// 向用户输出消息（通过 CLI 通道）
    async fn output(&self, message: &str) -> Result<(), CliError>;

    /// 查询 CLI 通道是否活跃
    fn is_alive(&self) -> bool;
}

/// CLI 错误类型
#[derive(Debug)]
pub enum CliError {
    ChannelClosed,
    Internal(String),
}
