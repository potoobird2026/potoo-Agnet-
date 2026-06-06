/*! CleanupService —— L3 向量垃圾回收（weight/age 标准 + 后台循环） */
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::metadata::{VectorFilter, VectorStoreError};
use super::store::VectorStore;

pub struct CleanupService {
    store: Arc<dyn VectorStore>,
    running: Arc<AtomicBool>,
    interval_secs: u64,
    min_weight: f64,
    max_age_days: u64,
}

impl Clone for CleanupService {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            running: self.running.clone(),
            interval_secs: self.interval_secs,
            min_weight: self.min_weight,
            max_age_days: self.max_age_days,
        }
    }
}

impl CleanupService {
    pub fn new(store: Arc<dyn VectorStore>) -> Self {
        Self {
            store,
            running: Arc::new(AtomicBool::new(false)),
            interval_secs: 600,
            min_weight: 0.05,
            max_age_days: 365,
        }
    }

    /// B-4: 配置清理参数
    pub fn set_params(&mut self, min_weight: f64, max_age_days: u64, interval_secs: u64) {
        self.min_weight = min_weight;
        self.max_age_days = max_age_days;
        self.interval_secs = interval_secs;
    }

    /// B-4: 启动后台清理循环
    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);
        let store = self.store.clone();
        let running = self.running.clone();
        let interval = self.interval_secs;
        let min_weight = self.min_weight;
        let max_age_days = self.max_age_days;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(interval));
            tick.tick().await;
            while running.load(Ordering::SeqCst) {
                tick.tick().await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                match cleanup_with_params(&store, min_weight, max_age_days).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("L3 cleanup: 清理 {} 条向量", n),
                    Err(e) => tracing::warn!("L3 cleanup 失败: {}", e),
                }
            }
            tracing::debug!("CleanupService: 后台循环已退出");
        });
    }

    /// B-4: 停止后台循环
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// 清理 is_invalid=true 的向量（兼容旧调用）
    pub async fn cleanup(&self) -> Result<usize, VectorStoreError> {
        cleanup_with_params(&self.store, self.min_weight, self.max_age_days).await
    }
}

/// B-4: 清理逻辑（is_invalid + weight < min_weight + age > max_age_days）
async fn cleanup_with_params(
    store: &Arc<dyn VectorStore>,
    min_weight: f64,
    max_age_days: u64,
) -> Result<usize, VectorStoreError> {
    let all = store
        .search(&[0.0; 1], 100000, &VectorFilter::default())
        .await?;
    let now = chrono::Utc::now();
    let invalid_ids: Vec<String> = all
        .iter()
        .filter(|(_, _, m)| {
            if m.is_invalid {
                return true;
            }
            if m.weight < min_weight {
                return true;
            }
            if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&m.last_accessed) {
                let age = now.signed_duration_since(last);
                if age.num_days() > max_age_days as i64 {
                    return true;
                }
            }
            false
        })
        .map(|(id, _, _)| id.clone())
        .collect();
    let count = invalid_ids.len();
    if !invalid_ids.is_empty() {
        store.delete(&invalid_ids).await?;
    }
    Ok(count)
}
