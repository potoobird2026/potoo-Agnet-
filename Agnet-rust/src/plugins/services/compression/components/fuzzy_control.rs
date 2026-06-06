use super::super::config::FuzzyConfig;
use super::super::services::{FuzzyControlService, FuzzyDecision};

pub struct FuzzyControl {
    config: FuzzyConfig,
}

impl FuzzyControl {
    pub fn new(config: FuzzyConfig) -> Self {
        Self { config }
    }
}

impl FuzzyControlService for FuzzyControl {
    fn decide(&self, keep_ratio: f64) -> FuzzyDecision {
        if keep_ratio <= self.config.low_threshold {
            FuzzyDecision::Compress
        } else if keep_ratio >= self.config.high_threshold {
            FuzzyDecision::Keep
        } else {
            FuzzyDecision::Borderline
        }
    }
}
