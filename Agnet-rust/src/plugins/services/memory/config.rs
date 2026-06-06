/*! Memory 配置层 —— 所有配置结构体实现 Default + Serialize/Deserialize */
use crate::core::types::error::PluginError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ============================================
// MemoryConfig
// ============================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_workspace")]
    pub workspace_dir: PathBuf,
    #[serde(default)]
    pub l1: L1Config,
    #[serde(default)]
    pub l2: L2Config,
    #[cfg(feature = "vector_db")]
    #[serde(default)]
    pub l3: L3Config,
    #[serde(default = "default_true")]
    pub forgetting_enabled: bool,
    #[serde(default = "default_86400")]
    pub forgetting_interval_seconds: u64,
    #[serde(default = "default_100")]
    pub max_active_files: usize,
    #[serde(default)]
    pub max_file_age_days: Option<u64>,
    #[serde(default)]
    pub backup_enabled: bool,
    #[serde(default)]
    pub backup_dir: Option<PathBuf>,
    #[serde(default)]
    pub forgetting: ForgettingConfig,
}
fn default_workspace() -> PathBuf {
    crate::plugins::services::storage::memory_dir()
}
fn default_true() -> bool {
    true
}
fn default_86400() -> u64 {
    86400
}
fn default_100() -> usize {
    100
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            workspace_dir: default_workspace(),
            l1: L1Config::default(),
            l2: L2Config::default(),
            #[cfg(feature = "vector_db")]
            l3: L3Config::default(),
            forgetting_enabled: true,
            forgetting_interval_seconds: 86400,
            max_active_files: 100,
            max_file_age_days: None,
            backup_enabled: false,
            backup_dir: None,
            forgetting: ForgettingConfig::default(),
        }
    }
}
impl MemoryConfig {
    pub fn resolve_paths(&mut self) {
        let home = dirs::home_dir().unwrap_or_default();
        expand_tilde(&mut self.workspace_dir, &home);
        self.l1.resolve_paths(&home);
        self.l2.resolve_paths(&home);
        #[cfg(feature = "vector_db")]
        self.l3.resolve_paths(&home);
        if let Some(ref mut bd) = self.backup_dir {
            expand_tilde(bd, &home);
        }
        if self.backup_dir.is_none() {
            self.backup_dir = Some(self.workspace_dir.join(".backup"));
        }
    }
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.forgetting_interval_seconds == 0 {
            return Err(PluginError::Config(
                "forgetting_interval_seconds 不能为 0".into(),
            ));
        }
        Ok(())
    }
}
fn expand_tilde(p: &mut PathBuf, home: &Path) {
    if let Some(s) = p.to_str() {
        if let Some(stripped) = s.strip_prefix("~/") {
            *p = home.join(stripped);
        }
    }
}

// ============================================
// ForgettingConfig
// ============================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingConfig {
    #[serde(default = "f02")]
    pub pid_kp: f64,
    #[serde(default = "f0005")]
    pub pid_ki: f64,
    #[serde(default = "f001")]
    pub pid_kd: f64,
    #[serde(default = "f02t")]
    pub threshold_min: f64,
    #[serde(default = "f08")]
    pub threshold_max: f64,
    #[serde(default = "d7")]
    pub access_protection_days: u64,
    #[serde(default = "d365")]
    pub deep_delete_age_days: u64,
    #[serde(default = "f005")]
    pub deep_delete_weight: f64,
    #[serde(default = "f105")]
    pub feedback_success_multiplier: f64,
    #[serde(default = "f09")]
    pub feedback_failure_multiplier: f64,
    #[serde(default = "f001w")]
    pub weight_floor: f64,
}
fn f02() -> f64 {
    0.02
}
fn f0005() -> f64 {
    0.005
}
fn f001() -> f64 {
    0.01
}
fn f02t() -> f64 {
    0.2
}
fn f08() -> f64 {
    0.8
}
fn d7() -> u64 {
    7
}
fn d365() -> u64 {
    365
}
fn f005() -> f64 {
    0.05
}
fn f105() -> f64 {
    1.05
}
fn f09() -> f64 {
    0.9
}
fn f001w() -> f64 {
    0.01
}
impl Default for ForgettingConfig {
    fn default() -> Self {
        Self {
            pid_kp: 0.02,
            pid_ki: 0.005,
            pid_kd: 0.01,
            threshold_min: 0.2,
            threshold_max: 0.8,
            access_protection_days: 7,
            deep_delete_age_days: 365,
            deep_delete_weight: 0.05,
            feedback_success_multiplier: 1.05,
            feedback_failure_multiplier: 0.9,
            weight_floor: 0.01,
        }
    }
}

