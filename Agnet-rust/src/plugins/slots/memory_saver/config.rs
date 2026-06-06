use serde::{Deserialize, Serialize};

pub const DEFAULT_MEMORY_WRITE_TIMEOUT_SECS: u64 = 10;
pub const DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE: usize = 5;
pub const LOG_PREFIX: &str = "memory_saver:";
pub const CFG_KEY: &str = "memory_saver";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySaverConfig {
    #[serde(default = "default_true")]
    pub persist_user_messages: bool,
    #[serde(default = "default_true")]
    pub persist_observations: bool,
    #[serde(default = "default_true")]
    pub update_vector_index: bool,
    #[serde(default)]
    pub enable_experience_extract: bool,
    #[serde(default = "default_min_messages_for_experience")]
    pub min_messages_for_experience: usize,
    #[serde(default = "default_write_timeout_secs")]
    pub write_timeout_secs: u64,
}

fn default_true() -> bool {
    true
}
fn default_min_messages_for_experience() -> usize {
    DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE
}
fn default_write_timeout_secs() -> u64 {
    DEFAULT_MEMORY_WRITE_TIMEOUT_SECS
}
