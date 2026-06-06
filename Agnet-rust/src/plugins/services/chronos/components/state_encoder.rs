use super::super::config::StateConfig;
use super::super::types::{IdleLevel, TimeCategory};
use super::{StateEncoderService, StateSnapshot};
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use chrono::{DateTime, Timelike, Utc};
use std::time::Duration;

const NAME: &str = "state_encoder";

pub struct StateEncoderComponent {
    config: StateConfig,
    init_done: bool,
}

impl StateEncoderComponent {
    pub fn new(config: StateConfig) -> Self {
        Self {
            config,
            init_done: false,
        }
    }
}

#[async_trait]
impl StateEncoderService for StateEncoderComponent {
    fn encode(
        &self,
        last_interaction: Option<DateTime<Utc>>,
        pending_count: usize,
        urgent_count: usize,
    ) -> StateSnapshot {
        let now = Utc::now();
        let time_category = match now.hour() {
            6..=11 => TimeCategory::Morning,
            12..=17 => TimeCategory::Afternoon,
            18..=21 => TimeCategory::Evening,
            _ => TimeCategory::Night,
        };
        let last_interaction_age = last_interaction
            .map(|t| {
                let delta = now.signed_duration_since(t);
                if delta.num_seconds() > 0 {
                    Duration::from_secs(delta.num_seconds() as u64)
                } else {
                    Duration::from_secs(0)
                }
            })
            .unwrap_or(Duration::from_secs(u64::MAX));
        let idle_level = if last_interaction_age.as_secs() < self.config.active_threshold_secs {
            IdleLevel::Active
        } else if last_interaction_age.as_secs() < self.config.idle_threshold_minutes * 60 {
            IdleLevel::Normal
        } else if last_interaction_age.as_secs() < self.config.idle_threshold_minutes * 60 * 6 {
            IdleLevel::Idle
        } else {
            IdleLevel::Dormant
        };
        StateSnapshot {
            time_category,
            idle_level,
            pending_task_count: pending_count,
            urgent_count,
            last_interaction_age,
        }
    }
}

#[async_trait]
impl Component for StateEncoderComponent {
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
