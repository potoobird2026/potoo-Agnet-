/*! ToolInstallManager —— 安装/卸载管理 */
use super::config::ToolsConfig;
use std::path::PathBuf;
pub struct ToolInstallManager {
    config: ToolsConfig,
}
impl ToolInstallManager {
    pub fn new(config: ToolsConfig) -> Self {
        Self { config }
    }
    pub fn install_path(&self, name: &str) -> PathBuf {
        self.config.tools_dir.join(name)
    }
    pub fn is_installed(&self, name: &str) -> bool {
        self.install_path(name).exists()
    }
    pub async fn uninstall(&self, name: &str) -> Result<(), String> {
        let path = self.install_path(name);
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| format!("卸载失败: {}", e))?;
        }
        Ok(())
    }
}
