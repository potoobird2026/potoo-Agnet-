use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, InitContext, MetricsHandle,
    ModuleConfig, Processing,
};

#[allow(dead_code)]
pub(crate) struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    // 使用 Box<dyn Any + Send + Sync> 而非 + Send，确保 AccessPoint: Send + Sync
    data_share: HashMap<String, Box<dyn Any + Send + Sync>>,
    config: ModuleConfig,
}

impl InternalAccessPointImpl {
    pub fn new(config: ModuleConfig) -> Self {
        Self {
            components: HashMap::new(),
            data_share: HashMap::new(),
            config,
        }
    }
}

impl AccessPoint for InternalAccessPointImpl {
    fn read_any(&self, key: &str) -> Option<&dyn Any> {
        self.data_share.get(key).map(|b| b.as_ref() as &dyn Any)
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
            .map(|c| c.clone_box())
            .ok_or_else(|| ComponentError::NotFound(name.to_string()))
    }

    fn config(&self) -> &ModuleConfig {
        &self.config
    }

    fn metrics(&self) -> &MetricsHandle {
        // 设计文档 §2.1——react_loop 不收集指标，返回静态占位
        static METRICS: MetricsHandle = MetricsHandle;
        &METRICS
    }
}

pub struct Orchestrator {
    components: Vec<Box<dyn Component>>,
    parallel_groups: Vec<Vec<usize>>,
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ModuleConfig,
}

#[allow(dead_code)]
impl Orchestrator {
    pub fn new(config: ModuleConfig) -> Self {
        let ap = InternalAccessPointImpl::new(config.clone());
        Self {
            components: Vec::new(),
            parallel_groups: Vec::new(),
            access_point: Arc::new(RwLock::new(ap)),
            config,
        }
    }

    /// 在 SlotPlugin::init() 解析配置后更新模块级 ModuleConfig
    pub(crate) fn set_config(&mut self, config: ModuleConfig) {
        self.config = config;
    }

    pub fn access_point(&self) -> Arc<RwLock<InternalAccessPointImpl>> {
        self.access_point.clone()
    }

    /// 注册组件，校验 requires/provides，拓扑排序
    pub async fn register(&mut self, component: Box<dyn Component>) -> Result<(), ComponentError> {
        // 复制元数据字段（避免 component 被 move 后引用失效）
        let name = component.meta().name.to_string();
        let requires: Vec<&'static str> = component.meta().requires.to_vec();
        let _provides: Vec<&'static str> = component.meta().provides.to_vec();

        // 校验组件名称是否已存在
        for existing in &self.components {
            if existing.meta().name == name {
                return Err(ComponentError::Config(format!(
                    "duplicate component name: {}",
                    name
                )));
            }
        }

        // 校验 requires 是否被已有组件的 provides 覆盖
        for req in &requires {
            let mut found = false;
            for existing in &self.components {
                if existing.meta().provides.contains(req) {
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(ComponentError::NotFound(format!(
                    "requirement '{}' for component '{}' is not provided by any registered component",
                    req, name
                )));
            }
        }

        // 将组件句柄注入 InternalAccessPointImpl
        {
            let mut ap = self.access_point.write().await;
            ap.components.insert(name.clone(), component.clone_box());
        }

        // 存储组件并重新计算拓扑序
        self.components.push(component);
        self.recompute_groups();

        Ok(())
    }

    /// 按拓扑序串行初始化全部组件
    pub async fn init_all(&mut self) -> Result<(), ComponentError> {
        let groups = self.parallel_groups.clone();
        for group in &groups {
            for &idx in group {
                let ctx = InitContext {
                    config: self.config.clone(),
                };
                self.components[idx].init(&ctx).await?;
            }
        }
        // 刷新 InternalAccessPointImpl 句柄——init() 可能修改组件内部状态
        {
            let mut ap = self.access_point.write().await;
            ap.components.clear();
            for comp in &self.components {
                ap.components
                    .insert(comp.meta().name.to_string(), comp.clone_box());
            }
        }
        Ok(())
    }

    /// 按 DAG 序串行执行全部 process
    pub async fn process_all(&mut self) -> Result<(), ComponentError> {
        let groups = self.parallel_groups.clone();
        let mut ap = self.access_point.write().await;
        for group in &groups {
            for &idx in group {
                let result = self.components[idx].process(&mut *ap).await?;
                match result {
                    Processing::Continue => continue,
                    Processing::BreakChain => return Ok(()),
                    Processing::Restart => {
                        drop(ap);
                        return Box::pin(self.process_all()).await;
                    }
                    Processing::Warn { message } => {
                        tracing::warn!("[{}] {}", self.components[idx].meta().name, message);
                    }
                }
            }
        }
        Ok(())
    }

    /// 逆序关闭全部组件
    pub async fn shutdown_all(&mut self) {
        for c in self.components.iter_mut().rev() {
            c.shutdown().await.ok();
        }
    }

    /// 重新计算拓扑序分组
    fn recompute_groups(&mut self) {
        let n = self.components.len();
        if n == 0 {
            self.parallel_groups.clear();
            return;
        }

        let mut levels: Vec<usize> = vec![0; n];

        for i in 0..n {
            let meta = self.components[i].meta();
            if meta.requires.is_empty() {
                levels[i] = 0;
            } else {
                let mut max_dep = 0;
                for req in meta.requires {
                    for (j, &dep_level) in levels.iter().enumerate().take(i) {
                        if self.components[j].meta().provides.contains(req) {
                            max_dep = max_dep.max(dep_level + 1);
                        }
                    }
                }
                levels[i] = max_dep;
            }
        }

        let max_level = *levels.iter().max().unwrap_or(&0);
        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); max_level + 1];
        for (i, &level) in levels.iter().enumerate() {
            groups[level].push(i);
        }

