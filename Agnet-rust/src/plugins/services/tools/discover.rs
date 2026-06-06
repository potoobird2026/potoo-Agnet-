/*! ToolDiscover —— 已安装工具扫描 */
use super::manifest::ToolManifest;
use std::path::PathBuf;
pub struct ToolDiscover {
    root_dir: PathBuf,
}
impl ToolDiscover {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }
    pub async fn discover(&self) -> Vec<ToolManifest> {
        let mut tools = Vec::new();
        if !self.root_dir.exists() {
            return tools;
        }
        if let Ok(mut entries) = tokio::fs::read_dir(&self.root_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let manifest_path = entry.path().join("manifest.toml");
                if manifest_path.exists() {
                    if let Ok(content_str) = tokio::fs::read_to_string(&manifest_path).await {
                        if let Ok(manifest) = toml::from_str::<ToolManifest>(&content_str) {
                            if manifest.validate().is_ok() {
                                tools.push(manifest);
                            }
                        }
                    }
                }
            }
        }
        tools
    }
}
