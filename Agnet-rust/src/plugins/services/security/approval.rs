/*!
 * ApprovalService —— 审批管理服务
 *
 * 管理待审批 (pending) 和已完成 (completed) 的审批请求。
 * 支持审批超时、oneshot 通道响应、GC 定期清理。
 */

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{oneshot, RwLock};
use uuid::Uuid;

use super::config::ApprovalConfig;
use super::models::{SecurityError, SecurityErrorKind};

// ============================================
// 审批决策
// ============================================

/// 审批响应决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// 本次允许
    AllowOnce,
    /// 总是允许（类似 remember）
    AllowAlways,
    /// 拒绝
    Deny,
}

// ============================================
// 待审批记录
// ============================================

/// 待审批操作
#[derive(Debug)]
pub struct PendingApproval {
    /// 审批 ID（UUID）
    pub id: String,
    /// 工具名称
    pub tool_name: String,
    /// 审批提示
    pub prompt: String,
    /// 创建时间戳
    pub created_at: crate::core::types::Timestamp,
    /// 超时时刻
    pub deadline: crate::core::types::Timestamp,
    /// oneshot 发送端（用于向等待方发送决策）
    pub tx: tokio::sync::Mutex<Option<oneshot::Sender<ApprovalDecision>>>,
}

// ============================================
// 已完成审批记录
// ============================================

/// 已完成审批记录
#[derive(Debug, Clone)]
pub struct CompletedApproval {
    /// 审批 ID
    pub id: String,
    /// 工具名称
    pub tool_name: String,
    /// 审批决策
    pub decision: ApprovalDecision,
    /// 完成时间戳
    pub completed_at: crate::core::types::Timestamp,
}

// ============================================
// ApprovalReceiver —— 调用方持有的接收端
// ============================================

/// 审批接收器（调用方持有，等待审批结果）
pub struct ApprovalReceiver {
    /// oneshot 接收端
    rx: oneshot::Receiver<ApprovalDecision>,
    /// 审批 ID（用于日志）
    id: String,
    /// 超时 duration
    timeout: Duration,
}

impl ApprovalReceiver {
    /// 等待审批结果
    ///
    /// 返回：
    /// - Ok(decision): 收到审批决策
    /// - Err(SecurityError): 超时或通道关闭
    pub async fn wait(self) -> Result<ApprovalDecision, SecurityError> {
        match tokio::time::timeout(self.timeout, self.rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => Err(SecurityError {
                kind: SecurityErrorKind::ApprovalCancelled,
                description: format!("审批 '{}' 的发送端已关闭（审批服务已关闭）", self.id),
                recommendation: Some("请重试操作，或检查审批服务状态".to_string()),
            }),
            Err(_) => Err(SecurityError {
                kind: SecurityErrorKind::ApprovalTimeout,
                description: format!("审批 '{}' 超时（{}s）", self.id, self.timeout.as_secs()),
                recommendation: Some("请重新发起操作并尽快确认审批".to_string()),
            }),
        }
    }
}

// ============================================
// ApprovalService
// ============================================

/// 审批服务
pub struct ApprovalService {
    pending: Arc<RwLock<HashMap<String, PendingApproval>>>,
    completed: Arc<RwLock<Vec<CompletedApproval>>>,
    config: ApprovalConfig,
    gc_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
    accepting_new: RwLock<bool>,
}

