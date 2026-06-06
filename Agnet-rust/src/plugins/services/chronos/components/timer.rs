use super::super::config::TimingConfig;
use super::{AdaptiveTimerService, StateSnapshot};
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use std::time::Duration;

const NAME: &str = "adaptive_timer";

pub struct AdaptiveTimerComponent {
    config: TimingConfig,
    init_done: bool,
}

impl AdaptiveTimerComponent {
    pub fn new(config: TimingConfig) -> Self {
        Self {
            config,
            init_done: false,
        }
    }
}

#[async_trait]
impl AdaptiveTimerService for AdaptiveTimerComponent {
    fn calculate_interval(&self, snapshot: &StateSnapshot, is_urgent: bool) -> Duration {
        use super::super::types::IdleLevel;
        if is_urgent {
            return Duration::from_secs(self.config.min_interval_secs);
        }
        let base = self.config.polling_interval_base_secs as f64;
        let multiplier = match snapshot.idle_level {
            IdleLevel::Active => self.config.active_multiplier,
            IdleLevel::Normal => 1.0,
            IdleLevel::Idle | IdleLevel::Dormant => self.config.idle_multiplier,
        };
        let interval_secs = (base * multiplier) as u64;
        let clamped =
            interval_secs.clamp(self.config.min_interval_secs, self.config.max_interval_secs);
        Duration::from_secs(clamped)
    }
}

#[async_trait]
impl Component for AdaptiveTimerComponent {
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
