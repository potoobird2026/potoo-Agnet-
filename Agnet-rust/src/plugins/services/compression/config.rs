/*! Compression 配置 */
use crate::core::types::error::PluginError;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CompressionConfig {
    pub pid: PidConfig,
    pub cold_start: ColdStartConfig,
    pub anchor: AnchorConfig,
    pub ucb: UcbConfig,
    pub fuzzy: FuzzyConfig,
    pub scoring: ScoringConfig,
    pub summary_temperature: f64,
    pub summary_max_tokens: u32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            pid: PidConfig::default(),
            cold_start: ColdStartConfig::default(),
            anchor: AnchorConfig::default(),
            ucb: UcbConfig::default(),
            fuzzy: FuzzyConfig::default(),
            scoring: ScoringConfig::default(),
            summary_temperature: 0.3,
            summary_max_tokens: 1024,
        }
    }
}

impl CompressionConfig {
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.pid.kp < 0.0 || self.pid.ki < 0.0 || self.pid.kd < 0.0 {
            return Err(PluginError::Config("PID 系数不能为负".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PidConfig {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub setpoint: f64,
    pub deadband: f64,
}
impl Default for PidConfig {
    fn default() -> Self {
        Self {
            kp: 0.5,
            ki: 0.1,
            kd: 0.05,
            setpoint: 0.5,
            deadband: 0.05,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColdStartConfig {
    pub collect_messages: usize,
    pub min_rounds: usize,
}
impl Default for ColdStartConfig {
    fn default() -> Self {
        Self {
            collect_messages: 50,
            min_rounds: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnchorConfig {
    pub anchor_min: f64,
    pub anchor_max: f64,
    pub window_ratio: f64,
}
impl Default for AnchorConfig {
    fn default() -> Self {
        Self {
            anchor_min: 0.1,
            anchor_max: 0.9,
            window_ratio: 0.3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UcbConfig {
    pub threshold_high: f64,
    pub threshold_low: f64,
    pub exploration_bonus: f64,
}
impl Default for UcbConfig {
    fn default() -> Self {
        Self {
            threshold_high: 0.8,
            threshold_low: 0.3,
            exploration_bonus: 2.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FuzzyConfig {
    pub low_threshold: f64,
    pub high_threshold: f64,
}
impl Default for FuzzyConfig {
    fn default() -> Self {
        Self {
            low_threshold: 0.3,
            high_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScoringConfig {
    pub weight_entropy: f64,
    pub weight_entity: f64,
    pub weight_position: f64,
    pub weight_reference: f64,
}
impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            weight_entropy: 0.3,
            weight_entity: 0.25,
            weight_position: 0.25,
            weight_reference: 0.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_config_default() {
        let c = CompressionConfig::default();
        assert_eq!(c.summary_temperature, 0.3);
        assert_eq!(c.summary_max_tokens, 1024);
        assert_eq!(c.pid.kp, 0.5);
    }

    #[test]
    fn test_validate_ok() {
        let c = CompressionConfig::default();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_validate_negative_pid() {
        let mut c = CompressionConfig::default();
        c.pid.kp = -1.0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_pid_config_default() {
        let p = PidConfig::default();
        assert_eq!(p.kp, 0.5);
        assert_eq!(p.ki, 0.1);
        assert_eq!(p.kd, 0.05);
    }

    #[test]
    fn test_cold_start_config_default() {
        let c = ColdStartConfig::default();
        assert_eq!(c.collect_messages, 50);
        assert_eq!(c.min_rounds, 5);
    }

    #[test]
    fn test_scoring_weights_sum() {
        let s = ScoringConfig::default();
        let sum = s.weight_entropy + s.weight_entity + s.weight_position + s.weight_reference;
        assert!((sum - 1.0).abs() < f64::EPSILON);
    }
}
