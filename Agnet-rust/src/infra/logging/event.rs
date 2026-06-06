/*!
 * Logger ?事件类型定义
 *
 * 功能描述：SystemEvent 枚举包含所有可记录的业务事件类型，
 * 每个变体携带事件专属的上下文数据? * 公共字段（timestamp, event_id, session_id, trace_id, module, level? * ?LogEntry 在写入时自动添加? */

use serde::Serialize;

use super::config::EventLevel;

// ============================================
// Payload types
// ============================================

macro_rules! impl_payload {
    ($t:ty, $type_str:expr, $level:expr, $module:expr) => {
        impl $t {
            pub fn event_type(&self) -> &'static str {
                $type_str
            }
            pub fn level(&self) -> &'static EventLevel {
                &$level
            }
            pub fn module(&self) -> &'static str {
                $module
            }
        }
    };
}

#[derive(Debug, Clone, Serialize)]
pub struct CompressionStarted {
    pub session_id: Option<String>,
    pub context_size_tokens: usize,
    pub target_tokens: usize,
}
impl_payload!(
    CompressionStarted,
    "CompressionStarted",
    EventLevel::Info,
    "compression"
);

#[derive(Debug, Clone, Serialize)]
pub struct CompressionCompleted {
    pub session_id: Option<String>,
    pub duration_ms: u64,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub compression_ratio: f64,
}
impl_payload!(
    CompressionCompleted,
    "CompressionCompleted",
    EventLevel::Info,
    "compression"
);

#[derive(Debug, Clone, Serialize)]
pub struct CompressionFailed {
    pub session_id: Option<String>,
    pub error_message: String,
}
impl_payload!(
    CompressionFailed,
    "CompressionFailed",
    EventLevel::Error,
    "compression"
);

#[derive(Debug, Clone, Serialize)]
pub struct CompressionCasConflict {
    pub session_id: Option<String>,
    pub expected_version: u64,
    pub actual_version: u64,
}
impl_payload!(
    CompressionCasConflict,
    "CompressionCasConflict",
    EventLevel::Warning,
    "compression"
);

#[derive(Debug, Clone, Serialize)]
pub struct PersistenceSnapshot {
    pub session_id: Option<String>,
    pub entry_count: usize,
    pub size_bytes: u64,
    pub duration_ms: u64,
}
impl_payload!(
    PersistenceSnapshot,
    "PersistenceSnapshot",
    EventLevel::Debug,
    "persistence"
);

#[derive(Debug, Clone, Serialize)]
pub struct PersistenceError {
    pub file_path: String,
    pub error_message: String,
}
impl_payload!(
    PersistenceError,
    "PersistenceError",
    EventLevel::Error,
    "persistence"
);

#[derive(Debug, Clone, Serialize)]
pub struct ChronosDecision {
    pub session_id: Option<String>,
    pub action: String,
    pub confidence: f64,
    pub source: String,
}
impl_payload!(
    ChronosDecision,
    "ChronosDecision",
    EventLevel::Info,
    "chronos"
);

#[derive(Debug, Clone, Serialize)]
pub struct ChronosFeedback {
    pub session_id: Option<String>,
    pub decision_id: String,
    pub feedback_type: String,
}
impl_payload!(
    ChronosFeedback,
    "ChronosFeedback",
    EventLevel::Info,
    "chronos"
);

#[derive(Debug, Clone, Serialize)]
pub struct SystemStartup {
    pub version: String,
}
impl_payload!(SystemStartup, "SystemStartup", EventLevel::Info, "system");

#[derive(Debug, Clone, Serialize)]
pub struct SystemShutdown {
    pub uptime_secs: u64,
    pub total_events: u64,
}
impl_payload!(SystemShutdown, "SystemShutdown", EventLevel::Info, "system");

#[derive(Debug, Clone, Serialize)]
pub struct AggregatedStats {
    pub total_compressions: u64,
    pub avg_ratio: f64,
    pub avg_duration_ms: u64,
    pub cas_conflicts: u64,
}
impl_payload!(
    AggregatedStats,
    "AggregatedStats",
    EventLevel::Info,
    "aggregator"
);

// ============================================
// 关键路径审计事件
// ============================================

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallStarted {
    pub session_id: Option<String>,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}
impl_payload!(
    ToolCallStarted,
    "ToolCallStarted",
    EventLevel::Debug,
    "tool_executor"
);

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallCompleted {
    pub session_id: Option<String>,
    pub tool_name: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}
impl_payload!(
    ToolCallCompleted,
    "ToolCallCompleted",
    EventLevel::Info,
    "tool_executor"
);

#[derive(Debug, Clone, Serialize)]
pub struct AuthDecided {
    pub session_id: Option<String>,
    pub resource: String,
    pub tool_name: String,
    pub decision: String,
    pub reason: String,
}
impl_payload!(AuthDecided, "AuthDecision", EventLevel::Info, "auth");

#[derive(Debug, Clone, Serialize)]
pub struct ConfigChanged {
    pub section: String,
    pub summary: String,
}
impl_payload!(
    ConfigChanged,
    "ConfigChanged",
    EventLevel::Info,
    "config_loader"
);

#[derive(Debug, Clone, Serialize)]
pub struct ComponentToggled {
    pub kind: String,
    pub name: String,
    pub enabled: bool,
}
impl_payload!(
    ComponentToggled,
    "ComponentToggled",
    EventLevel::Info,
    "component_switch"
);

