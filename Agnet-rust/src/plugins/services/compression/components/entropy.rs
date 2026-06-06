use super::super::services::EntropyService;
use crate::shared_types::Message;
use std::collections::HashMap;

pub struct Entropy;

impl Default for Entropy {
    fn default() -> Self {
        Self::new()
    }
}

impl Entropy {
    pub fn new() -> Self {
        Self
    }
}

impl EntropyService for Entropy {
    fn calculate(&self, messages: &[Message]) -> f64 {
        if messages.is_empty() {
            return 0.0;
        }
        let mut freq: HashMap<char, usize> = HashMap::new();
        let mut total = 0usize;
        for msg in messages {
            for c in msg.text_content().chars().filter(|c| c.is_alphanumeric()) {
                *freq.entry(c.to_ascii_lowercase()).or_insert(0) += 1;
                total += 1;
            }
        }
        if total == 0 {
            return 0.0;
        }
        let entropy: f64 = freq
            .values()
            .map(|&count| {
                let p = count as f64 / total as f64;
                -p * p.log2()
            })
            .sum();
        entropy
    }
}
