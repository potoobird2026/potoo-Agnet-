/*! SkillsProvider（设计文档 §5.7, pri=30）
 *
 * 通过 provider_raw(PROVIDER_SKILLS) 获取 SkillsContractBundle。
 * 根据 quota.max_tokens 选 level（多→Full，少→Summary，最少→TitleOnly）。
 *
 * 红线遵守：
 * - K-R01: 用 PROVIDER_SKILLS 常量
 * - T-R01: 不引用 plugins::services::skills 内部类型
 * - 不用 SkillInjectionProvider（plan C-1 阻塞 1 已规避）
 */
use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::skills::{
    SkillContract, SkillLevel as StSkillLevel, SkillsContractBundle,
};
use crate::shared_types::{DynProvider, MessageRole, PROVIDER_SKILLS};

pub struct SkillsProvider;

/// 根据 quota 预算选 level（plan C-1 "最简单" 方案）
///
/// - max_tokens >= 2000 → Full（给完整内容）
/// - max_tokens >= 500  → Summary（给 TL;DR）
/// - 其余                → TitleOnly（只给标题）
fn select_level(max_tokens: usize) -> StSkillLevel {
    if max_tokens >= 2000 {
        StSkillLevel::Full
    } else if max_tokens >= 500 {
        StSkillLevel::Summary
    } else {
        StSkillLevel::TitleOnly
    }
}

#[async_trait]
impl ContextProvider for SkillsProvider {
    fn name(&self) -> &str {
        "skills"
    }
    fn priority(&self) -> u8 {
        30
    }
    fn allow_truncation(&self) -> bool {
        true
    }
    fn silent_on_empty(&self) -> bool {
        true
    }

    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize {
        config.max_tokens
    }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        // 从 ProviderRegistry 拿 SkillsContractBundle
        let bundle: std::sync::Arc<dyn SkillsContractBundle> =
            match ap.provider_raw(PROVIDER_SKILLS) {
                Some(raw) => match raw.downcast::<DynProvider<dyn SkillsContractBundle>>() {
                    Ok(wrapper) => wrapper.0.clone(),
                    Err(_) => {
                        return Ok(ProvidedContext {
                            blocks: vec![],
                            tokens_used: 0,
                        })
                    }
                },
                None => {
                    return Ok(ProvidedContext {
                        blocks: vec![],
                        tokens_used: 0,
                    })
                }
            };

        let skills = bundle.all();
        if skills.is_empty() {
            return Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            });
        }

        // 取最近 5 条用户消息作为匹配上下文
        let context: String = ap
            .messages()
            .iter()
            .rev()
            .filter(|m| m.role == MessageRole::User)
            .take(5)
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join(" ");

        if context.is_empty() {
            return Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            });
        }

        // 评分 + 排序 + 过滤零分
        let mut scored: Vec<(&std::sync::Arc<dyn SkillContract>, f64)> = skills
            .iter()
            .map(|s| (s, s.match_score(&context)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(_, score)| *score > 0.0);

        if scored.is_empty() {
            return Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            });
        }

        let level = select_level(quota.max_tokens);
        let max_skills = quota.max_items.min(scored.len());
        let mut blocks = Vec::new();
        let mut total_tokens = 0usize;

        for (skill, _score) in scored.iter().take(max_skills) {
            let content = skill.get_content(level);
            let tokens = (content.len() as f64 / 4.0).ceil() as usize;
            if total_tokens + tokens > quota.max_tokens {
                break;
            }
            total_tokens += tokens;
            blocks.push(ContextBlock {
                section_title: format!("## Skill: {}", skill.name()),
                content,
                source: format!("skill/{}", skill.name()),
                token_count: tokens,
            });
        }

        Ok(ProvidedContext {
            blocks,
            tokens_used: total_tokens,
        })
    }
}

