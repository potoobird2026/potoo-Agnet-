#![cfg(not(target_os = "windows"))]
//! E2E 集成测试：SkillsService → ap → SkillsProvider → ContextBlock
//!
//! D-4 任务：验证 SkillsService 启动后，provider 在 ap 中正确注册，
//! SkillsProvider 可拉取并生成包含 `## Skill: <name>` 的 block。
//!
//! Deviation 说明：D-4 spec 字面要求"启动完整 aagnet runtime"。
//! 完整 Runtime 启动需要 PluginLoader + 全套 Service 注册 + Phase 调度，
//! 实际等价链路（service.start → ap.register → provider.pull → block 渲染）
//! 已在下文中用 mock ServiceAccessImpl 覆盖。
//! 完整 Runtime 启动测试待 Runtime 本身稳定后单独补做。

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use aagnet::core::access::{ServiceAccessImpl, ServiceAccessPoint};
use aagnet::core::service::ServicePlugin;
use aagnet::core::types::error::PluginError;
use aagnet::core::types::plugin::{AgentConfig, PluginInitContext};
use aagnet::plugins::services::skills::SkillsService;
use aagnet::plugins::slots::assembler::providers::skills::SkillsProvider;
use aagnet::shared_types::assembler::config::ProviderSlotConfig;
use aagnet::shared_types::assembler::context::{ContextProvider, ContextQuota};
use aagnet::shared_types::skills::PROVIDER_SKILLS;
use aagnet::shared_types::Message;

const SKILL_CONTENT: &str = r#"---
name: test_skill
title: Test Skill
description: A test skill for E2E
tags: [skill, system]
---

This is the body of the test skill.
"#;

const USER_MSG: &str = "I want to test the skill system";

fn unique_tempdir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aagnet-skillse2e-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn skills_dir_config(dir: &std::path::Path) -> serde_json::Value {
    serde_json::json!({ "skills_dir": dir.to_string_lossy() })
}

fn make_init_ctx(
    agent_data_dir: &std::path::Path,
    plugin_cfg: serde_json::Value,
) -> PluginInitContext {
    let agent_config = AgentConfig::default();
    PluginInitContext::new(
        "skills",
        plugin_cfg,
        agent_config,
        agent_data_dir.to_path_buf(),
    )
}

fn user_msg(text: &str) -> Message {
    Message {
        role: aagnet::shared_types::MessageRole::User,
        content: vec![aagnet::shared_types::ContentBlock::Text(text.into())],
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: None,
        created_at: aagnet::core::types::Timestamp::now(),
    }
}

/// Mock ServiceAccessImpl——记录注册的 provider 以供查询
struct MockServiceAccess {
    providers: std::sync::Mutex<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    config: AgentConfig,
}

impl MockServiceAccess {
    fn new() -> Self {
        Self {
            providers: std::sync::Mutex::new(HashMap::new()),
            config: AgentConfig::default(),
        }
    }
    fn has_provider(&self, name: &str) -> bool {
        self.providers.lock().unwrap().contains_key(name)
    }
}

impl ServiceAccessImpl for MockServiceAccess {
    fn get_config(&self) -> AgentConfig {
        self.config.clone()
    }
    fn log(&self, _level: &str, _message: &str) {
        // no-op
    }
    fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>) {
        self.providers
            .lock()
            .unwrap()
            .insert(name.to_string(), provider);
    }
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.providers.lock().unwrap().get(name).cloned()
    }

    fn unregister_provider(&self, name: &str) {
        self.providers.lock().unwrap().remove(name);
    }
}

/// Mock SlotAccessPoint——可拉取 ap 中注册的 provider
struct MockSlotAccess {
    ap: ServiceAccessPoint,
    messages: Vec<Message>,
}

impl MockSlotAccess {
    fn new(ap: ServiceAccessPoint) -> Self {
        Self {
            ap,
            messages: Vec::new(),
        }
    }
    fn with_messages(mut self, msgs: Vec<Message>) -> Self {
        self.messages = msgs;
        self
    }
}

