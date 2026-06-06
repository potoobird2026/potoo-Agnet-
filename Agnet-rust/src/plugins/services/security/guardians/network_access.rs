/*!
 * NetworkAccessGuardian —— 网络访问控制
 *
 * priority=70，检测 ResourceType::NetworkHost
 * 域名/IP 白名单匹配 → Allow
 * 域名/IP 不匹配 → Deny（含具体原因）
 *
 * 配置来源：guardian_configs["network_access"].allowed_hosts
 */

use async_trait::async_trait;

use super::super::config::GuardianConfig;
use super::super::models::{Action, GuardResult, Resource, ResourceType, Subject};
use super::Guardian;

const GUARDIAN_NAME: &str = "network_access";
const GUARDIAN_PRIORITY: i32 = 70;

/// 网络访问控制 Guardian
pub struct NetworkAccessGuardian {
    config: GuardianConfig,
}

#[allow(dead_code)]
impl NetworkAccessGuardian {
    pub fn new(config: GuardianConfig) -> Self {
        Self { config }
    }

    /// 检查是否匹配 IP 地址格式（简易检测）
    fn looks_like_ip(host: &str) -> bool {
        host.parse::<std::net::IpAddr>().is_ok()
    }

    /// 检查主机是否在允许列表中
    ///
    /// 支持精确匹配和域名后缀匹配（如 `.example.com` 匹配所有子域名）。
    fn check_host_allowed(&self, host: &str) -> Option<GuardResult> {
        // 如果没有配置 allowed_hosts，默认拒绝所有网络访问
        if self.config.allowed_hosts.is_empty() {
            return Some(GuardResult::Deny(
                "网络访问控制：未配置允许主机白名单，拒绝所有网络访问".to_string(),
            ));
        }

        let host_lower = host.to_lowercase();

        for allowed in &self.config.allowed_hosts {
            let allowed_lower = allowed.to_lowercase();

            // 精确匹配
            if host_lower == allowed_lower {
                return Some(GuardResult::Allow);
            }

            // 后缀匹配（如 `.example.com` 匹配 `api.example.com`）
            if allowed_lower.starts_with('.') && host_lower.ends_with(&allowed_lower) {
                return Some(GuardResult::Allow);
            }
        }

        Some(GuardResult::Deny(format!(
            "网络访问控制：主机 '{}' 不在允许列表中。允许的主机: {:?}",
            host, self.config.allowed_hosts
        )))
    }
}

#[async_trait]
impl Guardian for NetworkAccessGuardian {
    fn name(&self) -> &str {
        GUARDIAN_NAME
    }

    fn priority(&self) -> i32 {
        GUARDIAN_PRIORITY
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    async fn evaluate(
        &self,
        _subject: &Subject,
        _action: &Action,
        resource: &Resource,
    ) -> Option<GuardResult> {
        match resource.resource_type {
            ResourceType::NetworkHost => self.check_host_allowed(&resource.identifier),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(allowed_hosts: Vec<String>) -> GuardianConfig {
        GuardianConfig {
            enabled: true,
            priority: GUARDIAN_PRIORITY,
            allowed_dirs: Vec::new(),
            allowed_hosts,
            denied_patterns: Vec::new(),
        }
    }

    fn make_resource(host: &str) -> Resource {
        Resource {
            resource_type: ResourceType::NetworkHost,
            identifier: host.to_string(),
            metadata: HashMap::new(),
        }
    }

    fn make_subject() -> Subject {
        Subject {
            session_id: "test".to_string(),
            session_type: super::super::super::models::SessionType::Interactive,
            metadata: HashMap::new(),
        }
    }

    fn make_action() -> Action {
        Action {
            tool_name: "http_request".to_string(),
            operation: super::super::super::models::Operation::NetworkAccess,
            arguments: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn test_no_allowed_hosts_denies_all() {
        let guardian = NetworkAccessGuardian::new(make_config(vec![]));
        let resource = make_resource("example.com");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }

    #[tokio::test]
    async fn test_exact_host_match_allowed() {
        let guardian = NetworkAccessGuardian::new(make_config(vec!["api.example.com".to_string()]));
        let resource = make_resource("api.example.com");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Allow)));
    }

    #[tokio::test]
    async fn test_wildcard_subdomain_match_allowed() {
        let guardian = NetworkAccessGuardian::new(make_config(vec![".example.com".to_string()]));
        let resource = make_resource("api.example.com");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Allow)));
    }

    #[tokio::test]
    async fn test_unlisted_host_denied() {
        let guardian = NetworkAccessGuardian::new(make_config(vec!["trusted.com".to_string()]));
        let resource = make_resource("evil.com");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }

    #[tokio::test]
    async fn test_non_network_resource_skipped() {
        let guardian = NetworkAccessGuardian::new(make_config(vec![]));
        let mut resource = make_resource("any");
        resource.resource_type = ResourceType::File;
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_case_insensitive_match() {
        let guardian = NetworkAccessGuardian::new(make_config(vec!["API.Example.COM".to_string()]));
        let resource = make_resource("api.example.com");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Allow)));
    }
}
