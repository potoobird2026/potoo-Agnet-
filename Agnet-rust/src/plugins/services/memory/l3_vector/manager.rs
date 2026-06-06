/*! VectorStoreManager —— L3 统一管理入口 */
use super::super::config::L3Config;
use super::chunker::TextChunker;
use super::cleanup::CleanupService;
use super::embedding::{EmbeddingService, NoopEmbeddingModel};
use super::memory_store::MemoryVectorStore;
use super::metadata::VectorFilter;
use super::retrieval::RetrievalService;
use super::store::VectorStore;
use super::sync::VectorSyncService;
use crate::shared_types::{VectorError, VectorMemoryContract, VectorSearchHit, VectorStats};
use async_trait::async_trait;
use std::sync::Arc;

pub struct VectorStoreManager {
    pub store: Arc<dyn VectorStore>,
    pub embedder: Arc<EmbeddingService>,
    pub chunker: TextChunker,
    pub retrieval: RetrievalService,
    pub sync: VectorSyncService,
    pub cleanup: CleanupService,
}

impl Clone for VectorStoreManager {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            embedder: self.embedder.clone(),
            chunker: self.chunker.clone(),
            retrieval: self.retrieval.clone(),
            sync: self.sync.clone(),
            cleanup: self.cleanup.clone(),
        }
    }
}

impl VectorStoreManager {
    pub fn new(config: &L3Config) -> Self {
        let store: Arc<dyn VectorStore> = Arc::new(MemoryVectorStore::new(config.embedding.dim));
        let embedder = Arc::new(EmbeddingService::new(Box::new(NoopEmbeddingModel::new(
            config.embedding.dim,
        ))));
        let chunker = TextChunker::new(config.chunking.clone());
        let retrieval = RetrievalService::new(store.clone(), embedder.clone());
        let sync = VectorSyncService::new(store.clone(), chunker.clone(), embedder.clone());
        let cleanup = CleanupService::new(store.clone());
        Self {
            store,
            embedder,
            chunker,
            retrieval,
            sync,
            cleanup,
        }
    }

    /// B-2: 初始化 store + 启动后台 sync/cleanup
    pub async fn init(&mut self) -> Result<(), super::metadata::VectorStoreError> {
        self.store.init().await?;
        self.sync.start();
        self.cleanup.start();
        Ok(())
    }

    /// B-2: 停止后台任务
    pub fn shutdown(&mut self) {
        self.sync.stop();
        self.cleanup.stop();
    }
}

#[async_trait]
impl VectorMemoryContract for VectorStoreManager {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<VectorSearchHit>, VectorError> {
        let filter = VectorFilter::default();
        match self.retrieval.search(query, top_k, &filter).await {
            Ok(hits) => Ok(hits
                .into_iter()
                .map(|(id, score, meta)| VectorSearchHit {
                    id,
                    score,
                    text: meta.text,
                    source: meta.source_doc_id,
                })
                .collect()),
            Err(e) => Err(VectorError::SearchFailed(e)),
        }
    }

    async fn upsert(
        &self,
        id: &str,
        text: &str,
        _metadata: serde_json::Value,
    ) -> Result<(), VectorError> {
        let meta = super::metadata::VectorMetadata {
            source_doc_id: id.to_string(),
            section_title: String::new(),
            text: text.to_string(),
            weight: 1.0,
            tags: vec![],
            doc_type: "default".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            last_accessed: chrono::Utc::now().to_rfc3339(),
            access_count: 0,
            is_invalid: false,
        };
        let embedding = self
            .embedder
            .embed(&[text.to_string()])
            .await
            .map_err(VectorError::UpsertFailed)?;
        let vector = embedding.first().cloned().unwrap_or_default();
        self.store
            .upsert(vec![(id.to_string(), vector, meta)])
            .await
            .map_err(|e| VectorError::UpsertFailed(e.to_string()))
    }

    async fn delete(&self, ids: &[String]) -> Result<(), VectorError> {
        self.store
            .delete(ids)
            .await
            .map_err(|e| VectorError::DeleteFailed(e.to_string()))
    }

    async fn stats(&self) -> Result<VectorStats, VectorError> {
        self.store
            .stats()
            .await
            .map(|s| VectorStats {
                total_vectors: s.total_vectors,
                dim: s.dim,
            })
            .map_err(|e| VectorError::SearchFailed(e.to_string()))
    }
}
