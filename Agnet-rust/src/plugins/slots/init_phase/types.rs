use crate::core::types::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub is_new: bool,
    pub initialized_at: Timestamp,
}
