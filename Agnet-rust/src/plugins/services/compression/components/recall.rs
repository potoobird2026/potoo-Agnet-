use super::super::services::{RecallAction, RecallService};
use super::super::types::LossSignal;

pub struct Recall;

impl Default for Recall {
    fn default() -> Self {
        Self::new()
    }
}

impl Recall {
    pub fn new() -> Self {
        Self
    }
}

impl RecallService for Recall {
    fn recall(&self, signals: &[LossSignal]) -> RecallAction {
        if signals.is_empty() {
            RecallAction::None
        } else if signals.iter().any(|s| s.severity > 0.7) {
            RecallAction::RequestFullHistory
        } else {
            RecallAction::Restore {
                message_ids: vec![],
            }
        }
    }
}
