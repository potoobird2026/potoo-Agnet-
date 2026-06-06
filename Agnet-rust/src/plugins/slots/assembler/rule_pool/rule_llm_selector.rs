/*! RuleLlmSelector —— LLM 驱动的规则选择器（设计文档 §8）

通过 SlotAccessPoint 获取数据，不直接访问任何插件内部类型。
*/

use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::{RuleGroup, RulePoolConfig};
use crate::shared_types::llm::{LlmContract, PROVIDER_LLM};
use crate::shared_types::{
    ContentBlock, DynProvider, MemoryProvider, Message, MessageRole, PROVIDER_MEMORY,
};
use tokio::sync::RwLock;

/// LLM 驱动的规则选择器（设计文档 §8）
pub struct RuleLlmSelector {
    config: RulePoolConfig,
    cache: RwLock<Option<(String, RuleGroup)>>,
}

impl Clone for RuleLlmSelector {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            cache: RwLock::new(None), // 克隆时不复制缓存
        }
    }
}

impl RuleLlmSelector {
    pub fn new(config: RulePoolConfig) -> Self {
        Self {
            config,
            cache: RwLock::new(None),
        }
    }

    /// 选择规则组（设计文档 §8.3）
    pub async fn select(&self, ap: &dyn SlotAccessPoint) -> RuleGroup {
        if !self.config.enabled {
            return RuleGroup::empty();
        }

        let user_text = self.get_user_text(ap);
        if user_text.is_empty() {
            return RuleGroup::empty();
        }

        // 检查缓存
        {
            let cache = self.cache.read().await;
            if let Some((cached_text, cached_group)) = cache.as_ref() {
                if *cached_text == user_text {
                    return cached_group.clone();
                }
            }
        }

        let file_rules = self.load_file_rules().await;
        let l3_rules = if self.config.l3_rules.enabled {
            self.load_l3_rules(ap).await
        } else {
            vec![]
        };

        if file_rules.is_empty() && l3_rules.is_empty() {
            return RuleGroup::empty();
        }

        let result = self
            .select_with_llm(ap, &user_text, &file_rules, &l3_rules)
            .await;

        *self.cache.write().await = Some((user_text.to_string(), result.clone()));
        result
    }

