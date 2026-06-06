/*!
 * ChronosServicePlugin —— 自适应定时调度服务
 *
 * 实现 ServicePlugin trait，通过 ServiceAccessPoint 与核心交互。
 * 主循环委托给 ChronosOrchestrator 进行组件编排。
 */

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;

use super::components::action_executor::ActionExecutorComponent;
use super::components::decision_engine::DecisionEngineComponent;
use super::components::feedback::FeedbackEngineComponent;
use super::components::rule_engine::RuleEngineComponent;
use super::components::sample_store::SampleStoreComponent;
use super::components::state_encoder::StateEncoderComponent;
use super::components::task_queue::TaskQueueComponent;
use super::components::timer::AdaptiveTimerComponent;
use super::components::tool_bridge::ToolBridgeComponent;
use super::components::{
    ActionExecutorService, AdaptiveTimerService, DecisionEngineService, FeedbackService,
    RuleEngineService, SampleStoreService, StateEncoderService, TaskQueueService,
    ToolBridgeService,
};
use super::config::ChronosConfig;
use super::orchestrator::ChronosOrchestrator;
use crate::shared_types::chronos::{
    ChronosContract, ChronosError, ChronosStatus, PROVIDER_CHRONOS,
};
use crate::shared_types::DynProvider;

/// Chronos 内部状态
struct ChronosInner {
    config: ChronosConfig,
    orchestrator: ChronosOrchestrator,
    timer: Arc<dyn AdaptiveTimerService>,
    task_queue: Arc<dyn TaskQueueService>,
    state_encoder: Arc<dyn StateEncoderService>,
    rule_engine: Arc<dyn RuleEngineService>,
    decision_engine: Arc<dyn DecisionEngineService>,
    action_executor: Arc<dyn ActionExecutorService>,
    feedback: Arc<dyn FeedbackService>,
    /// SampleStore — P1 组件，未来用于样本收集
    #[allow(dead_code)]
    sample_store: Arc<dyn SampleStoreService>,
    /// ToolBridge — P1 组件，未来用于工具桥接
    #[allow(dead_code)]
    tool_bridge: Arc<dyn ToolBridgeService>,
    last_interaction_at: Option<chrono::DateTime<chrono::Utc>>,
    running: bool,
    suspended: bool,
}

/// Chronos 服务插件
pub struct ChronosServicePlugin {
    inner: Arc<RwLock<Option<ChronosInner>>>,
}

impl ChronosServicePlugin {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }
}

impl Clone for ChronosServicePlugin {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[async_trait]
impl ServicePlugin for ChronosServicePlugin {
    fn name(&self) -> &str {
        "chronos"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut config: ChronosConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("chronos: 配置解析失败: {}", e)))?;

        config.validate()?;
        config.resolve_paths();

        let timer: Arc<dyn AdaptiveTimerService> =
            Arc::new(AdaptiveTimerComponent::new(config.timing.clone()));
        let task_queue: Arc<dyn TaskQueueService> =
            Arc::new(TaskQueueComponent::new(config.storage.clone()));
        let state_encoder: Arc<dyn StateEncoderService> =
            Arc::new(StateEncoderComponent::new(config.state.clone()));
        let rule_engine: Arc<dyn RuleEngineService> = Arc::new(RuleEngineComponent::new());
        let decision_engine: Arc<dyn DecisionEngineService> =
            Arc::new(DecisionEngineComponent::new(config.decision.clone()));
        // ActionExecutor 不再被 Orchestrator 管理，直接创建
        let action_executor: Arc<dyn ActionExecutorService> =
            Arc::new(ActionExecutorComponent::new(config.actions.clone()));
        let feedback: Arc<dyn FeedbackService> = Arc::new(FeedbackEngineComponent::new());

        let sample_store: Arc<dyn SampleStoreService> =
            Arc::new(SampleStoreComponent::new(config.storage.clone()));
        let tool_bridge: Arc<dyn ToolBridgeService> = Arc::new(ToolBridgeComponent::new());

        let mut orchestrator = ChronosOrchestrator::new();
        // 注册 8 个组件（ActionExecutor 不再注册，改为动态查询 ToolProvider）
        orchestrator.register(
            Box::new(AdaptiveTimerComponent::new(config.timing.clone())),
            10,
        );
        orchestrator.register(
            Box::new(TaskQueueComponent::new(config.storage.clone())),
            10,
        );
        orchestrator.register(
            Box::new(StateEncoderComponent::new(config.state.clone())),
            10,
        );
        orchestrator.register(Box::new(FeedbackEngineComponent::new()), 10);
        orchestrator.register(
            Box::new(SampleStoreComponent::new(config.storage.clone())),
            10,
        );
        orchestrator.register(Box::new(ToolBridgeComponent::new()), 10);
        orchestrator.register(Box::new(RuleEngineComponent::new()), 20);
        orchestrator.register(
            Box::new(DecisionEngineComponent::new(config.decision.clone())),
            20,
        );
        orchestrator.sort();
        orchestrator
            .init_all()
            .await
            .map_err(|e| PluginError::InitFailed(format!("Chronos: 组件初始化失败: {}", e)))?;

