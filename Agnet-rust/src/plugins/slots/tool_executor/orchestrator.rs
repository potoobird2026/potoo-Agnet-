use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing;

use super::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, InitContext, MetricsHandle,
    ModuleConfig, Processing,
};
use crate::plugins::slots::tool_executor::config::LOG_PREFIX;

/// 内部 AccessPoint 实现
pub(crate) struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    data_share: HashMap<String, Box<dyn Any + Send + Sync>>,
    config: ModuleConfig,
}

#[allow(dead_code)]
impl InternalAccessPointImpl {
    pub fn new(config: ModuleConfig) -> Self {
        Self {
            components: HashMap::new(),
            data_share: HashMap::new(),
            config,
        }
    }

    pub fn register_component(&mut self, handle: Box<dyn ComponentHandle>) {
        self.components.insert(handle.name().to_string(), handle);
    }

    pub fn write_boxed(&mut self, key: &str, val: Box<dyn Any + Send + Sync>) {
        self.data_share.insert(key.to_string(), val);
    }
}

impl AccessPoint for InternalAccessPointImpl {
    fn read_any(&self, key: &str) -> Option<&dyn Any> {
        self.data_share.get(key).map(|v| v.as_ref() as &dyn Any)
    }

    fn write_any(
        &mut self,
        key: &str,
        val: Box<dyn Any + Send + Sync>,
    ) -> Result<(), ComponentError> {
        self.data_share.insert(key.to_string(), val);
        Ok(())
    }

    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError> {
        self.components
            .get(name)
            .cloned()
            .ok_or_else(|| ComponentError::NotFound(name.to_string()))
    }

    fn config(&self) -> &ModuleConfig {
        &self.config
    }

    fn metrics(&self) -> &MetricsHandle {
        static METRICS: MetricsHandle = MetricsHandle;
        &METRICS
    }
}

/// 工具执行器编排器
pub(crate) struct ToolExecutorOrchestrator {
    components: Vec<Box<dyn Component>>,
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ModuleConfig,
}

impl ToolExecutorOrchestrator {
    pub fn new(config: ModuleConfig) -> Self {
        let ap = InternalAccessPointImpl::new(config.clone());
        Self {
            components: Vec::new(),
            access_point: Arc::new(RwLock::new(ap)),
            config,
        }
    }

    pub async fn register(&mut self, component: Box<dyn Component>) {
        let handle = component.clone_box();
        self.access_point.write().await.register_component(handle);
        self.components.push(component);
    }

    pub fn access_point(&self) -> Arc<RwLock<InternalAccessPointImpl>> {
        self.access_point.clone()
    }

    pub async fn init_all(&mut self) -> Result<(), ComponentError> {
        let ctx = InitContext::new(self.config.clone());
        for c in &mut self.components {
            c.init(&ctx).await?;
        }
        Ok(())
    }

    pub async fn process_all(&mut self) -> Result<(), ComponentError> {
        loop {
            let mut break_chain = false;
            let mut restart = false;
            {
                let ap_arc = self.access_point.clone();
                let mut ap = ap_arc.write().await;
                for component in self.components.iter_mut() {
                    match component.process(&mut *ap).await? {
                        Processing::Continue => {}
                        Processing::BreakChain => {
                            break_chain = true;
                            break;
                        }
                        Processing::Restart => {
                            restart = true;
                            break;
                        }
                        Processing::Warn { message } => {
                            tracing::warn!("{} {}", LOG_PREFIX, message);
                        }
                    }
                }
            }
            if break_chain {
                return Ok(());
            }
            if restart {
                continue;
            }
            return Ok(());
        }
    }

    pub async fn shutdown_all(&mut self) {
        for c in self.components.iter_mut().rev() {
            c.shutdown().await.ok();
        }
    }
}
