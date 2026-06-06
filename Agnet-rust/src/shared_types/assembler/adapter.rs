/*! LlmOutputAdapter 厂商输出适配契约（设计文档 §3.4）

遵循 shared_types契约协议 T-R01。
*/

use async_trait::async_trait;

/// 厂商输出适配契约（设计文档 §3.4）
#[async_trait]
pub trait LlmOutputAdapter: Send + Sync {
    fn provider_name(&self) -> &str;

    fn adapt_system_prompt(&self, text: &str, _context_window: usize) -> String {
        text.to_string()
    }

    fn adapt_context_block(&self, section_title: &str, content: &str) -> String {
        format!("{}\n\n{}", section_title, content)
    }

    fn recommended_rule_count(&self, _context_window: usize) -> usize {
        usize::MAX
    }
}
