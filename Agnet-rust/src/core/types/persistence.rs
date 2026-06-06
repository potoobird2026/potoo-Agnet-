use tokio::sync::oneshot;

use super::Timestamp;
use crate::shared_types::Message;

/// 持久化命令——运行时通过 mpsc 通道发送给 PersistenceWorker
#[derive(Debug)]
pub enum PersistenceCommand {
    /// 保存会话消息
    SaveSession {
        session_id: String,
        messages: Vec<Message>,
        /// 可选 ACK 通道（调用方可等待确认）
        ack_tx: Option<oneshot::Sender<PersistenceAck>>,
    },
    /// 关闭持久化工作进程
    Shutdown,
}

/// 持久化操作确认
#[derive(Debug)]
pub enum PersistenceAck {
    Ok {
        message_count: usize,
    },
    Failed {
        reason: String,
        timestamp: Timestamp,
    },
}
