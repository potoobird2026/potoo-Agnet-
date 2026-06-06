/*! McpBundleImpl —— McpBundle trait 实现 */
use std::sync::Arc;
use tokio::sync::RwLock;

use super::proxy::McpToolProxy;
use crate::shared_types::{McpBundle, ToolProvider};

/// McpBundleImpl：持有所有 McpToolProxy 的可变集合
pub struct McpBundleImpl {
    proxies: Arc<RwLock<Vec<Arc<McpToolProxy>>>>,
}

impl McpBundleImpl {
    pub fn new(proxies: Arc<RwLock<Vec<Arc<McpToolProxy>>>>) -> Self {
        Self { proxies }
    }
}

impl McpBundle for McpBundleImpl {
    fn all(&self) -> Vec<Arc<dyn ToolProvider>> {
        // 非阻塞读——避免在 async runtime 内 block；若锁被持有返回空列表
        match self.proxies.try_read() {
            Ok(guard) => guard
                .iter()
                .map(|p| p.clone() as Arc<dyn ToolProvider>)
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}
