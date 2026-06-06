/*!
 * PathTraversalGuardian —— 路径穿越检测
 *
 * priority=100，检测 ResourceType::File / ResourceType::Directory
 * 使用 Path::canonicalize() 解析后对比 allowed_dirs 前缀。
 *
 * 配置来源：guardian_configs["path_traversal"].allowed_dirs
 */

use async_trait::async_trait;
use std::path::Path;

use super::super::config::GuardianConfig;
use super::super::models::{Action, GuardResult, Resource, ResourceType, Subject};
use super::Guardian;

const GUARDIAN_NAME: &str = "path_traversal";
const GUARDIAN_PRIORITY: i32 = 100;

/// 路径穿越检测 Guardian
pub struct PathTraversalGuardian {
    config: GuardianConfig,
}

impl PathTraversalGuardian {
    pub fn new(config: GuardianConfig) -> Self {
        Self { config }
    }

    /// 检查路径是否安全
    ///
    /// 检测项：
    /// 1. 路径包含 `..` 组件 → Deny
    /// 2. 绝对路径不在 allowed_dirs 内 → Deny
    /// 3. 符号链接目标在 allowed_dirs 外 → Deny
    fn check_path(&self, path_str: &str) -> Option<GuardResult> {
        let path = Path::new(path_str);

        // 检测 `..` 组件
        if path.components().any(|c| c.as_os_str() == "..") {
            return Some(GuardResult::Deny(format!(
                "路径穿越检测：'{}' 包含 '..' 组件，可能存在路径穿越风险",
                path_str
            )));
        }

        // 如果路径是绝对路径，或路径以 / 或 \ 开头（跨平台兼容：Windows 上 /etc/passwd 也是绝对路径语义），检查是否在允许目录内
        if path.is_absolute() || path_str.starts_with('/') || path_str.starts_with('\\') {
            // 尝试规范化路径
            if let Ok(canonical) = path.canonicalize() {
                let in_allowed = self
                    .config
                    .allowed_dirs
                    .iter()
                    .any(|dir| canonical.starts_with(dir));

                if !in_allowed {
                    return Some(GuardResult::Deny(format!(
                        "路径穿越检测：'{}' 解析为 '{}'，不在允许目录范围内",
                        path_str,
                        canonical.display()
                    )));
                }
            } else {
                // 路径不存在时，检查父目录
                if let Some(parent) = path.parent() {
                    if parent.exists() {
                        if let Ok(parent_canonical) = parent.canonicalize() {
                            let in_allowed = self
                                .config
                                .allowed_dirs
                                .iter()
                                .any(|dir| parent_canonical.starts_with(dir));

                            if !in_allowed {
                                return Some(GuardResult::Deny(format!(
                                    "路径穿越检测：'{}' 的父目录解析为 '{}'，不在允许目录范围内",
                                    path_str,
                                    parent_canonical.display()
                                )));
                            }
                        }
                    } else {
                        // 父目录也不存在——路径完全无法解析，默认拒绝（安全原则：默认拒绝）
                        return Some(GuardResult::Deny(format!(
                            "路径穿越检测：'{}' 无法解析（路径和父目录均不存在），拒绝访问",
                            path_str
                        )));
                    }
                } else {
                    // 路径无父目录（如 "/" 本身）但不存在——默认拒绝
                    return Some(GuardResult::Deny(format!(
                        "路径穿越检测：'{}' 为不存在的绝对路径，拒绝访问",
                        path_str
                    )));
                }
            }
        }

        // 路径在允许范围内 → Allow（短路）
        Some(GuardResult::Allow)
    }
}

#[async_trait]
impl Guardian for PathTraversalGuardian {
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
        // 仅处理文件/目录资源
        match resource.resource_type {
            ResourceType::File | ResourceType::Directory => self.check_path(&resource.identifier),
            _ => None, // 不适用于其他资源类型
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(allowed_dirs: Vec<String>) -> GuardianConfig {
        GuardianConfig {
            enabled: true,
            priority: GUARDIAN_PRIORITY,
            allowed_dirs,
            allowed_hosts: Vec::new(),
            denied_patterns: Vec::new(),
        }
    }

    fn make_resource(path: &str, is_dir: bool) -> Resource {
        Resource {
            resource_type: if is_dir {
                ResourceType::Directory
            } else {
                ResourceType::File
            },
            identifier: path.to_string(),
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
            tool_name: "test_tool".to_string(),
            operation: super::super::super::models::Operation::Read,
            arguments: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn test_dot_dot_path_denied() {
        let guardian = PathTraversalGuardian::new(make_config(vec!["/tmp".to_string()]));
        let resource = make_resource("../../etc/passwd", false);
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }

    #[tokio::test]
    async fn test_non_file_resource_skipped() {
        let guardian = PathTraversalGuardian::new(make_config(vec![]));
        let mut resource = make_resource("any", false);
        resource.resource_type = ResourceType::NetworkHost;
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(result.is_none(), "非文件资源应被跳过");
    }

    #[tokio::test]
    async fn test_absolute_path_outside_allowed_denied() {
        // 检测不在白名单的绝对路径
        let guardian = PathTraversalGuardian::new(make_config(vec!["/tmp".to_string()]));
        let resource = make_resource("/etc/passwd", false);
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        // /etc/passwd 不应在白名单内
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }
}
