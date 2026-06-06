/*! TextChunker —— Markdown 文本分块 */
use super::super::config::ChunkingConfig;

#[derive(Clone)]
pub struct TextChunker {
    config: ChunkingConfig,
}

impl TextChunker {
    pub fn new(config: ChunkingConfig) -> Self {
        Self { config }
    }
    pub fn chunk(&self, markdown: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for line in markdown.lines() {
            if line.starts_with("### ") && !current.is_empty() {
                // 有 ### 标题就分块，不设长度阈值——短文档也应正确按标题分割
                chunks.push(current.clone());
                current.clear();
            }
            if !current.is_empty() && current.len() + line.len() + 1 > self.config.chunk_size {
                chunks.push(current.clone());
                current.clear();
                // 重叠
                if let Some(last) = chunks.last() {
                    let overlap: String = last
                        .chars()
                        .rev()
                        .take(self.config.chunk_overlap)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    current.push_str(&overlap);
                    current.push('\n');
                }
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_chunk_by_heading() {
        let c = TextChunker::new(ChunkingConfig::default());
        let md = "### Intro\nhello\n### Body\nworld";
        let chunks = c.chunk(md);
        assert!(chunks.len() >= 2);
    }
}
