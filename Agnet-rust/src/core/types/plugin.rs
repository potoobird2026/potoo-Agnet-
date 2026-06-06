use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 插件运行模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RunMode {
    /// 后台持续运行
    #[serde(rename = "background")]
    #[default]
    Background,
    /// 按需启动
    #[serde(rename = "on_demand")]
    OnDemand,
    /// 定时触发
    #[serde(rename = "cron")]
    Cron,
}

/// Agent 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub agent_id: String,
    pub workspace: PathBuf,
    pub log_level: String,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub context_window: Option<usize>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: "aagnet-agent".to_string(),
            workspace: PathBuf::from("~/.aagnet"),
            log_level: "info".to_string(),
            data_dir: PathBuf::from("~/.aagnet/data"),
            context_window: None,
        }
    }
}

/// 插件初始化上下文——每个插件在 init() 时收到
#[derive(Debug, Clone)]
pub struct PluginInitContext {
    /// 插件名称，与 YAML 元数据中的 `name` 一致
    pub plugin_name: String,
    /// 该插件的专属配置段（由 PluginLoader 从 TOML 解析并注入）
    pub plugin_config: Value,
    /// Agent 全局配置
    pub agent_config: AgentConfig,
    /// 插件可用的私有数据目录
    pub data_dir: PathBuf,
}

impl PluginInitContext {
    pub fn new(
        plugin_name: impl Into<String>,
        plugin_config: Value,
        agent_config: AgentConfig,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            plugin_config,
            agent_config,
            data_dir,
        }
    }
}

/// 插件元数据声明——每个插件在 YAML 中声明，PluginLoader 读取校验
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// 全局唯一标识
    pub name: String,
    /// 类别："slot" 或 "service"
    pub category: String,
    /// 语义版本
    pub version: String,
    /// 运行模式
    #[serde(default)]
    pub run_mode: RunMode,
    /// Slot 用：声明需要 core 内建的哪些权限
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Service 用：声明注册哪些 Provider 名称
    #[serde(default)]
    pub provides: Vec<String>,
    /// 依赖的其他插件/Provider 名
    #[serde(default)]
    pub requires: Vec<String>,
    /// 冲突的插件名
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// JSON Schema 格式的配置定义（可选，用于 UI 自动生成配置表单）
    #[serde(default)]
    pub config_schema: Option<Value>,
}
