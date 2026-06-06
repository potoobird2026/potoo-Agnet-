/*! ToolPackage —— .atp 包解析 */
use super::manifest::ToolManifest;
use std::path::Path;
pub struct ToolPackage;
impl ToolPackage {
    pub async fn parse(path: &Path) -> Result<ToolManifest, String> {
        if !path.exists() {
            return Err(format!("包不存在: '{}'", path.display()));
        }
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("读取失败: {}", e))?;
        let manifest: ToolManifest =
            toml::from_str(&content).map_err(|e| format!("TOML 解析失败: {}", e))?;
        manifest.validate()?;
        Ok(manifest)
    }
}
