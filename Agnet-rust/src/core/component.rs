/*!
 * core/component —— 模块内部组件协议
 *
 * 将 Slot/Service 的扩展范式镜像到模块内部。
 * 模块内的功能单元统一实现 Component trait，
 * 通过 InternalAccessPoint 间接通信，
 * 由 Orchestrator 统一编排执行顺序。
 */

use std::any::Any;

/// 模块内部组件统一接口
///
/// 对比 SlotPlugin/ServicePlugin：
/// - SlotPlugin: Pipeline 中按 Phase 同步执行
/// - ServicePlugin: 后台独立异步运行
/// - Component:   模块内部串行/并行处理单元
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    /// 组件标识
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &ComponentInitContext) -> Result<(), ComponentError>;

    /// 核心处理逻辑（可能被多次调用）
    async fn process(
        &mut self,
        ap: &mut dyn InternalAccessPoint,
    ) -> Result<Processing, ComponentError>;

    /// 资源清理（只调用一次）
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}

/// 组件处理结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Processing {
    /// 正常完成，继续下一个
    Continue,
    /// 跳出当前串行链，跳过剩余组件
    BreakChain,
    /// 重启流程
    Restart,
    /// 仅记录警告日志，不影响流程
    Warn { message: String },
}

/// 组件句柄（用于跨组件调用）
///
/// 调用者拿到句柄后通过 `as_any()` 向下转型到具体类型接口，
/// 获得类型安全的方法调用。
pub trait ComponentHandle: Send + Sync {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// 自动为所有 Component 实现 ComponentHandle
impl<T: Component + 'static> ComponentHandle for T {
    fn name(&self) -> &str {
        self.name()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// 模块内部访问点
///
/// 组件之间、组件与模块基础设施之间的唯一通道。
/// 禁止直接引用兄弟组件的具体类型。
///
/// 注意：所有方法均为非泛型（dyn 兼容），
/// 类型安全由调用方通过 downcast 保证。
pub trait InternalAccessPoint: Send + Sync {
    /// 按 key 读取共享数据（返回类型擦除的引用，调用方自行 downcast）
    fn read_any(&self, key: &str) -> Option<&dyn Any>;

    /// 按 key 写入共享数据（接受类型擦除的 Box，调用方自行装箱）
    fn write_any(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), ComponentError>;

    /// 按名称查找兄弟组件（返回后由调用方 downcast）
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;

    /// 模块级配置
    fn config(&self) -> &ModuleConfig;

    /// 日志
    fn log(&self, level: &str, message: &str);
}

/// 组件初始化上下文
#[derive(Debug, Clone)]
pub struct ComponentInitContext {
    pub component_name: String,
    pub module_config: ModuleConfig,
}

impl ComponentInitContext {
    pub fn new(component_name: impl Into<String>, module_config: ModuleConfig) -> Self {
        Self {
            component_name: component_name.into(),
            module_config,
        }
    }
}

/// 模块配置（占位，具体模块自行扩展）
#[derive(Debug, Clone)]
pub struct ModuleConfig {
    pub name: String,
    pub enabled: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
        }
    }
}

/// 组件元数据（启动时声明）
#[derive(Debug, Clone)]
pub struct ComponentMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub requires: Vec<&'static str>,
    pub provides: Vec<&'static str>,
}

/// 组件错误
#[derive(Debug, Clone)]
pub enum ComponentError {
    NotFound(String),
    InitFailed(String),
    Runtime(String),
    Config(String),
    TypeMismatch { expected: String, actual: String },
    Internal(String),
}

impl std::fmt::Display for ComponentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentError::NotFound(name) => write!(f, "组件未找到: {}", name),
            ComponentError::InitFailed(msg) => write!(f, "初始化失败: {}", msg),
            ComponentError::Runtime(msg) => write!(f, "运行时错误: {}", msg),
            ComponentError::Config(msg) => write!(f, "配置错误: {}", msg),
            ComponentError::TypeMismatch { expected, actual } => {
                write!(f, "类型不匹配: 期望 {}, 实际 {}", expected, actual)
            }
            ComponentError::Internal(msg) => write!(f, "内部错误: {}", msg),
        }
    }
}

impl std::error::Error for ComponentError {}
