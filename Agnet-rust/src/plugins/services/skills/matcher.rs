/*! SkillMatcher —— Jaccard + TF-IDF 匹配器 */
use std::collections::HashMap;

const JACCARD_WEIGHT: f64 = 0.5;
const TFIDF_WEIGHT: f64 = 0.5;
const IDF_SMOOTH: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct SkillMatcher {
    doc_vectors: HashMap<String, HashMap<String, f64>>,
    idf: HashMap<String, f64>,
    doc_count: usize,
}

impl Default for SkillMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillMatcher {
    pub fn new() -> Self {
        Self {
            doc_vectors: HashMap::new(),
            idf: HashMap::new(),
            doc_count: 0,
        }
    }

    pub fn add_document(&mut self, name: &str, text: &str) {
        let tokens = Self::tokenize(text);
        let tf = Self::compute_tf(&tokens);
        self.doc_vectors.insert(name.to_string(), tf);
        self.doc_count += 1;
        // 更新 IDF
        for token in &tokens {
            *self.idf.entry(token.clone()).or_insert(0.0) += 1.0;
        }
    }

    pub fn remove_document(&mut self, name: &str) {
        if let Some(tf) = self.doc_vectors.remove(name) {
            self.doc_count = self.doc_count.saturating_sub(1);
            for token in tf.keys() {
                self.idf
                    .entry(token.clone())
                    .and_modify(|v| *v = (*v - 1.0).max(0.0));
            }
        }
    }

    pub fn clear_cache(&mut self) {
        self.doc_vectors.clear();
        self.idf.clear();
        self.doc_count = 0;
    }

    pub fn compute_score(
        &self,
        _name: &str,
        context: &str,
        tags: &[String],
        description: &str,
        summary: &str,
    ) -> f64 {
        // Jaccard 标签系数
        let context_lower = context.to_lowercase();
        let tag_hits = tags
            .iter()
            .filter(|t| context_lower.contains(&t.to_lowercase()))
            .count();
        let jaccard = if tags.is_empty() {
            0.5
        } else {
            tag_hits as f64 / tags.len() as f64
        };

        // 快速过滤：标签零交集且 tags 非空
        if tag_hits == 0 && !tags.is_empty() {
            return 0.0;
        }

        // TF-IDF
        let doc_text = format!("{} {}", description, summary);
        let doc_tokens = Self::tokenize(&doc_text);
        let doc_vec = Self::compute_tf(&doc_tokens);
        let query_tokens = Self::tokenize(context);
        let query_vec = Self::compute_tf(&query_tokens);
        let tfidf = Self::cosine_tfidf(&doc_vec, &query_vec, &self.idf, self.doc_count);

        JACCARD_WEIGHT * jaccard + TFIDF_WEIGHT * tfidf
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    fn compute_tf(tokens: &[String]) -> HashMap<String, f64> {
        let mut tf = HashMap::new();
        let total = tokens.len() as f64;
        for t in tokens {
            *tf.entry(t.clone()).or_insert(0.0) += 1.0;
        }
        for v in tf.values_mut() {
            *v /= total;
        }
        tf
    }

    fn cosine_tfidf(
        doc: &HashMap<String, f64>,
        query: &HashMap<String, f64>,
        idf: &HashMap<String, f64>,
        n: usize,
    ) -> f64 {
        let mut dot = 0.0;
        let mut norm_doc = 0.0;
        let mut norm_query = 0.0;
        for (term, tf) in query {
            let idf_val = ((n as f64 + 1.0) / (idf.get(term).copied().unwrap_or(0.0) + 1.0)).ln()
                + IDF_SMOOTH;
            let q_weight = tf * idf_val;
            norm_query += q_weight * q_weight;
            if let Some(&d_tf) = doc.get(term) {
                let d_weight = d_tf * idf_val;
                dot += q_weight * d_weight;
                norm_doc += d_weight * d_weight;
            }
        }
        for (term, tf) in doc {
            if !query.contains_key(term) {
                let idf_val = ((n as f64 + 1.0) / (idf.get(term).copied().unwrap_or(0.0) + 1.0))
                    .ln()
                    + IDF_SMOOTH;
                let d_weight = tf * idf_val;
                norm_doc += d_weight * d_weight;
            }
        }
        if norm_doc == 0.0 || norm_query == 0.0 {
            0.0
        } else {
            dot / (norm_doc.sqrt() * norm_query.sqrt())
        }
    }
}
