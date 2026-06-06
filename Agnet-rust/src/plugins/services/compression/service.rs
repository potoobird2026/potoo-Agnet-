/*! CompressionService —— 后台压缩服务（ServicePlugin 实现） */
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::core::access::ServiceAccessPoint;
use crate::core::runtime::SharedMessageStore;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::compression::{CompressionSummaryContract, PROVIDER_COMPRESSION_SUMMARY};
use crate::shared_types::{DynProvider, Message, MessageRole};

use super::components::*;
use super::config::CompressionConfig;
use super::services::*;
use super::types::*;

const MAIN_LOOP_TICK_MS: u64 = 500;
const _CAS_MAX_RETRIES: u32 = 3;

struct CompressionInner {
    config: CompressionConfig,
    pid: pid_controller::PidController,
    token_counter: token_counter::TokenCounter,
    anchor: anchor::Anchor,
    entity_extractor: entity_extractor::EntityExtractor,
    entropy: entropy::Entropy,
    scorer: scorer::Scorer,
    ucb: ucb_decision::UcbDecision,
    fuzzy: fuzzy_control::FuzzyControl,
    compressor: compressor::Compressor,
    feedback: feedback::Feedback,
    recall: recall::Recall,
    journal: journal::Journal,
    shared_store: Option<SharedMessageStore>,
    state: ServiceState,
    running: bool,
    suspended: bool,
    event_rx: Option<mpsc::UnboundedReceiver<HookEvent>>,
}

pub struct CompressionService {
    inner: Arc<RwLock<Option<CompressionInner>>>,
    event_tx: Option<mpsc::UnboundedSender<HookEvent>>,
}

impl CompressionService {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            event_tx: None,
        }
    }

    pub fn event_sender(&self) -> Option<mpsc::UnboundedSender<HookEvent>> {
        self.event_tx.clone()
    }

    /// 注入 SharedMessageStore（在 ServicePlugin::start() 之后调用）
    pub async fn set_shared_store(&self, store: SharedMessageStore) {
        let mut guard = self.inner.write().await;
        if let Some(inner) = guard.as_mut() {
            inner.shared_store = Some(store);
        }
    }
}

#[async_trait]
impl ServicePlugin for CompressionService {
    fn name(&self) -> &str {
        "compression"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: CompressionConfig = serde_json::from_value(_ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("compression: 配置解析失败: {}", e)))?;
        config.validate()?;

        let (tx, rx) = mpsc::unbounded_channel();
        self.event_tx = Some(tx);

        let inner = CompressionInner {
            pid: pid_controller::PidController::new(config.pid.clone()),
            token_counter: token_counter::TokenCounter::new(),
            anchor: anchor::Anchor::new(config.anchor.clone()),
            entity_extractor: entity_extractor::EntityExtractor::new(),
            entropy: entropy::Entropy::new(),
            scorer: scorer::Scorer::new(config.scoring.clone()),
            ucb: ucb_decision::UcbDecision::new(config.ucb.clone()),
            fuzzy: fuzzy_control::FuzzyControl::new(config.fuzzy.clone()),
            compressor: compressor::Compressor::new(),
            feedback: feedback::Feedback::new(),
            recall: recall::Recall::new(),
            journal: journal::Journal::new(),
            shared_store: None,
            config,
            state: ServiceState::Sleep,
            running: false,
            suspended: false,
            event_rx: Some(rx),
        };
        *self.inner.write().await = Some(inner);
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let mut guard = self.inner.write().await;
        let inner = guard
            .as_mut()
            .ok_or_else(|| PluginError::InitFailed("compression inner 未初始化".into()))?;
        inner.running = true;
        // 注册压缩摘要 Provider（设计文档 Compression §6.4，遵循 shared_types契约协议 D-R01）
        let summary_contract: Arc<dyn CompressionSummaryContract> =
            Arc::new(CompressionSummaryImpl {
                inner: Arc::clone(&self.inner),
            });
        ap.register_provider(
            PROVIDER_COMPRESSION_SUMMARY,
            Arc::new(DynProvider(summary_contract)),
        );
        let inner_clone = Arc::clone(&self.inner);
        tokio::spawn(async move {
            Self::run_loop(inner_clone).await;
        });
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
        self.inner.write().await.take();
        Ok(())
    }
}

