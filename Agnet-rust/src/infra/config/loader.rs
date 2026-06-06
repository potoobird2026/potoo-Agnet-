use std::path::PathBuf;

use super::types::AagnetConfig;

/// 配置加载器——开机时读 config.toml，存内存，提供读写接口
pub struct ConfigLoader {
    /// 当前生效的完整配置
    current: AagnetConfig,
    /// 配置文件路径（记录位置，update_config 时写回用）
    config_path: Option<PathBuf>,
}

impl ConfigLoader {
    /// 创建加载器并读取配置文件（宽松模式——失败时使用默认值 + 打警告）
    ///
    /// - 给了路径 → 读文件 → 解析 TOML → 存内存
    /// - 没给路径 → 用默认值（不自动创建文件）
    /// - 读文件失败 → 用默认值 + 打警告
    pub fn new(config_path: Option<PathBuf>) -> Self {
        match Self::load(config_path) {
            Ok(loader) => loader,
            Err(e) => {
                tracing::warn!("{}，使用默认配置", e);
                Self {
                    current: AagnetConfig::default(),
                    config_path: None,
                }
            }
        }
    }

    /// 创建加载器并读取配置文件（严格模式——失败时返回错误）
    pub fn load(config_path: Option<PathBuf>) -> Result<Self, String> {
        let config = match config_path.as_ref() {
            Some(path) if path.exists() => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("配置文件 {} 读取失败: {}", path.display(), e))?;
                toml::from_str::<AagnetConfig>(&content)
                    .map_err(|e| format!("配置文件 {} 解析失败: {}", path.display(), e))?
            }
            _ => AagnetConfig::default(),
        };

        Ok(Self {
            current: config,
            config_path,
        })
    }

    /// 获取当前完整配置
    pub fn current(&self) -> &AagnetConfig {
        &self.current
    }

    /// 获取某个插件的配置段（原始 TOML 值）
    ///
    /// 从 config.toml 的 `[plugins.xxx]` 中取出原始 TOML 值。
    pub fn get_section(&self, name: &str) -> Option<&toml::Value> {
        self.current.plugins.get(name)
    }

    /// 获取某个插件的配置段（JSON 格式）
    ///
    /// 将 `[plugins.xxx]` 的 TOML 值转换为 `serde_json::Value`，
    /// 适配 PluginInitContext 的 `plugin_config: Value` 字段。
    pub fn get_section_json(&self, name: &str) -> Option<serde_json::Value> {
        self.current.plugins.get(name).and_then(|v| {
            // toml::Value 实现了 serde::Serialize → 可序列化为 JSON
            serde_json::to_value(v).ok()
        })
    }

    /// 获取全部插件配置（JSON Map 格式）
    ///
    /// 返回 `{ "plugin_name": { ... }, ... }`，适配 ServiceManager::init_all。
    pub fn plugins_as_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for name in self.current.plugins.keys() {
            if let Some(val) = self.get_section_json(name) {
                map.insert(name.clone(), val);
            }
        }
        serde_json::Value::Object(map)
    }

    /// 更新配置并写回文件
    ///
    /// 收到新配置 → 写回 config.toml → 更新内存。
    /// 未来加校验/通知都在这个函数里扩。
    pub fn update_config(&mut self, new_config: AagnetConfig) -> Result<(), String> {
        if let Some(path) = &self.config_path {
            let content = toml::to_string_pretty(&new_config)
                .map_err(|e| format!("配置序列化失败: {}", e))?;
            std::fs::write(path, &content).map_err(|e| format!("配置文件写入失败: {}", e))?;
        }
        self.current = new_config;
        Ok(())
    }
}
