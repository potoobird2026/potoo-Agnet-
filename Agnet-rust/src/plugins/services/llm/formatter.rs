//! MultimodalFormatter — 多模态内容格式转换器
//!
//! 设计文档 §4.4：原 MultimodalFormatter（llm_thinker/components/multimodal_formatter.rs），
//! 去掉 Component trait 后降级为普通 struct。
//!
//! 职责：
//! - to_openai() — ContentBlock 列表 → OpenAI API content array
//! - to_anthropic() — ContentBlock 列表 → Anthropic API content array
//! - 支持 text / image / audio / file 四种模态

use crate::shared_types::ContentBlock;

/// 无状态多模态格式转换器（设计文档 §4.4）
pub struct MultimodalFormatter;

impl MultimodalFormatter {
    /// 创建新的格式转换器
    pub fn new() -> Self {
        Self
    }

    /// 将 ContentBlock 列表转换为 OpenAI API content array
    pub fn to_openai(&self, blocks: &[ContentBlock], multimodal: bool) -> Vec<serde_json::Value> {
        if !multimodal {
            return to_openai_text_only(blocks);
        }
        blocks.iter().map(block_to_openai).collect()
    }

    /// 将 ContentBlock 列表转换为 Anthropic API content array
    pub fn to_anthropic(
        &self,
        blocks: &[ContentBlock],
        multimodal: bool,
    ) -> Vec<serde_json::Value> {
        if !multimodal {
            return to_anthropic_text_only(blocks);
        }
        let mut out = Vec::new();
        for block in blocks {
            if let Some(v) = block_to_anthropic(block) {
                out.push(v);
            }
        }
        out
    }
}

impl Default for MultimodalFormatter {
    fn default() -> Self {
        Self::new()
    }
}

// ── 内部辅助函数 ─────────────────────────────────────────────────────

fn to_openai_text_only(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    let parts: Vec<&str> = blocks.iter().filter_map(|b| b.as_text()).collect();
    let text = parts.join("\n");
    vec![serde_json::json!({"type": "text", "text": text})]
}

fn to_anthropic_text_only(blocks: &[ContentBlock]) -> Vec<serde_json::Value> {
    let parts: Vec<&str> = blocks.iter().filter_map(|b| b.as_text()).collect();
    let text = parts.join("\n");
    vec![serde_json::json!({"type": "text", "text": text})]
}

fn block_to_openai(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text(t) => {
            serde_json::json!({"type": "text", "text": t})
        }
        ContentBlock::Image { base64, mime_type } => {
            serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}", mime_type, base64)
                }
            })
        }
        ContentBlock::Audio { base64, mime_type } => {
            // 设计文档 §3.2: 去掉 "audio/" 前缀作为 format
            let format = mime_type.strip_prefix("audio/").unwrap_or(mime_type);
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {
                    "data": base64,
                    "format": format
                }
            })
        }
        ContentBlock::File {
            base64,
            mime_type,
            filename,
        } => {
            serde_json::json!({
                "type": "file",
                "file": {
                    "filename": filename,
                    "file_data": format!("data:{};base64,{}", mime_type, base64)
                }
            })
        }
    }
}

fn block_to_anthropic(block: &ContentBlock) -> Option<serde_json::Value> {
    match block {
        ContentBlock::Text(t) => Some(serde_json::json!({"type": "text", "text": t})),
        ContentBlock::Image { base64, mime_type } => Some(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": mime_type,
                "data": base64
            }
        })),
        ContentBlock::Audio { .. } => {
            // 设计文档 §3.2: Anthropic 不支持 Audio，记录 warn 并跳过
            tracing::warn!("Anthropic API does not support Audio blocks; block dropped");
            None
        }
        ContentBlock::File { filename, .. } => {
            // 设计文档 §3.2: Anthropic 不支持 File，记录 warn 并跳过
            tracing::warn!(
                "Anthropic API does not support File blocks (filename: {}); block dropped",
                filename
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_openai_text_only() {
        let formatter = MultimodalFormatter;
        let blocks = vec![ContentBlock::Text("Hello".into())];
        let result = formatter.to_openai(&blocks, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "text");
        assert_eq!(result[0]["text"], "Hello");
    }

    #[test]
    fn test_to_anthropic_text_only() {
        let formatter = MultimodalFormatter;
        let blocks = vec![ContentBlock::Text("World".into())];
        let result = formatter.to_anthropic(&blocks, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "text");
        assert_eq!(result[0]["text"], "World");
    }

    #[test]
    fn test_multimodal_image() {
        let formatter = MultimodalFormatter;
        let blocks = vec![ContentBlock::Image {
            base64: "abc123".into(),
            mime_type: "image/png".into(),
        }];
        let openai = formatter.to_openai(&blocks, true);
        assert_eq!(openai[0]["type"], "image_url");
        assert!(openai[0]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));

        let anthropic = formatter.to_anthropic(&blocks, true);
        assert_eq!(anthropic[0]["type"], "image");
    }

    #[test]
    fn test_anthropic_skips_unsupported() {
        let formatter = MultimodalFormatter;
        let blocks = vec![
            ContentBlock::Text("hi".into()),
            ContentBlock::Audio {
                base64: "x".into(),
                mime_type: "audio/wav".into(),
            },
        ];
        let result = formatter.to_anthropic(&blocks, true);
        assert_eq!(result.len(), 1); // Audio 被跳过
    }
}
