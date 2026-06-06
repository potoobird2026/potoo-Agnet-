/*!
 * Chronos 配置层
 *
 * 包含 ChronosConfig（顶层配置）及所有子配置结构体。
 * resolve_paths() 展开 ~ 并处理所有子路径。
 * validate() 在 init 阶段执行，无效配置立即报错。
 */

use std::path::PathBuf;

use serde::Deserialize;

use crate::core::types::error::PluginError;

// ============================================
// ChronosConfig —— 顶层配置
// ============================================

/// Chronos 顶层配置
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ChronosConfig {
    pub timing: TimingConfig,
    pub decision: DecisionConfig,
    pub state: StateConfig,
    pub storage: StorageConfig,
    pub actions: ActionsConfig,
    pub preferences: PreferencesConfig,
    pub max_polling_interval_secs: u64,
}

impl Default for ChronosConfig {
    fn default() -> Self {
        Self {
            timing: TimingConfig::default(),
            decision: DecisionConfig::default(),
            state: StateConfig::default(),
            storage: StorageConfig::default(),
            actions: ActionsConfig::default(),
            preferences: PreferencesConfig::default(),
            max_polling_interval_secs: 300,
        }
    }
}

impl ChronosConfig {
    /// 展开所有路径中的 `~`，转换为绝对路径
    pub fn resolve_paths(&mut self) {
        self.storage.resolve_paths();
    }

    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), PluginError> {
        self.timing.validate()?;
        self.decision.validate()?;
        self.actions.validate()?;

        if self.max_polling_interval_secs == 0 {
            return Err(PluginError::Config(
                "max_polling_interval_secs 不能为 0".to_string(),
            ));
        }

        if self.max_polling_interval_secs < self.timing.polling_interval_base_secs {
            return Err(PluginError::Config(format!(
                "max_polling_interval_secs ({}) 不能小于 polling_interval_base_secs ({})",
                self.max_polling_interval_secs, self.timing.polling_interval_base_secs
            )));
        }

        Ok(())
    }
}

// ============================================
// TimingConfig —— 定时配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct TimingConfig {
    /// 基础轮询间隔秒数
    pub polling_interval_base_secs: u64,
    /// 空闲时乘数
    pub idle_multiplier: f64,
    /// 活跃时乘数
    pub active_multiplier: f64,
    /// 最大轮询间隔秒数
    pub max_interval_secs: u64,
    /// 最小轮询间隔秒数
    pub min_interval_secs: u64,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            polling_interval_base_secs: 5,
            idle_multiplier: 1.5,
            active_multiplier: 0.5,
            max_interval_secs: 300,
            min_interval_secs: 1,
        }
    }
}

impl TimingConfig {
    fn validate(&self) -> Result<(), PluginError> {
        if self.polling_interval_base_secs == 0 {
            return Err(PluginError::Config(
                "polling_interval_base_secs 不能为 0".to_string(),
            ));
        }
        if self.min_interval_secs > self.max_interval_secs {
            return Err(PluginError::Config(
                "min_interval_secs 不能大于 max_interval_secs".to_string(),
            ));
        }
        Ok(())
    }
}

// ============================================
// DecisionConfig —— 决策配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionConfig {
    /// LLM 模型名
    pub generation_llm_model: String,
    /// 生成超时秒数
    pub generation_timeout_secs: u64,
    /// 提醒模板
    pub remind_template: String,
    /// 主动发起模板
    pub proactive_template: String,
    /// 升级配置
    pub escalation: EscalationConfig,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            generation_llm_model: "gpt-4o-mini".to_string(),
            generation_timeout_secs: 30,
            remind_template: "You have pending tasks...".to_string(),
            proactive_template: "".to_string(),
            escalation: EscalationConfig::default(),
        }
    }
}

impl DecisionConfig {
    fn validate(&self) -> Result<(), PluginError> {
        if self.generation_timeout_secs == 0 {
            return Err(PluginError::Config(
                "generation_timeout_secs 不能为 0".to_string(),
            ));
        }
        if self.generation_llm_model.is_empty() {
            return Err(PluginError::Config(
                "generation_llm_model 不能为空".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EscalationConfig {
    pub timeout_secs: u64,
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self { timeout_secs: 120 }
    }
}

// ============================================
// StateConfig —— 状态配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    pub idle_threshold_minutes: u64,
    pub active_threshold_secs: u64,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            idle_threshold_minutes: 5,
            active_threshold_secs: 30,
        }
    }
}

// ============================================
// StorageConfig —— 存储配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub task_queue_file: PathBuf,
    pub sample_store_dir: PathBuf,
    pub max_samples: usize,
    pub base_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let base = dirs::home_dir()
            .unwrap_or_default()
            .join(".aagnet")
            .join("chronos");

        Self {
            task_queue_file: base.join("tasks.json"),
            sample_store_dir: base.join("samples"),
            max_samples: 1000,
            base_dir: base,
        }
    }
}

impl StorageConfig {
    fn resolve_paths(&mut self) {
        // 展开 ~ 为 home_dir
        let home = dirs::home_dir().unwrap_or_default();
        let expand_tilde = |p: &mut PathBuf| {
            if let Some(s) = p.to_str() {
                if let Some(stripped) = s.strip_prefix("~/") {
                    *p = home.join(stripped);
                }
            }
        };
        expand_tilde(&mut self.task_queue_file);
        expand_tilde(&mut self.sample_store_dir);
        expand_tilde(&mut self.base_dir);
    }
}

// ============================================
// ActionsConfig —— 动作执行配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct ActionsConfig {
    pub max_concurrent_actions: usize,
    pub action_timeout_secs: u64,
}

impl Default for ActionsConfig {
    fn default() -> Self {
        Self {
            max_concurrent_actions: 5,
            action_timeout_secs: 60,
        }
    }
}

impl ActionsConfig {
    fn validate(&self) -> Result<(), PluginError> {
        if self.max_concurrent_actions == 0 {
            return Err(PluginError::Config(
                "max_concurrent_actions 不能为 0".to_string(),
            ));
        }
        Ok(())
    }
}

// ============================================
// PreferencesConfig —— 偏好配置
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct PreferencesConfig {
    pub enabled: bool,
    pub quiet_hours_start: u8,
    pub quiet_hours_end: u8,
}

impl Default for PreferencesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            quiet_hours_start: 22,
            quiet_hours_end: 7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chronos_config_default() {
        let c = ChronosConfig::default();
        assert_eq!(c.max_polling_interval_secs, 300);
        assert_eq!(c.timing.polling_interval_base_secs, 5);
        assert!(c.preferences.enabled);
    }

    #[test]
    fn test_validate_ok() {
        let c = ChronosConfig::default();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_polling_interval() {
        let mut c = ChronosConfig::default();
        c.timing.polling_interval_base_secs = 0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_validate_min_greater_than_max() {
        let mut c = ChronosConfig::default();
        c.timing.min_interval_secs = 100;
        c.timing.max_interval_secs = 10;
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_validate_zero_generation_timeout() {
        let mut c = ChronosConfig::default();
        c.decision.generation_timeout_secs = 0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_validate_empty_model() {
        let mut c = ChronosConfig::default();
        c.decision.generation_llm_model.clear();
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_validate_zero_max_concurrent() {
        let mut c = ChronosConfig::default();
        c.actions.max_concurrent_actions = 0;
        assert!(c.validate().is_err());
    }

    #[allow(clippy::field_reassign_with_default)]
    #[test]
    fn test_validate_zero_max_polling() {
        let mut c = ChronosConfig::default();
        c.max_polling_interval_secs = 0;
        assert!(c.validate().is_err());
    }
}
