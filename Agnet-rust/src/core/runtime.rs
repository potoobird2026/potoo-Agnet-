use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

use super::access::{ProviderRegistry, ServiceAccessImpl, ServiceAccessPoint};
use super::context::{StepContext, StepInput};
use super::phase::Phase;
use super::pipeline::Pipeline;
use super::slot::SlotPlugin;
use super::types::error::AgentError;
use super::types::error::PluginError;
use super::types::persistence::PersistenceCommand;
use super::types::plugin::AgentConfig;
use super::types::plugin::PluginInitContext;
use super::types::Timestamp;
use crate::shared_types::context::{CONTEXT_AGENT_CONFIG, CONTEXT_LLM_CONFIG, CONTEXT_THOUGHT};
use crate::shared_types::llm::LlmConfig;
use crate::shared_types::thought::Thought;
use crate::shared_types::{ContentBlock, Message, MessageRole, StepResponse};

// ============================================
// SharedMessageStore —— 共享消息仓库（带 CAS）
// ============================================

/// 内部条目
#[derive(Debug, Clone)]
struct StoreEntry {
    messages: Vec<Message>,
    version: u64,
}

/// 共享消息仓库（带版本号和 CAS 支持）
///
/// 运行时与压缩服务之间唯一的消息权威来源。
/// 每次写入递增版本号，压缩服务写回前做 CAS 检查，
/// 防止丢失运行时写入的新消息。
#[derive(Clone)]
pub struct SharedMessageStore {
    inner: Arc<RwLock<HashMap<String, StoreEntry>>>,
}

impl SharedMessageStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 读取消息和当前版本号
    pub async fn read(&self, session_id: &str) -> (Vec<Message>, u64) {
        self.inner
            .read()
            .await
            .get(session_id)
            .map(|e| (e.messages.clone(), e.version))
            .unwrap_or_default()
    }

    /// 仅读消息（不关心版本号）
    pub async fn get_messages(&self, session_id: &str) -> Vec<Message> {
        self.inner
            .read()
            .await
            .get(session_id)
            .map(|e| e.messages.clone())
            .unwrap_or_default()
    }

    /// 无条件写入（Runtime 使用），返回新版本号
    pub async fn write(&self, session_id: &str, messages: Vec<Message>) -> u64 {
        let mut store = self.inner.write().await;
        let entry = store
            .entry(session_id.to_string())
            .or_insert_with(|| StoreEntry {
                messages: Vec::new(),
                version: 0,
            });
        entry.messages = messages;
        entry.version = entry.version.wrapping_add(1);
        entry.version
    }

    /// CAS 写入（CompressionService 使用）
    ///
    /// 仅当当前版本号与 expected_version 一致时写入成功。
    /// 返回 Ok(新版本号) 或 Err(())。
    pub async fn compare_and_write(
        &self,
        session_id: &str,
        expected_version: u64,
        messages: Vec<Message>,
    ) -> Result<u64, ()> {
        let mut store = self.inner.write().await;
        let entry = store
            .entry(session_id.to_string())
            .or_insert_with(|| StoreEntry {
                messages: Vec::new(),
                version: 0,
            });
        if entry.version != expected_version {
            return Err(());
        }
        entry.messages = messages;
        entry.version = entry.version.wrapping_add(1);
        Ok(entry.version)
    }

    /// 获取全量快照（供持久化使用）
    pub async fn snapshot(&self) -> HashMap<String, Vec<Message>> {
        self.inner
            .read()
            .await
            .iter()
            .map(|(k, v)| (k.clone(), v.messages.clone()))
            .collect()
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) {
        self.inner.write().await.remove(session_id);
    }

    /// 会话数量
    pub async fn session_count(&self) -> usize {
        self.inner.read().await.len()
    }

    /// 是否为空
    pub async fn is_empty(&self) -> bool {
        self.inner.read().await.is_empty()
    }
}

impl Default for SharedMessageStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// RuntimeServiceAccessImpl —— ServiceAccessImpl 的运行时实现
// ============================================

struct RuntimeServiceAccessImpl {
    provider_registry: Arc<ProviderRegistry>,
    config: AgentConfig,
}

impl ServiceAccessImpl for RuntimeServiceAccessImpl {
    fn get_config(&self) -> AgentConfig {
        self.config.clone()
    }

    fn log(&self, level: &str, message: &str) {
        tracing::info!(target: "service", level = %level, "{message}");
    }

    fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>) {
        self.provider_registry.register_raw(name, provider);
    }

    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.provider_registry.get_raw(name)
    }

    fn unregister_provider(&self, name: &str) {
        self.provider_registry.unregister(name);
    }
}

/// 会话状
#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub messages: Vec<Message>,
    pub max_turns: usize,
    pub context_window: usize,
}

impl SessionState {
    pub fn new(session_id: String, max_turns: usize, context_window: usize) -> Self {
        Self {
            session_id,
            messages: Vec::new(),
            max_turns,
            context_window,
        }
    }

