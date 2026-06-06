/*! VectorStore trait + VectorStoreStats */
use super::metadata::{VectorFilter, VectorMetadata, VectorStoreError};
use async_trait::async_trait;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VectorStoreStats {
    pub total_vectors: usize,
    pub dim: usize,
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn init(&self) -> Result<(), VectorStoreError>;
    async fn upsert(
        &self,
        items: Vec<(String, Vec<f32>, VectorMetadata)>,
    ) -> Result<(), VectorStoreError>;
    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<(String, f32, VectorMetadata)>, VectorStoreError>;
    async fn delete(&self, ids: &[String]) -> Result<(), VectorStoreError>;
    async fn stats(&self) -> Result<VectorStoreStats, VectorStoreError>;
}
