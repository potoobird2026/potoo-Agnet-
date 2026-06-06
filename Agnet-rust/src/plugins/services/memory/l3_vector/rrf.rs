/*! RRFFusion —— 倒数排名融合 */
#[derive(Debug, Clone)]
pub struct RRFConfig {
    pub k: f64,
}
impl Default for RRFConfig {
    fn default() -> Self {
        Self { k: 60.0 }
    }
}

pub struct RRFFusion {
    config: RRFConfig,
}

impl Clone for RRFFusion {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}
impl RRFFusion {
    pub fn new(config: RRFConfig) -> Self {
        Self { config }
    }
    pub fn merge(&self, lists: &[Vec<(String, f32)>]) -> Vec<(String, f64)> {
        use std::collections::HashMap;
        let mut scores: HashMap<String, f64> = HashMap::new();
        for list in lists {
            for (rank, (id, _)) in list.iter().enumerate() {
                *scores.entry(id.clone()).or_insert(0.0) +=
                    1.0 / (self.config.k + (rank + 1) as f64);
            }
        }
        let mut results: Vec<(String, f64)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_rrf_formula() {
        let rrf = RRFFusion::new(RRFConfig::default());
        // 使用不对称排名：b 在两个列表都排第1，a 在两个列表都排第2
        // 这样 b 的 RRF 分数必然高于 a，排序结果确定
        let list_a = vec![("b".into(), 0.9), ("a".into(), 0.8)];
        let list_b = vec![("b".into(), 0.7), ("a".into(), 0.6)];
        let merged = rrf.merge(&[list_a, list_b]);
        assert!(!merged.is_empty());
        // b 在两个列表都排第1，RRF 分数必然高于 a
        assert_eq!(merged[0].0, "b");
    }
}
