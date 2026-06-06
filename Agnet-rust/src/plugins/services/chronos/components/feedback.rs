use super::{FeedbackService, FeedbackSignal, FeedbackStats, FeedbackType};
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

const NAME: &str = "feedback";

pub struct FeedbackEngineComponent {
    stats: Arc<RwLock<FeedbackStats>>,
    init_done: bool,
}

impl Default for FeedbackEngineComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedbackEngineComponent {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(FeedbackStats::default())),
            init_done: false,
        }
    }
}

#[async_trait]
impl FeedbackService for FeedbackEngineComponent {
    async fn process_feedback(&self, signal: FeedbackSignal) -> Result<(), String> {
        let mut stats = self.stats.write().await;
        match signal.feedback_type {
            FeedbackType::Positive => stats.positive += 1,
            FeedbackType::Negative => stats.negative += 1,
            FeedbackType::Neutral => stats.neutral += 1,
        }
        Ok(())
    }
    async fn get_stats(&self) -> FeedbackStats {
        self.stats.read().await.clone()
    }
}

#[async_trait]
impl Component for FeedbackEngineComponent {
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
