/*!
 * 会话持久化工作器
 *
 * PersistenceWorker：后台异步任务，通过 mpsc 通道接收 PersistenceCommand，
 * 将 Session 消息序列化为 JSON 文件。
 *
 * 关键设计：
 * - 原子写入：先写 .tmp，再 rename → .json（防止写入中途崩溃导致文件损坏）
 * - 会话 ID 清理：文件名中的特殊字符替换为 _，防止路径注入
 * - 无界通道：PersistenceCommand 体积小（< 1KB），写入频率低，不会堆积
 */

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::core::runtime::SharedMessageStore;
use crate::core::types::persistence::{PersistenceAck, PersistenceCommand};
use crate::shared_types::Message;

use super::storage::sessions_dir;

use tokio::io::AsyncWriteExt;

/// 会话文件名中允许的字符集（字母数字 + 连字符 + 下划线）
fn sanitize_session_filename(session_id: &str) -> String {
    session_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 持久化工作器
///
/// 持有 mpsc 接收端和数据根路径。
/// 调用 `run()` 启动主循环，阻塞当前任务直到 Shutdown 或 channel 关闭。
pub struct PersistenceWorker {
    receiver: mpsc::UnboundedReceiver<PersistenceCommand>,
    base_path: PathBuf,
}

impl PersistenceWorker {
    /// 创建新的持久化工作器
    ///
    /// - `receiver`: 从 Runtime 传入的 PersistenceCommand 接收端
    /// - `base_path`: 会话持久化根目录（通常为 `sessions_dir()`）
    pub fn new(receiver: mpsc::UnboundedReceiver<PersistenceCommand>, base_path: PathBuf) -> Self {
        Self {
            receiver,
            base_path,
        }
    }

    /// 使用默认 sessions_dir() 作为 base_path
    pub fn with_default_path(receiver: mpsc::UnboundedReceiver<PersistenceCommand>) -> Self {
        Self {
            receiver,
            base_path: sessions_dir(),
        }
    }

    /// 启动后台持久化循环（阻塞当前任务直到 Shutdown 或 channel 关闭）
    pub async fn run(&mut self) {
        // 确保基础路径存在
        if let Err(e) = tokio::fs::create_dir_all(&self.base_path).await {
            tracing::error!(
                "PersistenceWorker: 无法创建持久化目录 {}: {}",
                self.base_path.display(),
                e
            );
            return;
        }

        loop {
            match self.receiver.recv().await {
                Some(PersistenceCommand::SaveSession {
                    session_id,
                    messages,
                    ack_tx,
                }) => {
                    let result = self.save_session(&session_id, &messages).await;
                    if let Some(tx) = ack_tx {
                        let ack = match &result {
                            Ok(count) => PersistenceAck::Ok {
                                message_count: *count,
                            },
                            Err(reason) => PersistenceAck::Failed {
                                reason: reason.clone(),
                                timestamp: crate::core::types::Timestamp::now(),
                            },
                        };
                        // ACK 发送失败仅记录日志（调用方可能已不再等待）
                        if tx.send(ack).is_err() {
                            tracing::warn!(
                                "PersistenceWorker: ACK 发送失败（调用方可能已取消等待）session={}",
                                session_id
                            );
                        }
                    }
                }
                Some(PersistenceCommand::Shutdown) => {
                    tracing::info!("PersistenceWorker: 收到 Shutdown，即将退出");
                    break;
                }
                None => {
                    // channel 已关闭，退出
                    tracing::info!("PersistenceWorker: channel 已关闭，退出");
                    break;
                }
            }
        }
    }

    /// 保存单个会话：序列化 → 写入 .tmp → rename .json
    async fn save_session(&self, session_id: &str, messages: &[Message]) -> Result<usize, String> {
        let safe_name = sanitize_session_filename(session_id);
        if safe_name.is_empty() {
            return Err(format!(
                "会话 ID 清理后为空（原始: '{}'），拒绝写入",
                session_id
            ));
        }

        let json_path = self.base_path.join(format!("{}.json", safe_name));
        let tmp_path = self.base_path.join(format!("{}.json.tmp", safe_name));

        // 1. 序列化消息为 JSON
        let json = serde_json::to_string_pretty(messages)
            .map_err(|e| format!("序列化会话 '{}' 失败: {}", session_id, e))?;

        // 2. 写入临时文件
        let mut f = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("创建临时文件 '{}' 失败: {}", tmp_path.display(), e))?;

        f.write_all(json.as_bytes())
            .await
            .map_err(|e| format!("写入临时文件 '{}' 失败: {}", tmp_path.display(), e))?;

        // 确保数据刷入磁盘
        f.flush()
            .await
            .map_err(|e| format!("刷新临时文件 '{}' 失败: {}", tmp_path.display(), e))?;

        // 3. 原子 rename（Windows 上目标文件存在时需先删除）
        if json_path.exists() {
            tokio::fs::remove_file(&json_path)
                .await
                .map_err(|e| format!("删除旧文件 '{}' 失败: {}", json_path.display(), e))?;
        }

        tokio::fs::rename(&tmp_path, &json_path)
            .await
            .map_err(|e| {
                format!(
                    "重命名 '{}' → '{}' 失败: {}",
                    tmp_path.display(),
                    json_path.display(),
                    e
                )
            })?;

        Ok(messages.len())
    }
}

