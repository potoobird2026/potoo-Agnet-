use super::super::config::AnchorConfig;
use super::super::services::AnchorService;
use super::super::types::PidPhase;

pub struct Anchor {
    config: AnchorConfig,
}

impl Anchor {
    pub fn new(config: AnchorConfig) -> Self {
        Self { config }
    }
}

impl AnchorService for Anchor {
    fn calculate(&self, total: usize, phase: PidPhase) -> (usize, usize) {
        if total == 0 {
            return (0, 0);
        }
        let ratio = if matches!(phase, PidPhase::ColdStart) {
            0.5
        } else {
            self.config.window_ratio
        };
        let window = ((total as f64) * ratio).ceil() as usize;
        let start = ((total.saturating_sub(window)) as f64 * self.config.anchor_min) as usize;
        let end = (start + window).min(total);
        (start, end)
    }
}
