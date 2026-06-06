/*! McpService —— MCP 连接服务（ServicePlugin 实现） */
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::{DynProvider, PROVIDER_MCP_TOOLS};

use super::bundle::McpBundleImpl;
use super::config::McpConfig;
use super::connector::StdioMcpConnector;
use super::proxy::McpToolProxy;
use crate::shared_types::McpBundle;

/// B-6: 错误分类
#[derive(Debug, Clone)]
pub enum McpServerError {
    ParseError(String),
    Timeout(String),
    Io(String),
    Other(String),
}

impl McpServerError {
    pub fn parse_from_str(s: &str) -> Self {
        if s.contains("超时") || s.contains("timeout") {
            McpServerError::Timeout(s.to_string())
        } else if s.contains("反序列化") || s.contains("解析") {
            McpServerError::ParseError(s.to_string())
        } else if s.contains("启动") || s.contains("读取") || s.contains("写入") {
            McpServerError::Io(s.to_string())
        } else {
            McpServerError::Other(s.to_string())
        }
    }

    pub fn message(&self) -> &str {
        match self {
            McpServerError::ParseError(m)
            | McpServerError::Timeout(m)
            | McpServerError::Io(m)
            | McpServerError::Other(m) => m,
        }
    }
}

pub struct McpService {
    config: Option<McpConfig>,
    proxies: Arc<RwLock<Vec<Arc<McpToolProxy>>>>,
    connectors: Arc<RwLock<Vec<Arc<Mutex<StdioMcpConnector>>>>>,
    /// B-6: per-server 错误记录
    errors: Arc<RwLock<Vec<(String, McpServerError)>>>,
    running: bool,
}

impl McpService {
    pub fn new() -> Self {
        Self {
            config: None,
            proxies: Arc::new(RwLock::new(Vec::new())),
            connectors: Arc::new(RwLock::new(Vec::new())),
            errors: Arc::new(RwLock::new(Vec::new())),
            running: false,
        }
    }

    /// B-6: 获取错误记录（健康检查/调试用）
    pub async fn errors(&self) -> Vec<(String, McpServerError)> {
        self.errors.read().await.clone()
    }

    /// B-8: 元数据——暴露给 health check / 监控 / 调试（inherent method，非 trait 必需）
    pub fn metadata(&self) -> McpServiceMetadata {
        let enabled_count = self
            .config
            .as_ref()
            .map(|c| c.servers.iter().filter(|s| s.enabled).count())
            .unwrap_or(0);
        McpServiceMetadata {
            name: self.name().to_string(),
            enabled_servers: enabled_count,
            running: self.running,
        }
    }
}

/// B-8: McpService 元数据
#[derive(Debug, Clone)]
pub struct McpServiceMetadata {
    pub name: String,
    pub enabled_servers: usize,
    pub running: bool,
}

