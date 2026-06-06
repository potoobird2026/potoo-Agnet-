use super::super::services::CompressorService;
use super::super::types::CompressResult;
use crate::shared_types::Message;
use std::time::Instant;

pub struct Compressor;

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compressor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CompressorService for Compressor {
    async fn compress(
        &self,
        session_id: &str,
        messages: &[Message],
        keep_indices: &[usize],
    ) -> Result<CompressResult, String> {
        let start = Instant::now();
        let keep_set: std::collections::HashSet<usize> = keep_indices.iter().copied().collect();
        let compressed_count = messages.len() - keep_set.len();
        // 生成摘要：保留高重要性消息的文本内容
        let summary_parts: Vec<String> = messages
            .iter()
            .enumerate()
            .filter(|(i, _)| keep_set.contains(i))
            .map(|(_, m)| m.text_content())
            .filter(|s| !s.is_empty())
            .take(10)
            .collect();
        let summary = summary_parts.join("\n---\n");
        let token_saved = messages
            .iter()
            .enumerate()
            .filter(|(i, _)| !keep_set.contains(i))
            .map(|(_, m)| m.estimate_tokens())
            .sum();
        Ok(CompressResult {
            session_id: session_id.to_string(),
            compressed_count,
            summary,
            token_saved,
            elapsed: start.elapsed(),
        })
    }
}
