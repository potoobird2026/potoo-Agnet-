use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::types::Timestamp;

// ============================================
// 多模态内容块
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    Image {
        base64: String,
        mime_type: String,
    },
    Audio {
        base64: String,
        mime_type: String,
    },
    File {
        base64: String,
        mime_type: String,
        filename: String,
    },
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text(text.into())
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text(t) => Some(t.as_str()),
            _ => None,
        }
    }
}

impl From<String> for ContentBlock {
    fn from(s: String) -> Self {
        ContentBlock::Text(s)
    }
}

impl From<&str> for ContentBlock {
    fn from(s: &str) -> Self {
        ContentBlock::Text(s.to_string())
    }
}

// ============================================
// LLM 工具调用（协议层概念）
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// LLM 分配的调用 ID（OpenAI: `call_xxx`，Anthropic: `toolu_xxx`）
    pub id: String,
    /// 工具名称
    pub name: String,
    /// 工具参数（JSON）
    pub arguments: Value,
}

// ============================================
// 对话消息
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub created_at: Timestamp,
}

impl Message {
    pub fn text(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text(text.into())],
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            created_at: Timestamp::now(),
        }
    }

    pub fn with_tool_call(mut self, tool_call_id: String) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self
    }

    pub fn with_created_at(mut self, created_at: Timestamp) -> Self {
        self.created_at = created_at;
        self
    }

    /// 获取文本内容（如果是纯文本消息）
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| c.as_text())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// 估算消息 token 数（快速近似，用于硬限制）
    /// 精确 token 计数请使用 TokenCounter（compression/algorithms/tokenizer.rs）。
    pub fn estimate_tokens(&self) -> usize {
        let text = self.text_content();
        // 保守估算：中英文混合文本约 1 token / 2 chars
        text.chars().count() / 2 + 1
    }
}

/// 消息角色
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}