#[derive(Debug, Clone, Serialize)]
pub struct SecurityDecided {
    pub session_id: Option<String>,
    pub policy_name: String,
    pub decision: String,
    pub reason: String,
}
impl_payload!(
    SecurityDecided,
    "SecurityDecided",
    EventLevel::Info,
    "security_policy"
);

// ============================================
// SystemEvent — dispatch enum with pre-serialized payload
// ============================================

pub enum SystemEvent {
    CompressionStarted(CompressionStarted),
    CompressionCompleted(CompressionCompleted),
    CompressionFailed(CompressionFailed),
    CompressionCasConflict(CompressionCasConflict),
    PersistenceSnapshot(PersistenceSnapshot),
    PersistenceError(PersistenceError),
    ChronosDecision(ChronosDecision),
    ChronosFeedback(ChronosFeedback),
    SystemStartup(SystemStartup),
    SystemShutdown(SystemShutdown),
    AggregatedStats(AggregatedStats),
    ToolCallStarted(ToolCallStarted),
    ToolCallCompleted(ToolCallCompleted),
    AuthDecision(AuthDecided),
    ConfigChanged(ConfigChanged),
    ComponentToggled(ComponentToggled),
    /// 安全策略决策（来自 `security_policy` 模块）
    SecurityDecided(SecurityDecided),
}

/// Pre-computed event metadata extracted from the payload.
pub struct EventMeta {
    pub event_type: &'static str,
    pub level: EventLevel,
    pub module: &'static str,
    pub payload_json: serde_json::Value,
}

impl SystemEvent {
    /// Extract metadata and serialize the payload to JSON.
    /// This avoids needing a trait object.
    pub fn into_meta(self) -> EventMeta {
        macro_rules! meta {
            ($payload:expr) => {{
                let p = $payload;
                EventMeta {
                    event_type: p.event_type(),
                    level: *p.level(),
                    module: p.module(),
                    payload_json: serde_json::to_value(&p).unwrap_or_default(),
                }
            }};
        }
        match self {
            Self::CompressionStarted(p) => meta!(p),
            Self::CompressionCompleted(p) => meta!(p),
            Self::CompressionFailed(p) => meta!(p),
            Self::CompressionCasConflict(p) => meta!(p),
            Self::PersistenceSnapshot(p) => meta!(p),
            Self::PersistenceError(p) => meta!(p),
            Self::ChronosDecision(p) => meta!(p),
            Self::ChronosFeedback(p) => meta!(p),
            Self::SystemStartup(p) => meta!(p),
            Self::SystemShutdown(p) => meta!(p),
            Self::AggregatedStats(p) => meta!(p),
            Self::ToolCallStarted(p) => meta!(p),
            Self::ToolCallCompleted(p) => meta!(p),
            Self::AuthDecision(p) => meta!(p),
            Self::ConfigChanged(p) => meta!(p),
            Self::ComponentToggled(p) => meta!(p),
            Self::SecurityDecided(p) => meta!(p),
        }
    }

    pub fn level(&self) -> EventLevel {
        match self {
            Self::CompressionStarted(p) => *p.level(),
            Self::CompressionCompleted(p) => *p.level(),
            Self::CompressionFailed(p) => *p.level(),
            Self::CompressionCasConflict(p) => *p.level(),
            Self::PersistenceSnapshot(p) => *p.level(),
            Self::PersistenceError(p) => *p.level(),
            Self::ChronosDecision(p) => *p.level(),
            Self::ChronosFeedback(p) => *p.level(),
            Self::SystemStartup(p) => *p.level(),
            Self::SystemShutdown(p) => *p.level(),
            Self::AggregatedStats(p) => *p.level(),
            Self::ToolCallStarted(p) => *p.level(),
            Self::ToolCallCompleted(p) => *p.level(),
            Self::AuthDecision(p) => *p.level(),
            Self::ConfigChanged(p) => *p.level(),
            Self::ComponentToggled(p) => *p.level(),
            Self::SecurityDecided(p) => *p.level(),
        }
    }
}

// ============================================
// Public log entry (serialized to JSONL)
// ============================================

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub module: &'static str,
    pub level: &'static str,
    pub event_type: &'static str,
    pub payload: serde_json::Value,
}

impl LogEntry {
    pub fn from_meta(meta: EventMeta, session_id: Option<String>) -> Self {
        let level_str = match meta.level {
            EventLevel::Debug => "DEBUG",
            EventLevel::Info => "INFO",
            EventLevel::Warning => "WARN",
            EventLevel::Error => "ERROR",
        };
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id,
            trace_id: Self::extract_trace_id(),
            module: meta.module,
            level: level_str,
            event_type: meta.event_type,
            payload: meta.payload_json,
        }
    }

    /// 从 tracing::Span 的字段中提取 trace_id（若有）
    fn extract_trace_id() -> Option<String> {
        let current_span = tracing::Span::current();
        if current_span.is_disabled() {
            return None;
        }
        // 尝试通过 Span 的 extensions 获取 trace_id
        // 当前 tracing 未配置 OpenTelemetry，此方法留空
        // 后续可扩展为从 Span::current().extensions() 提取
        None
    }
}
