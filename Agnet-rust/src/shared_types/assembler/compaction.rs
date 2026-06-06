/*! CompactionConfig 文档压缩器配置（设计文档 §3.5） */

/// DocumentCompactor 配置（设计文档 §3.5）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompactionConfig {
    pub chars_per_token: f64,
    pub preserve_unique_entities: bool,
    pub min_sentences_for_compaction: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            chars_per_token: 4.0,
            preserve_unique_entities: true,
            min_sentences_for_compaction: 3,
        }
    }
}
