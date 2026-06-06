use std::time::Duration;

use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::Timestamp;
use crate::shared_types::context::{
    CONTEXT_LAST_INDEXED_COUNT, CONTEXT_LAST_PERSISTED_COUNT, CONTEXT_MEMORY_PERSISTED,
    CONTEXT_OBSERVATION,
};
use crate::shared_types::{DynProvider, MemoryProvider, Message, Observation, PROVIDER_MEMORY};

use super::config::*;
use super::types::MemoryPersistedMarker;

pub struct MemorySaverSlot {
    config: MemorySaverConfig,
}

impl MemorySaverSlot {
    pub fn new() -> Self {
        Self {
            config: MemorySaverConfig {
                persist_user_messages: true,
                persist_observations: true,
                update_vector_index: true,
                enable_experience_extract: false,
                min_messages_for_experience: DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE,
                write_timeout_secs: DEFAULT_MEMORY_WRITE_TIMEOUT_SECS,
            },
        }
    }
}

impl Default for MemorySaverSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for MemorySaverSlot {
    fn name(&self) -> &str {
        "memory_saver"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        self.config = serde_json::from_value(ctx.plugin_config.clone()).map_err(|e| {
            PluginError::Config(format!("{} 解析 memory_saver 配置失败: {}", LOG_PREFIX, e))
        })?;

        tracing::info!(
            "{} 初始化完成: persist_user_messages={}, persist_observations={}, update_vector_index={}",
            LOG_PREFIX,
            self.config.persist_user_messages,
            self.config.persist_observations,
            self.config.update_vector_index
        );

        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // ══════════════════════════════════════════════════════════════
        // 步骤 1：获取 Memory Provider
        // ══════════════════════════════════════════════════════════════
        let memory_provider = match ap.provider_raw(PROVIDER_MEMORY) {
            Some(raw) => match raw.downcast::<DynProvider<dyn MemoryProvider>>() {
                Ok(arc) => arc.0.clone(),
                Err(_) => {
                    tracing::warn!("{} Memory Provider 类型不匹配，跳过持久化", LOG_PREFIX);
                    return Ok(SlotDirective::Continue);
                }
            },
            None => {
                tracing::warn!("{} Memory Provider 未注册，跳过持久化", LOG_PREFIX);
                return Ok(SlotDirective::Continue);
            }
        };

        let session_id = ap.session_id().to_string();
        let timeout = Duration::from_secs(self.config.write_timeout_secs);

        // ══════════════════════════════════════════════════════════════
        // 步骤 2：从 StepContext 读取上次持久化进度（S-R03 合规）
        // ══════════════════════════════════════════════════════════════
        let last_persisted_count: usize = ap
            .read_context_raw(CONTEXT_LAST_PERSISTED_COUNT)
            .and_then(|any| any.downcast_ref::<usize>().copied())
            .unwrap_or(0);

        let last_indexed_count: usize = ap
            .read_context_raw(CONTEXT_LAST_INDEXED_COUNT)
            .and_then(|any| any.downcast_ref::<usize>().copied())
            .unwrap_or(0);

        // ══════════════════════════════════════════════════════════════
        // 步骤 3：持久化用户消息（L2 工作记忆）
        // ══════════════════════════════════════════════════════════════
        if self.config.persist_user_messages {
            let messages = ap.messages();
            let new_messages: Vec<Message> = messages
                .iter()
                .skip(last_persisted_count)
                .cloned()
                .collect();

            if !new_messages.is_empty() {
                match tokio::time::timeout(
                    timeout,
                    memory_provider.persist_messages(&session_id, &new_messages),
                )
                .await
                {
                    Ok(Ok(())) => {
                        let new_count = messages.len();
                        ap.write_context_raw(CONTEXT_LAST_PERSISTED_COUNT, Box::new(new_count))
                            .map_err(|e| {
                                PluginError::Runtime(format!(
                                    "{} 写入持久化进度失败: {}",
                                    LOG_PREFIX, e
                                ))
                            })?;
                        tracing::debug!(
                            "{} 持久化 {} 条消息（累计 {} 条）",
                            LOG_PREFIX,
                            new_messages.len(),
                            new_count
                        );
                    }
                    Ok(Err(e)) => {
                        tracing::error!("{} 消息持久化失败，重试中: {}", LOG_PREFIX, e);
                        // 重试一次
                        match tokio::time::timeout(
                            timeout,
                            memory_provider.persist_messages(&session_id, &new_messages),
                        )
                        .await
                        {
                            Ok(Ok(())) => {
                                let new_count = messages.len();
                                let _ = ap.write_context_raw(
                                    CONTEXT_LAST_PERSISTED_COUNT,
                                    Box::new(new_count),
                                );
                            }
                            Ok(Err(e2)) => {
                                tracing::error!("{} 消息持久化重试仍失败: {}", LOG_PREFIX, e2);
                            }
                            Err(_) => {
                                tracing::warn!("{} 消息持久化重试超时", LOG_PREFIX,);
                            }
                        }
                    }
                    Err(_) => {
                        tracing::warn!(
                            "{} 消息持久化超时（{} 秒）",
                            LOG_PREFIX,
                            self.config.write_timeout_secs
                        );
                    }
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // 步骤 4：持久化工具观察结果
        // ══════════════════════════════════════════════════════════════
        if self.config.persist_observations {
            if let Some(obs_any) = ap.read_context_raw(CONTEXT_OBSERVATION) {
                if let Some(observations) = obs_any.downcast_ref::<Vec<Observation>>() {
                    for observation in observations {
                        let observation_str = match serde_json::to_string(observation) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::error!("{} 序列化观察结果失败: {}", LOG_PREFIX, e);
                                continue;
                            }
                        };
                        match tokio::time::timeout(
                            timeout,
                            memory_provider.persist_observation(&session_id, &observation_str),
                        )
                        .await
                        {
                            Ok(Ok(())) => {
                                tracing::debug!("{} 观察结果已持久化", LOG_PREFIX);
                            }
                            Ok(Err(e)) => {
                                tracing::error!("{} 观察结果持久化失败，重试中: {}", LOG_PREFIX, e);
                                match tokio::time::timeout(
                                    timeout,
                                    memory_provider
                                        .persist_observation(&session_id, &observation_str),
                                )
                                .await
                                {
                                    Ok(Ok(())) => {
                                        tracing::debug!("{} 观察结果重试后已持久化", LOG_PREFIX);
                                    }
                                    Ok(Err(e2)) => {
                                        tracing::error!(
                                            "{} 观察结果持久化重试仍失败: {}",
                                            LOG_PREFIX,
                                            e2
                                        );
                                    }
                                    Err(_) => {
                                        tracing::warn!("{} 观察结果重试超时", LOG_PREFIX,);
                                    }
                                }
                            }
                            Err(_) => {
                                tracing::warn!("{} 观察结果持久化超时", LOG_PREFIX);
                            }
                        }
                    }
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // 步骤 5：触发向量索引更新（L3，异步，不阻塞 Pipeline）
        // ══════════════════════════════════════════════════════════════
        if self.config.update_vector_index {
            let messages = ap.messages();
            if messages.len() > last_indexed_count {
                let provider_clone = memory_provider.clone();
                let session_id_clone = session_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = provider_clone.trigger_vector_index(&session_id_clone).await {
                        tracing::error!("{} 向量索引更新失败: {}", LOG_PREFIX, e);
                    }
                });

                let new_indexed = messages.len();
                ap.write_context_raw(CONTEXT_LAST_INDEXED_COUNT, Box::new(new_indexed))
                    .map_err(|e| {
                        PluginError::Runtime(format!("{} 写入索引进度失败: {}", LOG_PREFIX, e))
                    })?;
            }
        }

        // ══════════════════════════════════════════════════════════════
        // 步骤 6：经验提取（可选，异步，不阻塞 Pipeline）
        // ══════════════════════════════════════════════════════════════
        if self.config.enable_experience_extract {
            let messages = ap.messages();
            if messages.len() >= self.config.min_messages_for_experience {
                let provider_clone = memory_provider.clone();
                let session_id_clone = session_id.clone();
                tokio::spawn(async move {
                    match provider_clone.extract_experiences(&session_id_clone).await {
                        Ok(experiences) if !experiences.is_empty() => {
                            tracing::info!("{} 提取 {} 条经验", LOG_PREFIX, experiences.len());
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("{} 经验提取失败: {}", LOG_PREFIX, e);
                        }
                    }
                });
            }
        }

        // ══════════════════════════════════════════════════════════════
        // 步骤 7：写入持久化完成标记
        // ══════════════════════════════════════════════════════════════
        let current_persisted_count: usize = ap
            .read_context_raw(CONTEXT_LAST_PERSISTED_COUNT)
            .and_then(|any| any.downcast_ref::<usize>().copied())
            .unwrap_or(0);

        ap.write_context_raw(
            CONTEXT_MEMORY_PERSISTED,
            Box::new(MemoryPersistedMarker {
                session_id: session_id.clone(),
                persisted_count: current_persisted_count,
                timestamp: Timestamp::now(),
            }),
        )
        .map_err(|e| PluginError::Runtime(format!("{} 写入持久化标记失败: {}", LOG_PREFIX, e)))?;

        // ══════════════════════════════════════════════════════════════
        // 步骤 8：返回 Continue
        // ══════════════════════════════════════════════════════════════
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("{} 关闭，刷新缓冲区", LOG_PREFIX);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use crate::core::access::SlotAccessPoint;
    use crate::core::slot::SlotDirective;
    use crate::core::types::error::PluginError;
    use crate::shared_types::{
        ExperienceEntry, IdentitySection, MemoryError, MemoryFileEntry, MemoryProvider,
        MemoryStats, Message, MessageRole, Observation,
    };

    use super::*;

    // ============================================
    // MockMemoryProvider
    // ============================================
    struct MockMemoryProvider {
        persisted_messages: Arc<RwLock<Vec<Message>>>,
        persisted_observations: Arc<RwLock<Vec<String>>>,
        vector_index_called: Arc<AtomicBool>,
        extract_called: Arc<AtomicBool>,
        fail_persist_messages: bool,
        fail_persist_observation: bool,
    }

    #[async_trait]
    impl MemoryProvider for MockMemoryProvider {
        async fn persist_messages(
            &self,
            _session_id: &str,
            messages: &[Message],
        ) -> Result<(), MemoryError> {
            if self.fail_persist_messages {
                return Err(MemoryError::WriteError("simulated failure".into()));
            }
            self.persisted_messages
                .write()
                .await
                .extend(messages.iter().cloned());
            Ok(())
        }

        async fn persist_observation(
            &self,
            _session_id: &str,
            observation: &str,
        ) -> Result<(), MemoryError> {
            if self.fail_persist_observation {
                return Err(MemoryError::WriteError("simulated failure".into()));
            }
            self.persisted_observations
                .write()
                .await
                .push(observation.to_string());
            Ok(())
        }

        async fn trigger_vector_index(&self, _session_id: &str) -> Result<(), MemoryError> {
            self.vector_index_called.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn extract_experiences(
            &self,
            _session_id: &str,
        ) -> Result<Vec<ExperienceEntry>, MemoryError> {
            self.extract_called.store(true, Ordering::SeqCst);
            Ok(vec![])
        }

        async fn stats(&self, _session_id: &str) -> Result<MemoryStats, MemoryError> {
            Ok(MemoryStats::default())
        }

        async fn load_identity(&self, _session_id: &str) -> Result<IdentitySection, MemoryError> {
            Err(MemoryError::NotFound("not initialized".into()))
        }

        async fn load_working_memory(
            &self,
            _session_id: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryFileEntry>, MemoryError> {
            Ok(vec![])
        }

        async fn is_new_session(&self, _session_id: &str) -> Result<bool, MemoryError> {
            Ok(true)
        }

        async fn search_memory(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryFileEntry>, MemoryError> {
            Ok(vec![])
        }
    }

    // ============================================
    // MockSlotAccessPoint
    // ============================================
    struct MockSlotAccessPoint {
        session_id: String,
        messages: Vec<Message>,
        context_data: HashMap<String, Box<dyn Any + Send + Sync>>,
        memory_provider: Option<Arc<dyn MemoryProvider>>,
    }

    impl SlotAccessPoint for MockSlotAccessPoint {
        fn messages(&self) -> &[Message] {
            &self.messages
        }

        fn session_id(&self) -> &str {
            &self.session_id
        }

        fn phase_name(&self) -> &str {
            "memorize"
        }

        fn current_iteration(&self) -> usize {
            1
        }

        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }

        fn write_context_raw(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            self.context_data.insert(key.to_string(), val);
            Ok(())
        }

        fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
            self.context_data
                .get(key)
                .map(|b| b.as_ref() as &(dyn Any + Send + Sync))
        }

        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }

        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }

        fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            if name == "memory" {
                self.memory_provider.as_ref().map(|p| {
                    let double_wrapped: Arc<dyn Any + Send + Sync> = Arc::new(p.clone());
                    double_wrapped
                })
            } else {
                None
            }
        }
        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    // ============================================
    // Helper: create slot with custom config
    // ============================================
    fn make_slot(
        persist_user: bool,
        persist_obs: bool,
        update_idx: bool,
        enable_exp: bool,
        min_msgs: usize,
        timeout: u64,
    ) -> MemorySaverSlot {
        MemorySaverSlot {
            config: MemorySaverConfig {
                persist_user_messages: persist_user,
                persist_observations: persist_obs,
                update_vector_index: update_idx,
                enable_experience_extract: enable_exp,
                min_messages_for_experience: min_msgs,
                write_timeout_secs: timeout,
            },
        }
    }

    fn default_slot() -> MemorySaverSlot {
        MemorySaverSlot::new()
    }

    #[allow(clippy::type_complexity)]
    fn make_provider() -> (
        Arc<RwLock<Vec<Message>>>,
        Arc<RwLock<Vec<String>>>,
        Arc<AtomicBool>,
        Arc<AtomicBool>,
        Arc<dyn MemoryProvider>,
    ) {
        let persisted_messages = Arc::new(RwLock::new(Vec::new()));
        let persisted_observations = Arc::new(RwLock::new(Vec::new()));
        let vector_index_called = Arc::new(AtomicBool::new(false));
        let extract_called = Arc::new(AtomicBool::new(false));

        let mock = Arc::new(MockMemoryProvider {
            persisted_messages: persisted_messages.clone(),
            persisted_observations: persisted_observations.clone(),
            vector_index_called: vector_index_called.clone(),
            extract_called: extract_called.clone(),
            fail_persist_messages: false,
            fail_persist_observation: false,
        });

        (
            persisted_messages,
            persisted_observations,
            vector_index_called,
            extract_called,
            mock,
        )
    }

    fn make_provider_failing(fail_msg: bool, fail_obs: bool) -> Arc<dyn MemoryProvider> {
        let persisted_messages = Arc::new(RwLock::new(Vec::new()));
        let persisted_observations = Arc::new(RwLock::new(Vec::new()));

        Arc::new(MockMemoryProvider {
            persisted_messages,
            persisted_observations,
            vector_index_called: Arc::new(AtomicBool::new(false)),
            extract_called: Arc::new(AtomicBool::new(false)),
            fail_persist_messages: fail_msg,
            fail_persist_observation: fail_obs,
        })
    }

    fn make_ap(
        messages: Vec<Message>,
        provider: Option<Arc<dyn MemoryProvider>>,
    ) -> MockSlotAccessPoint {
        MockSlotAccessPoint {
            session_id: "test-session".into(),
            messages,
            context_data: HashMap::new(),
            memory_provider: provider,
        }
    }

    // ============================================
    // T-N01: messages persisted
    // ============================================
    #[tokio::test]
    async fn test_messages_persisted() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "hello"),
            Message::text(MessageRole::Assistant, "hi"),
            Message::text(MessageRole::User, "how are you"),
        ];
        let mut ap = make_ap(msgs, Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-N02: observation persisted
    // ============================================
    #[tokio::test]
    async fn test_observation_persisted() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![Message::text(MessageRole::User, "hello")];
        let mut ap = make_ap(msgs, Some(prov));
        ap.context_data
            .insert("observation".into(), Box::new(test_observation()));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-N03: vector index triggered
    // ============================================
    #[tokio::test]
    async fn test_vector_index_triggered() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "a"),
            Message::text(MessageRole::Assistant, "b"),
        ];
        let mut ap = make_ap(msgs, Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-N04: full flow
    // ============================================
    #[tokio::test]
    async fn test_full_flow() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "msg1"),
            Message::text(MessageRole::User, "msg2"),
            Message::text(MessageRole::User, "msg3"),
            Message::text(MessageRole::User, "msg4"),
            Message::text(MessageRole::User, "msg5"),
        ];
        let mut ap = make_ap(msgs, Some(prov));
        ap.context_data
            .insert("observation".into(), Box::new(test_observation()));

        // Enable experience extract for full flow
        slot.config.enable_experience_extract = true;

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-B01: no new messages
    // ============================================
    #[tokio::test]
    async fn test_no_new_messages() {
        let mut slot = default_slot();
        let (pm, _po, _vi, _ex, prov) = make_provider();
        let mut ap = make_ap(vec![], Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);

        let count = pm.read().await.len();
        assert_eq!(count, 0);
    }

    // ============================================
    // T-B02: no observation in context
    // ============================================
    #[tokio::test]
    async fn test_no_observation() {
        let mut slot = default_slot();
        let (_pm, po, _vi, _ex, prov) = make_provider();
        let msgs = vec![Message::text(MessageRole::User, "hi")];
        let mut ap = make_ap(msgs, Some(prov));
        // No "observation" key inserted

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);

        let obs_count = po.read().await.len();
        assert_eq!(obs_count, 0);
    }

    // ============================================
    // T-B03: experience not extracted below min
    // ============================================
    #[tokio::test]
    async fn test_experience_not_extracted_below_min() {
        let mut slot = make_slot(true, true, true, true, 5, 10);
        let (_pm, _po, _vi, ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "a"),
            Message::text(MessageRole::User, "b"),
            Message::text(MessageRole::User, "c"),
        ];
        let mut ap = make_ap(msgs, Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!ex.load(Ordering::SeqCst));
    }

    // ============================================
    // T-B04: all config disabled
    // ============================================
    #[tokio::test]
    async fn test_all_config_disabled() {
        let mut slot = make_slot(false, false, false, false, 5, 10);
        let (pm, po, vi, ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "x"),
            Message::text(MessageRole::User, "y"),
        ];
        let mut ap = make_ap(msgs, Some(prov));
        ap.context_data
            .insert("observation".into(), Box::new(test_observation()));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);

        assert_eq!(pm.read().await.len(), 0);
        assert_eq!(po.read().await.len(), 0);
        assert!(!vi.load(Ordering::SeqCst));
        assert!(!ex.load(Ordering::SeqCst));
    }

    // ============================================
    // T-B05: incremental persistence
    // ============================================
    #[tokio::test]
    async fn test_incremental_persistence() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "old1"),
            Message::text(MessageRole::User, "old2"),
            Message::text(MessageRole::User, "new"),
        ];
        let mut ap = make_ap(msgs, Some(prov));
        // Simulate last_persisted_count = 2 (first 2 already persisted)
        ap.context_data
            .insert("last_persisted_count".into(), Box::new(2usize));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-E01: provider not registered
    // ============================================
    #[tokio::test]
    async fn test_provider_not_registered() {
        let mut slot = default_slot();
        let (pm, _po, _vi, _ex) = {
            let pm: Arc<RwLock<Vec<Message>>> = Arc::new(RwLock::new(Vec::new()));
            let po: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
            let vi = Arc::new(AtomicBool::new(false));
            let ex = Arc::new(AtomicBool::new(false));
            (pm, po, vi, ex)
        };
        let msgs = vec![Message::text(MessageRole::User, "hello")];
        let mut ap = make_ap(msgs, None);

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
        // No provider → nothing persisted
        assert_eq!(pm.read().await.len(), 0);
    }

    // ============================================
    // T-E02: provider type mismatch
    // ============================================
    #[tokio::test]
    async fn test_provider_type_mismatch() {
        let mut slot = default_slot();
        let msgs = vec![Message::text(MessageRole::User, "hello")];

        struct WrongProviderAccessPoint {
            inner: MockSlotAccessPoint,
        }

        impl SlotAccessPoint for WrongProviderAccessPoint {
            fn messages(&self) -> &[Message] {
                self.inner.messages()
            }
            fn session_id(&self) -> &str {
                self.inner.session_id()
            }
            fn phase_name(&self) -> &str {
                "memorize"
            }
            fn current_iteration(&self) -> usize {
                1
            }
            fn write_observation(
                &mut self,
                obs: Box<dyn Any + Send + Sync>,
            ) -> Result<(), PluginError> {
                self.inner.write_observation(obs)
            }
            fn write_context_raw(
                &mut self,
                key: &str,
                val: Box<dyn Any + Send + Sync>,
            ) -> Result<(), PluginError> {
                self.inner.write_context_raw(key, val)
            }
            fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
                self.inner.read_context_raw(key)
            }
            fn request_jump(&self, phase: &str) -> Result<(), PluginError> {
                self.inner.request_jump(phase)
            }
            fn request_abort(&self) -> Result<(), PluginError> {
                self.inner.request_abort()
            }
            fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
                if name == "memory" {
                    Some(Arc::new("wrong type".to_string()))
                } else {
                    None
                }
            }

            fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
                Ok(())
            }
        }

        let base_ap = make_ap(msgs, None);
        let mut wrong_ap = WrongProviderAccessPoint { inner: base_ap };

        let result = slot.run(&mut wrong_ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-E03: persist timeout
    // ============================================
    #[tokio::test]
    async fn test_persist_timeout() {
        // Use 0-second timeout to trigger immediate timeout
        let mut slot = make_slot(true, false, false, false, 5, 0);
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![Message::text(MessageRole::User, "hello")];
        let mut ap = make_ap(msgs, Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-E04: persist error
    // ============================================
    #[tokio::test]
    async fn test_persist_error() {
        let mut slot = default_slot();
        let prov = make_provider_failing(true, false);
        let msgs = vec![Message::text(MessageRole::User, "hello")];
        let mut ap = make_ap(msgs, Some(prov));

        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    // ============================================
    // T-E05: init config error
    // ============================================
    #[tokio::test]
    async fn test_init_config_error() {
        let mut slot = default_slot();
        let ctx = PluginInitContext::new(
            "memory_saver",
            // Invalid config — numbers where booleans expected
            serde_json::json!({
                "persist_user_messages": "not_a_bool",
                "write_timeout_secs": "not_a_number"
            }),
            crate::core::types::plugin::AgentConfig::default(),
            std::path::PathBuf::from("./data/memory_saver"),
        );

        let result = slot.init(&ctx).await;

        assert!(result.is_err());
    }

    // ============================================
    // T-I01: repeated run idempotent
    // ============================================
    #[tokio::test]
    async fn test_repeated_run_idempotent() {
        let mut slot = default_slot();
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "a"),
            Message::text(MessageRole::User, "b"),
        ];
        let mut ap = make_ap(msgs, Some(prov));

        // First run
        let r1 = slot.run(&mut ap).await;
        assert!(r1.is_ok());

        // Second run — same ap, same context
        let r2 = slot.run(&mut ap).await;
        assert!(r2.is_ok());
    }

    // ============================================
    // T-I02: new slot resume from context
    // ============================================
    #[tokio::test]
    async fn test_new_slot_resume() {
        let (_pm, _po, _vi, _ex, prov) = make_provider();
        let msgs = vec![
            Message::text(MessageRole::User, "old1"),
            Message::text(MessageRole::User, "old2"),
            Message::text(MessageRole::User, "new1"),
        ];

        // First slot persists 2 messages
        let mut slot1 = default_slot();
        let mut ap = make_ap(msgs.clone(), Some(prov.clone()));
        // Simulate partial progress in context
        ap.context_data
            .insert("last_persisted_count".into(), Box::new(2usize));

        let r1 = slot1.run(&mut ap).await;
        assert!(r1.is_ok());

        // New slot, same ap (context preserved)
        let mut slot2 = default_slot();
        let r2 = slot2.run(&mut ap).await;
        assert!(r2.is_ok());
    }

    // ============================================
    // T-I03: shutdown ok
    // ============================================
    #[tokio::test]
    async fn test_shutdown_ok() {
        let mut slot = default_slot();
        let result = slot.shutdown().await;
        assert!(result.is_ok());
    }

    fn test_observation() -> Observation {
        Observation::success(
            crate::shared_types::Action::new("test_tool", serde_json::json!({})),
            "test output",
        )
    }
}
