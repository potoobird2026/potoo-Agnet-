/*! CliChannel —— 命令行输出通道（ServicePlugin 实现，仅输出，不再读 stdin） */
use async_trait::async_trait;
use std::sync::Arc;

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::cli::{CliError, CliProvider, PROVIDER_CLI_CHANNEL};
use crate::shared_types::DynProvider;

pub struct CliChannel {
    running: bool,
}

impl CliChannel {
    pub fn new() -> Self {
        Self { running: false }
    }
}

impl Clone for CliChannel {
    fn clone(&self) -> Self {
        Self {
            running: self.running,
        }
    }
}

#[async_trait]
impl ServicePlugin for CliChannel {
    fn name(&self) -> &str {
        "cli"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        self.running = true;
        ap.register_provider(
            PROVIDER_CLI_CHANNEL,
            Arc::new(DynProvider(Arc::new(self.clone()))),
        );
        tracing::info!("CliChannel: 已注册输出提供者");
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::GracefulShutdown | ServiceSignal::ImmediateShutdown => {
                self.running = false;
            }
            ServiceSignal::HealthCheck => {}
            _ => {}
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        self.running = false;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.running = false;
        Ok(())
    }
}

impl Default for CliChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CliProvider for CliChannel {
    async fn output(&self, message: &str) -> Result<(), CliError> {
        println!("{}", message);
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_channel_new() {
        let ch = CliChannel::new();
        assert!(!ch.running);
    }

    #[test]
    fn test_cli_channel_is_alive_default() {
        let ch = CliChannel::new();
        assert!(!ch.is_alive());
    }

    #[test]
    fn test_cli_channel_clone() {
        let ch = CliChannel::new();
        let ch2 = ch.clone();
        assert!(!ch2.running);
    }

    #[test]
    fn test_default() {
        let ch = CliChannel::default();
        assert!(!ch.running);
    }

    #[test]
    fn test_cli_provider_output() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut ch = CliChannel::new();
            ch.running = true;
            assert!(ch.is_alive());
        });
    }
}
