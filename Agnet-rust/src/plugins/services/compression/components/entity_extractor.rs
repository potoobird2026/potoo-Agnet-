use super::super::services::EntityExtractorService;

pub struct EntityExtractor;

impl Default for EntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl EntityExtractorService for EntityExtractor {
    fn extract(&self, text: &str) -> Vec<String> {
        // 简易实体提取：大写开头连续词、数字+单位、URL
        let mut entities = Vec::new();
        for word in text.split_whitespace() {
            let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.chars().next().is_some_and(|c| c.is_uppercase()) && trimmed.len() > 1 {
                entities.push(trimmed.to_string());
            }
        }
        entities.sort();
        entities.dedup();
        entities
    }
}
