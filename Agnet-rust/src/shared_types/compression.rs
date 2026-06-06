/*! compression —— 压缩服务共享契约类型

遵循 shared_types契约协议 定义跨插件类型。
设计依据：docs/services/compression/Compression 严格 AI 开发计划.md §6.4
*/

use async_trait::async_trait;

/// Provider key —— CompressionSummaryContract
pub const PROVIDER_COMPRESSION_SUMMARY: &str = "compression_summary";

/// 压缩摘要契约 —— 由 CompressionService 实现并注册
///
/// 其他 Slot 通过 provider_raw(PROVIDER_COMPRESSION_SUMMARY) + downcast 获取摘要。
#[async_trait]
pub trait CompressionSummaryContract: Send + Sync {
    /// 获取指定会话的压缩摘要文本
    async fn get_summary(&self, session_id: &str) -> Option<String>;
}
