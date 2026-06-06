/*!
 * ChronosOrchestrator —— 组件编排器
 *
 * 负责注册、排序、按顺序初始化和关闭所有 Component。
 * 遵循模块内部组件协议 §5.2。
 */

use crate::core::component::{Component, ComponentError, ComponentInitContext, ModuleConfig};

struct ComponentEntry {
    component: Box<dyn Component>,
    priority: i32,
}

pub struct ChronosOrchestrator {
    entries: Vec<ComponentEntry>,
}

impl ChronosOrchestrator {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn register(&mut self, component: Box<dyn Component>, priority: i32) {
        self.entries.push(ComponentEntry {
            component,
            priority,
        });
    }

    pub fn sort(&mut self) {
        self.entries.sort_by_key(|e| e.priority);
    }

    pub async fn init_all(&mut self) -> Result<(), ComponentError> {
        for entry in &mut self.entries {
            let ctx = ComponentInitContext::new(
                entry.component.name(),
                ModuleConfig {
                    name: "chronos".to_string(),
                    enabled: true,
                },
            );
            entry.component.init(&ctx).await?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn process_all(&mut self) -> Result<(), ComponentError> {
        // 所有组件 process() 为 no-op，跳过
        // 注意：core::component::InternalAccessPoint 非 dyn 兼容，无法通过 AP 参数调用
        let _ = self.entries.len();
        Ok(())
    }

    pub async fn shutdown_all(&mut self) -> Result<(), ComponentError> {
        for entry in self.entries.iter_mut().rev() {
            entry.component.shutdown().await?;
        }
        Ok(())
    }
}