        self.parallel_groups = groups;
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use std::any::Any;

    use super::*;
    use crate::plugins::slots::react_loop::component::ComponentMeta;

    // ── CompA: priority=10, provides=["cap_a"] ──────────────

    struct CompA;

    #[async_trait]
    impl Component for CompA {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "comp_a",
                version: "0.1.0",
                priority: 10,
                provides: &["cap_a"],
                requires: &[],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(CompA)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            Ok(Processing::Continue)
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    // ── CompB: priority=20, requires=["cap_a"] ─────────────

    struct CompB;

    #[async_trait]
    impl Component for CompB {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "comp_b",
                version: "0.1.0",
                priority: 20,
                provides: &["cap_b"],
                requires: &["cap_a"],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(CompB)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            Ok(Processing::Continue)
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    // ── DupComp: duplicate name test ───────────────────────

    struct DupComp;

    #[async_trait]
    impl Component for DupComp {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "dup",
                version: "0.1.0",
                priority: 0,
                provides: &[],
                requires: &[],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(DupComp)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            Ok(Processing::Continue)
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    // ── NeedyComponent: requires missing capability ────────

    struct NeedyComponent;

    #[async_trait]
    impl Component for NeedyComponent {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "needy",
                version: "0.1.0",
                priority: 10,
                provides: &["needy_cap"],
                requires: &["missing_cap"],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(NeedyComponent)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            Ok(Processing::Continue)
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    // ── BreakComponent + AfterBreakComponent ──────────────

    struct BreakComponent;

    #[async_trait]
    impl Component for BreakComponent {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "breaker",
                version: "0.1.0",
                priority: 10,
                provides: &["break_test"],
                requires: &[],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(BreakComponent)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            Ok(Processing::BreakChain)
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    struct AfterBreakComponent;

    #[async_trait]
    impl Component for AfterBreakComponent {
        fn meta(&self) -> &ComponentMeta {
            static META: ComponentMeta = ComponentMeta {
                name: "after_breaker",
                version: "0.1.0",
                priority: 20,
                provides: &["after"],
                requires: &["break_test"],
                config_key: None,
            };
            &META
        }

        fn clone_box(&self) -> Box<dyn ComponentHandle> {
            Box::new(AfterBreakComponent)
        }

        async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
            Ok(())
        }
        async fn process(
            &mut self,
            _ap: &mut dyn AccessPoint,
        ) -> Result<Processing, ComponentError> {
            panic!("should not be reached — BreakChain should stop before this component")
        }
        fn name(&self) -> &str {
            self.meta().name
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn clonable(&self) -> bool {
            true
        }
        fn ready(&self) -> bool {
            true
        }
        async fn shutdown(&mut self) -> Result<(), ComponentError> {
            Ok(())
        }
    }

    // ═══════════════════════════════════════════════════════════
    // Tests
    // ═══════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_orchestrator_init_shutdown_order() {
        let config = ModuleConfig::new(serde_json::json!({"max_turns": 10}));
        let mut orch = Orchestrator::new(config);

        orch.register(Box::new(CompA)).await.unwrap();
        orch.register(Box::new(CompB)).await.unwrap();

        orch.init_all().await.unwrap();
        orch.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_register_duplicate_name() {
        let config = ModuleConfig::new(serde_json::json!({"max_turns": 10}));
        let mut orch = Orchestrator::new(config);

        orch.register(Box::new(DupComp)).await.unwrap();
        let result = orch.register(Box::new(DupComp)).await;
        assert!(result.is_err());
        match result {
            Err(ComponentError::Config(msg)) => {
                assert!(msg.contains("dup"));
            }
            _ => panic!("expected Config error"),
        }
    }

    #[tokio::test]
    async fn test_register_missing_requires() {
        let config = ModuleConfig::new(serde_json::json!({"max_turns": 10}));
        let mut orch = Orchestrator::new(config);

        let result = orch.register(Box::new(NeedyComponent)).await;
        assert!(result.is_err());
        match result {
            Err(ComponentError::NotFound(msg)) => {
                assert!(msg.contains("missing_cap"));
            }
            _ => panic!("expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_process_all_break_chain() {
        let config = ModuleConfig::new(serde_json::json!({"max_turns": 10}));
        let mut orch = Orchestrator::new(config);

        orch.register(Box::new(BreakComponent)).await.unwrap();
        orch.register(Box::new(AfterBreakComponent)).await.unwrap();
        orch.init_all().await.unwrap();

        let result = orch.process_all().await;
        assert!(result.is_ok());
    }
}