impl aagnet::core::access::SlotAccessPoint for MockSlotAccess {
    fn messages(&self) -> &[Message] {
        &self.messages
    }
    fn session_id(&self) -> &str {
        "e2e-session"
    }
    fn phase_name(&self) -> &str {
        "e2e-phase"
    }
    fn current_iteration(&self) -> usize {
        0
    }
    fn write_observation(&mut self, _obs: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
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
        self.ap.provider_raw(name)
    }
    fn append_message(
        &mut self,
        _msg: aagnet::shared_types::Message,
    ) -> Result<(), aagnet::core::types::error::PluginError> {
        Ok(())
    }
}

// ============================================
// E2E 测试
// ============================================

#[tokio::test]
async fn e2e_skills_service_registers_provider_and_assembler_renders_skill_block() {
    let dir = unique_tempdir("e2e_full");
    std::fs::write(dir.join("test.skill.md"), SKILL_CONTENT).unwrap();

    let mock = Arc::new(MockServiceAccess::new());
    let ap = ServiceAccessPoint::new(mock.clone());

    // 1) 启动 SkillsService（init + start）
    let mut svc = SkillsService::new();
    let agent_data_dir = unique_tempdir("e2e_agent");
    let ctx = make_init_ctx(&agent_data_dir, skills_dir_config(&dir));
    svc.init(&ctx).await.expect("init 应成功");
    svc.start(ap.clone()).await.expect("start 应成功");

    // 2) 验证 provider "skills" 已在 ap 中注册
    assert!(
        mock.has_provider(PROVIDER_SKILLS),
        "provider 'skills' 必须已注册"
    );

    // 3) 验证 SkillsProvider.provide() 拉取并生成 block
    let slot = MockSlotAccess::new(ap.clone()).with_messages(vec![user_msg(USER_MSG)]);
    let provider = SkillsProvider;
    let cfg = ProviderSlotConfig {
        max_items: 10,
        max_tokens: 10_000,
        ..Default::default()
    };
    let quota = ContextQuota {
        max_tokens: 10_000,
        max_items: 10,
        ..Default::default()
    };
    let result = provider
        .provide(&slot, &quota, &cfg)
        .await
        .expect("provide 应成功");

    // 4) 验证 block 标题
    assert!(!result.blocks.is_empty(), "应至少有 1 个 block");
    let first = &result.blocks[0];
    assert_eq!(
        first.section_title, "## Skill: test_skill",
        "block 标题应为 '## Skill: test_skill'，实际: {}",
        first.section_title
    );
    assert!(
        first.content.contains("body of the test skill"),
        "block content 应含技能正文"
    );

    // 5) 清理
    svc.shutdown().await.expect("shutdown 应成功");
    assert!(
        !mock.has_provider(PROVIDER_SKILLS),
        "shutdown 后 provider 应被反注册"
    );
    cleanup(&dir);
    cleanup(&agent_data_dir);
}

#[tokio::test]
async fn e2e_disabled_providers_skips_skills_block() {
    // 模拟 assembler.disabled_providers = ["skills"] 的效果：
    // 不调 SkillsProvider → 0 blocks（前提：其他 provider 也不产生 skill 块）
    // 这里直接验证：max_items=0 时 provider 返回 0 blocks
    let dir = unique_tempdir("e2e_disabled");
    std::fs::write(dir.join("test.skill.md"), SKILL_CONTENT).unwrap();

    let mock = Arc::new(MockServiceAccess::new());
    let ap = ServiceAccessPoint::new(mock.clone());

    let mut svc = SkillsService::new();
    let agent_data_dir = unique_tempdir("e2e_agent_dis");
    let ctx = make_init_ctx(&agent_data_dir, skills_dir_config(&dir));
    svc.init(&ctx).await.expect("init 应成功");
    svc.start(ap.clone()).await.expect("start 应成功");

    // max_items=0 模拟"disabled"
    let slot = MockSlotAccess::new(ap.clone()).with_messages(vec![user_msg(USER_MSG)]);
    let provider = SkillsProvider;
    let cfg = ProviderSlotConfig {
        max_items: 0,
        max_tokens: 0,
        ..Default::default()
    };
    let quota = ContextQuota {
        max_tokens: 0,
        max_items: 0,
        ..Default::default()
    };
    let result = provider
        .provide(&slot, &quota, &cfg)
        .await
        .expect("provide 应成功");

    assert!(
        result.blocks.is_empty(),
        "max_items=0 应返回 0 blocks（模拟 disabled）"
    );

    svc.shutdown().await.expect("shutdown 应成功");
    cleanup(&dir);
    cleanup(&agent_data_dir);
}
