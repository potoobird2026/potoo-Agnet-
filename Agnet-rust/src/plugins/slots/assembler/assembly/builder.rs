/*! MessageBuilder —— 拼装 System 消息 + 紧急裁剪（设计文档 §7.5）

职责：
- 将 ContextBlock 列表拼接为 System 消息
- 追加历史消息
- 超限时紧急裁剪（从前往后移除低优先级 block，身份永不裁剪）
*/

use crate::core::types::Timestamp;
use crate::shared_types::assembler::*;
use crate::shared_types::{ContentBlock, Message, MessageRole};

/// 消息拼装器（设计文档 §7.5）
pub struct MessageBuilder;

impl MessageBuilder {
    /// 拼装最终消息列表（设计文档 §7.5）
    ///
    /// 1. 将所有 ContextBlock 拼接为一条 System 消息
    /// 2. 追加历史消息
    pub fn build(
        blocks: &[ContextBlock],
        history_messages: &[Message],
        config: &AssemblerConfig,
    ) -> (Vec<Message>, bool) {
        let mut messages = Vec::new();

        // 1. 拼接 System 消息
        if !blocks.is_empty() {
            let mut content = String::new();
            for block in blocks {
                if !content.is_empty() {
                    content.push_str("\n\n");
                }
                content.push_str(&block.section_title);
                content.push('\n');
                content.push_str(&block.content);
            }
            messages.push(Message {
                role: MessageRole::System,
                content: vec![ContentBlock::Text(content)],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: Timestamp::now(),
            });
        }

        // 2. 追加历史消息（设计文档 §7.5：历史消息在 System 消息之后）
        messages.extend_from_slice(history_messages);

        // 3. 紧急裁剪（设计文档 §7.5：超限时从前往后移除非 System 消息）
        let total: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        let context_window = config.max_injection_tokens + 5000; // 估算的 context_window
        let mut truncation_applied = false;
        if total > context_window {
            Self::emergency_truncate(&mut messages, context_window);
            truncation_applied = true;
        }

        (messages, truncation_applied)
    }

    /// 紧急裁剪（设计文档 §7.5，设计总纲 §1.2：身份永远不裁剪）
    ///
    /// 从前往后遍历，移除非 System 消息直到总 token 不超过 context_window。
    /// 保留所有 System 角色消息（身份注入在 System 消息中，受到保护）。
    pub fn emergency_truncate(messages: &mut Vec<Message>, context_window: usize) {
        let before = messages.len();
        let total: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        if total <= context_window {
            return;
        }

        let mut tokens_to_remove = total.saturating_sub(context_window);
        let mut removed_count = 0usize;

        // 从前往后移除（最早的历史消息优先移除）
        messages.retain(|msg| {
            if tokens_to_remove == 0 {
                return true;
            }
            if msg.role == MessageRole::System {
                return true; // System 消息永不裁剪（设计总纲 §1.2）
            }
            let tokens = msg.estimate_tokens();
            if tokens <= tokens_to_remove {
                tokens_to_remove = tokens_to_remove.saturating_sub(tokens);
                removed_count += 1;
                false
            } else {
                true // 这条消息太大不能完全移除，跳过
            }
        });

        if removed_count > 0 {
            tracing::warn!(
                "{} 紧急裁剪: 移除了 {}/{} 条消息 (总 token 超过 context_window)",
                crate::plugins::slots::assembler::config::LOG_PREFIX,
                removed_count,
                before,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_empty_blocks_returns_empty() {
        let (messages, truncation) = MessageBuilder::build(&[], &[], &AssemblerConfig::default());
        assert!(messages.is_empty());
        assert!(!truncation);
    }

    #[test]
    fn test_build_with_blocks_creates_system_message() {
        let blocks = vec![ContextBlock {
            section_title: "## Test".into(),
            content: "content".into(),
            source: "test".into(),
            token_count: 10,
        }];
        let (messages, _truncation) =
            MessageBuilder::build(&blocks, &[], &AssemblerConfig::default());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::System);
    }

    #[test]
    fn test_emergency_truncate_keeps_system_messages() {
        let mut messages = vec![
            Message {
                role: MessageRole::System,
                content: vec![ContentBlock::Text("system".repeat(1000))],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: Timestamp::now(),
            },
            Message {
                role: MessageRole::User,
                content: vec![ContentBlock::Text("user".repeat(500))],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: Timestamp::now(),
            },
        ];
        MessageBuilder::emergency_truncate(&mut messages, 100);
        assert_eq!(messages.len(), 1, "System 消息应被保留");
        assert_eq!(messages[0].role, MessageRole::System);
    }
}
