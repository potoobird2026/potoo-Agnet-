/*!
 * Logger ?保留策略
 *
 * 功能描述：独立的后台任务，定期扫描日志目录，
 * 删除超过保留天数的文件，或在磁盘使用超限时删旧文件? */

use std::path::PathBuf;

use tokio::time::{interval, Duration};

use super::config::RetentionPolicy;

/// Spawn the retention task.
pub fn spawn_retention(output_dir: PathBuf, policy: RetentionPolicy) {
    if policy.days == 0 && policy.max_disk_mb == 0 {
        return;
    }
    let check_interval = Duration::from_secs(3600); // every hour
    tokio::spawn(async move {
        let mut tick = interval(check_interval);
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = run_cleanup(&output_dir, &policy).await {
                tracing::warn!("logger retention cleanup failed: {}", e);
            }
        }
    });
}

async fn run_cleanup(output_dir: &PathBuf, policy: &RetentionPolicy) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(output_dir)
        .await
        .map_err(|e| format!("read dir: {}", e))?;
    let mut files = Vec::new();

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        if let Ok(meta) = tokio::fs::metadata(&path).await {
            files.push((path, meta.len(), meta.modified().ok()));
        }
    }

    // Delete by age
    if policy.days > 0 {
        let max_age = std::time::Duration::from_secs(policy.days as u64 * 86400);
        for (path, _, modified) in &files {
            if let Some(modified_time) = modified.as_ref() {
                if let Ok(age) = modified_time.elapsed() {
                    if age > max_age {
                        if let Err(e) = tokio::fs::remove_file(path).await {
                            tracing::warn!(
                                "logger retention: failed to delete {}: {}",
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    // Delete by size
    if policy.max_disk_mb > 0 {
        let total_bytes: u64 = files.iter().map(|(_, s, _)| s).sum();
        let max_bytes = policy.max_disk_mb * 1024 * 1024;
        if total_bytes > max_bytes {
            let mut sorted: Vec<_> = files
                .iter()
                .filter_map(|(p, _, m)| m.as_ref().map(|mt| (p, mt)))
                .collect();
            sorted.sort_by_key(|(_, mt)| *mt);
            let mut to_free = total_bytes - max_bytes;
            for (path, _) in sorted {
                if to_free == 0 {
                    break;
                }
                if let Ok(meta) = tokio::fs::metadata(path).await {
                    let size = meta.len();
                    let _ = tokio::fs::remove_file(path).await;
                    to_free = to_free.saturating_sub(size);
                }
            }
        }
    }

    Ok(())
}
