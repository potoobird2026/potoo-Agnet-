use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 顶层配置结构
///
/// 对应 config.toml 格式：
/// ```toml
/// [core]                     # → core: CoreConfig
/// [agent]                    # → agent: AgentSettings
/// [plugins.llm]              # → plugins["llm"]
/// [plugins.tools]            # → plugins["tools"]
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AagnetConfig {
    /// 核心必需配置（对应 `[core]` 段）
    #[serde(default)]
    pub core: CoreConfig,
    /// Agent 身份与行为配置（对应 `[agent]` 段）
    #[serde(default)]
    pub agent: AgentSettings,
    /// 插件配置（对应 `[plugins.*]` 段，key = 插件名）
    #[serde(default)]
    pub plugins: HashMap<String, toml::Value>,
}

/// Agent 身份与行为配置——从 config.toml `[agent]` 段解析
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentSettings {
    /// Agent 名称（映射到 core.CoreConfig.agent_id）
    pub name: Option<String>,
    /// ReAct 最大迭代次数
    pub max_iterations: Option<u64>,
    /// 基础系统提示词
    pub system_prompt: Option<String>,
}

/// 核心配置——极简，不含任何插件细节
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreConfig {
    #[serde(default = "default_agent_id")]
    pub agent_id: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

fn default_agent_id() -> String {
    "aagnet-agent".into()
}
fn default_workspace() -> String {
    "~/.aagnet".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_data_dir() -> String {
    "~/.aagnet/data".into()
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            agent_id: default_agent_id(),
            workspace: default_workspace(),
            log_level: default_log_level(),
            data_dir: default_data_dir(),
        }
    }
}

impl CoreConfig {
    /// 转换为 core 使用的 AgentConfig
    pub fn to_agent_config(&self) -> crate::core::types::plugin::AgentConfig {
        crate::core::types::plugin::AgentConfig {
            agent_id: self.agent_id.clone(),
            workspace: PathBuf::from(&self.workspace),
            log_level: self.log_level.clone(),
            data_dir: PathBuf::from(&self.data_dir),
            context_window: None,
        }
    }
}

impl From<CoreConfig> for crate::core::types::plugin::AgentConfig {
    fn from(c: CoreConfig) -> Self {
        c.to_agent_config()
    }
}