impl CompressionService {
    async fn run_loop(inner: Arc<RwLock<Option<CompressionInner>>>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(MAIN_LOOP_TICK_MS));
        loop {
            tick.tick().await;
            let mut guard = inner.write().await;
            let inner = match guard.as_mut() {
                Some(i) if i.running && !i.suspended => i,
                Some(_) => continue,
                None => break,
            };
            // 检查事件
            if let Some(rx) = &mut inner.event_rx {
                match rx.try_recv() {
                    Ok(HookEvent::NewMessagesArrived { session_id }) => {
                        if inner.state == ServiceState::Sleep {
                            inner.state = ServiceState::Compressing;
                            tracing::info!("Compression: 开始压缩 session={}", session_id);

                            // ---- 压缩管道：按顺序调用所有组件 ----
                            // 1. 从 shared_store 获取消息
                            if let Some(store) = &inner.shared_store {
                                let (messages, version) = store.read(&session_id).await;
                                if messages.is_empty() {
                                    tracing::debug!("Compression: session={} 无消息", session_id);
                                    inner.state = ServiceState::Sleep;
                                    continue;
                                }

                                // 2. Token 计数
                                let total_tokens = inner.token_counter.count(&messages);
                                if total_tokens == 0 {
                                    inner.state = ServiceState::Sleep;
                                    continue;
                                }

                                // 3. 检查冷启动阈值（config）
                                if messages.len() < inner.config.cold_start.collect_messages {
                                    tracing::debug!(
                                        "Compression: session={} 消息不足 {} 条，跳过",
                                        session_id,
                                        inner.config.cold_start.collect_messages
                                    );
                                    inner.state = ServiceState::Sleep;
                                    continue;
                                }

                                // 4. PID 控制——计算目标保留比例
                                let pid_error = 0.0;
                                let keep_ratio = inner
                                    .pid
                                    .update(pid_error, MAIN_LOOP_TICK_MS as f64 / 1000.0);
                                let keep_ratio = keep_ratio.clamp(0.1, 1.0);

                                // 5. Fuzzy 决策——决定是否压缩
                                let fuzzy_decision = inner.fuzzy.decide(keep_ratio);
                                if fuzzy_decision == FuzzyDecision::Keep {
                                    tracing::debug!("Compression: fuzzy 决定保持，跳过压缩");
                                    inner.state = ServiceState::Sleep;
                                    continue;
                                }
                                let effective_ratio = if fuzzy_decision == FuzzyDecision::Borderline
                                {
                                    inner.config.fuzzy.high_threshold
                                } else {
                                    keep_ratio
                                };

                                // 6. Anchor 计算
                                let (anchor_start, anchor_end) =
                                    inner.anchor.calculate(messages.len(), inner.pid.phase());

                                // 7. 实体提取 + 熵计算
                                let entities: Vec<String> = messages
                                    .iter()
                                    .flat_map(|m| inner.entity_extractor.extract(&m.text_content()))
                                    .collect();
                                let entropy = inner.entropy.calculate(&messages);

                                // 8. 逐条评分 + 保留决策
                                let mut keep_indices: Vec<usize> = Vec::new();
                                let target_keep = std::cmp::max(
                                    1,
                                    (messages.len() as f64 * effective_ratio) as usize,
                                );
                                for (i, msg) in messages.iter().enumerate() {
                                    if i >= anchor_start && i < anchor_end {
                                        keep_indices.push(i);
                                        continue;
                                    }
                                    let score = inner.scorer.score(
                                        msg,
                                        entropy,
                                        &entities,
                                        i,
                                        messages.len(),
                                    );
                                    let category = match msg.role {
                                        MessageRole::System => CategoryRole::System,
                                        MessageRole::User => CategoryRole::User,
                                        MessageRole::Assistant => CategoryRole::Assistant,
                                        _ => CategoryRole::Other,
                                    };
                                    let content_type = ContentType::Text;
                                    let length = if msg.estimate_tokens() > 500 {
                                        LengthBucket::Long
                                    } else if msg.estimate_tokens() > 100 {
                                        LengthBucket::Medium
                                    } else {
                                        LengthBucket::Short
                                    };
                                    if inner.ucb.decide(category, content_type, length, score) {
                                        keep_indices.push(i);
                                    }
                                }

                                // 9. 确保至少保留 target_keep 条
                                while keep_indices.len() < target_keep
                                    && keep_indices.len() < messages.len()
                                {
                                    let next = keep_indices.last().map(|i| i + 1).unwrap_or(0);
                                    if next < messages.len() && !keep_indices.contains(&next) {
                                        keep_indices.push(next);
                                    } else {
                                        break;
                                    }
                                }

                                // 10. 执行压缩
                                match inner
                                    .compressor
                                    .compress(&session_id, &messages, &keep_indices)
                                    .await
                                {
                                    Ok(result) => {
                                        let summary = result.summary.clone();
                                        tracing::info!(
                                            "Compression: session={} 压缩完成——保留 {}/{} 条, 摘要 {} 字符, 节省 {} tokens",
                                            session_id, keep_indices.len(), messages.len(),
                                            summary.len(), result.token_saved,
                                        );

                                        // 11. 反馈检测
                                        let compressed_msg = Message {
                                            role: crate::shared_types::MessageRole::System,
                                            content: vec![crate::shared_types::ContentBlock::Text(
                                                summary.clone(),
                                            )],
                                            tool_calls: None,
                                            tool_call_id: None,
                                            reasoning: None,
                                            metadata: None,
                                            created_at: crate::core::types::Timestamp::now(),
                                        };
                                        let compressed_messages = vec![compressed_msg];
                                        let loss_signals = inner
                                            .feedback
                                            .detect_loss(&messages, &compressed_messages);
                                        if !loss_signals.is_empty() {
                                            tracing::warn!(
                                                "Compression: 检测到 {} 个损失信号",
                                                loss_signals.len()
                                            );
                                            let recall_action = inner.recall.recall(&loss_signals);
                                            match recall_action {
                                                RecallAction::RequestFullHistory => {
                                                    tracing::info!("Compression: recall 请求完整历史，保留原始消息");
                                                    inner.state = ServiceState::Sleep;
                                                    continue;
                                                }
                                                RecallAction::Restore { message_ids }
                                                    if !message_ids.is_empty() =>
                                                {
                                                    tracing::info!(
                                                        "Compression: recall 请求恢复 {} 条消息",
                                                        message_ids.len()
                                                    );
                                                }
                                                _ => {}
                                            }
                                        }

                                        // 12. 记录日志
                                        inner.journal.record(JournalEntry {
                                            timestamp: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_secs() as i64)
                                                .unwrap_or(0),
                                            session_id: session_id.clone(),
                                            compressed_count: keep_indices.len(),
                                            token_saved: result.token_saved,
                                            success: true,
                                        });

                                        // 13. 写回 shared_store
                                        match store
                                            .compare_and_write(
                                                &session_id,
                                                version,
                                                compressed_messages,
                                            )
                                            .await
                                        {
                                            Ok(new_version) => {
                                                tracing::debug!(
                                                    "Compression: CAS 写入成功, version={}",
                                                    new_version
                                                );
                                            }
                                            Err(()) => {
                                                tracing::warn!("Compression: CAS 冲突——运行时在此期间写入了新消息，跳过本轮压缩");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Compression: compress() 失败: {}", e);
                                        inner.journal.record(JournalEntry {
                                            timestamp: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .map(|d| d.as_secs() as i64)
                                                .unwrap_or(0),
                                            session_id: session_id.clone(),
                                            compressed_count: 0,
                                            token_saved: 0,
                                            success: false,
                                        });
                                    }
                                }
                            } else {
                                tracing::warn!("Compression: shared_store 未设置，跳过压缩");
                            }

                            inner.state = ServiceState::Sleep;
                        }
                    }
                    Ok(HookEvent::RoundComplete { .. }) => {}
                    Err(mpsc::error::TryRecvError::Disconnected) => break,
                    Err(mpsc::error::TryRecvError::Empty) => {}
                }
            }
        }
    }
}

/// CompressionSummaryContract 的实现——包装 inner 状态以读取压缩摘要
///
/// 遵循设计文档 Compression §6.4 和 shared_types契约协议。
struct CompressionSummaryImpl {
    inner: Arc<RwLock<Option<CompressionInner>>>,
}

#[async_trait]
impl CompressionSummaryContract for CompressionSummaryImpl {
    async fn get_summary(&self, session_id: &str) -> Option<String> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref()?;
        // 从 shared_store 读取压缩后的消息列表，取最后一条 System 消息的文本作为摘要
        let store = inner.shared_store.as_ref()?;
        let messages = store.get_messages(session_id).await;
        messages
            .iter()
            .rfind(|m| m.role == MessageRole::System)
            .map(|m| m.text_content())
    }
}

impl Default for CompressionService {
    fn default() -> Self {
        Self::new()
    }
}
