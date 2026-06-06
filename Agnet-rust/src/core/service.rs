use async_trait::async_trait;

use super::access::ServiceAccessPoint;
use super::types::error::PluginError;
use super::types::plugin::PluginInitContext;

/// Service 插件接口
///
/// Service 是独立于 Pipeline 的后台服务，通过 `ServiceAccessPoint` 与核心交互。
/// 在 `start()` 中调用 `register_provider()` 将自己的能力暴露给其他插件。
///
/// 生命周期：init → start → [handle_signal ↔ 运行中] → stop → shutdown
#[async_trait]
pub trait ServicePlugin: Send + Sync {
    /// 服务名称
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    /// 校验配置、建立连接。失败则服务不被加载。
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 启动（传入受控访问句柄）
    /// 在此方法中调用 `register_provider()` 注册业务能力。
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;

    /// 处理运行时信号
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>;

    /// 停止服务（暂停，不销毁）
    async fn stop(&mut self) -> Result<(), PluginError>;

    /// 销毁（只调用一次）
    /// 反注册 Provider、释放所有资源。
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

/// 运行时信号
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceSignal {
    /// 优雅关闭
    GracefulShutdown,
    /// 强制关闭
    ImmediateShutdown,
    /// 重载配置
    ConfigReload,
    /// 健康检查
    HealthCheck,
    /// 暂停运行
    Suspend,
    /// 恢复运行
    Resume,
}
