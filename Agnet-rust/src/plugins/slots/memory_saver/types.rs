use crate::core::types::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPersistedMarker {
    pub session_id: String,
    pub persisted_count: usize,
    pub timestamp: Timestamp,
}
