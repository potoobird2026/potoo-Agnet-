/*!
 * Chronos 类型定义
 *
 * 包含调度服务所需的所有数据结构：
 * StateSnapshot、ScheduledTask、Decision、Action、FeedbackSignal 等。
 */

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ============================================
// 时间分类
// ============================================

/// 一天中的时间段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeCategory {
    /// 早晨（6-12）
    Morning,
    /// 下午（12-18）
    Afternoon,
    /// 晚间（18-22）
    Evening,
    /// 深夜（22-6）
    Night,
}

// ============================================
// 空闲等级
// ============================================

/// 用户空闲等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IdleLevel {
    /// 活跃（最近交互 < active_threshold）
    Active,
    /// 正常
    Normal,
    /// 空闲（> idle_threshold）
    Idle,
    /// 休眠（长时间无交互）
    Dormant,
}

// ============================================
// StateSnapshot —— 状态快照
// ============================================

/// 系统状态快照
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// 时间分类
    pub time_category: TimeCategory,
    /// 空闲等级
    pub idle_level: IdleLevel,
    /// 待处理任务数
    pub pending_task_count: usize,
    /// 紧急任务数
    pub urgent_count: usize,
    /// 上次交互距今时长
    pub last_interaction_age: Duration,
}

// ============================================
// ScheduledTask —— 定时任务
// ============================================

/// 任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    /// 提醒
    Reminder,
    /// 维护
    Maintenance,
    /// 主动发起
    ProactiveAction,
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// 待执行
    Pending,
    /// 执行中
    Running,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

/// 定时任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    /// 任务 ID（UUID）
    pub id: String,
    /// 任务类型
    pub task_type: TaskType,
    /// 预定执行时间
    pub scheduled_at: DateTime<Utc>,
    /// 任务负载
    pub payload: Value,
    /// 任务状态
    pub status: TaskStatus,
    /// 重试次数
    pub retry_count: u8,
}

impl ScheduledTask {
    pub fn new(task_type: TaskType, scheduled_at: DateTime<Utc>, payload: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_type,
            scheduled_at,
            payload,
            status: TaskStatus::Pending,
            retry_count: 0,
        }
    }

    /// 最大重试次数
    pub const MAX_RETRIES: u8 = 3;
}

// ============================================
// Decision —— 决策
// ============================================

/// Chronos 决策
#[derive(Debug, Clone)]
pub enum Decision {
    /// 执行动作
    Execute { actions: Vec<Action> },
    /// 跳过
    Skip { reason: String },
    /// 升级（需要人工干预）
    Escalate { reason: String, timeout: Duration },
}

// ============================================
// RuleDecision —— 规则决策
// ============================================

/// 规则引擎决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    /// 执行
    Execute,
    /// 跳过
    Skip,
    /// 升级
    Escalate,
    /// 无匹配（交给 LLM 决策）
    None,
}

// ============================================
// Action —— 动作
// ============================================

/// 动作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// 通知提醒
    Notify,
    /// 执行工具
    ExecuteTool,
    /// 主动消息
    ProactiveMessage,
}

/// 执行动作
#[derive(Debug, Clone)]
pub struct Action {
    /// 动作类型
    pub action_type: ActionType,
    /// 动作负载
    pub payload: Value,
    /// 优先级（0-255，越高越优先）
    pub priority: u8,
}

// ============================================
// FeedbackSignal —— 反馈信号
// ============================================

/// 反馈类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackType {
    /// 正面反馈
    Positive,
    /// 负面反馈
    Negative,
    /// 中性
    Neutral,
}

/// 反馈信号
#[derive(Debug, Clone)]
pub struct FeedbackSignal {
    /// 关联动作 ID
    pub action_id: String,
    /// 反馈类型
    pub feedback_type: FeedbackType,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_task_new() {
        let task = ScheduledTask::new(
            TaskType::Reminder,
            Utc::now(),
            serde_json::json!({"msg": "test"}),
        );
        assert_eq!(task.task_type, TaskType::Reminder);
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.retry_count, 0);
        assert!(!task.id.is_empty());
    }

    #[test]
    fn test_max_retries() {
        assert_eq!(ScheduledTask::MAX_RETRIES, 3);
    }

    #[test]
    fn test_time_category_variants() {
        let cats = [
            TimeCategory::Morning,
            TimeCategory::Afternoon,
            TimeCategory::Evening,
            TimeCategory::Night,
        ];
        assert_eq!(cats.len(), 4);
    }

    #[test]
    fn test_idle_level_ordering() {
        assert!(IdleLevel::Active < IdleLevel::Normal);
        assert!(IdleLevel::Normal < IdleLevel::Idle);
        assert!(IdleLevel::Idle < IdleLevel::Dormant);
    }

    #[test]
    fn test_rule_decision_variants() {
        let _ = RuleDecision::Execute;
        let _ = RuleDecision::Skip;
        let _ = RuleDecision::Escalate;
        let _ = RuleDecision::None;
    }
}
