use super::super::config::ScoringConfig;
use super::super::services::ScorerService;
use crate::shared_types::Message;

pub struct Scorer {
    config: ScoringConfig,
}

impl Scorer {
    pub fn new(config: ScoringConfig) -> Self {
        Self { config }
    }
}

impl ScorerService for Scorer {
    fn score(
        &self,
        msg: &Message,
        entropy: f64,
        entities: &[String],
        pos: usize,
        total: usize,
    ) -> f64 {
        let text = msg.text_content();
        let entity_score = if entities.iter().any(|e| text.contains(e.as_str())) {
            1.0
        } else {
            0.0
        };
        let position_score = if total > 1 {
            1.0 - (pos as f64 / (total - 1) as f64)
        } else {
            0.5
        };
        let reference_score = if text.contains('@') || text.contains("ref") {
            1.0
        } else {
            0.0
        };
        self.config.weight_entropy * entropy
            + self.config.weight_entity * entity_score
            + self.config.weight_position * position_score
            + self.config.weight_reference * reference_score
    }
}
