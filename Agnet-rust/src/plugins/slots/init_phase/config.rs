use serde::{Deserialize, Serialize};

pub const DEFAULT_WORKING_MEMORY_LIMIT: usize = 10;
pub const DEFAULT_MAX_MESSAGES_PRECHECK: usize = 100;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InitPhaseConfig {
    #[serde(default = "default_true")]
    pub load_identity: bool,
    #[serde(default = "default_true")]
    pub load_working_memory: bool,
    #[serde(default = "default_working_memory_limit")]
    pub working_memory_limit: usize,
    #[serde(default = "default_true")]
    pub assemble_system_prompt: bool,
    #[serde(default)]
    pub system_prompt_template: Option<String>,
    #[serde(default = "default_max_messages")]
    pub max_messages_precheck: usize,
}

fn default_true() -> bool {
    true
}
fn default_working_memory_limit() -> usize {
    DEFAULT_WORKING_MEMORY_LIMIT
}
fn default_max_messages() -> usize {
    DEFAULT_MAX_MESSAGES_PRECHECK
}