#[async_trait]
impl ServicePlugin for McpService {
    fn name(&self) -> &str {
        "mcp"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: McpConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("mcp 配置解析: {}", e)))?;
        self.config = Some(config);
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        self.running = true;
        let config = self.config.clone().unwrap_or_default();

        // 连接所有已启用的 MCP Server
        let mut proxies = self.proxies.write().await;
        let mut connectors = self.connectors.write().await;

        for server_config in &config.servers {
            if !server_config.enabled {
                continue;
            }
            // B-6: 预检——可执行文件存在
            if let Err(e) = tokio::fs::metadata(&server_config.command).await {
                tracing::warn!(
                    "MCP: 跳过 Server '{}'，命令 '{}' 不可访问: {}",
                    server_config.name,
                    server_config.command,
                    e
                );
                self.errors.write().await.push((
                    server_config.name.clone(),
                    McpServerError::Io(format!("命令不可访问: {}", e)),
                ));
                continue;
            }
            let conn_config = server_config.to_connection_config(&config);
            let mut connector = StdioMcpConnector::new(server_config.clone(), conn_config);
            match connector.connect().await {
                Ok(tools) => {
                    let conn = Arc::new(Mutex::new(connector));
                    for tool_manifest in tools {
                        let proxy = Arc::new(McpToolProxy::new(
                            tool_manifest,
                            conn.clone(),
                            &server_config.name,
                        ));
                        proxies.push(proxy);
                    }
                    connectors.push(conn);
                }
                Err(e) => {
                    let classified = McpServerError::parse_from_str(&e);
                    tracing::warn!(
                        "MCP: 连接 Server '{}' 失败: {}",
                        server_config.name,
                        classified.message()
                    );
                    self.errors
                        .write()
                        .await
                        .push((server_config.name.clone(), classified));
                }
            }
        }

        let proxy_count = proxies.len();
        drop(proxies); // 释放锁

        // C-1: 注册 McpBundle（让 ToolsService 可拉取 MCP 工具列表）
        let bundle_struct = McpBundleImpl::new(self.proxies.clone());
        let bundle_trait: Arc<dyn McpBundle> = Arc::new(bundle_struct);
        ap.register_provider(
            PROVIDER_MCP_TOOLS,
            Arc::new(DynProvider(bundle_trait)) as Arc<dyn std::any::Any + Send + Sync>,
        );
        tracing::info!("McpService: 已启动，加载了 {} 个工具代理", proxy_count);
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                // B-7: 健康检查——报告错误数 + 代理数
                let err_count = self.errors.read().await.len();
                let proxy_count = self.proxies.read().await.len();
                if err_count > 0 {
                    tracing::warn!("MCP 健康: {} 个代理，{} 个错误", proxy_count, err_count);
                } else {
                    tracing::debug!("MCP 健康: {} 个代理", proxy_count);
                }
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                // B-7: 配置重载——断开旧连接，重新连接所有 enabled server
                let config = self.config.clone();
                let proxies = self.proxies.clone();
                let connectors = self.connectors.clone();
                let errors = self.errors.clone();
                tokio::spawn(async move {
                    let config = match config {
                        Some(c) => c,
                        None => return,
                    };
                    // 断开旧连接
                    {
                        let conns = connectors.read().await;
                        for conn in conns.iter() {
                            conn.lock().await.disconnect().await;
                        }
                    }
                    // 清空旧状态
                    proxies.write().await.clear();
                    connectors.write().await.clear();
                    errors.write().await.clear();
                    // 重新连接
                    let mut new_proxies = Vec::new();
                    let mut new_connectors = Vec::new();
                    for server_config in &config.servers {
                        if !server_config.enabled {
                            continue;
                        }
                        if let Err(e) = tokio::fs::metadata(&server_config.command).await {
                            tracing::warn!(
                                "MCP Reload: 跳过 '{}'，命令不可访问: {}",
                                server_config.name,
                                e
                            );
                            errors.write().await.push((
                                server_config.name.clone(),
                                McpServerError::Io(format!("命令不可访问: {}", e)),
                            ));
                            continue;
                        }
                        let conn_config = server_config.to_connection_config(&config);
                        let mut connector =
                            StdioMcpConnector::new(server_config.clone(), conn_config);
                        match connector.connect().await {
                            Ok(tools) => {
                                let conn = Arc::new(Mutex::new(connector));
                                for tool_manifest in tools {
                                    let proxy = Arc::new(McpToolProxy::new(
                                        tool_manifest,
                                        conn.clone(),
                                        &server_config.name,
                                    ));
                                    new_proxies.push(proxy);
                                }
                                new_connectors.push(conn);
                            }
                            Err(e) => {
                                let classified = McpServerError::parse_from_str(&e);
                                tracing::warn!(
                                    "MCP Reload: 连接 '{}' 失败: {}",
                                    server_config.name,
                                    classified.message()
                                );
                                errors
                                    .write()
                                    .await
                                    .push((server_config.name.clone(), classified));
                            }
                        }
                    }
                    *proxies.write().await = new_proxies;
                    *connectors.write().await = new_connectors;
                    let count = proxies.read().await.len();
                    tracing::info!("MCP: 配置重载完成，重新加载了 {} 个工具代理", count);
                });
                Ok(())
            }
            ServiceSignal::GracefulShutdown | ServiceSignal::ImmediateShutdown => {
                for conn in self.connectors.read().await.iter() {
                    conn.lock().await.disconnect().await;
                }
                self.running = false;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        self.running = false;
        Ok(())
    }
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        for conn in self.connectors.read().await.iter() {
            conn.lock().await.disconnect().await;
        }
        self.connectors.write().await.clear();
        self.proxies.write().await.clear();
        self.errors.write().await.clear();
        Ok(())
    }
}

impl Default for McpService {
    fn default() -> Self {
        Self::new()
    }
}
