use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::Timestamp;
use crate::plugins::slots::audit_phase::config::{
    AuditPhaseConfig, RiskAction, SensitiveRuleConfig, DEFAULT_AUDIT_LOG_CAPACITY,
};
use crate::plugins::slots::audit_phase::types::{AuditEvent, AuditResult};
use crate::shared_types::thought::{Action, Thought};
use crate::shared_types::context::{CONTEXT_AUDIT_LOG, CONTEXT_AUDIT_RESULT, CONTEXT_AUDIT_WARNINGS, CONTEXT_THOUGHT};
use crate::shared_types::{
    AuditContext, AuditWarning, DynProvider, RiskSeverity, SecurityDecision,
    SecurityPolicyProvider, PROVIDER_SECURITY,
};

pub struct AuditPhaseSlot {
    config: Option<AuditPhaseConfig>,
    compiled_rules: Vec<(SensitiveRuleConfig, regex::Regex)>,
}

impl AuditPhaseSlot {
    pub fn new() -> Self {
        Self {
            config: None,
            compiled_rules: Vec::new(),
        }
    }

    fn assess_risk(&self, tool_name: &str) -> RiskSeverity {
        let Some(config) = self.config.as_ref() else {
            return RiskSeverity::Low;
        };
        if config.high_risk_tools.contains(&tool_name.to_string()) {
            RiskSeverity::High
        } else if config.medium_risk_tools.contains(&tool_name.to_string()) {
            RiskSeverity::Medium
        } else {
            RiskSeverity::Low
        }
    }

    fn record_audit_event(
        &self,
        ap: &mut dyn SlotAccessPoint,
        action: &Action,
        result: &str,
        reason: &str,
    ) -> Result<(), PluginError> {
        let event = AuditEvent {
            timestamp: Timestamp::now(),
            session_id: ap.session_id().to_string(),
            tool_name: action.tool_name.clone(),
            result: result.to_string(),
            reason: reason.to_string(),
            risk_level: self.assess_risk(&action.tool_name),
        };

        let mut log_buffer: Vec<AuditEvent> = ap
            .read_context_raw(CONTEXT_AUDIT_LOG)
            .and_then(|any| any.downcast_ref::<Vec<AuditEvent>>())
            .cloned()
            .unwrap_or_default();

        log_buffer.push(event.clone());

        let capacity = self
            .config
            .as_ref()
            .map(|c| c.audit_log_capacity)
            .unwrap_or(DEFAULT_AUDIT_LOG_CAPACITY);
        if log_buffer.len() > capacity {
            log_buffer.drain(0..log_buffer.len() - capacity);
        }

        if let Err(e) = ap.write_context_raw(CONTEXT_AUDIT_LOG, Box::new(log_buffer)) {
            tracing::warn!("audit_phase: 写入 audit_log 失败: {}，继续", e);
        }

        match result {
            "passed" => {
                tracing::info!(
                    audit = true,
                    tool = %event.tool_name,
                    result = %event.result,
                    "审计通过"
                );
            }
            _ => {
                tracing::warn!(
                    audit = true,
                    tool = %event.tool_name,
                    result = %event.result,
                    reason = %event.reason,
                    "审计拦截"
                );
            }
        }

        Ok(())
    }
}

impl Default for AuditPhaseSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for AuditPhaseSlot {
    fn name(&self) -> &str {
        "audit_phase"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: AuditPhaseConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::InitFailed(format!("audit_phase 配置解析失败: {}", e)))?;

        let mut compiled_rules = Vec::new();
        for rule in &config.sensitive_rules {
            let re = regex::Regex::new(&rule.pattern).map_err(|e| {
                PluginError::InitFailed(format!(
                    "audit_phase: 正则编译失败 [{}]: {} - {}",
                    rule.name, rule.pattern, e
                ))
            })?;
            compiled_rules.push((rule.clone(), re));
        }

        self.config = Some(config);
        self.compiled_rules = compiled_rules;

