/*! OpenAiOutputAdapter —— 简洁 Markdown 适配（设计文档 §9.3）*/

use async_trait::async_trait;
use crate::shared_types::assembler::LlmOutputAdapter;

pub struct OpenAiOutputAdapter;

#[async_trait]
impl LlmOutputAdapter for OpenAiOutputAdapter {
    fn provider_name(&self) -> &str { "openai" }

    fn adapt_system_prompt(&self, text: &str, _cw: usize) -> String {
        text.to_string()
    }

    fn adapt_context_block(&self, title: &str, content: &str) -> String {
        format!("{}\n{}", title, content)
    }

    fn recommended_rule_count(&self, cw: usize) -> usize {
        if cw < 16000 { 3 } else if cw < 64000 { 5 } else { usize::MAX }
    }
}
