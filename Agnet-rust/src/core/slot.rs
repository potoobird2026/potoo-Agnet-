use async_trait::async_trait;

use super::access::SlotAccessPoint;
use super::phase::Phase;
use super::types::error::PluginError;
use super::types::plugin::PluginInitContext;

/// 槽口在管道中返回的执行指令
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotDirective {
    /// 正常继续，进入同阶段下一 Slot 或下一阶段
    Continue,
    /// 跳过当前阶段剩余 Slot，进入下一阶段
    BreakPhase,
    /// 终止本轮 Step，返回当前结果
    BreakStep,
    /// 丢弃本轮所有状态，重新开始 Step（如重试）
    RestartStep,
    /// 终止本轮 Step 并标记错误，但不关闭 Agent
    AbortStep,
    /// 致命错误，终止整个 AgentLoop
    AbortPipeline,
    /// 跳到指定阶段重新执行（用于 ReAct 循环等场景）
    JumpTo(Phase),
}

/// Slot 插件接口
///
/// 所有管道内处理单元通过实现此 trait 接入框架。
/// 与核心的唯一交互通道是 `SlotAccessPoint`。
///
/// 生命周期：init → run (多次) → shutdown
#[async_trait]
pub trait SlotPlugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    /// 校验配置、建立连接、分配资源。失败则插件不被加载。
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 每次 Phase 触发时调用
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;

    /// 清理（只调用一次）
    /// 释放资源、关闭连接。
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

// ============================================
// SlotEntry——Pipeline 存储 Slot 时携带元数据
// ============================================

/// Pipeline 内部使用的 Slot 包装，携带阶段
pub struct SlotEntry {
    pub plugin: Box<dyn SlotPlugin>,
    pub phase: Phase,
}

impl SlotEntry {
    pub fn new(plugin: Box<dyn SlotPlugin>, phase: Phase) -> Self {
        Self { plugin, phase }
    }

    pub fn name(&self) -> &str {
        self.plugin.name()
    }
}
