//! ConfigHolder — LLM 配置持有者
//!
//! 设计文档 §4.1：原 ConfigProvider（llm_thinker/components/config_provider.rs），
//! 去掉 Component trait 后降级为普通 struct。
//!
//! 职责：
//! - 持有 LlmConfig（通过 RwLock 线程安全读写）
//! - 可选 LlmPairConfig（主/备配置）
//! - 提供快捷方法 provider_kind()、is_stream_enabled()

use std::sync::RwLock;

use crate::shared_types::llm::{LlmConfig, LlmPairConfig, ProviderKind};

/// LLM 配置持有者（设计文档 §4.1）
#[allow(dead_code)]
pub struct ConfigHolder {
    config: RwLock<LlmConfig>,
    pair_config: Option<LlmPairConfig>,
}

#[allow(dead_code)]
impl ConfigHolder {
    /// 创建新的配置持有者
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config: RwLock::new(config),
            pair_config: None,
        }
    }

    /// 获取当前配置（克隆）
    pub fn get(&self) -> LlmConfig {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 更新配置
    pub fn update(&self, config: LlmConfig) {
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = config;
    }

    /// 获取提供商类型
    pub fn provider_kind(&self) -> ProviderKind {
        self.get().provider
    }

    /// 是否启用流式
    pub fn is_stream_enabled(&self) -> bool {
        self.config.read().unwrap_or_else(|e| e.into_inner()).stream
    }
}

impl Default for ConfigHolder {
    fn default() -> Self {
        Self::new(LlmConfig::default())
    }
}
