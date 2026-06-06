/*! EmbeddingService + EmbeddingBackend */
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingBackend {
    Noop,
    OpenAI,
    LocalONNX,
}

#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
    fn dim(&self) -> usize;
}

/// Noop 嵌入模型（确定性 hash mock，用于测试）
pub struct NoopEmbeddingModel {
    dim: usize,
}
impl NoopEmbeddingModel {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}
#[async_trait]
impl EmbeddingModel for NoopEmbeddingModel {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut hasher = DefaultHasher::new();
                t.hash(&mut hasher);
                let hash = hasher.finish();
                (0..self.dim)
                    .map(|i| ((hash >> (i % 64)) & 1) as f32 * 2.0 - 1.0)
                    .collect()
            })
            .collect())
    }
    fn dim(&self) -> usize {
        self.dim
    }
}

pub struct EmbeddingService {
    model: Box<dyn EmbeddingModel>,
}
impl EmbeddingService {
    pub fn new(model: Box<dyn EmbeddingModel>) -> Self {
        Self { model }
    }
    pub async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.model.embed(texts).await
    }
    pub fn dim(&self) -> usize {
        self.model.dim()
    }
}
