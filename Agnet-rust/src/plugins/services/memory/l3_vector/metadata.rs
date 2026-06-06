/*! VectorMetadata / VectorFilter / VectorStoreError */
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMetadata {
    pub source_doc_id: String,
    #[serde(default)]
    pub section_title: String,
    pub text: String,
    pub weight: f64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub doc_type: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_accessed: String,
    #[serde(default)]
    pub access_count: u64,
    #[serde(default)]
    pub is_invalid: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VectorFilter {
    pub doc_type: Option<String>,
    pub min_weight: Option<f64>,
    pub tags: Vec<String>,
    pub exclude_invalid: bool,
}

#[derive(Debug, Clone)]
pub struct VectorStoreError {
    pub kind: String,
    pub message: String,
}
impl std::fmt::Display for VectorStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind, self.message)
    }
}
impl std::error::Error for VectorStoreError {}
