use super::super::config::UcbConfig;
use super::super::services::UcbDecisionService;
use super::super::types::{CategoryRole, ContentType, LengthBucket};
use std::collections::HashMap;

pub struct UcbDecision {
    config: UcbConfig,
    counts: HashMap<(CategoryRole, ContentType, LengthBucket), (usize, f64)>,
}

impl UcbDecision {
    pub fn new(config: UcbConfig) -> Self {
        Self {
            config,
            counts: HashMap::new(),
        }
    }
    fn ucb_value(&self, key: &(CategoryRole, ContentType, LengthBucket)) -> f64 {
        let (n, mean) = self.counts.get(key).copied().unwrap_or((0, 0.0));
        let total: usize = self.counts.values().map(|(n, _)| n).sum();
        if n == 0 {
            return f64::INFINITY;
        }
        mean + self.config.exploration_bonus * ((total as f64).ln() / n as f64).sqrt()
    }
}

impl UcbDecisionService for UcbDecision {
    fn decide(
        &mut self,
        category: CategoryRole,
        content: ContentType,
        length: LengthBucket,
        score: f64,
    ) -> bool {
        let key = (category, content, length);
        let entry = self.counts.entry(key).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 = (entry.1 * (entry.0 - 1) as f64 + score) / entry.0 as f64;
        let ucb = self.ucb_value(&key);
        ucb > self.config.threshold_high || (ucb > self.config.threshold_low && score > 0.5)
    }
}