        tracing::info!(
            "audit_phase: 初始化完成，编译 {} 条正则规则",
            self.compiled_rules.len()
        );
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // 步骤 1：从 StepContext 读取 Thought
        let thought = match ap.read_context_raw(CONTEXT_THOUGHT) {
            Some(any) => match any.downcast_ref::<Thought>() {
                Some(t) => t.clone(),
                None => {
                    tracing::warn!("audit_phase: thought 类型不匹配，跳过审计");
                    return Ok(SlotDirective::Continue);
                }
            },
            None => {
                tracing::debug!("audit_phase: StepContext 中无 Thought，跳过审计");
                return Ok(SlotDirective::Continue);
            }
        };

        // 步骤 2：判断 Thought 类型
        let action = match &thought {
            Thought::Action { action, .. } => action,
            Thought::Final { .. } => {
                tracing::debug!("audit_phase: Thought 为 Final，跳过审计");
                return Ok(SlotDirective::Continue);
            }
        };

        let mut audit_warnings: Vec<AuditWarning> = Vec::new();

        // 步骤 3：安全策略评估（Provider 扩展，可选）
        if let Some(ref config) = self.config {
            if config.enable_security_check {
                if let Some(raw) = ap.provider_raw(PROVIDER_SECURITY) {
                    if let Ok(wrapper) = raw.downcast::<DynProvider<dyn SecurityPolicyProvider>>() {
                        let audit_ctx = AuditContext {
                            session_id: ap.session_id().to_string(),
                            phase_name: ap.phase_name().to_string(),
                        };

                        match wrapper
                            .0
                            .evaluate(&action.tool_name, &action.arguments, &audit_ctx)
                            .await
                        {
                            Ok(SecurityDecision::Deny { reason }) => {
                                tracing::warn!(
                                    "audit_phase: 安全策略拒绝工具 {}: {}",
                                    action.tool_name,
                                    reason
                                );
                                if let Err(e) =
                                    self.record_audit_event(ap, action, "denied", &reason)
                                {
                                    tracing::warn!(
                                        "audit_phase: 审计日志写入失败: {}，继续拦截",
                                        e
                                    );
                                }
                                ap.write_context_raw(
                                    CONTEXT_AUDIT_RESULT,
                                    Box::new(AuditResult {
                                        passed: false,
                                        reason: reason.clone(),
                                    }),
                                )?;
                                return Ok(SlotDirective::AbortStep);
                            }
                            Ok(SecurityDecision::AllowWithWarnings { warnings }) => {
                                audit_warnings.extend(warnings);
                            }
                            Ok(SecurityDecision::Allow) => {}
                            Ok(SecurityDecision::RequireConfirmation { prompt }) => {
                                tracing::info!("audit_phase: 需要人工确认: {}", prompt);
                                ap.write_context_raw(
                                    CONTEXT_AUDIT_RESULT,
                                    Box::new(AuditResult {
                                        passed: false,
                                        reason: format!("需要人工确认: {}", prompt),
                                    }),
                                )?;
                                return Ok(SlotDirective::AbortStep);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "audit_phase: 安全策略引擎故障: {}，降级为内置规则",
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }

        // 步骤 4：内置风险评级
        let risk_level = self.assess_risk(&action.tool_name);

        match risk_level {
            RiskSeverity::Critical | RiskSeverity::High => {
                let high_risk_action = self
                    .config
                    .as_ref()
                    .map(|c| c.high_risk_action.clone())
                    .unwrap_or(RiskAction::Block);
                if high_risk_action == RiskAction::Block {
                    let reason = format!("高风险工具: {}", action.tool_name);
                    if let Err(e) = self.record_audit_event(ap, action, "blocked", &reason) {
                        tracing::warn!("audit_phase: 审计日志写入失败: {}，继续拦截", e);
                    }
                    ap.write_context_raw(
                        CONTEXT_AUDIT_RESULT,
                        Box::new(AuditResult {
                            passed: false,
                            reason: reason.clone(),
                        }),
                    )?;
                    return Ok(SlotDirective::AbortStep);
                } else {
                    audit_warnings.push(AuditWarning {
                        rule_name: "high_risk_tool".to_string(),
                        severity: RiskSeverity::High,
                        description: "高风险工具".to_string(),
                        detail: format!("工具 {} 在高风险列表中", action.tool_name),
                    });
                }
            }
            RiskSeverity::Medium => {
                audit_warnings.push(AuditWarning {
                    rule_name: "medium_risk_tool".to_string(),
                    severity: RiskSeverity::Medium,
                    description: "中风险工具".to_string(),
                    detail: format!("工具 {} 在中风险列表中", action.tool_name),
                });
            }
            RiskSeverity::Low => {}
            RiskSeverity::Info => {}
        }

        // 步骤 5：敏感信息检测
        if let Some(ref config) = self.config {
            if config.enable_sensitive_detection {
                let args_str = action.arguments.to_string();

                for (rule, compiled_re) in &self.compiled_rules {
                    if compiled_re.is_match(&args_str) {
                        let warning = AuditWarning {
                            rule_name: rule.name.clone(),
                            severity: rule.severity.clone(),
                            description: rule.description.clone(),
                            detail: format!(
                                "工具 {} 参数中检测到: {}",
                                action.tool_name, rule.description
                            ),
                        };

                        if rule.severity >= RiskSeverity::High {
                            if let Err(e) = self.record_audit_event(
                                ap,
                                action,
                                "sensitive_detected",
                                &rule.description,
                            ) {
                                tracing::warn!("audit_phase: 审计日志写入失败: {}，继续拦截", e);
                            }
                            ap.write_context_raw(
                                CONTEXT_AUDIT_RESULT,
                                Box::new(AuditResult {
                                    passed: false,
                                    reason: format!("敏感信息: {}", rule.description),
                                }),
                            )?;
                            return Ok(SlotDirective::AbortStep);
                        }

                        audit_warnings.push(warning);
                    }
                }
            }
        }

        // 步骤 6：写入审计结果（Continue 路径，非关键路径，降级处理）
        if !audit_warnings.is_empty() {
            if let Err(e) = ap.write_context_raw(CONTEXT_AUDIT_WARNINGS, Box::new(audit_warnings.clone()))
            {
                tracing::warn!("audit_phase: 写入 audit_warnings 失败: {}，跳过", e);
            }
            for w in &audit_warnings {
                tracing::warn!(
                    "audit_phase: 警告 [{:?}] {} - {}",
                    w.severity,
                    w.rule_name,
                    w.description
                );
            }
        }

        if let Err(e) = ap.write_context_raw(
            CONTEXT_AUDIT_RESULT,
            Box::new(AuditResult {
                passed: true,
                reason: String::new(),
            }),
        ) {
            tracing::warn!("audit_phase: 写入 audit_result 失败: {}，跳过", e);
        }

        // 步骤 7：记录审计日志（存入 StepContext，S-R03 合规）
        if let Some(ref config) = self.config {
            if config.enable_audit_log {
                if let Err(e) = self.record_audit_event(ap, action, "passed", "审计通过") {
                    tracing::warn!("audit_phase: 审计日志写入失败: {}，跳过", e);
                }
            }
        }

        // 步骤 8：返回 Continue（S-R01）
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("audit_phase: shutdown 完成");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::Value;

    use super::*;
    use crate::core::access::SlotAccessPoint;
    use crate::core::slot::SlotDirective;
    use crate::core::types::error::PluginError;
    use crate::core::types::Timestamp;
    use crate::plugins::slots::audit_phase::config::{
        AuditPhaseConfig, RiskAction, SensitiveRuleConfig,
    };
    use crate::plugins::slots::audit_phase::types::{AuditEvent, AuditResult};
    use crate::shared_types::thought::{Action, Thought};
    use crate::shared_types::{
        AuditContext, AuditWarning, DynProvider, RiskSeverity, SecurityDecision, SecurityError,
        SecurityPolicyProvider,
    };

    // ============================================
    // MockSlotAccessPoint
    // ============================================

    struct MockSlotAccessPoint {
        thought: Option<Thought>,
        session_id: String,
        phase_name: String,
        context_data: HashMap<String, Box<dyn Any + Send + Sync>>,
        security_provider: Option<Arc<dyn Any + Send + Sync>>,
        iteration: usize,
    }

    impl MockSlotAccessPoint {
        fn new(thought: Option<Thought>) -> Self {
            Self {
                thought,
                session_id: "test-session".to_string(),
                phase_name: "audit".to_string(),
                context_data: HashMap::new(),
                security_provider: None,
                iteration: 1,
            }
        }

        fn with_provider(mut self, provider: DynProvider<dyn SecurityPolicyProvider>) -> Self {
            self.security_provider = Some(Arc::new(provider));
            self
        }
    }

    impl SlotAccessPoint for MockSlotAccessPoint {
        fn messages(&self) -> &[crate::shared_types::Message] {
            &[]
        }

        fn session_id(&self) -> &str {
            &self.session_id
        }

        fn phase_name(&self) -> &str {
            &self.phase_name
        }

        fn current_iteration(&self) -> usize {
            self.iteration
        }

        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }

        fn write_context_raw(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            self.context_data.insert(key.to_string(), val);
            Ok(())
        }

        fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
            if key == "thought" {
                return self.thought.as_ref().map(|t| t as &(dyn Any + Send + Sync));
            }
            self.context_data
                .get(key)
                .map(|b| b.as_ref() as &(dyn Any + Send + Sync))
        }

        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }

        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }

        fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            if name == "security" {
                self.security_provider.clone()
            } else {
                None
            }
        }
    }

