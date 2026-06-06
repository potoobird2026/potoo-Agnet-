use super::super::services::FeedbackService;
use super::super::types::LossSignal;
use crate::shared_types::Message;
use std::collections::HashSet;

pub struct Feedback;

impl Default for Feedback {
    fn default() -> Self {
        Self::new()
    }
}

impl Feedback {
    pub fn new() -> Self {
        Self
    }
}

impl FeedbackService for Feedback {
    fn detect_loss(&self, before: &[Message], after: &[Message]) -> Vec<LossSignal> {
        let before_entities: HashSet<String> = before
            .iter()
            .flat_map(|m| {
                m.text_content()
                    .split_whitespace()
                    .map(|s| s.to_lowercase())
                    .collect::<Vec<_>>()
            })
            .filter(|w| w.len() > 4)
            .collect();
        let after_entities: HashSet<String> = after
            .iter()
            .flat_map(|m| {
                m.text_content()
                    .split_whitespace()
                    .map(|s| s.to_lowercase())
                    .collect::<Vec<_>>()
            })
            .filter(|w| w.len() > 4)
            .collect();
        let missing: Vec<String> = before_entities
            .difference(&after_entities)
            .cloned()
            .collect();
        if missing.is_empty() {
            vec![]
        } else {
            vec![LossSignal {
                session_id: String::new(),
                missing_info: missing,
                severity: 0.5,
            }]
        }
    }
}
