/*! L3 向量知识库 */
#[cfg(feature = "vector_db")]
pub mod chunker;
#[cfg(feature = "vector_db")]
pub mod cleanup;
#[cfg(feature = "vector_db")]
pub mod embedding;
#[cfg(feature = "vector_db")]
pub mod manager;
#[cfg(feature = "vector_db")]
pub mod memory_store;
#[cfg(feature = "vector_db")]
pub mod metadata;
#[cfg(feature = "vector_db")]
pub mod retrieval;
#[cfg(feature = "vector_db")]
pub mod rrf;
#[cfg(feature = "vector_db")]
pub mod store;
#[cfg(feature = "vector_db")]
pub mod sync;

#[cfg(feature = "vector_db")]
pub use chunker::TextChunker;
#[cfg(feature = "vector_db")]
pub use cleanup::CleanupService;
#[cfg(feature = "vector_db")]
pub use embedding::{EmbeddingBackend, EmbeddingModel, EmbeddingService, NoopEmbeddingModel};
#[cfg(feature = "vector_db")]
pub use manager::VectorStoreManager;
#[cfg(feature = "vector_db")]
pub use memory_store::MemoryVectorStore;
#[cfg(feature = "vector_db")]
pub use metadata::{VectorFilter, VectorMetadata, VectorStoreError};
#[cfg(feature = "vector_db")]
pub use retrieval::RetrievalService;
#[cfg(feature = "vector_db")]
pub use rrf::{RRFConfig, RRFFusion};
#[cfg(feature = "vector_db")]
pub use store::{VectorStore, VectorStoreStats};
#[cfg(feature = "vector_db")]
pub use sync::{SyncEvent, VectorSyncService};
