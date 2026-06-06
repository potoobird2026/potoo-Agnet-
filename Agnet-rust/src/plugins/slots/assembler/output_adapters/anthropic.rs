/*! AnthropicOutputAdapter —— XML 标签适配（设计文档 §9.3）*/

use async_trait::async_trait;
use crate::shared_types::assembler::LlmOutputAdapter;

pub struct AnthropicOutputAdapter;

#[async_trait]
impl LlmOutputAdapter for AnthropicOutputAdapter {
    fn provider_name(&self) -> &str { "anthropic" }

    fn adapt_system_prompt(&self, text: &str, _cw: usize) -> String {
        let mut result = String::from("<system_prompt>\n");
        result.push_str(text);
        result.push_str("\n</system_prompt>");
        result
    }

    fn adapt_context_block(&self, title: &str, content: &str) -> String {
        format!("<context_block>\n<title>{}</title>\n{}\n</context_block>", title, content)
    }

    fn recommended_rule_count(&self, _cw: usize) -> usize { usize::MAX }
}
