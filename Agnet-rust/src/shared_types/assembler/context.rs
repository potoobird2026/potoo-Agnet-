/*! ContextProvider trait 与关联类型（设计文档 §3.2）

遵循 shared_types契约协议 T-R01：Provider trait 定义在 shared_types 中。
*/

use async_trait::async_trait;

// ── 跨插件数据结构 ──

/// 单个内容块（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ContextBlock {
    pub section_title: String,
    pub content: String,
    pub source: String,
    pub token_count: usize,
}

/// 提供者返回的完整内容（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ProvidedContext {
    pub blocks: Vec<ContextBlock>,
    pub tokens_used: usize,
}

/// 上下文配额（设计文档 §3.2）
#[derive(Debug, Clone)]
pub struct ContextQuota {
    pub max_tokens: usize,
    pub max_items: usize,
    pub max_chars_per_item: usize,
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,
}

impl Default for ContextQuota {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            max_items: 5,
            max_chars_per_item: 0,
            min_guaranteed_tokens: 0,
            allow_compaction: true,
        }
    }
}

// ── Provider 错误 ──

/// 内容提供者错误（设计文档 §3.2）
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("内容缺失: {0}")]
    Missing(String),
    #[error("配额超限: used={used}, max={max}")]
    QuotaExceeded { used: usize, max: usize },
    #[error("内部错误: {0}")]
    Internal(String),
}

// ── Provider trait（遵循 shared_types契约协议 T-R01）──

/// 内容提供者 trait
///
/// 定义在 shared_types 中，不归属于 Assembler 或任何 Provider 实现方。
/// 参数 ap: &dyn SlotAccessPoint（非 &StepContext），遵循 Slot接入协议 §2。
///
/// 设计文档 §3.2
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> u8;
    fn allow_truncation(&self) -> bool {
        true
    }
    fn silent_on_empty(&self) -> bool {
        true
    }
    fn estimate_max_tokens(&self, config: &super::config::ProviderSlotConfig) -> usize;

    async fn provide(
        &self,
        ap: &dyn crate::core::access::SlotAccessPoint,
        quota: &ContextQuota,
        config: &super::config::ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError>;
}
