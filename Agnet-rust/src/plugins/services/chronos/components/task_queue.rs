use super::super::config::StorageConfig;
use super::{ScheduledTask, TaskQueueService, TaskStatus};
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

const NAME: &str = "task_queue";

pub struct TaskQueueComponent {
    tasks: Arc<RwLock<Vec<ScheduledTask>>>,
    storage_config: StorageConfig,
    init_done: bool,
}

impl TaskQueueComponent {
    pub fn new(config: StorageConfig) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(Vec::new())),
            storage_config: config,
            init_done: false,
        }
    }
}

#[async_trait]
impl TaskQueueService for TaskQueueComponent {
    async fn add_task(&self, task: ScheduledTask) -> Result<(), String> {
        self.tasks.write().await.push(task);
        Ok(())
    }
    async fn pop_due_tasks(&self) -> Result<Vec<ScheduledTask>, String> {
        let now = Utc::now();
        let mut tasks = self.tasks.write().await;
        let (due, remaining): (Vec<_>, Vec<_>) = tasks
            .drain(..)
            .partition(|t| t.scheduled_at <= now && t.status == TaskStatus::Pending);
        *tasks = remaining;
        Ok(due)
    }
    async fn complete_task(&self, task_id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = TaskStatus::Completed;
        }
        Ok(())
    }
    async fn pending_count(&self) -> usize {
        self.tasks
            .read()
            .await
            .iter()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }
    async fn save(&self) -> Result<(), String> {
        let tasks = self.tasks.read().await;
        let json =
            serde_json::to_string_pretty(&*tasks).map_err(|e| format!("序列化失败: {}", e))?;
        if let Some(parent) = self.storage_config.task_queue_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        std::fs::write(&self.storage_config.task_queue_file, json)
            .map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }
    async fn load(&self) -> Result<(), String> {
        if !self.storage_config.task_queue_file.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(&self.storage_config.task_queue_file)
            .map_err(|e| format!("读取失败: {}", e))?;
        let loaded: Vec<ScheduledTask> =
            serde_json::from_str(&json).map_err(|e| format!("反序列化失败: {}", e))?;
        *self.tasks.write().await = loaded;
        Ok(())
    }
}

#[async_trait]
impl Component for TaskQueueComponent {
    fn name(&self) -> &str {
        NAME
    }
    async fn init(&mut self, _ctx: &ComponentInitContext) -> Result<(), ComponentError> {
        self.init_done = true;
        Ok(())
    }
    async fn process(
        &mut self,
        _ap: &mut dyn InternalAccessPoint,
    ) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }
    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
