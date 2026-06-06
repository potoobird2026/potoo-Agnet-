/*! MemoryService —— 三层记忆系统 ServicePlugin */
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::{
    DynProvider, ExperienceEntry, IdentitySection, MemoryError, MemoryFileEntry, MemoryProvider,
    MemoryStats, PROVIDER_MEMORY,
};
#[cfg(feature = "vector_db")]
use crate::shared_types::{VectorMemoryContract, PROVIDER_VECTOR};

use super::config::MemoryConfig;
#[cfg(feature = "vector_db")]
use super::config::VectorBackend;
use super::dream::DreamOptimizerService;
use super::experience_extract::ExperienceExtractService;
use super::feedback::{FeedbackConfig, FeedbackMonitor};
use super::l1_identity::IdentityManager;
use super::l2_working::{ForgettingService, WorkingMemoryManager};
#[cfg(feature = "vector_db")]
use super::l3_vector::VectorStoreManager;

struct MemoryInner {
    config: MemoryConfig,
    identity: IdentityManager,
    working_memory: WorkingMemoryManager,
    forgetting: ForgettingService,
    #[cfg(feature = "vector_db")]
    vector_store: Option<VectorStoreManager>,
    dream: DreamOptimizerService,
    experience_extract: ExperienceExtractService,
    feedback: FeedbackMonitor,
    running: bool,
    suspended: bool,
}

pub struct MemoryService {
    inner: Arc<RwLock<Option<MemoryInner>>>,
}

impl MemoryService {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl ServicePlugin for MemoryService {
    fn name(&self) -> &str {
        "memory"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut config: MemoryConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("memory: 配置解析失败: {}", e)))?;
        config.resolve_paths();
        config.validate()?;

        let workspace = config.workspace_dir.clone();
        tokio::fs::create_dir_all(&workspace)
            .await
            .map_err(|e| PluginError::InitFailed(format!("创建目录失败: {}", e)))?;

        let mut identity = IdentityManager::new(config.l1.clone(), &workspace);
        identity
            .load()
            .map_err(|e| PluginError::InitFailed(format!("L1 加载失败: {}", e)))?;

        let mut working_memory = WorkingMemoryManager::new(config.l2.clone(), &workspace);
        working_memory
            .init()
            .map_err(|e| PluginError::InitFailed(format!("L2 初始化失败: {}", e)))?;

        let mut forgetting = ForgettingService::new(config.forgetting.clone(), &workspace);
        forgetting.load_cache();

        #[cfg(feature = "vector_db")]
        let vector_store = if config.l3.backend == VectorBackend::Memory {
            Some(VectorStoreManager::new(&config.l3))
        } else {
            None
        };

        let dream = DreamOptimizerService::new(86400);
        let experience_extract = ExperienceExtractService::new();
        let feedback = FeedbackMonitor::new(FeedbackConfig {
            success_multiplier: config.forgetting.feedback_success_multiplier,
            failure_multiplier: config.forgetting.feedback_failure_multiplier,
            weight_floor: config.forgetting.weight_floor,
        });

        *self.inner.write().await = Some(MemoryInner {
            config,
            identity,
            working_memory,
            forgetting,
            #[cfg(feature = "vector_db")]
            vector_store,
            dream,
            experience_extract,
            feedback,
            running: false,
            suspended: false,
        });

