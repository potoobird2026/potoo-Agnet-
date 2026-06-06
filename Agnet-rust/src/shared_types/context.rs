/*!
 * StepContext.data 键常量——所有通过 SlotAccessPoint.write/read_context_raw
 * 和 StepContext.get_context/set_context 读写的 key 必须使用此处定义的常量，
 * 禁止硬编码字符串。
 */

// ── InitPhaseSlot ─────────────────────────────────────────────
pub const CONTEXT_SESSION_META: &str = "session_meta";
pub const CONTEXT_IDENTITY: &str = "identity";
pub const CONTEXT_WORKING_MEMORY: &str = "working_memory";
pub const CONTEXT_SYSTEM_PROMPT: &str = "system_prompt";

// ── ToolRegistrySlot ──────────────────────────────────────────
pub const CONTEXT_TOOLS: &str = "tools";

// ── LlmThinkerSlot ────────────────────────────────────────────
pub const CONTEXT_THOUGHT: &str = "thought";

// ── AssemblerSlot ─────────────────────────────────────────────
pub const CONTEXT_ASSEMBLER_MESSAGES: &str = "assembler_messages";

// ── ToolExecutorSlot ──────────────────────────────────────────
pub const CONTEXT_OBSERVATION: &str = "observation";
pub const CONTEXT_FINAL_ANSWER: &str = "final_answer";
pub const CONTEXT_CIRCUIT_BREAKER: &str = "circuit_breaker";

// ── AuditPhaseSlot ────────────────────────────────────────────
pub const CONTEXT_AUDIT_LOG: &str = "audit_log";
pub const CONTEXT_AUDIT_WARNINGS: &str = "audit_warnings";
pub const CONTEXT_AUDIT_RESULT: &str = "audit_result";

// ── MemorySaverSlot ───────────────────────────────────────────
pub const CONTEXT_LAST_PERSISTED_COUNT: &str = "last_persisted_count";
pub const CONTEXT_LAST_INDEXED_COUNT: &str = "last_indexed_count";
pub const CONTEXT_MEMORY_PERSISTED: &str = "memory_persisted";

// ── AssemblerSlot (读入) ──────────────────────────────────────
pub const CONTEXT_LLM_CONFIG: &str = "llm_config";

// ── AgentRuntime / InitPhaseSlot —— Agent 配置摘要 ────────────
pub const CONTEXT_AGENT_CONFIG: &str = "agent_config_info";

// ── StepContext.write_observation() 内部键 ────────────────────
pub const CONTEXT_OBSERVATION_INTERNAL: &str = "__observation";

// ── LlmThinkerSlot (K-R01 fix) ────────────────────────────
pub const PROVIDER_SESSION_CONTEXT: &str = "session-context";
