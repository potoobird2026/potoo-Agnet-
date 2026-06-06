/*!
 * Guardian trait —— 可插拔的安全检查器
 *
 * 每个 Guardian 实现 evaluate() 方法，返回 Option<GuardResult>：
 * - None: 该 Guardian 不适用于当前操作（跳过）
 * - Some(Deny): 拒绝操作
 * - Some(Allow): 放行操作
 * - Some(Guard): 标记问题
 * - Some(Approve): 需要审批
 *
 * priority() 越大越先执行。
 */

use async_trait::async_trait;

use crate::plugins::services::security::models::{Action, GuardResult, Resource, Subject};

/// 可插拔的安全检查器 trait
#[async_trait]
pub trait Guardian: Send + Sync {
    /// Guardian 名称（全局唯一）
    fn name(&self) -> &str;

    /// 优先级（越高越先执行）
    fn priority(&self) -> i32;

    /// 是否启用
    fn enabled(&self) -> bool;

    /// 评估当前操作的安全性
    ///
    /// 返回 `None` 表示该 Guardian 不适用于当前操作（应被跳过）。
    /// 返回 `Some(GuardResult)` 表示 Guard/Deny/Allow/Approve 决策。
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Option<GuardResult>;
}

/// 按 priority 降序排列 Guardian 列表
pub fn sort_guardians(guardians: &mut [Box<dyn Guardian>]) {
    guardians.sort_by_key(|g| -g.priority());
}

pub mod command_injection;
pub mod file_permission;
pub mod network_access;
pub mod path_traversal;