        tracing::info!("MemoryService: 初始化完成（L1+L2）");
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| PluginError::InitFailed("memory inner 未初始化".into()))?;
        inner.running = true;

        // 注册 MemoryProvider——包装 inner 状态以实现 MemoryProvider trait
        // 注意：必须显式转为 Arc<dyn MemoryProvider>，否则 DynProvider 的类型参数
        // 是 MemoryProviderImpl（具体类型）而非 dyn MemoryProvider，
        // 导致消费者 downcast::<DynProvider<dyn MemoryProvider>>() 失败。
        let provider: Arc<dyn MemoryProvider> = Arc::new(MemoryProviderImpl {
            inner: Arc::clone(&self.inner),
        });
        ap.register_provider(PROVIDER_MEMORY, Arc::new(DynProvider(provider)));
        #[cfg(feature = "vector_db")]
        if let Some(vsm) = &inner.vector_store {
            let vsm_arc: Arc<dyn VectorMemoryContract> = Arc::new(vsm.clone());
            ap.register_provider(PROVIDER_VECTOR, Arc::new(DynProvider(vsm_arc)));
        }

        // B-6: L3 init（store.init + 启动后台 sync/cleanup）——在遗忘任务之前
        #[cfg(feature = "vector_db")]
        if let Some(vsm) = &mut inner.vector_store {
            if let Err(e) = vsm.init().await {
                tracing::warn!("MemoryService: L3 init 失败: {}", e);
            }
        }

        // 启动后台遗忘任务
        if inner.config.forgetting_enabled {
            let inner_clone = Arc::clone(&self.inner);
            let interval = inner.config.forgetting_interval_seconds;
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval));
                loop {
                    tick.tick().await;
                    let mut guard = inner_clone.write().await;
                    if let Some(inner) = guard.as_mut() {
                        if !inner.running {
                            break;
                        }
                        let (retired, deleted) = inner.forgetting.run(&mut inner.working_memory);
                        if !retired.is_empty() || !deleted.is_empty() {
                            tracing::info!(
                                "Memory Forgetting: 退役 {} 条, 深度删除 {} 条",
                                retired.len(),
                                deleted.len()
                            );
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        // 启动后台 dream 优化
        let inner_clone = Arc::clone(&self.inner);
        let dream_interval = inner.dream.interval();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(dream_interval);
            loop {
                tick.tick().await;
                let guard = inner_clone.read().await;
                if let Some(inner) = guard.as_ref() {
                    if !inner.running || !inner.dream.is_enabled() {
                        break;
                    }
                    match inner.dream.run_cycle().await {
                        Ok(result) => {
                            if result.merged > 0 || result.updated_l1 || result.cleaned_l3 > 0 {
                                tracing::info!(
                                    "Memory Dream: 合并 {} 条, L1更新 {}, L3清理 {} 条",
                                    result.merged,
                                    result.updated_l1,
                                    result.cleaned_l3
                                );
                            }
                        }
                        Err(e) => tracing::warn!("Memory Dream: 周期运行失败: {}", e),
                    }
                } else {
                    break;
                }
            }
        });

        tracing::info!("MemoryService: 已启动");
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        if let Some(inner) = self.inner.write().await.as_mut() {
            match signal {
                ServiceSignal::GracefulShutdown | ServiceSignal::ImmediateShutdown => {
                    inner.running = false
                }
                ServiceSignal::Suspend => inner.suspended = true,
                ServiceSignal::Resume => inner.suspended = false,
                ServiceSignal::HealthCheck => return Ok(()),
                ServiceSignal::ConfigReload => {}
            }
        }
        Ok(())
    }
    async fn stop(&mut self) -> Result<(), PluginError> {
        if let Some(inner) = self.inner.write().await.as_mut() {
            inner.running = false;
        }
        Ok(())
    }
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        if let Some(inner) = guard.take() {
            inner.forgetting.save_cache();
            tracing::info!("MemoryService: 已关闭，评分缓存已保存");
        }
        Ok(())
    }
}

impl Default for MemoryService {
    fn default() -> Self {
        Self::new()
    }
}

/// MemoryProvider 的实现——包装 MemoryInner，通过 Arc<RwLock> 共享状态
struct MemoryProviderImpl {
    inner: Arc<RwLock<Option<MemoryInner>>>,
}

