use std::path::PathBuf;

use aagnet::core::{
    AgentConfig, AgentRuntime, Phase, Pipeline, PluginInitContext, ServiceManager, ServicePlugin,
    StepInput,
};
use aagnet::infra::config::ConfigLoader;
use aagnet::plugins::services::chronos::ChronosServicePlugin;
use aagnet::plugins::services::cli::CliChannel;
use aagnet::plugins::services::compression::{CompressionHookSlot, CompressionService};
use aagnet::plugins::services::llm::LlmService;
use aagnet::plugins::services::mcp::McpService;
use aagnet::plugins::services::memory::MemoryService;
use aagnet::plugins::services::security::SecurityService;
use aagnet::plugins::services::skills::SkillsService;
use aagnet::plugins::services::tools::ToolsService;
use aagnet::plugins::slots::assembler::AssemblerSlot;
use aagnet::plugins::slots::audit_phase::AuditPhaseSlot;
use aagnet::plugins::slots::init_phase::InitPhaseSlot;
use aagnet::plugins::slots::llm_thinker::llm_thinker_slot::LlmThinkerSlot;
use aagnet::plugins::slots::memory_saver::MemorySaverSlot;
use aagnet::plugins::slots::react_loop::ReActLoopSlot;
use aagnet::plugins::slots::tool_executor::ToolExecutorSlot;
use aagnet::plugins::slots::tool_registry::ToolRegistrySlot;
use aagnet::shared_types::llm::LlmConfig;

