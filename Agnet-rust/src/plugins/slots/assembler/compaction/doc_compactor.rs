/*! DocumentCompactor —— 轻量、临时、只读文档压缩器（设计文档 §6）

不污染原文件，不调 LLM，只做文本级压缩。
复用 regex 进行中英文分句和实体提取。
*/

use crate::shared_types::assembler::CompactionConfig;

/// 轻量文档压缩器（设计文档 §6）
#[derive(Clone)]
pub struct DocumentCompactor {
    config: CompactionConfig,
}

impl DocumentCompactor {
    pub fn new(config: CompactionConfig) -> Self {
        Self { config }
    }

    /// 压缩文本到 max_tokens 以内（设计文档 §6.1）
    ///
    /// preserve_entities = true 时优先保留含独有实体的句子。
    /// 无法压缩到目标大小时返回原始文本（降级，不截断）。
    pub fn compact(&self, text: &str, max_tokens: usize, preserve_entities: bool) -> String {
        if text.is_empty() {
            return String::new();
        }

        let max_chars = (max_tokens as f64 * self.config.chars_per_token) as usize;
        if text.len() <= max_chars {
            return text.to_string();
        }

        let sentences = self.split_sentences(text);
        if sentences.len() < self.config.min_sentences_for_compaction {
            return text.to_string(); // 句子太少，不压缩
        }

        // 1. 提取独有实体
        let unique_entities = if preserve_entities && self.config.preserve_unique_entities {
            self.extract_unique_entities(text)
        } else {
            Vec::new()
        };

        // 2. 逐句评分
        let mut scored: Vec<(usize, &str, f64)> = sentences
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.as_str(), self.score_sentence(s, &unique_entities)))
            .collect();

        // 3. 标记 must_keep（含独有实体的句子）
        let must_keep: std::collections::HashSet<usize> = if preserve_entities
            && self.config.preserve_unique_entities
            && !unique_entities.is_empty()
        {
            scored
                .iter()
                .filter(|(_, s, _)| unique_entities.iter().any(|e| s.contains(e.as_str())))
                .map(|(i, _, _)| *i)
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        // 4. 按分数降序排列（must_keep 优先）
        scored.sort_by(|a, b| {
            let a_keep = must_keep.contains(&a.0);
            let b_keep = must_keep.contains(&b.0);
            b_keep
                .cmp(&a_keep)
                .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
        });

        // 5. 取 top 句直到满配额
        let mut result = String::new();
        let mut selected_indices: Vec<usize> = Vec::new();
        for &(idx, sentence, _) in &scored {
            let would_add = if result.is_empty() {
                sentence.len()
            } else {
                result.len() + 1 + sentence.len()
            };
            if would_add <= max_chars || must_keep.contains(&idx) {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(sentence);
                selected_indices.push(idx);
            }
        }

        // 6. 如果有 must_keep 未加入（配额不够），按原顺序重排
        if !must_keep.is_empty() && selected_indices.len() < sentences.len() {
            selected_indices.sort();
            let mut reordered = String::new();
            for i in selected_indices {
                let s = &sentences[i];
                if !reordered.is_empty() {
                    reordered.push('\n');
                }
                reordered.push_str(s);
            }
            // 超预算也返回完整句子——截断会产生乱码且破坏实体保留
            // 内容完整性优先于预算，预算不是硬限制
            return reordered;
        }

        result
    }

    /// 中英文分句（设计文档 §6.1）
    fn split_sentences(&self, text: &str) -> Vec<String> {
        // 用中英文句号、感叹号、问号、分号、换行作为分隔符
        let re =
            regex::Regex::new(r"[。！？；！？.!?;\n]+").expect("split_sentences: 正则编译失败");
        re.split(text)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 提取独有实体（大写+数字组合，如 API_KEY_SK_XXXX）（设计文档 §6.1）
    fn extract_unique_entities(&self, text: &str) -> Vec<String> {
        let re = regex::Regex::new(r"[A-Z][A-Z_0-9]{4,}")
            .expect("extract_unique_entities: 正则编译失败");
        let mut entities: Vec<String> =
            re.find_iter(text).map(|m| m.as_str().to_string()).collect();
        entities.sort();
        entities.dedup();
        entities
    }

    /// 句子评分（字数 + 关键词密度）（设计文档 §6.1）
    fn score_sentence(&self, sentence: &str, entities: &[String]) -> f64 {
        let entity_count = entities
            .iter()
            .filter(|e| sentence.contains(e.as_str()))
            .count();
        sentence.len() as f64 * (1.0 + 0.1 * entity_count as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_empty_input() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        assert_eq!(compactor.compact("", 100, true), "");
    }

    #[test]
    fn test_compact_short_text_no_truncation() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "这是一条短文本。";
        let result = compactor.compact(text, 1000, true);
        assert_eq!(result, text);
    }

    #[test]
    fn test_compact_entity_preservation() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "第一句无关内容。第二句包含 API_KEY_SK_TEST 重要信息。第三句又无关。";
        let max_tokens = 5; // 只够保留 2-3 句
        let result = compactor.compact(text, max_tokens, true);
        // 含 API_KEY_SK_TEST 的句子应该被保留
        assert!(
            result.contains("API_KEY_SK_TEST"),
            "含实体的句子应被保留: {}",
            result
        );
    }

    #[test]
    fn test_compact_no_entity_preservation() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "第一句。第二句包含 API_KEY。第三句。";
        let max_tokens = 3;
        let result = compactor.compact(text, max_tokens, false);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_split_chinese_sentences() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "你好。世界！测试？完成；继续";
        let sentences = compactor.split_sentences(text);
        assert!(sentences.len() >= 3);
    }

    #[test]
    fn test_split_english_sentences() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "Hello world. How are you? I'm fine!";
        let sentences = compactor.split_sentences(text);
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_split_mixed_language() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "你好 world。How are you? 我很好！";
        let sentences = compactor.split_sentences(text);
        assert!(sentences.len() >= 3);
    }

    #[test]
    fn test_extract_unique_entities() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let entities = compactor.extract_unique_entities("API_KEY_SK_TEST and SECRET_TOKEN_ABC");
        assert!(entities.contains(&"API_KEY_SK_TEST".to_string()));
        assert!(entities.contains(&"SECRET_TOKEN_ABC".to_string()));
    }

    #[test]
    fn test_extract_unique_entities_no_match() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let entities = compactor.extract_unique_entities("普通文本没有特殊实体");
        assert!(entities.is_empty());
    }

    #[test]
    fn test_compact_low_quota_returns_original() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let text = "短文本。";
        let result = compactor.compact(text, 1, true);
        assert_eq!(result, text);
    }

    #[test]
    fn test_compact_score_sentence_with_entities() {
        let compactor = DocumentCompactor::new(CompactionConfig::default());
        let entities = vec!["API_KEY".to_string()];
        let score_with = compactor.score_sentence("包含 API_KEY 的句子", &entities);
        let score_without = compactor.score_sentence("没有实体的句子", &[]);
        assert!(score_with > score_without);
    }
}
