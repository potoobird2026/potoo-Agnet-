/*! FeedbackMonitor —— 隐式反馈权重闭环 */
pub struct FeedbackConfig {
    pub success_multiplier: f64,
    pub failure_multiplier: f64,
    pub weight_floor: f64,
}

pub struct FeedbackMonitor {
    config: FeedbackConfig,
}

impl FeedbackMonitor {
    pub fn new(config: FeedbackConfig) -> Self {
        Self { config }
    }
    pub fn process_feedback(&self, current_weight: f64, positive: bool) -> f64 {
        let multiplier = if positive {
            self.config.success_multiplier
        } else {
            self.config.failure_multiplier
        };
        (current_weight * multiplier).max(self.config.weight_floor)
    }
}
