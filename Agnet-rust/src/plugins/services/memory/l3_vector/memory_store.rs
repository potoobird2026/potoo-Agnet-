/*! MemoryVectorStore —— 内存实现 */
use super::metadata::{VectorFilter, VectorMetadata, VectorStoreError};
use super::store::{VectorStore, VectorStoreStats};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

struct Entry {
    vector: Vec<f32>,
    metadata: VectorMetadata,
}

pub struct MemoryVectorStore {
    dim: usize,
    data: RwLock<HashMap<String, Entry>>,
}

impl MemoryVectorStore {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            data: RwLock::new(HashMap::new()),
        }
    }
    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let (dot, na, nb) = a
            .iter()
            .zip(b.iter())
            .fold((0.0f32, 0.0f32, 0.0f32), |(d, na, nb), (&x, &y)| {
                (d + x * y, na + x * x, nb + y * y)
            });
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            dot / (na.sqrt() * nb.sqrt())
        }
    }
}

#[async_trait]
impl VectorStore for MemoryVectorStore {
    async fn init(&self) -> Result<(), VectorStoreError> {
        Ok(())
    }
    async fn upsert(
        &self,
        items: Vec<(String, Vec<f32>, VectorMetadata)>,
    ) -> Result<(), VectorStoreError> {
        let mut data = self.data.write().map_err(|e| VectorStoreError {
            kind: "lock".into(),
            message: e.to_string(),
        })?;
        for (id, vector, metadata) in items {
            data.insert(id, Entry { vector, metadata });
        }
        Ok(())
    }
    async fn search(
        &self,
        query: &[f32],
        top_k: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<(String, f32, VectorMetadata)>, VectorStoreError> {
        let data = self.data.read().map_err(|e| VectorStoreError {
            kind: "lock".into(),
            message: e.to_string(),
        })?;
        let mut results: Vec<(String, f32, VectorMetadata)> = data
            .iter()
            .filter(|(_, e)| {
                if filter.exclude_invalid && e.metadata.is_invalid {
                    return false;
                }
                if let Some(ref dt) = filter.doc_type {
                    if e.metadata.doc_type != *dt {
                        return false;
                    }
                }
                if let Some(mw) = filter.min_weight {
                    if e.metadata.weight < mw {
                        return false;
                    }
                }
                if !filter.tags.is_empty()
                    && !filter.tags.iter().any(|t| e.metadata.tags.contains(t))
                {
                    return false;
                }
                true
            })
            .map(|(id, e)| {
                (
                    id.clone(),
                    Self::cosine_sim(query, &e.vector),
                    e.metadata.clone(),
                )
            })
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }
    async fn delete(&self, ids: &[String]) -> Result<(), VectorStoreError> {
        let mut data = self.data.write().map_err(|e| VectorStoreError {
            kind: "lock".into(),
            message: e.to_string(),
        })?;
        for id in ids {
            data.remove(id);
        }
        Ok(())
    }
    async fn stats(&self) -> Result<VectorStoreStats, VectorStoreError> {
        let data = self.data.read().map_err(|e| VectorStoreError {
            kind: "lock".into(),
            message: e.to_string(),
        })?;
        Ok(VectorStoreStats {
            total_vectors: data.len(),
            dim: self.dim,
        })
    }
}