    pub fn with_system_prompt(mut self, system_prompt: String) -> Self {
        if !system_prompt.is_empty() {
            self.messages.push(Message {
                role: MessageRole::System,
                content: vec![ContentBlock::Text(system_prompt)],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: Timestamp::now(),
            });
        }
        self
    }

    /// 添加消息，超过 token 上限时丢弃最早的 System 消息
    pub fn push_message(&mut self, msg: Message) {
        let cw = self.context_window;
        self.messages.push(msg);
        Self::enforce_message_limit(cw, &mut self.messages);
    }

    /// 替换全部消息，同时做大小限制
    pub fn replace_messages(&mut self, new_messages: Vec<Message>) {
        let cw = self.context_window;
        self.messages = new_messages;
        Self::enforce_message_limit(cw, &mut self.messages);
    }

    /// 消息数量硬限制：超过 context_window 时裁剪最早的非 System 消息，
    /// 但始终保留最近 MIN_RETAIN 条消息以避免丢失关键上下文。
    fn enforce_message_limit(context_window: usize, messages: &mut Vec<Message>) {
        const MIN_RETAIN: usize = 10;

        let max_tokens = if context_window > 0 {
            context_window
        } else {
            128_000
        };
        let total_tokens: usize = messages.iter().map(|m| m.estimate_tokens()).sum();
        if total_tokens <= max_tokens {
            return;
        }

        let tokens_to_remove = total_tokens.saturating_sub(max_tokens);
        let keep_from_end = MIN_RETAIN.min(messages.len());
        let can_remove_count = messages.len().saturating_sub(keep_from_end);
        let mut removed_tokens: usize = 0;
        let mut removed_count: usize = 0;

        messages.retain(|m| {
            if removed_tokens >= tokens_to_remove {
                return true;
            }
            // System 消息保留
            if m.role == MessageRole::System {
                return true;
            }
            // 保留最近 MIN_RETAIN 条非 System 消息
            if removed_count >= can_remove_count {
                return true;
            }
            removed_tokens += m.estimate_tokens();
            removed_count += 1;
            false
        });
    }
}

/// Agent 运行时
///
/// 管理会话状态和 Pipeline 的执行生命周期。
/// 通过 step() 接收输入，执行 Pipeline，返回回复。
pub struct AgentRuntime {
    sessions: HashMap<String, SessionState>,
    pipeline: Pipeline,
    shared_store: SharedMessageStore,
    session_max_turns: usize,
    session_context_window: usize,
    provider_registry: Arc<ProviderRegistry>,
    persistence_tx: Option<mpsc::UnboundedSender<PersistenceCommand>>,
    config: AgentConfig,
    llm_config: Option<LlmConfig>,
}

impl AgentRuntime {
    /// 创建新的运行时（使用默认配置）
    pub fn new(pipeline: Pipeline) -> Self {
        Self::new_with_config(pipeline, AgentConfig::default())
    }

    /// 创建带配置的运行时
    pub fn new_with_config(pipeline: Pipeline, config: AgentConfig) -> Self {
        Self {
            sessions: HashMap::new(),
            pipeline,
            shared_store: SharedMessageStore::new(),
            session_max_turns: 50,
            session_context_window: config.context_window.unwrap_or(128_000),
            provider_registry: Arc::new(ProviderRegistry::new()),
            persistence_tx: None,
            config,
            llm_config: None,
        }
    }

    /// 设置 LLM 配置（供 AssemblerSlot 读取 context_window）
    pub fn with_llm_config(mut self, config: LlmConfig) -> Self {
        self.llm_config = Some(config);
        self
    }

    /// 设置共享消息仓库
    pub fn with_shared_store(mut self, store: SharedMessageStore) -> Self {
        self.shared_store = store;
        self
    }

    /// 设置持久化通道发送端
    pub fn with_persistence(
        mut self,
        persistence_tx: mpsc::UnboundedSender<PersistenceCommand>,
    ) -> Self {
        self.persistence_tx = Some(persistence_tx);
        self
    }

    /// 设置新会话默认 max_turns
    pub fn with_max_turns(mut self, n: usize) -> Self {
        self.session_max_turns = n;
        self
    }

    /// 设置新会话默认 context_window
    pub fn with_context_window(mut self, n: usize) -> Self {
        self.session_context_window = n;
        self
    }

    /// 获取共享消息仓库引用
    pub fn shared_store(&self) -> &SharedMessageStore {
        &self.shared_store
    }

    /// 获取当前会话数量
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// 获取当前 Pipeline 引用
    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    /// 获取 ProviderRegistry 引用
    pub fn provider_registry(&self) -> &Arc<ProviderRegistry> {
        &self.provider_registry
    }

