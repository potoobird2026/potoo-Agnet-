/*! ToolManifest —— 工具清单解析与校验 */
use crate::shared_types::ToolSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub entry: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
    /// A-4: 工具来源标签（默认 Builtin，老 YAML/TOML 不带此字段不报错）
    #[serde(default)]
    pub source: ToolSource,
}
impl ToolManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("工具名不能为空".into());
        }
        if self.version.is_empty() {
            return Err("版本不能为空".into());
        }
        Ok(())
    }
}