    // ============================================
    // MockSecurityProvider
    // ============================================

    struct MockSecurityProvider {
        decision: SecurityDecision,
        should_error: bool,
    }

    #[async_trait]
    impl SecurityPolicyProvider for MockSecurityProvider {
        async fn evaluate(
            &self,
            _tool_name: &str,
            _arguments: &Value,
            _context: &AuditContext,
        ) -> Result<SecurityDecision, SecurityError> {
            if self.should_error {
                Err(SecurityError::EngineError("模拟引擎故障".to_string()))
            } else {
                Ok(self.decision.clone())
            }
        }
    }

    // ============================================
    // 辅助函数
    // ============================================

    async fn create_slot_with_config(config: AuditPhaseConfig) -> AuditPhaseSlot {
        let ctx = PluginInitContext::new(
            "audit_phase",
            serde_json::to_value(config).expect("测试中安全"),
            crate::core::types::plugin::AgentConfig::default(),
            std::env::temp_dir().join("audit_phase_test"),
        );
        let mut slot = AuditPhaseSlot::new();
        slot.init(&ctx).await.expect("测试中安全");
        slot
    }

    fn default_config() -> AuditPhaseConfig {
        AuditPhaseConfig {
            enable_security_check: true,
            enable_sensitive_detection: true,
            sensitive_rules: vec![
                SensitiveRuleConfig {
                    name: "api_key".to_string(),
                    // JSON 序列化后格式为 {"api_key":"sk-..."}，key 带引号
                    pattern: r#"(?i)"(api_key|apikey)"\s*:\s*['"]?[a-zA-Z0-9_\-]{16,}['"]?"#
                        .to_string(),
                    description: "API 密钥泄露".to_string(),
                    severity: RiskSeverity::High,
                },
                SensitiveRuleConfig {
                    name: "private_key".to_string(),
                    pattern: r"-----BEGIN PRIVATE KEY-----".to_string(),
                    description: "私钥泄露".to_string(),
                    severity: RiskSeverity::Critical,
                },
            ],
            high_risk_tools: vec![
                "execute_command".to_string(),
                "write_file".to_string(),
                "delete_file".to_string(),
                "http_request".to_string(),
            ],
            medium_risk_tools: vec![
                "read_file".to_string(),
                "list_directory".to_string(),
                "search_web".to_string(),
            ],
            high_risk_action: RiskAction::Block,
            enable_audit_log: true,
            audit_log_capacity: 100,
        }
    }