#[async_trait]
impl MemoryProvider for MemoryProviderImpl {
    async fn persist_messages(
        &self,
        _session_id: &str,
        messages: &[crate::shared_types::Message],
    ) -> Result<(), MemoryError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| MemoryError::WriteError("memory 未初始化".into()))?;
        for msg in messages {
            let text = msg
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join(" ");
            let now = chrono::Utc::now().to_rfc3339();
            let entry = super::l2_working::MemoryFile {
                path: std::path::PathBuf::new(),
                frontmatter: super::l2_working::MemoryFileFrontmatter {
                    weight: 1.0,
                    tags: vec!["memory_saver".to_string()],
                    created: now.clone(),
                    last_accessed: now.clone(),
                    access_count: 1,
                    source: "memory_saver".to_string(),
                },
                content: text,
                file_type: super::l2_working::MemoryFileType::Experience,
            };
            inner
                .working_memory
                .write_entry(entry)
                .map_err(MemoryError::WriteError)?;
        }
        Ok(())
    }

    async fn persist_observation(
        &self,
        _session_id: &str,
        observation: &str,
    ) -> Result<(), MemoryError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| MemoryError::WriteError("memory 未初始化".into()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let entry = super::l2_working::MemoryFile {
            path: std::path::PathBuf::new(),
            frontmatter: super::l2_working::MemoryFileFrontmatter {
                weight: 1.0,
                tags: vec!["observation".to_string()],
                created: now.clone(),
                last_accessed: now.clone(),
                access_count: 1,
                source: "observation".to_string(),
            },
            content: observation.to_string(),
            file_type: super::l2_working::MemoryFileType::Experience,
        };
        inner
            .working_memory
            .write_entry(entry)
            .map_err(MemoryError::WriteError)?;
        inner.feedback.process_feedback(1.0, true);
        Ok(())
    }

    async fn trigger_vector_index(&self, _session_id: &str) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn extract_experiences(
        &self,
        session_id: &str,
    ) -> Result<Vec<ExperienceEntry>, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| MemoryError::ReadError("memory 未初始化".into()))?;
        let text = inner
            .working_memory
            .active_files()
            .iter()
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let raw = inner.experience_extract.extract(&text);
        let now = chrono::Utc::now().to_rfc3339();
        let entries: Vec<ExperienceEntry> = raw
            .into_iter()
            .map(|e| ExperienceEntry {
                summary: format!("[{}] {}", e.exp_type, e.content),
                source_session: session_id.to_string(),
                created_at: now.clone(),
                tags: e.tags,
            })
            .collect();
        Ok(entries)
    }

    async fn stats(&self, _session_id: &str) -> Result<MemoryStats, MemoryError> {
        Ok(MemoryStats::default())
    }

    async fn load_identity(&self, session_id: &str) -> Result<IdentitySection, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| MemoryError::ReadError("memory 未初始化".into()))?;
        let sections = inner.identity.sections();
        if sections.is_empty() {
            return Err(MemoryError::NotFound("无身份记忆".into()));
        }
        let section = &sections[0];
        Ok(IdentitySection {
            user_id: session_id.to_string(),
            content: section.content.clone(),
            metadata: inner.identity.metadata().map(|m| m.version.clone()),
        })
    }

    async fn load_working_memory(
        &self,
        _session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryFileEntry>, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| MemoryError::ReadError("memory 未初始化".into()))?;
        let files = inner.working_memory.active_files();
        let end = limit.min(files.len());
        Ok(files[..end]
            .iter()
            .map(|f| MemoryFileEntry {
                id: f.path.to_string_lossy().to_string(),
                summary: f.frontmatter.source.clone(),
                content: Some(f.content.clone()),
                created_at: f.frontmatter.created.clone(),
                entry_type: format!("{:?}", f.file_type),
            })
            .collect())
    }

    async fn is_new_session(&self, _session_id: &str) -> Result<bool, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| MemoryError::ReadError("memory 未初始化".into()))?;
        Ok(inner.identity.sections().is_empty())
    }

    async fn search_memory(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryFileEntry>, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| MemoryError::ReadError("memory 未初始化".into()))?;
        let results = inner.working_memory.search(&[], query, limit);
        Ok(results
            .into_iter()
            .map(|f| MemoryFileEntry {
                id: f.path.to_string_lossy().to_string(),
                summary: f.frontmatter.tags.join(", "),
                content: Some(f.content.clone()),
                created_at: f.frontmatter.created.clone(),
                entry_type: format!("{:?}", f.file_type),
            })
            .collect())
    }
}