        let inner = ChronosInner {
            config,
            orchestrator,
            timer,
            task_queue,
            state_encoder,
            rule_engine,
            decision_engine,
            action_executor,
            feedback,
            sample_store,
            tool_bridge,
            last_interaction_at: Some(Utc::now()),
            running: false,
            suspended: false,
        };

        *self.inner.write().await = Some(inner);

        tracing::info!("ChronosServicePlugin: 初始化完成");
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| PluginError::InitFailed("Chronos: inner 未初始化".to_string()))?;

        inner.running = true;
        tracing::debug!(
            "Chronos: max_polling_interval={}",
            inner.config.max_polling_interval_secs
        );
        let _ = inner.feedback.get_stats().await;

        if let Err(e) = inner.task_queue.load().await {
            tracing::warn!("Chronos: 加载任务队列失败（非致命）: {}", e);
        }

        // 注册 Provider
        ap.register_provider(
            PROVIDER_CHRONOS,
            Arc::new(DynProvider(Arc::new(self.clone()))),
        );

        // 启动后台主循环
        let inner_clone = Arc::clone(&self.inner);
        let ap_clone = ap.clone();
        tokio::spawn(async move {
            Self::run_loop(inner_clone, ap_clone).await;
        });

        tracing::info!("ChronosServicePlugin: 已启动");
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| PluginError::InitFailed("Chronos: inner 未初始化".to_string()))?;

        match signal {
            ServiceSignal::GracefulShutdown => {
                inner.running = false;
                tracing::info!("Chronos: 收到 GracefulShutdown");
            }
            ServiceSignal::ImmediateShutdown => {
                inner.running = false;
                inner.suspended = true;
                tracing::info!("Chronos: 收到 ImmediateShutdown");
            }
            ServiceSignal::ConfigReload => {
                tracing::info!("Chronos: 收到 ConfigReload（配置将在下一轮 tick 生效）");
            }
            ServiceSignal::HealthCheck => {
                return Ok(());
            }
            ServiceSignal::Suspend => {
                inner.suspended = true;
                tracing::info!("Chronos: 已暂停");
            }
            ServiceSignal::Resume => {
                inner.suspended = false;
                tracing::info!("Chronos: 已恢复");
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        if let Some(inner) = self.inner.write().await.as_mut() {
            inner.running = false;
        }
        tracing::info!("ChronosServicePlugin: 已停止");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        if let Some(mut inner) = guard.take() {
            // 保存任务队列
            if let Err(e) = inner.task_queue.save().await {
                tracing::warn!("Chronos: 关闭前保存任务队列失败: {}", e);
            }

            // 关闭所有组件
            if let Err(e) = inner.orchestrator.shutdown_all().await {
                tracing::warn!("Chronos: 组件关闭失败: {}", e);
            }
        }
        tracing::info!("ChronosServicePlugin: 已关闭");
        Ok(())
    }
}

impl ChronosServicePlugin {
    /// 后台主循环（每秒 tick）
    async fn run_loop(inner: Arc<RwLock<Option<ChronosInner>>>, ap: ServiceAccessPoint) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            let guard = inner.read().await;
            let inner = match guard.as_ref() {
                Some(i) if i.running && !i.suspended => i,
                Some(_) => continue,
                None => break,
            };

            // 1. 状态编码
            let pending = inner.task_queue.pending_count().await;
            let urgent = 0; // TODO: 紧急任务计数
            let snapshot = inner
                .state_encoder
                .encode(inner.last_interaction_at, pending, urgent);

            // 2. 计算轮询间隔
            let is_urgent = urgent > 0;
            let _next_interval = inner.timer.calculate_interval(&snapshot, is_urgent);

            // 3. 规则决策
            let rule_decision = inner.rule_engine.decide(&snapshot);

            // 4. 获取到期任务
            let due_tasks = match inner.task_queue.pop_due_tasks().await {
                Ok(tasks) => tasks,
                Err(e) => {
                    tracing::warn!("Chronos: 获取到期任务失败: {}", e);
                    continue;
                }
            };

            // 5. 决策引擎
            let decision = match inner
                .decision_engine
                .decide(&snapshot, &due_tasks, rule_decision)
                .await
            {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!("Chronos: 决策失败: {}", e);
                    continue;
                }
            };

            // 6. 执行动作（传入 ap，让 ActionExecutor 动态查询 ToolProvider）
            match inner
                .action_executor
                .execute(&decision, &*inner.task_queue, &ap)
                .await
            {
                Ok(count) if count > 0 => {
                    tracing::info!("Chronos: 执行了 {} 个动作", count);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Chronos: 执行动作失败: {}", e);
                }
            }

            drop(guard);
        }
    }
}

impl Default for ChronosServicePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChronosContract for ChronosServicePlugin {
    async fn status(&self) -> ChronosStatus {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            Some(inner) => ChronosStatus {
                running: inner.running,
                suspended: inner.suspended,
                pending_tasks: inner.task_queue.pending_count().await,
            },
            None => ChronosStatus {
                running: false,
                suspended: false,
                pending_tasks: 0,
            },
        }
    }

    async fn suspend(&self) -> Result<(), ChronosError> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or(ChronosError::NotInitialized)?;
        inner.suspended = true;
        Ok(())
    }

    async fn resume(&self) -> Result<(), ChronosError> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or(ChronosError::NotInitialized)?;
        inner.suspended = false;
        Ok(())
    }
}
