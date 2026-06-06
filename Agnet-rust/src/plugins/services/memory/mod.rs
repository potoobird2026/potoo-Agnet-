/*! Memory —— 三层记忆系统 */
pub mod config;
pub mod dream;
pub mod experience_extract;
pub mod feedback;
pub mod l1_identity;
pub mod l2_working;
#[cfg(feature = "vector_db")]
pub mod l3_vector;
mod service;

pub use config::{
    ChunkingConfig, EmbeddingConfig, ForgettingConfig, L1Config, L2Config, MemoryConfig,
};
pub use dream::DreamOptimizerService;
pub use experience_extract::{ExperienceEntry, ExperienceExtractService};
pub use feedback::FeedbackMonitor;
pub use l1_identity::{IdentityManager, IdentityMetadata};
pub use l2_working::{
    ActiveMemoryHookSlot, ForgettingService, MemoryFile, MemoryFileFrontmatter, MemoryFileType,
    WorkingMemoryManager,
};
#[cfg(feature = "vector_db")]
pub use l3_vector::{
    CleanupService, EmbeddingBackend, EmbeddingModel, EmbeddingService, MemoryVectorStore,
    NoopEmbeddingModel, RRFConfig, RRFFusion, RetrievalService, SyncEvent, TextChunker,
    VectorFilter, VectorMetadata, VectorStore, VectorStoreError, VectorStoreManager,
    VectorStoreStats, VectorSyncService,
};
pub use service::MemoryService;
