use serde::{Deserialize, Serialize};

/// Provider key 常量——memory_saver slot 和 init_phase slot 通过此 key 查找 MemoryProvider
pub const PROVIDER_MEMORY: &str = "memory";

/// 记忆操作错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum MemoryError {
    #[error("未找到: {0}")]
    NotFound(String),

    #[error("写入失败: {0}")]
    WriteError(String),

    #[error("读取失败: {0}")]
    ReadError(String),

    #[error("超时: {0}")]
    Timeout(String),
}

/// 记忆统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub vector_index_size: usize,
    pub last_persisted_at: Option<String>,
}

/// 经验条目（experience extract 输出）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceEntry {
    pub summary: String,
    pub source_session: String,
    pub created_at: String,
    pub tags: Vec<String>,
}

/// 身份记忆片段 —— 跨插件共享的契约类型
///
/// MemoryService 实现 MemoryProvider trait 时，
/// 将内部 `l1_identity::IdentitySection` 映射为此类型返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentitySection {
    pub user_id: String,
    pub content: String,
    pub metadata: Option<String>,
}

/// 工作记忆条目 —— 跨插件共享的契约类型
///
/// MemoryService 实现 MemoryProvider trait 时，
/// 将内部 `l2_working::MemoryFile` 映射为此类型返回。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFileEntry {
    pub id: String,
    pub summary: String,
    pub content: Option<String>,
    pub created_at: String,
    pub entry_type: String,
}

/// 记忆 Provider trait —— 由 MemoryService 实现并注册到 ProviderRegistry
#[async_trait::async_trait]
pub trait MemoryProvider: Send + Sync {
    // ── 写入方法（memory_saver 使用）──

    /// 持久化消息
    async fn persist_messages(
        &self,
        session_id: &str,
        messages: &[super::Message],
    ) -> Result<(), MemoryError>;

    /// 持久化观察结果
    async fn persist_observation(
        &self,
        session_id: &str,
        observation: &str,
    ) -> Result<(), MemoryError>;

    /// 触发向量索引更新（异步）
    async fn trigger_vector_index(&self, session_id: &str) -> Result<(), MemoryError>;

    /// 提取经验（异步）
    async fn extract_experiences(
        &self,
        session_id: &str,
    ) -> Result<Vec<ExperienceEntry>, MemoryError>;

    /// 获取记忆统计
    async fn stats(&self, session_id: &str) -> Result<MemoryStats, MemoryError>;

    // ── 读取方法（init_phase 使用）──

    /// 加载身份记忆（L1）
    async fn load_identity(&self, session_id: &str) -> Result<IdentitySection, MemoryError>;

    /// 加载最近工作记忆（L2）
    async fn load_working_memory(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryFileEntry>, MemoryError>;

    /// 检查会话是否为新会话
    async fn is_new_session(&self, session_id: &str) -> Result<bool, MemoryError>;

    /// 语义搜索记忆内容（L3 向量检索，降级为关键词匹配）
    ///
    /// 根据查询文本搜索相关记忆条目。当 L3 向量检索未就绪时，
    /// 实现方应做关键词匹配降级，不返回错误。
    async fn search_memory(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryFileEntry>, MemoryError>;
}

// 不再需要独立的 DynMemoryProvider——统一使用 shared_types::DynProvider<T>。
// 参见 protocol-shared_types契约协议.md §4。