    /// 注册 Provider
    pub fn register_provider<T: Send + Sync + 'static>(&self, name: &str, provider: Arc<T>) {
        self.provider_registry.register(name, provider);
    }

    /// 按名称查找 Provider
    pub fn get_provider<T: Send + Sync + 'static>(&self, name: &str) -> Option<Arc<T>> {
        self.provider_registry.get::<T>(name)
    }

    /// 从 StepContext 提取最后一条 assistant 回复
    fn extract_response(ctx: &StepContext) -> String {
        // 优先从 context["thought"] 提取 Final answer
        if let Some(Thought::Final { answer, .. }) = ctx.get_context::<Thought>(CONTEXT_THOUGHT) {
            if !answer.is_empty() {
                return answer.clone();
            }
        }
        // 回退：从 messages 中找最后一条 assistant 消息
        for msg in ctx.messages.iter().rev() {
            if msg.role == MessageRole::Assistant {
                let text: String = msg
                    .content
                    .iter()
                    .filter_map(|c| {
                        if let ContentBlock::Text(t) = c {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !text.is_empty() {
                    return text;
                }
            }
        }
        String::new()
    }

    /// 创建 ServiceAccessPoint（供 ServicePlugin::start() 使用）
    pub fn create_service_access_point(&self) -> ServiceAccessPoint {
        let impl_ = Arc::new(RuntimeServiceAccessImpl {
            provider_registry: self.provider_registry.clone(),
            config: self.config.clone(),
        });
        ServiceAccessPoint::new(impl_)
    }

    /// 注册 Slot 到 Pipeline（自动调用 init()）
    ///
    /// 封装了「init() → add_slot」两步，防止遗忘 init()。
    pub async fn register_slot(
        &mut self,
        phase: Phase,
        mut slot: Box<dyn SlotPlugin>,
        ctx: &PluginInitContext,
    ) -> Result<(), PluginError> {
        slot.init(ctx).await?;
        self.pipeline.add_slot_mut(phase, slot);
        Ok(())
    }

    /// 直接触发一步，返回执行结果和回复内容
    pub async fn step(&mut self, input: StepInput) -> Result<StepResponse, AgentError> {
        let step_received_at: Timestamp = Timestamp::now();
        tracing::debug!(
            timestamp = %step_received_at,
            session_id = %input.session_id,
            source = ?input.source,
            "[runtime] Direct step input received"
        );

        let state = self
            .sessions
            .entry(input.session_id.clone())
            .or_insert_with(|| {
                SessionState::new(
                    input.session_id.clone(),
                    self.session_max_turns,
                    self.session_context_window,
                )
            });

        let (mut messages, _version) = self.shared_store.read(&input.session_id).await;

        let user_msg = Message {
            role: MessageRole::User,
            content: input.message.clone(),
            tool_calls: None,
            tool_call_id: None,
            reasoning: None,
            metadata: None,
            created_at: Timestamp::now(),
        };
        messages.push(user_msg);

        let mut ctx = StepContext::new(input.session_id.clone(), messages, state.max_turns)
            .with_source(input.source.clone().unwrap_or_default())
            .with_provider_registry(self.provider_registry.clone());

        // 写入 Agent 配置摘要，供 SystemPromptProvider 注入到 LLM prompt
        ctx.set_context(
            CONTEXT_AGENT_CONFIG,
            format!(
                "Agent: {}\n工作目录: {}\n数据目录: {}\n最大回合数: {}\n上下文窗口: {}",
                self.config.agent_id,
                self.config.workspace.display(),
                self.config.data_dir.display(),
                self.session_max_turns,
                self.session_context_window,
            ),
        );

        // 写入 LLM 配置，供 AssemblerSlot 读取 context_window（设计文档 §7.1）
        if let Some(ref llm_cfg) = self.llm_config {
            ctx.set_context(CONTEXT_LLM_CONFIG, llm_cfg.clone());
        }

        let pipeline_started_at: Timestamp = Timestamp::now();
        let result = self.pipeline.run(&mut ctx).await;
        let pipeline_completed_at: Timestamp = Timestamp::now();
        let pipeline_duration_ms = pipeline_completed_at
            .duration_since(pipeline_started_at)
            .as_millis() as i64;

        tracing::info!(
            timestamp = %pipeline_completed_at,
            duration_ms = pipeline_duration_ms,
            session_id = %input.session_id,
            source = ?input.source,
            "[runtime] Direct step pipeline execution completed"
        );

        let response = Self::extract_response(&ctx);

        let session_messages = ctx.messages.clone();
        self.shared_store
            .write(&input.session_id, session_messages.clone())
            .await;

        if let Some(tx) = &self.persistence_tx {
            let _ = tx.send(PersistenceCommand::SaveSession {
                session_id: input.session_id.clone(),
                messages: session_messages.clone(),
                ack_tx: None,
            });
        }

        state.replace_messages(ctx.messages);

        result.map(|sr| sr.with_response(response))
    }
}
