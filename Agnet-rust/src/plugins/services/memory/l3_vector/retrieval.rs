/*! RetrievalService —— 混合检索 */
use super::embedding::EmbeddingService;
use super::metadata::VectorFilter;
use super::rrf::RRFFusion;
use super::store::VectorStore;

pub struct RetrievalService {
    store: std::sync::Arc<dyn VectorStore>,
    embedder: std::sync::Arc<EmbeddingService>,
    rrf: RRFFusion,
}

impl Clone for RetrievalService {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            embedder: self.embedder.clone(),
            rrf: self.rrf.clone(),
        }
    }
}

impl RetrievalService {
    pub fn new(
        store: std::sync::Arc<dyn VectorStore>,
        embedder: std::sync::Arc<EmbeddingService>,
    ) -> Self {
        Self {
            store,
            embedder,
            rrf: RRFFusion::new(Default::default()),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<(String, f32, super::metadata::VectorMetadata)>, String> {
        let vecs = self.embedder.embed(&[query.to_string()]).await?;
        let query_vec = vecs.first().cloned().unwrap_or_default();
        self.store
            .search(&query_vec, top_k, filter)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        top_k: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<(String, f32, super::metadata::VectorMetadata)>, String> {
        let semantic = self.search(query, top_k * 2, filter).await?;
        // 简化的 BM25 关键词匹配（基于文本包含度）
        let text_results: Vec<(String, f32)> = semantic
            .iter()
            .map(|(id, _, meta)| {
                let score = if meta.text.to_lowercase().contains(&query.to_lowercase()) {
                    1.0
                } else {
                    0.0
                };
                (id.clone(), score)
            })
            .collect();
        let semantic_scores: Vec<(String, f32)> =
            semantic.iter().map(|(id, s, _)| (id.clone(), *s)).collect();
        let merged = self.rrf.merge(&[semantic_scores, text_results]);
        // 重新从 store 获取完整元数据
        let mut results = Vec::new();
        for (id, score) in merged.iter().take(top_k) {
            if let Some((_, _, meta)) = semantic.iter().find(|(sid, _, _)| sid == id) {
                results.push((id.clone(), *score as f32, meta.clone()));
            }
        }
        Ok(results)
    }
}
