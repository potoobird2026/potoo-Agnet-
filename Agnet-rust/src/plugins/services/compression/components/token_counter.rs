use super::super::services::TokenCounterService;
use crate::shared_types::{ContentBlock, Message};

const CJK_MULTIPLIER: f64 = 1.5;
const ASCII_MULTIPLIER: f64 = 0.25;
const IMAGE_TOKENS: usize = 85;
const AUDIO_TOKENS: usize = 100;
const FILE_TOKENS: usize = 50;

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}')
}

pub struct TokenCounter;

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter {
    pub fn new() -> Self {
        Self
    }
}

impl TokenCounterService for TokenCounter {
    fn count(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| {
                m.content
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text(t) => {
                            let chars: f64 = t
                                .chars()
                                .map(|c| {
                                    if is_cjk(c) {
                                        CJK_MULTIPLIER
                                    } else {
                                        ASCII_MULTIPLIER
                                    }
                                })
                                .sum();
                            chars.ceil() as usize + 4 // +4 for message overhead
                        }
                        ContentBlock::Image { .. } => IMAGE_TOKENS,
                        ContentBlock::Audio { .. } => AUDIO_TOKENS,
                        ContentBlock::File { .. } => FILE_TOKENS,
                    })
                    .sum::<usize>()
            })
            .sum()
    }
}
