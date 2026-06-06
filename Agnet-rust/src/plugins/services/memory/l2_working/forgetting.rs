/*! ForgettingService —— PID 遗忘控制 */
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::config::ForgettingConfig;
use super::manager::{MemoryFileType, WorkingMemoryManager};

const SCORE_CACHE_FILE: &str = ".forgetting_score.json";
const NO_TAG_HEAT_SCORE: f64 = 0.5;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ForgettingScore {
    integral: f64,
    prev_error: f64,
    file_scores: HashMap<String, f64>,
}

pub struct ForgettingService {
    config: ForgettingConfig,
    cache_path: PathBuf,
    scores: ForgettingScore,
    cycle: u64,
}

impl ForgettingService {
    pub fn new(config: ForgettingConfig, workspace_dir: &Path) -> Self {
        let cache_path = workspace_dir.join(SCORE_CACHE_FILE);
        Self {
            config,
            cache_path,
            scores: ForgettingScore {
                integral: 0.0,
                prev_error: 0.0,
                file_scores: HashMap::new(),
            },
            cycle: 0,
        }
    }

    pub fn load_cache(&mut self) {
        if let Ok(content) = fs::read_to_string(&self.cache_path) {
            if let Ok(cached) = serde_json::from_str::<ForgettingScore>(&content) {
                self.scores = cached;
            }
        }
    }

    pub fn save_cache(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.scores) {
            let _ = fs::write(&self.cache_path, json);
        }
    }

    /// 执行一次遗忘扫描（返回退役和深度删除的文件路径列表）
    pub fn run(&mut self, manager: &mut WorkingMemoryManager) -> (Vec<PathBuf>, Vec<PathBuf>) {
        self.cycle += 1;
        self.load_cache();
        let now = Utc::now();
        let mut retired = Vec::new();
        let mut deep_deleted = Vec::new();

        for file in manager
            .active_files()
            .iter()
            .filter(|f| f.file_type != MemoryFileType::Archive)
        {
            let path_key = file.path.to_string_lossy().to_string();
            let current_weight = file.frontmatter.weight;

            // 保护：近期访问不过期
            if let Ok(la) = DateTime::parse_from_rfc3339(&file.frontmatter.last_accessed) {
                let age_days = (now - la.with_timezone(&Utc)).num_days();
                if age_days < self.config.access_protection_days as i64 {
                    continue;
                }
            }

            // 标签补偿
            let tag_bonus = if file.frontmatter.tags.is_empty() {
                NO_TAG_HEAT_SCORE
            } else {
                0.0
            };
            let target = (self.config.threshold_min + self.config.threshold_max) / 2.0 + tag_bonus;
            let error = target - current_weight;
            self.scores.integral += error;
            let derivative = error - self.scores.prev_error;
            let pid = self.config.pid_kp * error
                + self.config.pid_ki * self.scores.integral
                + self.config.pid_kd * derivative;
            self.scores.prev_error = error;

            let new_weight = (current_weight - pid).max(self.config.weight_floor);
            self.scores.file_scores.insert(path_key.clone(), new_weight);

            // 决策：退役
            if new_weight < self.config.threshold_min {
                retired.push(file.path.clone());
            }
            // 决策：深度删除
            if let Ok(created) = DateTime::parse_from_rfc3339(&file.frontmatter.created) {
                let age_days = (now - created.with_timezone(&Utc)).num_days();
                if age_days > self.config.deep_delete_age_days as i64
                    && new_weight < self.config.deep_delete_weight
                {
                    deep_deleted.push(file.path.clone());
                }
            }
        }

        // 退役：移动到 archive/
        let archive_dir = manager.archive_dir();
        let _ = fs::create_dir_all(&archive_dir);
        for path in &retired {
            if let Some(name) = path.file_name() {
                let dest = archive_dir.join(name);
                let _ = fs::rename(path, &dest);
            }
        }

        // 深度删除
        for path in &deep_deleted {
            let _ = fs::remove_file(path);
        }

        self.save_cache();

        // 重建索引
        if !retired.is_empty() || !deep_deleted.is_empty() {
            let _ = manager.rebuild_index();
        }

        (retired, deep_deleted)
    }

    /// 应用反馈调整权重
    pub fn apply_feedback(&mut self, file_path: &str, positive: bool) {
        let key = file_path.to_string();
        if let Some(weight) = self.scores.file_scores.get_mut(&key) {
            let multiplier = if positive {
                self.config.feedback_success_multiplier
            } else {
                self.config.feedback_failure_multiplier
            };
            *weight =
                (*weight * multiplier).clamp(self.config.weight_floor, self.config.threshold_max);
        }
        self.save_cache();
    }

    pub fn cycle_count(&self) -> u64 {
        self.cycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_formula() {
        let config = ForgettingConfig::default();
        let ws = std::env::temp_dir().join("test_forgetting");
        let svc = ForgettingService::new(config.clone(), &ws);
        assert!((config.pid_kp - 0.02).abs() < 0.001);
        assert!((config.pid_ki - 0.005).abs() < 0.001);
        assert!(svc.cycle_count() == 0);
    }

    #[test]
    fn test_feedback_multipliers() {
        let config = ForgettingConfig::default();
        let ws = std::env::temp_dir().join("test_feedback");
        let mut svc = ForgettingService::new(config, &ws);
        svc.scores.file_scores.insert("test".into(), 0.5);
        svc.apply_feedback("test", true);
        assert!(*svc.scores.file_scores.get("test").unwrap() > 0.5);
        svc.apply_feedback("test", false);
        assert!(*svc.scores.file_scores.get("test").unwrap() < 0.55);
    }
}
