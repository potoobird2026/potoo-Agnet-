/*! ExperienceExtractService —— 从压缩输出提取结构化经验 */
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceEntry {
    pub exp_type: String,
    pub content: String,
    pub trigger_condition: Option<String>,
    pub error_type: Option<String>,
    pub weight: f64,
    pub tags: Vec<String>,
}

pub struct ExperienceExtractService;

impl Default for ExperienceExtractService {
    fn default() -> Self {
        Self::new()
    }
}

impl ExperienceExtractService {
    pub fn new() -> Self {
        Self
    }
    pub fn extract(&self, summary: &str) -> Vec<ExperienceEntry> {
        let mut entries = Vec::new();
        // 简化提取：按段落分割，检测关键词
        for para in summary.split("\n\n") {
            let para = para.trim();
            if para.is_empty() {
                continue;
            }
            let exp_type = if para.contains("error") || para.contains("bug") {
                "error_fix"
            } else if para.contains("learn") || para.contains("found") {
                "learning"
            } else if para.contains("decision") {
                "decision"
            } else {
                "general"
            };
            entries.push(ExperienceEntry {
                exp_type: exp_type.into(),
                content: para.into(),
                trigger_condition: None,
                error_type: None,
                weight: 0.5,
                tags: Vec::new(),
            });
        }
        entries
    }
}
