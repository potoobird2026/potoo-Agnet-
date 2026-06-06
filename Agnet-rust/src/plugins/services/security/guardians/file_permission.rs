/*!
 * FilePermissionGuardian —— 文件权限检测
 *
 * priority=80，检测 ResourceType::File / ResourceType::Directory
 * 路径不在 allowed_dirs 内 → Deny
 * 路径在 allowed_dirs 内 → Allow（短路）
 *
 * 配置来源：guardian_configs["file_permission"].allowed_dirs
 */

use async_trait::async_trait;
use std::path::Path;

use super::super::config::GuardianConfig;
use super::super::models::{Action, GuardResult, Resource, ResourceType, Subject};
use super::Guardian;

const GUARDIAN_NAME: &str = "file_permission";
const GUARDIAN_PRIORITY: i32 = 80;

/// 文件权限检测 Guardian
pub struct FilePermissionGuardian {
    config: GuardianConfig,
}

impl FilePermissionGuardian {
    pub fn new(config: GuardianConfig) -> Self {
        Self { config }
    }

    /// 检查文件路径是否在允许目录内
    fn check_permission(&self, path_str: &str) -> Option<GuardResult> {
        // 如果没有配置 allowed_dirs，默认拒绝所有文件访问
        if self.config.allowed_dirs.is_empty() {
            return Some(GuardResult::Deny(
                "文件权限检测：未配置允许目录白名单，拒绝所有文件访问".to_string(),
            ));
        }

        let path = Path::new(path_str);

        // 规范化路径以进行比对
        let check_path = if let Ok(canonical) = path.canonicalize() {
            canonical
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            // 相对路径：以当前目录为基准
            std::env::current_dir().unwrap_or_default().join(path)
        };

        let in_allowed = self
            .config
            .allowed_dirs
            .iter()
            .any(|dir| check_path.starts_with(dir));

        if in_allowed {
            Some(GuardResult::Allow)
        } else {
            Some(GuardResult::Deny(format!(
                "文件权限检测：路径 '{}' 不在允许目录范围内。允许目录: {:?}",
                path_str, self.config.allowed_dirs
            )))
        }
    }
}

#[async_trait]
impl Guardian for FilePermissionGuardian {
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
            ResourceType::File | ResourceType::Directory => {
                self.check_permission(&resource.identifier)
            }
            _ => None,
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

    fn make_resource(path: &str) -> Resource {
        Resource {
            resource_type: ResourceType::File,
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
            tool_name: "read_file".to_string(),
            operation: super::super::super::models::Operation::Read,
            arguments: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn test_no_allowed_dirs_denies_all() {
        let guardian = FilePermissionGuardian::new(make_config(vec![]));
        let resource = make_resource("/tmp/test.txt");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }

    #[tokio::test]
    async fn test_non_file_resource_skipped() {
        let guardian = FilePermissionGuardian::new(make_config(vec!["/tmp".to_string()]));
        let mut resource = make_resource("any");
        resource.resource_type = ResourceType::NetworkHost;
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_path_in_allowed_dir_allowed() {
        let temp = std::env::temp_dir();
        let guardian =
            FilePermissionGuardian::new(make_config(vec![temp.to_string_lossy().to_string()]));
        let resource = make_resource(temp.join("test.txt").to_str().unwrap());
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        // 文件可能不存在，但权限检查应在白名单内通过
        assert!(matches!(result, Some(GuardResult::Allow)));
    }

    #[tokio::test]
    async fn test_path_outside_allowed_denied() {
        let guardian = FilePermissionGuardian::new(make_config(vec!["/tmp".to_string()]));
        let resource = make_resource("/etc/shadow");
        let result = guardian
            .evaluate(&make_subject(), &make_action(), &resource)
            .await;
        assert!(matches!(result, Some(GuardResult::Deny(_))));
    }
}
