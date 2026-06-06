/*!
 * shared_types/vector —— L3 向量检索跨插件契约
 *
 * 定义内容（按 protocol-shared_types契约协议.md §1）：
 * 1. Provider key 常量 PROVIDER_VECTOR
 * 2. Provider trait VectorMemoryContract
 * 3. 辅助类型 VectorSearchHit / VectorStats / VectorError
 *
 * 归属：shared_types（中立层，不归属 MemoryService 也不归属 Assembler）
 * 服务方：MemoryService::start() 注册 Arc<DynProvider<dyn VectorMemoryContract>>
 * 消费方：Assembler VectorMemoryProvider::provide() 中查找并 downcast
 *
 * 红线遵守：
 * - K-R01: PROVIDER_VECTOR 常量在此定义，调用方禁止用裸字符串
 * - K-R02: 跨插件 key 必须先在此定义再被引用
 * - T-R01: trait 在此定义，禁止在 services/memory/l3_vector/ 或 slots/ 内部定义
 * - T-R02: 谁先开发谁定义 trait——本计划先定义
 * - T-R03: trait 不写归属注释
 * - D-R01: 用现有的 DynProvider<T>，不造 DynVectorProvider
 */

use async_trait::async_trait;

// ============================================
// Provider key 常量
// ============================================

/// L3 向量检索 provider key——MemoryService::start() 注册，Assembler 查找
pub const PROVIDER_VECTOR: &str = "vector";

// ============================================
// Provider trait
// ============================================

/// 向量检索契约——Assembler VectorMemoryProvider 通过此 trait 调用 L3 语义检索
#[async_trait]
pub trait VectorMemoryContract: Send + Sync {
    /// 语义搜索：返回最相关的 top_k 条结果
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<VectorSearchHit>, VectorError>;
    /// 向量存储：写入一条向量
    async fn upsert(
        &self,
        id: &str,
        text: &str,
        metadata: serde_json::Value,
    ) -> Result<(), VectorError>;
    /// 批量删除
    async fn delete(&self, ids: &[String]) -> Result<(), VectorError>;
    /// 统计信息
    async fn stats(&self) -> Result<VectorStats, VectorError>;
}

// ============================================
// 辅助类型
// ============================================

/// 搜索结果
#[derive(Clone)]
pub struct VectorSearchHit {
    pub id: String,
    pub score: f32,
    pub text: String,
    pub source: String,
}

/// 统计信息
pub struct VectorStats {
    pub total_vectors: usize,
    pub dim: usize,
}

/// 向量检索错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum VectorError {
    #[error("向量检索未就绪: {0}")]
    NotReady(String),
    #[error("搜索失败: {0}")]
    SearchFailed(String),
    #[error("写入失败: {0}")]
    UpsertFailed(String),
    #[error("删除失败: {0}")]
    DeleteFailed(String),
}
