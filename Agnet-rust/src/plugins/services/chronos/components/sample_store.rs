use super::super::config::StorageConfig;
use super::SampleStoreService;
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

const NAME: &str = "sample_store";

pub struct SampleStoreComponent {
    samples: Arc<RwLock<Vec<Value>>>,
    config: StorageConfig,
    init_done: bool,
}

impl SampleStoreComponent {
    pub fn new(config: StorageConfig) -> Self {
        Self {
            samples: Arc::new(RwLock::new(Vec::new())),
            config,
            init_done: false,
        }
    }
}

#[async_trait]
impl SampleStoreService for SampleStoreComponent {
    async fn store_sample(&self, data: Value) -> Result<(), String> {
        let mut samples = self.samples.write().await;
        samples.push(data);
        if samples.len() > self.config.max_samples {
            let excess = samples.len() - self.config.max_samples;
            samples.drain(0..excess);
        }
        Ok(())
    }
    async fn query_samples(&self, limit: usize) -> Result<Vec<Value>, String> {
        let samples = self.samples.read().await;
        let result: Vec<Value> = samples.iter().rev().take(limit).cloned().collect();
        Ok(result)
    }
    async fn cleanup(&self) -> Result<(), String> {
        let mut samples = self.samples.write().await;
        if samples.len() > self.config.max_samples {
            let excess = samples.len() - self.config.max_samples;
            samples.drain(0..excess);
        }
        Ok(())
    }
}

#[async_trait]
impl Component for SampleStoreComponent {
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