/// 从磁盘恢复所有会话到 SharedMessageStore
///
/// 扫描 base_path 下所有 .json 文件，反序列化后写入 store。
/// 返回成功恢复的会话数量。
///
/// # 错误处理
///
/// 单个会话文件反序列化失败不影响其他会话的恢复，
/// 仅记录警告日志并跳过该文件。
pub async fn load_sessions_from_disk(
    base_path: &Path,
    store: &SharedMessageStore,
) -> Result<usize, std::io::Error> {
    // 确保目录存在
    if !base_path.exists() {
        return Ok(0);
    }

    let mut restored_count = 0;

    let mut entries = tokio::fs::read_dir(base_path).await?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();

        // 仅处理 .json 文件，跳过 .tmp 文件
        if path.extension().map(|e| e == "json") != Some(true) {
            continue;
        }

        // 从文件名提取 session_id（去掉 .json 后缀）
        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match serde_json::from_str::<Vec<Message>>(&content) {
                Ok(messages) => {
                    store.write(&session_id, messages).await;
                    restored_count += 1;
                }
                Err(e) => {
                    tracing::warn!("会话恢复：解析文件 '{}' 失败: {}，跳过", path.display(), e);
                }
            },
            Err(e) => {
                tracing::warn!("会话恢复：读取文件 '{}' 失败: {}，跳过", path.display(), e);
            }
        }
    }

    Ok(restored_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::persistence::PersistenceCommand;
    use crate::shared_types::{Message, MessageRole};
    use tokio::sync::mpsc;

    #[test]
    fn test_sanitize_session_filename_preserves_valid_chars() {
        let result = sanitize_session_filename("session-123_abc");
        assert_eq!(result, "session-123_abc");
    }

    #[test]
    fn test_sanitize_session_filename_replaces_special_chars() {
        let result = sanitize_session_filename("session/../etc/passwd");
        assert!(!result.contains('/'));
        assert!(!result.contains('.'));
        assert!(result.starts_with("session"));
    }

    #[test]
    fn test_sanitize_session_filename_empty_result() {
        let result = sanitize_session_filename("../../../");
        // 全部被替换为 _，但不应为空（至少有一个 _）
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_persistence_worker_save_and_load() {
        let temp_dir = std::env::temp_dir().join("aagnet_test_persistence");
        // 测试中安全：清理旧的测试数据
        let _ = std::fs::remove_dir_all(&temp_dir);

        let (tx, rx) = mpsc::unbounded_channel();
        let session_id = "test_session_001".to_string();

        // 创建测试消息
        let messages = vec![
            Message::text(MessageRole::User, "Hello"),
            Message::text(MessageRole::Assistant, "Hi there!"),
        ];

        // 通过 channel 发送保存命令（在后台运行 worker）
        let mut worker = PersistenceWorker::new(rx, temp_dir.clone());

        // 发送保存命令
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let cmd = PersistenceCommand::SaveSession {
            session_id: session_id.clone(),
            messages,
            ack_tx: Some(ack_tx),
        };
        tx.send(cmd).ok();

        // 发送 Shutdown
        tx.send(PersistenceCommand::Shutdown).ok();
        drop(tx);

        // 运行 worker
        worker.run().await;

        // 等待 ACK
        let ack = ack_rx.await;
        assert!(ack.is_ok());
        match ack.unwrap() {
            PersistenceAck::Ok { message_count } => {
                assert_eq!(message_count, 2);
            }
            PersistenceAck::Failed { reason, .. } => {
                panic!("持久化失败: {}", reason);
            }
        }

        // 验证文件存在
        let json_path = temp_dir.join("test_session_001.json");
        assert!(json_path.exists(), "JSON 文件应存在");

        // 验证 load_sessions_from_disk
        let store = SharedMessageStore::new();
        let restored = load_sessions_from_disk(&temp_dir, &store).await;
        assert!(restored.is_ok());
        assert_eq!(restored.unwrap(), 1);

        let (loaded_msgs, _) = store.read("test_session_001").await;
        assert_eq!(loaded_msgs.len(), 2);

        // 清理
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