impl ApprovalService {
    pub fn new(config: ApprovalConfig) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            completed: Arc::new(RwLock::new(Vec::new())),
            config,
            gc_handle: RwLock::new(None),
            accepting_new: RwLock::new(true),
        }
    }

    /// 发起审批请求
    ///
    /// 返回 ApprovalReceiver 供调用方等待审批结果。
    pub async fn request_approval(
        &self,
        tool_name: &str,
        prompt: &str,
        timeout_secs: Option<u64>,
    ) -> Result<ApprovalReceiver, SecurityError> {
        // 检查是否接受新审批
        if !*self.accepting_new.read().await {
            return Err(SecurityError {
                kind: SecurityErrorKind::Internal,
                description: "审批服务已暂停接受新请求".to_string(),
                recommendation: Some("请等待服务恢复或稍后重试".to_string()),
            });
        }

        let timeout = timeout_secs.unwrap_or(self.config.default_timeout_secs);
        let timeout_dur = Duration::from_secs(timeout);
        let now = crate::core::types::Timestamp::now();

        // 检查 pending 数量
        let pending_count = self.pending.read().await.len();
        if pending_count >= self.config.max_pending {
            return Err(SecurityError {
                kind: SecurityErrorKind::Internal,
                description: format!(
                    "待审批队列已满（当前 {}，上限 {}），拒绝新审批请求",
                    pending_count, self.config.max_pending
                ),
                recommendation: Some("请等待部分审批完成后重试".to_string()),
            });
        }

        // 生成 UUID 审批 ID
        let approval_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        let pending = PendingApproval {
            id: approval_id.clone(),
            tool_name: tool_name.to_string(),
            prompt: prompt.to_string(),
            created_at: now,
            deadline: crate::core::types::Timestamp::from_millis(
                now.as_millis() + (timeout * 1000) as i64,
            ),
            tx: tokio::sync::Mutex::new(Some(tx)),
        };

        self.pending
            .write()
            .await
            .insert(approval_id.clone(), pending);

        Ok(ApprovalReceiver {
            rx,
            id: approval_id,
            timeout: timeout_dur,
        })
    }

    /// 响应审批请求
    ///
    /// 管理员/CLI 调用此方法对某个待审批操作做出决策。
    pub async fn respond(
        &self,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), SecurityError> {
        let mut pending_map = self.pending.write().await;

        let pending = pending_map
            .remove(approval_id)
            .ok_or_else(|| SecurityError {
                kind: SecurityErrorKind::Internal,
                description: format!(
                    "审批 '{}' 不存在或已完成（可能重复响应或已超时）",
                    approval_id
                ),
                recommendation: Some("请确认审批 ID 正确".to_string()),
            })?;

        // 发送决策（通过 oneshot channel）
        let mut tx_guard = pending.tx.lock().await;
        if let Some(tx) = tx_guard.take() {
            if tx.send(decision).is_err() {
                // 接收端已关闭（可能已超时），仅记录
                tracing::warn!("审批 '{}' 的接收端已关闭，决策未能送达", approval_id);
            }
        }

        // 记录到 completed
        let completed = CompletedApproval {
            id: approval_id.to_string(),
            tool_name: pending.tool_name,
            decision,
            completed_at: crate::core::types::Timestamp::now(),
        };
        self.completed.write().await.push(completed);

        Ok(())
    }

    /// 启动 GC 后台任务
    ///
    /// 定期清理超时的 pending 审批和过期的 completed 记录。
    pub async fn start_gc(self: &Arc<Self>) {
        let self_clone = Arc::clone(self);
        let gc_interval = Duration::from_secs(self.config.gc_interval_secs);
        let max_age = Duration::from_secs(self.config.completed_max_age_secs);
        let max_count = self.config.completed_max_count;

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(gc_interval);
            // 第一次不立即触发
            interval.tick().await;

            loop {
                interval.tick().await;
                self_clone.run_gc_cycle(max_age, max_count).await;
            }
        });

        *self.gc_handle.write().await = Some(handle);
    }

    /// 停止 GC 任务
    pub async fn stop_gc(&self) {
        if let Some(handle) = self.gc_handle.write().await.take() {
            handle.abort();
        }
    }

    /// 执行一次 GC 循环
    async fn run_gc_cycle(&self, max_age: Duration, max_count: usize) {
        let now = crate::core::types::Timestamp::now();

        // 1. 清理超时的 pending
        {
            let mut pending_map = self.pending.write().await;
            let timeout_ids: Vec<String> = pending_map
                .iter()
                .filter(|(_, p)| now > p.deadline)
                .map(|(id, _)| id.clone())
                .collect();

            for id in &timeout_ids {
                if let Some(pending) = pending_map.remove(id) {
                    // 关闭 channel（接收端会收到超时错误）
                    let mut tx_guard = pending.tx.lock().await;
                    drop(tx_guard.take());
                }
            }

            if !timeout_ids.is_empty() {
                tracing::info!(
                    "ApprovalService GC: 清理了 {} 个超时的 pending 审批",
                    timeout_ids.len()
                );
            }
        }

        // 2. 清理过期的 completed（> max_age）
        // 3. 截断超量的 completed（> max_count）
        {
            let mut completed_list = self.completed.write().await;
            let original_len = completed_list.len();

            completed_list.retain(|c| {
                let age = now.duration_since(c.completed_at);
                age < max_age
            });

            let after_age_clean = completed_list.len();

            if completed_list.len() > max_count {
                // 保留最新的 max_count 条
                completed_list.sort_by_key(|c| c.completed_at);
                let excess = completed_list.len() - max_count;
                completed_list.drain(0..excess);
            }

            let after_count_clean = completed_list.len();
            if original_len != after_count_clean {
                tracing::info!(
                    "ApprovalService GC: completed 记录 {} → {}（年龄清理 {}，数量清理 {}）",
                    original_len,
                    after_count_clean,
                    original_len - after_age_clean,
                    after_age_clean - after_count_clean
                );
            }
        }
    }

    /// 设置是否接受新审批请求
    pub async fn set_accepting_new(&self, accept: bool) {
        *self.accepting_new.write().await = accept;
    }

    /// 清除所有 pending 审批（用于 shutdown）
    pub async fn clear_all_pending(&self) {
        let mut pending_map = self.pending.write().await;
        for (_, pending) in pending_map.drain() {
            let mut tx_guard = pending.tx.lock().await;
            drop(tx_guard.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_request_and_respond_normal_flow() {
        let config = ApprovalConfig::default();
        let service = ApprovalService::new(config);

        let receiver = service
            .request_approval("test_tool", "Are you sure?", Some(5))
            .await;
        assert!(receiver.is_ok());

        let receiver = receiver.unwrap();
        let approval_id = receiver.id.clone();

        // 管理员响应
        let respond_result = service
            .respond(&approval_id, ApprovalDecision::AllowOnce)
            .await;
        assert!(respond_result.is_ok());

        // 等待接收端
        let decision = receiver.wait().await;
        assert!(decision.is_ok());
        assert_eq!(decision.unwrap(), ApprovalDecision::AllowOnce);
    }

    #[tokio::test]
    async fn test_approval_timeout() {
        let config = ApprovalConfig::default();
        let service = ApprovalService::new(config);

        let receiver = service
            .request_approval("test_tool", "Are you sure?", Some(1))
            .await
            .unwrap();

        // 不响应，等待超时
        let decision = receiver.wait().await;
        assert!(decision.is_err());
        assert_eq!(
            decision.unwrap_err().kind,
            SecurityErrorKind::ApprovalTimeout
        );
    }

    #[tokio::test]
    async fn test_duplicate_response_error() {
        let config = ApprovalConfig::default();
        let service = ApprovalService::new(config);

        let receiver = service
            .request_approval("test_tool", "Are you sure?", Some(5))
            .await
            .unwrap();

        let approval_id = receiver.id.clone();
        service
            .respond(&approval_id, ApprovalDecision::AllowOnce)
            .await
            .ok();

        // 重复响应应失败
        let dup = service.respond(&approval_id, ApprovalDecision::Deny).await;
        assert!(dup.is_err());
    }

    #[allow(clippy::field_reassign_with_default)]
    #[tokio::test]
    async fn test_max_pending_enforcement() {
        let mut config = ApprovalConfig::default();
        config.max_pending = 2;
        let service = Arc::new(ApprovalService::new(config));

        // 填满 pending
        let _r1 = service
            .request_approval("t1", "prompt1", Some(30))
            .await
            .unwrap();
        let _r2 = service
            .request_approval("t2", "prompt2", Some(30))
            .await
            .unwrap();

        // 第3个应被拒绝
        let r3 = service.request_approval("t3", "prompt3", Some(30)).await;
        assert!(r3.is_err());
    }
}
