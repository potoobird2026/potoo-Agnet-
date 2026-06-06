/*!
 * core/access —— 接入协议
 *
 * SlotAccessPoint     —— Slot 插件与核心交互的受控通道
 * ServiceAccessPoint  —— Service 插件与核心交互的受控通道
 * ProviderRegistry    —— 运行时 Provider 注册表
 *
 * 这些是插件与核心的"唯一接触面"。
 * 插件不能直接访问 StepContext、AgentRuntime 等内部结构。
 *
 * 红线：Core 不定义任何业务 Provider 接口（如 MemoryProvider、ToolProvider），
 * 这些由注册方自行定义和使用方通过 downcast 获取。
 */

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::types::error::PluginError;
use super::types::plugin::AgentConfig;
use crate::shared_types::Message;

// 注意：write_observation 接受类型擦除的 Box<dyn Any + Send>
// 具体的 Observation 类型由 llm_thinker Slot 自行定义并装箱传入

// ============================================
// ProviderRegistry —— 运行时 Provider 注册表
// ============================================

/// 运行时 Provider 注册表——Service 在此注册能力，Slot 在此查找能力。
///
/// Core 不定义任何 Provider 接口，只提供按名称查找和类型向下转型的机制。
/// 所有 Provider 以 `Arc<T>` 形式存储，线程安全。
///
/// 使用 `std::sync::RwLock`（非 tokio）以保证 `get()` 是同步方法，
/// 可在 `SlotAccessPoint::provider()` 中直接调用。锁竞争极低（仅 HashMap 操作）。
#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Arc<RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Service 调用：注册一个类型安全的 Provider
    pub fn register<T: Send + Sync + 'static>(&self, name: &str, provider: Arc<T>) {
        if let Ok(mut map) = self.providers.write() {
            map.insert(name.to_string(), provider);
        }
    }

    /// 内部使用：注册一个已擦除类型的 Provider（ServiceAccessImpl 使用）
    pub fn register_raw(&self, name: &str, provider: Arc<dyn Any + Send + Sync>) {
        if let Ok(mut map) = self.providers.write() {
            map.insert(name.to_string(), provider);
        }
    }

    /// Slot/Service 调用：按名称和类型查找 Provider（T 必须为 'static 以便 downcast）
    pub fn get<T: Send + Sync + 'static>(&self, name: &str) -> Option<Arc<T>> {
        if let Ok(map) = self.providers.read() {
            map.get(name).and_then(|p| p.clone().downcast::<T>().ok())
        } else {
            None
        }
    }

    /// 按名称查找原始 Provider（返回类型擦除的 Arc）
    pub fn get_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        if let Ok(map) = self.providers.read() {
            map.get(name).cloned()
        } else {
            None
        }
    }

    /// 反注册 Provider（Service shutdown 时调用）
    pub fn unregister(&self, name: &str) {
        if let Ok(mut map) = self.providers.write() {
            map.remove(name);
        }
    }

    /// 检查 Provider 是否存在
    pub fn has(&self, name: &str) -> bool {
        if let Ok(map) = self.providers.read() {
            map.contains_key(name)
        } else {
            false
        }
    }

    /// 获取当前所有 Provider 名称列表
    pub fn list(&self) -> Vec<String> {
        if let Ok(map) = self.providers.read() {
            map.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// SlotAccessPoint
// ============================================

/// Slot 接入点——Slot 插件与核心交互的唯一通道
///
/// 提供 core 内建方法（消息读取、观察写入、流程控制等）
/// 和 Provider 扩展机制。
pub trait SlotAccessPoint: Send + Sync {
    // ── Core 内建 ──

    /// 读取当前会话对话历史（只读）
    fn messages(&self) -> &[Message];

    /// 当前 Session ID
    fn session_id(&self) -> &str;

    /// 当前 Phase 名称
    fn phase_name(&self) -> &str;

    /// 当前迭代次数
    fn current_iteration(&self) -> usize;

    /// 写入观察结果（类型擦除，由 Slot 自行装箱具体类型）
    fn write_observation(&mut self, obs: Box<dyn Any + Send + Sync>) -> Result<(), PluginError>;

    /// 写入上下文数据（类型擦除）
    fn write_context_raw(
        &mut self,
        key: &str,
        val: Box<dyn Any + Send + Sync>,
    ) -> Result<(), PluginError>;

    /// 读取上下文数据（类型擦除）
    fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)>;

    /// 请求跳转到指定 Phase
    fn request_jump(&self, phase: &str) -> Result<(), PluginError>;

    /// 请求中止当前 Pipeline
    fn request_abort(&self) -> Result<(), PluginError>;

    // ── Provider 扩展：获取其他 Service 注册的能力 ──

    /// 按名称查找业务 Provider（返回类型擦除的 Arc，调用方自行 downcast）
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>>;
}

// ============================================
// ServiceAccessPoint
// ============================================

/// 服务接入点——Service 插件与核心交互的唯一通道
///
/// 可通过 Clone 在多个异步任务间共享。
#[derive(Clone)]
pub struct ServiceAccessPoint {
    inner: Arc<dyn ServiceAccessImpl>,
}

/// Service 访问实现（运行时提供）
pub trait ServiceAccessImpl: Send + Sync {
    /// 获取 Agent 配置
    fn get_config(&self) -> AgentConfig;

    /// 发送日志
    fn log(&self, level: &str, message: &str);

    /// 注册 Provider
    fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>);

    /// 按名称查找 Provider（返回类型擦除的 Arc）
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>>;

    /// 反注册 Provider
    fn unregister_provider(&self, name: &str);
}

impl ServiceAccessPoint {
    pub fn new(inner: Arc<dyn ServiceAccessImpl>) -> Self {
        Self { inner }
    }

    /// 读取 Agent 配置
    pub fn get_config(&self) -> AgentConfig {
        self.inner.get_config()
    }

    /// 写入日志
    pub fn log(&self, level: &str, message: &str) {
        self.inner.log(level, message);
    }

    /// 按名称查找 Provider（返回类型擦除的 Arc）
    pub fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.inner.provider_raw(name)
    }

    /// 注册 Provider——将本服务的业务能力暴露给其他插件
    ///
    /// 接受 Arc<dyn Any + Send + Sync>（调用方自行装箱），
    /// 避免泛型对 Sized 的限制。
    pub fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>) {
        self.inner.register_provider(name, provider);
    }

    /// 反注册 Provider
    pub fn unregister_provider(&self, name: &str) {
        self.inner.unregister_provider(name);
    }
}
