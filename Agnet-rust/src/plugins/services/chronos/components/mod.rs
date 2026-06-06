/*!
 * Chronos 内部组件
 *
 * 9 个组件，按优先级分组：
 * 组 A (10): AdaptiveTimer, TaskQueue, StateEncoder, FeedbackEngine, SampleStore, ToolBridge
 * 组 B (20): RuleEngine, DecisionEngine
 * 组 C (30): ActionExecutor
 *
 * 所有组件的 process() 为 no-op，业务逻辑由主循环驱动。
 */

use async_trait::async_trait;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::types::{
    Decision, FeedbackSignal, FeedbackType, RuleDecision, ScheduledTask, StateSnapshot, TaskStatus,
};

// ============================================
// 组 A：优先级 10（P0，无依赖）
// ============================================

pub mod action_executor;
pub mod decision_engine;
pub mod feedback;
pub mod rule_engine;
pub mod sample_store;
pub mod state_encoder;
pub mod task_queue;
pub mod timer;
pub mod tool_bridge;

// ============================================
// 业务接口 traits
// ============================================

/// 自适应定时器服务
#[async_trait]
pub trait AdaptiveTimerService: Send + Sync {
    fn calculate_interval(&self, snapshot: &StateSnapshot, is_urgent: bool) -> Duration;
}

/// 任务队列服务
#[async_trait]
pub trait TaskQueueService: Send + Sync {
    async fn add_task(&self, task: ScheduledTask) -> Result<(), String>;
    async fn pop_due_tasks(&self) -> Result<Vec<ScheduledTask>, String>;
    async fn complete_task(&self, task_id: &str) -> Result<(), String>;
    async fn pending_count(&self) -> usize;
    async fn save(&self) -> Result<(), String>;
    async fn load(&self) -> Result<(), String>;
}

/// 状态编码服务
#[async_trait]
pub trait StateEncoderService: Send + Sync {
    fn encode(
        &self,
        last_interaction: Option<DateTime<Utc>>,
        pending_count: usize,
        urgent_count: usize,
    ) -> StateSnapshot;
}

/// 规则引擎服务
#[async_trait]
pub trait RuleEngineService: Send + Sync {
    fn decide(&self, snapshot: &StateSnapshot) -> RuleDecision;
}

/// 决策引擎服务
#[async_trait]
pub trait DecisionEngineService: Send + Sync {
    async fn decide(
        &self,
        snapshot: &StateSnapshot,
        due_tasks: &[ScheduledTask],
        rule_decision: RuleDecision,
    ) -> Result<Decision, String>;
}

/// 动作执行器服务
#[async_trait]
pub trait ActionExecutorService: Send + Sync {
    async fn execute(
        &self,
        decision: &Decision,
        task_queue: &dyn TaskQueueService,
        ap: &crate::core::access::ServiceAccessPoint,
    ) -> Result<usize, String>;
}

/// 反馈服务
#[async_trait]
pub trait FeedbackService: Send + Sync {
    async fn process_feedback(&self, signal: FeedbackSignal) -> Result<(), String>;
    async fn get_stats(&self) -> FeedbackStats;
}

/// 样本存储服务
#[async_trait]
pub trait SampleStoreService: Send + Sync {
    async fn store_sample(&self, data: Value) -> Result<(), String>;
    async fn query_samples(&self, limit: usize) -> Result<Vec<Value>, String>;
    async fn cleanup(&self) -> Result<(), String>;
}

/// 工具桥接服务
#[async_trait]
pub trait ToolBridgeService: Send + Sync {
    async fn execute_tool(&self, name: &str, args: Value) -> Result<Value, String>;
}

/// 反馈统计
#[derive(Debug, Clone, Default)]
pub struct FeedbackStats {
    pub positive: u64,
    pub negative: u64,
    pub neutral: u64,
}