// ============================================
// L1 / L2 / L3 configs
// ============================================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L1Config {
    #[serde(default = "default_identity_path")]
    pub identity_path: PathBuf,
    #[serde(default = "default_true")]
    pub auto_update: bool,
    #[serde(default = "default_inject_prefix")]
    pub inject_prefix: String,
}
fn default_identity_path() -> PathBuf {
    PathBuf::from("IDENTITY.md")
}
fn default_inject_prefix() -> String {
    "## Agent Identity\n\n".into()
}
impl Default for L1Config {
    fn default() -> Self {
        Self {
            identity_path: default_identity_path(),
            auto_update: true,
            inject_prefix: default_inject_prefix(),
        }
    }
}
impl L1Config {
    pub fn resolve_paths(&mut self, _home: &PathBuf) {}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L2Config {
    #[serde(default = "default_l2_base")]
    pub base_dir: PathBuf,
    #[serde(default = "default_500")]
    pub max_files: usize,
    #[serde(default = "default_index")]
    pub index_path: PathBuf,
}
fn default_l2_base() -> PathBuf {
    PathBuf::from("memory")
}
fn default_500() -> usize {
    500
}
fn default_index() -> PathBuf {
    PathBuf::from("INDEX.md")
}
impl Default for L2Config {
    fn default() -> Self {
        Self {
            base_dir: default_l2_base(),
            max_files: 500,
            index_path: default_index(),
        }
    }
}
impl L2Config {
    pub fn resolve_paths(&mut self, _home: &PathBuf) {}
}

#[cfg(feature = "vector_db")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct L3Config {
    #[serde(default)]
    pub backend: VectorBackend,
    #[serde(default)]
    pub chunking: ChunkingConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
}
#[cfg(feature = "vector_db")]
impl Default for L3Config {
    fn default() -> Self {
        Self {
            backend: VectorBackend::Memory,
            chunking: ChunkingConfig::default(),
            embedding: EmbeddingConfig::default(),
        }
    }
}
#[cfg(feature = "vector_db")]
impl L3Config {
    pub fn resolve_paths(&mut self, _home: &PathBuf) {}
}

#[cfg(feature = "vector_db")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VectorBackend {
    #[serde(rename = "memory")]
    #[default]
    Memory,
    #[serde(rename = "sqlite")]
    Sqlite,
    #[serde(rename = "lancedb")]
    LanceDb,
    #[serde(rename = "qdrant")]
    Qdrant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
    #[serde(default = "d512")]
    pub chunk_size: usize,
    #[serde(default = "d50")]
    pub chunk_overlap: usize,
}
fn d512() -> usize {
    512
}
fn d50() -> usize {
    50
}
impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size: 512,
            chunk_overlap: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_url")]
    pub base_url: String,
    #[serde(default = "d1536")]
    pub dim: usize,
    #[serde(default = "d100b")]
    pub batch_size: usize,
}
fn default_model() -> String {
    "text-embedding-3-small".into()
}
fn default_url() -> String {
    "https://api.openai.com/v1/embeddings".into()
}
fn d1536() -> usize {
    1536
}
fn d100b() -> usize {
    100
}
impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            base_url: default_url(),
            dim: 1536,
            batch_size: 100,
        }
    }
}