    fn get_user_text(&self, ap: &dyn SlotAccessPoint) -> String {
        ap.messages()
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.text_content())
            .unwrap_or_default()
    }

    async fn load_file_rules(&self) -> Vec<(String, String)> {
        if self.config.rules_file.is_empty() {
            return vec![];
        }
        let content = match tokio::fs::read_to_string(&self.config.rules_file).await {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut rules = Vec::new();
        let mut current_group = String::new();
        for line in content.lines() {
            if let Some(group_name) = line.strip_prefix("## group: ") {
                current_group = group_name.trim().to_string();
            } else if line.starts_with("- ") && !current_group.is_empty() {
                rules.push((
                    current_group.clone(),
                    line.trim_start_matches("- ").to_string(),
                ));
            }
        }
        rules
    }

    async fn load_l3_rules(&self, ap: &dyn SlotAccessPoint) -> Vec<String> {
        // L3 检索：通过 provider_raw(PROVIDER_MEMORY) 获取 MemoryProvider
        // 遵循 K-R01：使用 PROVIDER_MEMORY 常量，非裸字符串
        let user_text = self.get_user_text(ap);
        if user_text.is_empty() {
            return vec![];
        }

        let provider = match ap.provider_raw(PROVIDER_MEMORY) {
            Some(raw) => match raw.downcast::<DynProvider<dyn MemoryProvider>>() {
                Ok(wrapper) => wrapper.0.clone(),
                Err(_) => return vec![],
            },
            None => return vec![],
        };

        // 用模板 + 用户消息构建查询（设计文档 §8.3）
        let _query = self
            .config
            .l3_rules
            .query_template
            .replace("{user_text}", &user_text);

        match provider
            .load_working_memory(ap.session_id(), self.config.l3_rules.max_items)
            .await
        {
            Ok(entries) if !entries.is_empty() => {
                let rules: Vec<String> = entries
                    .iter()
                    .map(|e| format!("- {}: {}", e.entry_type, e.summary))
                    .collect();
                tracing::debug!("RuleLlmSelector: 从 L3 加载了 {} 条规则", rules.len());
                rules
            }
            _ => {
                tracing::debug!("RuleLlmSelector: L3 无相关规则");
                vec![]
            }
        }
    }

    async fn select_with_llm(
        &self,
        ap: &dyn SlotAccessPoint,
        user_text: &str,
        file_rules: &[(String, String)],
        l3_rules: &[String],
    ) -> RuleGroup {
        let prompt = self.build_selection_prompt(user_text, file_rules, l3_rules);

        let llm_response: Option<String> = match ap
            .provider_raw(PROVIDER_LLM)
            .and_then(|raw| raw.downcast::<DynProvider<dyn LlmContract>>().ok())
        {
            Some(wrapper) => {
                let msg = Message {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text(prompt)],
                    tool_calls: None,
                    tool_call_id: None,
                    reasoning: None,
                    metadata: None,
                    created_at: crate::core::types::Timestamp::now(),
                };
                match wrapper.0.chat(None, &[msg], &[], "rule_selector").await {
                    Ok(crate::shared_types::llm::ChatResponse::Complete(
                        crate::shared_types::Thought::Final { answer, .. },
                    )) => Some(answer),
                    _ => None,
                }
            }
            None => None,
        };

        match llm_response {
            Some(response) => self.parse_rules_from_response(&response, file_rules, l3_rules),
            None => {
                if self.config.fallback_enabled {
                    self.merge_fallback(file_rules, l3_rules)
                } else {
                    RuleGroup::empty()
                }
            }
        }
    }

    /// 构建 LLM 选择 prompt（设计文档 §8.3）
    fn build_selection_prompt(
        &self,
        user_text: &str,
        file_rules: &[(String, String)],
        l3_rules: &[String],
    ) -> String {
        let mut prompt = format!(
            "用户任务：「{}」\n\n请从以下规则组中选择最适合的 1-3 组规则。\n\n可用规则组：\n",
            user_text
        );
        for (group_name, description) in file_rules {
            prompt.push_str(&format!("- [{}] {}\n", group_name, description));
        }
        if !l3_rules.is_empty() {
            prompt.push_str("\n===== 从历史经验中检索到的相关规则：=====\n");
            for rule in l3_rules {
                prompt.push_str(&format!("{}\n", rule));
            }
        }
        prompt.push_str(
            "\n请只输出选中的规则组名（用逗号分隔，如 'code,general'），不要输出其他内容。",
        );
        prompt
    }

    /// 从 LLM 响应解析规则（设计文档 §8.3）
    fn parse_rules_from_response(
        &self,
        response: &str,
        file_rules: &[(String, String)],
        _l3_rules: &[String],
    ) -> RuleGroup {
        let selected: std::collections::HashSet<String> = response
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let mut rules = Vec::new();
        for (group_name, rule_text) in file_rules {
            if selected.contains(&group_name.to_lowercase()) {
                rules.push(rule_text.clone());
            }
        }

        let name = selected.into_iter().collect::<Vec<_>>().join(",");
        RuleGroup { name, rules }
    }

    /// LLM 失败时的回退：合并所有可用规则（设计文档 §8.3）
    fn merge_fallback(&self, file_rules: &[(String, String)], l3_rules: &[String]) -> RuleGroup {
        let mut all_rules: Vec<String> = file_rules.iter().map(|(_, r)| r.clone()).collect();
        all_rules.extend(l3_rules.iter().cloned());
        RuleGroup {
            name: "fallback".into(),
            rules: all_rules,
        }
    }
}
