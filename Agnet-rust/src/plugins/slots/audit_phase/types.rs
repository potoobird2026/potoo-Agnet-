use serde::{Deserialize, Serialize};

use crate::core::types::Timestamp;
use crate::shared_types::RiskSeverity;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: Timestamp,
    pub session_id: String,
    pub tool_name: String,
    pub result: String,
    pub reason: String,
    pub risk_level: RiskSeverity,
}

#[derive(Debug, Clone)]
pub struct AuditContext {
    pub session_id: String,
    pub phase_name: String,
}