    fn make_action(tool_name: &str, args: Value) -> Thought {
        Thought::Action {
            action: Action {
                tool_name: tool_name.to_string(),
                arguments: args,
                tool_call_id: None,
                tool_calls: None,
                created_at: Timestamp::now(),
            },
            reasoning: "test".to_string(),
            generated_at: Timestamp::now(),
        }
    }

    fn make_final() -> Thought {
        Thought::Final {
            answer: "最终答案".to_string(),
            reasoning: "done".to_string(),
            generated_at: Timestamp::now(),
        }
    }

    fn read_audit_result(ap: &MockSlotAccessPoint) -> Option<AuditResult> {
        ap.context_data
            .get("audit_result")
            .and_then(|b| b.downcast_ref::<AuditResult>())
            .cloned()
    }

    fn read_audit_warnings(ap: &MockSlotAccessPoint) -> Vec<AuditWarning> {
        ap.context_data
            .get("audit_warnings")
            .and_then(|b| b.downcast_ref::<Vec<AuditWarning>>())
            .cloned()
            .unwrap_or_default()
    }

    fn read_audit_log(ap: &MockSlotAccessPoint) -> Vec<AuditEvent> {
        ap.context_data
            .get("audit_log")
            .and_then(|b| b.downcast_ref::<Vec<AuditEvent>>())
            .cloned()
            .unwrap_or_default()
    }