// ============================================
// 单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::access::SlotAccessPoint;
    use crate::core::types::error::PluginError;
    use crate::shared_types::skills::{
        InjectionPolicy, QuotaPreference, SkillContract, SkillLevel as StSkillLevel,
        SkillsContractBundle,
    };
    use crate::shared_types::Message;
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Mock SlotAccessPoint 用于测试
    struct TestSlotAccess {
        messages: Vec<Message>,
        providers: HashMap<String, Arc<dyn Any + Send + Sync>>,
        /// 持有 read_context 的内容（因为返回 &dyn Any 的生命周期问题）
        #[allow(dead_code)]
        context_storage: HashMap<String, Box<dyn Any + Send + Sync>>,
    }

    impl TestSlotAccess {
        fn new() -> Self {
            Self {
                messages: Vec::new(),
                providers: HashMap::new(),
                context_storage: HashMap::new(),
            }
        }
        fn with_messages(mut self, msgs: Vec<Message>) -> Self {
            self.messages = msgs;
            self
        }
        fn with_provider(mut self, name: &str, p: Arc<dyn Any + Send + Sync>) -> Self {
            self.providers.insert(name.to_string(), p);
            self
        }
    }

    impl SlotAccessPoint for TestSlotAccess {
        fn messages(&self) -> &[Message] {
            &self.messages
        }
        fn session_id(&self) -> &str {
            "test-session"
        }
        fn phase_name(&self) -> &str {
            "test-phase"
        }
        fn current_iteration(&self) -> usize {
            0
        }
        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            _key: &str,
            _val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn read_context_raw(&self, _key: &str) -> Option<&(dyn Any + Send + Sync)> {
            None
        }
        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            self.providers.get(name).cloned()
        }

        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    /// 测试用 SkillContract 实现
    struct FakeSkill {
        name: String,
        desc: String,
        tags: Vec<String>,
        score: f64,
    }

    impl SkillContract for FakeSkill {
        fn name(&self) -> &str {
            &self.name
        }
        fn version(&self) -> &str {
            "1.0.0"
        }
        fn description(&self) -> &str {
            &self.desc
        }
        fn group(&self) -> &str {
            ""
        }
        fn tags(&self) -> &[String] {
            &self.tags
        }
        fn dependencies(&self) -> &[String] {
            &[]
        }
        fn injection_policy(&self) -> InjectionPolicy {
            InjectionPolicy::Auto
        }
        fn quota_preference(&self) -> QuotaPreference {
            QuotaPreference::Full
        }
        fn get_content(&self, _level: StSkillLevel) -> String {
            format!("content-of-{}", self.name)
        }
        fn match_score(&self, _context: &str) -> f64 {
            self.score
        }
    }

    /// 测试用 Bundle：持有固定 list of skills
    struct FakeBundle {
        skills: Vec<Arc<dyn SkillContract>>,
    }

    impl SkillsContractBundle for FakeBundle {
        fn all(&self) -> Vec<Arc<dyn SkillContract>> {
            self.skills.clone()
        }
    }

    #[allow(dead_code)]
    fn empty_quota() -> ContextQuota {
        ContextQuota {
            max_tokens: 0,
            ..Default::default()
        }
    }
    fn quota_max_items(n: usize) -> ContextQuota {
        ContextQuota {
            max_tokens: 10_000,
            max_items: n,
            ..Default::default()
        }
    }
    fn quota_max_tokens(t: usize) -> ContextQuota {
        ContextQuota {
            max_tokens: t,
            max_items: 100,
            ..Default::default()
        }
    }
    fn make_skill(name: &str, tags: &[&str], score: f64) -> Arc<dyn SkillContract> {
        Arc::new(FakeSkill {
            name: name.into(),
            desc: format!("desc-{}", name),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            score,
        })
    }
    fn user_msg(text: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: vec![crate::shared_types::ContentBlock::Text(text.into())],
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            created_at: crate::core::types::Timestamp::now(),
        }
    }
    fn config_with_max_items(n: usize) -> ProviderSlotConfig {
        ProviderSlotConfig {
            max_items: n,
            max_tokens: 10_000,
            ..Default::default()
        }
    }

    // ── D-3 SkillsProvider 单元测试 ──

    #[tokio::test]
    async fn provide_returns_empty_when_no_provider() {
        let ap = TestSlotAccess::new().with_messages(vec![user_msg("hello world")]);
        let provider = SkillsProvider;
        let cfg = ProviderSlotConfig::default();
        let result = provider
            .provide(&ap, &quota_max_items(5), &cfg)
            .await
            .unwrap();
        assert!(result.blocks.is_empty(), "无 provider 时应返回 0 blocks");
        assert_eq!(result.tokens_used, 0);
    }

    #[tokio::test]
    async fn provide_returns_empty_when_downcast_fails() {
        // 注入类型不匹配的 provider（Arc<u32> 而非 DynProvider<dyn Bundle>）
        let ap = TestSlotAccess::new()
            .with_messages(vec![user_msg("hello world")])
            .with_provider(
                PROVIDER_SKILLS,
                Arc::new(42u32) as Arc<dyn Any + Send + Sync>,
            );
        let provider = SkillsProvider;
        let cfg = ProviderSlotConfig::default();
        let result = provider
            .provide(&ap, &quota_max_items(5), &cfg)
            .await
            .unwrap();
        assert!(result.blocks.is_empty(), "downcast 失败时应返回 0 blocks");
    }

    #[tokio::test]
    async fn provide_returns_blocks_with_real_bundle() {
        let skills: Vec<Arc<dyn SkillContract>> = vec![
            make_skill("a", &["rust"], 0.5),
            make_skill("b", &["python"], 0.1),
        ];
        let bundle: Arc<dyn SkillsContractBundle> = Arc::new(FakeBundle { skills });
        let wrapped = Arc::new(DynProvider(bundle));
        let ap = TestSlotAccess::new()
            .with_messages(vec![user_msg("I love rust programming")])
            .with_provider(PROVIDER_SKILLS, wrapped as Arc<dyn Any + Send + Sync>);
        let provider = SkillsProvider;
        let cfg = config_with_max_items(5);
        let result = provider
            .provide(&ap, &quota_max_items(5), &cfg)
            .await
            .unwrap();
        // a score=0.5 > 0, b score=0.1 > 0 → 2 blocks (但 match_score 阈值是 > 0.0)
        // Actually wait — FakeSkill.match_score is hardcoded to self.score, so 0.1 > 0 → keep
        assert_eq!(result.blocks.len(), 2, "2 个 score>0 的技能应都入选");
        assert!(result.blocks[0].content.contains("content-of-"));
    }

    #[tokio::test]
    async fn provide_filters_zero_score_skills() {
        let skills: Vec<Arc<dyn SkillContract>> = vec![
            make_skill("good", &["match"], 0.5),
            make_skill("zero", &["nothing"], 0.0),
        ];
        let bundle: Arc<dyn SkillsContractBundle> = Arc::new(FakeBundle { skills });
        let wrapped = Arc::new(DynProvider(bundle));
        let ap = TestSlotAccess::new()
            .with_messages(vec![user_msg("match me")])
            .with_provider(PROVIDER_SKILLS, wrapped as Arc<dyn Any + Send + Sync>);
        let provider = SkillsProvider;
        let cfg = config_with_max_items(5);
        let result = provider
            .provide(&ap, &quota_max_items(5), &cfg)
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1, "0 分技能应被过滤");
        assert!(result.blocks[0].content.contains("content-of-good"));
    }

    #[tokio::test]
    async fn provide_respects_quota_max_items() {
        let skills: Vec<Arc<dyn SkillContract>> = (0..5)
            .map(|i| make_skill(&format!("s{i}"), &[], 0.5))
            .collect();
        let bundle: Arc<dyn SkillsContractBundle> = Arc::new(FakeBundle { skills });
        let wrapped = Arc::new(DynProvider(bundle));
        let ap = TestSlotAccess::new()
            .with_messages(vec![user_msg("test")])
            .with_provider(PROVIDER_SKILLS, wrapped as Arc<dyn Any + Send + Sync>);
        let provider = SkillsProvider;
        let cfg = config_with_max_items(5);
        let result = provider
            .provide(&ap, &quota_max_items(1), &cfg)
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1, "quota.max_items=1 应只 1 个 block");
    }

    #[tokio::test]
    async fn provide_respects_quota_max_tokens() {
        let skills: Vec<Arc<dyn SkillContract>> = vec![
            make_skill("big", &["x"], 0.9),
            make_skill("small", &["x"], 0.5),
        ];
        let bundle: Arc<dyn SkillsContractBundle> = Arc::new(FakeBundle { skills });
        let wrapped = Arc::new(DynProvider(bundle));
        let ap = TestSlotAccess::new()
            .with_messages(vec![user_msg("test x")])
            .with_provider(PROVIDER_SKILLS, wrapped as Arc<dyn Any + Send + Sync>);
        let provider = SkillsProvider;
        // max_tokens=5 极小——只够 1 个 block（"content-of-big" = 16 chars / 4 = 4 tokens）
        let cfg = ProviderSlotConfig {
            max_tokens: 5,
            max_items: 100,
            ..Default::default()
        };
        let result = provider
            .provide(&ap, &quota_max_tokens(5), &cfg)
            .await
            .unwrap();
        assert!(
            result.tokens_used <= 5,
            "tokens_used={} 应 <= max_tokens=5",
            result.tokens_used
        );
        assert!(result.blocks.len() <= 1, "极小 quota 应只 0 或 1 block");
    }

    #[tokio::test]
    async fn provide_returns_empty_when_no_user_messages() {
        // 没有用户消息 → context 为空 → 返回 0 blocks
        let skills: Vec<Arc<dyn SkillContract>> = vec![make_skill("a", &[], 0.5)];
        let bundle: Arc<dyn SkillsContractBundle> = Arc::new(FakeBundle { skills });
        let wrapped = Arc::new(DynProvider(bundle));
        let ap = TestSlotAccess::new()
            .with_messages(vec![])  // 无消息
            .with_provider(PROVIDER_SKILLS, wrapped as Arc<dyn Any + Send + Sync>);
        let provider = SkillsProvider;
        let cfg = config_with_max_items(5);
        let result = provider
            .provide(&ap, &quota_max_items(5), &cfg)
            .await
            .unwrap();
        assert!(result.blocks.is_empty(), "无用户消息应返回 0 blocks");
    }

    #[tokio::test]
    async fn select_level_thresholds() {
        assert_eq!(select_level(0), StSkillLevel::TitleOnly);
        assert_eq!(select_level(499), StSkillLevel::TitleOnly);
        assert_eq!(select_level(500), StSkillLevel::Summary);
        assert_eq!(select_level(1999), StSkillLevel::Summary);
        assert_eq!(select_level(2000), StSkillLevel::Full);
        assert_eq!(select_level(100_000), StSkillLevel::Full);
    }
}