use aagnet::plugins::slots::observation_sync::ObservationSyncSlot;
use aagnet::plugins::slots::thought_sync::ThoughtSyncSlot;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 通过 ConfigLoader 加载配置（严格模式——失败直接退出）
    let config_loader = ConfigLoader::load(Some(PathBuf::from("config/config.toml")))
        .map_err(|e| format!("配置加载失败: {e}"))?;

    let agent_config: AgentConfig = config_loader.current().core.to_agent_config();

    // 2. 提取 LLM 配置（单独处理，因需要包裹为 {"llm": {…}} 格式）
    let llm_config_value = config_loader
        .get_section_json("llm")
        .ok_or_else(|| "[plugins.llm] 未在 config.toml 中配置".to_string())?;
    let llm_config: LlmConfig = serde_json::from_value(llm_config_value.clone())
        .map_err(|e| format!("LlmConfig 解析失败: {}", e))?;

    // 3. 创建 Slot 实例（init() 由 runtime.register_slot() 自动调用）
    let llm_slot = LlmThinkerSlot::new();
    let react_loop_slot = ReActLoopSlot::new();
    let audit_slot = AuditPhaseSlot::new();
    let memory_saver_slot = MemorySaverSlot::new();

    // 4. 注册到 Pipeline（通过 register_slot 自动 init() + add_slot）
    let mut runtime =
        AgentRuntime::new_with_config(Pipeline::with_recommended_phases(), agent_config.clone())
            .with_llm_config(llm_config.clone());

    // 辅助函数：获取插件 JSON 配置段，缺失时返回空对象
    let plugin_cfg = |name: &str| -> serde_json::Value {
        config_loader
            .get_section_json(name)
            .unwrap_or(serde_json::json!({}))
    };

    runtime
        .register_slot(
            Phase::init(),
            Box::new(InitPhaseSlot::new()),
            &PluginInitContext::new(
                "init_phase",
                plugin_cfg("init_phase"),
                agent_config.clone(),
                PathBuf::from("./data/init_phase"),
            ),
        )
        .await
        .map_err(|e| format!("InitPhaseSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::context(),
            Box::new(ToolRegistrySlot::new()),
            &PluginInitContext::new(
                "tool_registry",
                plugin_cfg("tool_registry"),
                agent_config.clone(),
                PathBuf::from("./data/tool_registry"),
            ),
        )
        .await
        .map_err(|e| format!("ToolRegistrySlot.init() 失败: {e}"))?;

    // AssemblerSlot —— 上下文组装（CONTEXT 阶段，ToolRegistrySlot 之后、LlmThinkerSlot 之前）
    runtime
        .register_slot(
            Phase::context(),
            Box::new(AssemblerSlot::new()),
            &PluginInitContext::new(
                "assembler",
                plugin_cfg("assembler"),
                agent_config.clone(),
                PathBuf::from("./data/assembler"),
            ),
        )
        .await
        .map_err(|e| format!("AssemblerSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::think(),
            Box::new(llm_slot),
            &PluginInitContext::new(
                "llm_thinker",
                serde_json::json!({ "llm": llm_config_value.clone() }),
                agent_config.clone(),
                PathBuf::from("./data/llm_thinker"),
            ),
        )
        .await
        .map_err(|e| format!("LlmThinkerSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::think(),
            Box::new(ThoughtSyncSlot::new()),
            &PluginInitContext::new(
                "thought_sync",
                serde_json::json!({}),
                agent_config.clone(),
                PathBuf::from("./data/thought_sync"),
            ),
        )
        .await
        .map_err(|e| format!("ThoughtSyncSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::audit(),
            Box::new(audit_slot),
            &PluginInitContext::new(
                "audit_phase",
                plugin_cfg("audit_phase"),
                agent_config.clone(),
                PathBuf::from("./data/audit_phase"),
            ),
        )
        .await
        .map_err(|e| format!("AuditPhaseSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::execute(),
            Box::new(ToolExecutorSlot::new()),
            &PluginInitContext::new(
                "tool_executor",
                plugin_cfg("tool_executor"),
                agent_config.clone(),
                PathBuf::from("./data/tool_executor"),
            ),
        )
        .await
        .map_err(|e| format!("ToolExecutorSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::execute(),
            Box::new(ObservationSyncSlot::new()),
            &PluginInitContext::new(
                "observation_sync",
                serde_json::json!({}),
                agent_config.clone(),
                PathBuf::from("./data/observation_sync"),
            ),
        )
        .await
        .map_err(|e| format!("ObservationSyncSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::loop_phase(),
            Box::new(react_loop_slot),
            &PluginInitContext::new(
                "react_loop",
                plugin_cfg("react_loop"),
                agent_config.clone(),
                PathBuf::from("./data/react_loop"),
            ),
        )
        .await
        .map_err(|e| format!("ReActLoopSlot.init() 失败: {e}"))?;

    runtime
        .register_slot(
            Phase::memorize(),
            Box::new(memory_saver_slot),
            &PluginInitContext::new(
                "memory_saver",
                plugin_cfg("memory_saver"),
                agent_config.clone(),
                PathBuf::from("./data/memory_saver"),
            ),
        )
        .await
        .map_err(|e| format!("MemorySaverSlot.init() 失败: {e}"))?;

    // 5. 初始化并启动所有 ServicePlugin
    let ap = runtime.create_service_access_point();

    // 5a. 标准服务：通过 ServiceManager 统一管理
    let mut sm = ServiceManager::new();

    // MCP 必须先注册和启动，Tools 才能拉取 MCP 工具代理
    sm.register(
        "mcp",
        PathBuf::from("./data/mcp"),
        Box::new(McpService::new()),
    );
    // ToolsService 必须在 MCP 之后启动
    sm.register(
        "tools",
        PathBuf::from("./data/tools"),
        Box::new(ToolsService::new()),
    );
    sm.register(
        "chronos",
        PathBuf::from("./data/chronos"),
        Box::new(ChronosServicePlugin::new()),
    );
    sm.register(
        "memory",
        PathBuf::from("./data/memory"),
        Box::new(MemoryService::new()),
    );
    sm.register(
        "security",
        PathBuf::from("./data/security"),
        Box::new(SecurityService::new()),
    );
    sm.register(
        "skills",
        PathBuf::from("./data/skills"),
        Box::new(SkillsService::new()),
    );
    sm.register(
        "cli",
        PathBuf::from("./data/cli"),
        Box::new(CliChannel::new()),
    );

    let plugins_json = config_loader.plugins_as_json();
    sm.init_all(&plugins_json, &agent_config).await?;
    sm.start_all(ap.clone()).await?;

    // 5b. LlmService（配置需 {"llm": ...} 包裹，单独初始化）
    let mut llm_service = LlmService::new();
    let llm_ctx = PluginInitContext::new(
        "llm",
        serde_json::json!({ "llm": llm_config_value.clone() }),
        agent_config.clone(),
        PathBuf::from("./data/llm"),
    );
    llm_service
        .init(&llm_ctx)
        .await
        .map_err(|e| format!("LlmService.init() 失败: {e}"))?;
    llm_service
        .start(ap.clone())
        .await
        .map_err(|e| format!("LlmService.start() 失败: {e}"))?;

    // 5c. CompressionService（需注入 SharedMessageStore，单独管理）
    let mut compression = CompressionService::new();
    let compression_ctx = PluginInitContext::new(
        "compression",
        plugin_cfg("compression"),
        agent_config.clone(),
        PathBuf::from("./data/compression"),
    );
    compression
        .init(&compression_ctx)
        .await
        .map_err(|e| format!("CompressionService.init() 失败: {e}"))?;
    compression
        .start(ap.clone())
        .await
        .map_err(|e| format!("CompressionService.start() 失败: {e}"))?;
    compression
        .set_shared_store(runtime.shared_store().clone())
        .await;

    // 6. CompressionHookSlot —— Memorize 阶段钩子，在 MemorySaverSlot 之后触发压缩
    let event_tx = compression.event_sender();
    let compression_hook_slot = CompressionHookSlot::new(event_tx);
    runtime
        .register_slot(
            Phase::memorize(),
            Box::new(compression_hook_slot),
            &PluginInitContext::new(
                "compression_hook",
                plugin_cfg("compression"),
                agent_config.clone(),
                PathBuf::from("./data/compression_hook"),
            ),
        )
        .await
        .map_err(|e| format!("CompressionHookSlot.init() 失败: {e}"))?;

    // 7. 交互循环
    let mut session_counter = 0u64;
    loop {
        session_counter += 1;
        let session_id = format!("session-{}", session_counter);
        let mut input = String::new();
        tracing::info!("[{}] 输入消息（空行退出）:", session_id);
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("读取输入失败: {e}"))?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            tracing::info!("退出。");
            break;
        }

        let result = runtime
            .step(StepInput::new(&session_id, trimmed))
            .await
            .map_err(|e| format!("step 执行失败: {e}"))?;

        // 提取回复内容
        let response = result.response();
        tracing::info!("回应: {}", response);
    }

    Ok(())
}
