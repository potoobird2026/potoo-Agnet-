//! ServiceManager —— 统一管理所有 ServicePlugin 的生命周期
//!
//! 职责：
//! - 按注册顺序持有所有 ServicePlugin
//! - 提供 init_all() / start_all() / broadcast() / shutdown_all()
//! - 确保 shutdown 按启动的逆序执行
//!
//! 设计依据：AI开发红线与纪律.md §8

use std::path::PathBuf;

use super::access::ServiceAccessPoint;
use super::service::{ServicePlugin, ServiceSignal};
use super::types::plugin::{AgentConfig, PluginInitContext};

/// 服务注册条目
struct ServiceEntry {
    name: String,
    data_dir: PathBuf,
    service: Box<dyn ServicePlugin>,
}

/// 服务生命周期管理器
pub struct ServiceManager {
    entries: Vec<ServiceEntry>,
    start_order: Vec<usize>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            start_order: Vec::new(),
        }
    }

    /// 注册一个服务（按调用顺序排列）
    ///
    /// - name: 服务名称（用于 config 查找和日志）
    /// - data_dir: 服务数据目录
    /// - service: 已构造的服务实例
    pub fn register(
        &mut self,
        name: impl Into<String>,
        data_dir: PathBuf,
        service: Box<dyn ServicePlugin>,
    ) {
        self.entries.push(ServiceEntry {
            name: name.into(),
            data_dir,
            service,
        });
    }

    /// 依次调用每个服务的 init()
    pub async fn init_all(
        &mut self,
        plugins_config: &serde_json::Value,
        agent_config: &AgentConfig,
    ) -> Result<(), String> {
        for entry in &mut self.entries {
            let config = plugins_config
                .get(&entry.name)
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let ctx = PluginInitContext::new(
                &entry.name,
                config,
                agent_config.clone(),
                entry.data_dir.clone(),
            );
            entry
                .service
                .init(&ctx)
                .await
                .map_err(|e| format!("{}.init() 失败: {e}", entry.name))?;
        }
        Ok(())
    }

    /// 依次调用每个服务的 start()
    pub async fn start_all(&mut self, ap: ServiceAccessPoint) -> Result<(), String> {
        self.start_order.clear();
        for (i, entry) in self.entries.iter_mut().enumerate() {
            entry
                .service
                .start(ap.clone())
                .await
                .map_err(|e| format!("{}.start() 失败: {e}", entry.name))?;
            self.start_order.push(i);
            tracing::info!("ServiceManager: {} 已启动", entry.name);
        }
        Ok(())
    }

    /// 广播信号到所有已启动的服务
    pub async fn broadcast(&mut self, signal: ServiceSignal) {
        for &idx in &self.start_order {
            let entry = &mut self.entries[idx];
            if let Err(e) = entry.service.handle_signal(signal).await {
                tracing::warn!(
                    "ServiceManager: {}.handle_signal({:?}) 失败: {}",
                    entry.name,
                    signal,
                    e
                );
            }
        }
    }

    /// 按启动逆序调用 shutdown()
    pub async fn shutdown_all(&mut self) {
        for &idx in self.start_order.iter().rev() {
            let entry = &mut self.entries[idx];
            if let Err(e) = entry.service.shutdown().await {
                tracing::warn!("ServiceManager: {}.shutdown() 失败: {}", entry.name, e);
            } else {
                tracing::info!("ServiceManager: {} 已关闭", entry.name);
            }
        }
        self.start_order.clear();
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}
