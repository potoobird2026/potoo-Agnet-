/*! MCP 配置 */
use serde::Deserialize;

/// B-2: per-server 连接配置（per-server timeout 可覆写 McpConfig 默认值）
#[derive(Debug, Clone, Deserialize)]
pub struct McpConnectionConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

impl Default for McpConnectionConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
    /// 全局 connect timeout——可被 per-server 覆写
    pub connect_timeout_secs: u64,
    /// 全局 request timeout——可被 per-server 覆写
    pub request_timeout_secs: u64,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub enabled: bool,
    /// B-2: per-server timeout 覆写（None = 用 McpConfig 全局值）
    #[serde(default)]
    pub connect_timeout_secs: Option<u64>,
    /// B-2: per-server request timeout 覆写
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
}

impl McpServerConfig {
    /// B-2: 构造 McpConnectionConfig——per-server 优先，无则用全局
    pub fn to_connection_config(&self, global: &McpConfig) -> McpConnectionConfig {
        McpConnectionConfig {
            connect_timeout_secs: self
                .connect_timeout_secs
                .unwrap_or(global.connect_timeout_secs),
            request_timeout_secs: self
                .request_timeout_secs
                .unwrap_or(global.request_timeout_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_default() {
        let c = McpConfig::default();
        assert!(c.servers.is_empty());
        assert_eq!(c.connect_timeout_secs, 10);
        assert_eq!(c.request_timeout_secs, 30);
    }

    #[test]
    fn test_connection_config_default() {
        let c = McpConnectionConfig::default();
        assert_eq!(c.connect_timeout_secs, 10);
        assert_eq!(c.request_timeout_secs, 30);
    }

    #[test]
    fn test_to_connection_config_uses_global() {
        let global = McpConfig {
            servers: vec![],
            connect_timeout_secs: 20,
            request_timeout_secs: 60,
        };
        let server = McpServerConfig {
            name: "test".into(),
            command: "cmd".into(),
            args: vec![],
            enabled: true,
            connect_timeout_secs: None,
            request_timeout_secs: None,
        };
        let cc = server.to_connection_config(&global);
        assert_eq!(cc.connect_timeout_secs, 20);
        assert_eq!(cc.request_timeout_secs, 60);
    }

    #[test]
    fn test_to_connection_config_per_server_overrides() {
        let global = McpConfig {
            servers: vec![],
            connect_timeout_secs: 20,
            request_timeout_secs: 60,
        };
        let server = McpServerConfig {
            name: "test".into(),
            command: "cmd".into(),
            args: vec![],
            enabled: true,
            connect_timeout_secs: Some(5),
            request_timeout_secs: Some(10),
        };
        let cc = server.to_connection_config(&global);
        assert_eq!(cc.connect_timeout_secs, 5);
        assert_eq!(cc.request_timeout_secs, 10);
    }

    #[test]
    fn test_deserialize_server_config() {
        let json = serde_json::json!({
            "name": "my_server",
            "command": "npx",
            "args": ["-y", "mcp-server"],
            "enabled": true
        });
        let s: McpServerConfig = serde_json::from_value(json).unwrap();
        assert_eq!(s.name, "my_server");
        assert!(s.connect_timeout_secs.is_none());
    }
}