    // ============================================
    // 测试用例
    // ============================================

    #[tokio::test]
    async fn test_low_risk_passes() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("search_web", serde_json::json!({"query": "rust"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_medium_risk_warns() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("read_file", serde_json::json!({"path": "/tmp/test.txt"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
        let warnings = read_audit_warnings(&ap);
        assert!(!warnings.is_empty());
        assert_eq!(warnings[0].rule_name, "medium_risk_tool");
    }

    #[tokio::test]
    async fn test_final_skips_audit() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_final();
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
        let result = read_audit_result(&ap);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_no_thought_skips() {
        let mut slot = create_slot_with_config(default_config()).await;
        let mut ap = MockSlotAccessPoint::new(None);

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
    }

    #[tokio::test]
    async fn test_high_risk_blocked() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("execute_command", serde_json::json!({"cmd": "rm -rf /"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::AbortStep);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(!result.passed);
        assert!(result.reason.contains("高风险工具"));
    }

    #[tokio::test]
    async fn test_high_risk_warn_mode() {
        let mut config = default_config();
        config.high_risk_action = RiskAction::Warn;
        let mut slot = create_slot_with_config(config).await;
        let thought = make_action("execute_command", serde_json::json!({"cmd": "ls"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
        let warnings = read_audit_warnings(&ap);
        assert!(!warnings.is_empty());
        assert_eq!(warnings[0].rule_name, "high_risk_tool");
    }

    #[tokio::test]
    async fn test_sensitive_api_key() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action(
            "read_file",
            serde_json::json!({"api_key": "sk-1234567890abcdef1234567890abcdef"}),
        );
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::AbortStep);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_sensitive_private_key() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action(
            "write_file",
            serde_json::json!({"content": "-----BEGIN PRIVATE KEY-----\nABCDEF=="}),
        );
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::AbortStep);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_security_deny() {
        let mut slot = create_slot_with_config(default_config()).await;
        let provider = DynProvider(Arc::new(MockSecurityProvider {
            decision: SecurityDecision::Deny {
                reason: "策略拒绝".to_string(),
            },
            should_error: false,
        }) as Arc<dyn SecurityPolicyProvider>);
        let thought = make_action("search_web", serde_json::json!({"q": "hello"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought)).with_provider(provider);

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::AbortStep);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_security_engine_fallback() {
        let mut slot = create_slot_with_config(default_config()).await;
        let provider = DynProvider(Arc::new(MockSecurityProvider {
            decision: SecurityDecision::Allow,
            should_error: true,
        }) as Arc<dyn SecurityPolicyProvider>);
        let thought = make_action("search_web", serde_json::json!({"q": "hello"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought)).with_provider(provider);

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
    }

    #[tokio::test]
    async fn test_provider_unavailable() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("search_web", serde_json::json!({"q": "hello"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
    }

    #[tokio::test]
    async fn test_regex_compile_invalid() {
        let mut config = default_config();
        config.sensitive_rules = vec![SensitiveRuleConfig {
            name: "bad".to_string(),
            pattern: "[invalid".to_string(),
            description: "无效正则".to_string(),
            severity: RiskSeverity::High,
        }];

        let ctx = PluginInitContext::new(
            "audit_phase",
            serde_json::to_value(config).expect("测试中安全"),
            crate::core::types::plugin::AgentConfig::default(),
            std::env::temp_dir().join("audit_phase_test"),
        );
        let mut slot = AuditPhaseSlot::new();
        let result = slot.init(&ctx).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_audit_log_append() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("search_web", serde_json::json!({"q": "rust"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        slot.run(&mut ap).await.expect("测试中安全");
        let log_after_first = read_audit_log(&ap);
        assert_eq!(log_after_first.len(), 1);

        let thought2 = make_action("search_web", serde_json::json!({"q": "tokio"}));
        ap.thought = Some(thought2);
        slot.run(&mut ap).await.expect("测试中安全");
        let log_after_second = read_audit_log(&ap);
        assert_eq!(log_after_second.len(), 2);
    }

    #[tokio::test]
    async fn test_empty_args() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("search_web", serde_json::Value::Null);
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let mut slot = create_slot_with_config(default_config()).await;
        let thought = make_action("unknown_tool_xyz", serde_json::json!({}));
        let mut ap = MockSlotAccessPoint::new(Some(thought));

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::Continue);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_audit_log_capacity() {
        let mut config = default_config();
        config.audit_log_capacity = 3;
        let mut slot = create_slot_with_config(config).await;
        let mut ap = MockSlotAccessPoint::new(None);

        let existing: Vec<AuditEvent> = (0..5)
            .map(|i| AuditEvent {
                timestamp: Timestamp::now(),
                session_id: "test".to_string(),
                tool_name: format!("tool_{}", i),
                result: "passed".to_string(),
                reason: "".to_string(),
                risk_level: RiskSeverity::Low,
            })
            .collect();
        ap.context_data
            .insert("audit_log".to_string(), Box::new(existing));

        let thought = make_action("search_web", serde_json::json!({"q": "test"}));
        ap.thought = Some(thought);
        slot.run(&mut ap).await.expect("测试中安全");

        let log = read_audit_log(&ap);
        assert_eq!(log.len(), 3);
    }

    #[tokio::test]
    async fn test_require_confirmation() {
        let mut slot = create_slot_with_config(default_config()).await;
        let provider = DynProvider(Arc::new(MockSecurityProvider {
            decision: SecurityDecision::RequireConfirmation {
                prompt: "确认执行?".to_string(),
            },
            should_error: false,
        }) as Arc<dyn SecurityPolicyProvider>);
        let thought = make_action("search_web", serde_json::json!({"q": "hello"}));
        let mut ap = MockSlotAccessPoint::new(Some(thought)).with_provider(provider);

        let directive = slot.run(&mut ap).await.expect("测试中安全");

        assert_eq!(directive, SlotDirective::AbortStep);
        let result = read_audit_result(&ap).expect("应写入 audit_result");
        assert!(!result.passed);
        assert!(result.reason.contains("人工确认"));
    }
}
