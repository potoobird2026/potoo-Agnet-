use std::collections::HashMap;

use super::context::StepContext;
use super::phase::Phase;
use super::slot::{SlotDirective, SlotEntry, SlotPlugin};
use super::types::error::AgentError;
use super::types::Timestamp;
use crate::shared_types::StepResponse;

/// Pipeline: an ordered list of phases, each containing Slots.
/// The core makes no semantic assumptions, just iterates phases and Slots in order.
///
/// **Clone 注意**：Pipeline 当前未实现 Clone。若未来需 Clone，
/// `slots` 中的 `Box<dyn SlotPlugin>` 不会被深拷贝（trait object 不可 Clone），
/// 需要额外设计（如 Arc 包裹或手动重建）。
pub struct Pipeline {
    phases: Vec<Phase>,
    slots: HashMap<Phase, Vec<SlotEntry>>,
    /// 记录注册顺序，保证同阶段 Slot 有序执行
    order: HashMap<Phase, Vec<usize>>,
    next_id: usize,
    /// 单次 Step 中允许的最大后向跳转次数（防止死循环），0 = 不限制
    max_backward_jumps: usize,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            phases: Vec::new(),
            slots: HashMap::new(),
            order: HashMap::new(),
            next_id: 0,
            max_backward_jumps: 10,
        }
    }

    /// 设置最大后向跳转次数（0 = 不限制）
    pub fn with_max_backward_jumps(mut self, n: usize) -> Self {
        self.max_backward_jumps = n;
        self
    }

    /// 创建一个具有推荐阶段序列的 Pipeline。
    /// Init → Context → Think → Audit → Execute → Loop → Memorize
    /// 用户可在返回的 Pipeline 上继续添加/移除阶段。
    pub fn with_recommended_phases() -> Self {
        Self::new()
            .add_phase(Phase::init())
            .add_phase(Phase::context())
            .add_phase(Phase::think())
            .add_phase(Phase::audit())
            .add_phase(Phase::execute())
            .add_phase(Phase::loop_phase())
            .add_phase(Phase::memorize())
    }

    // ---- 阶段编排 ----

    /// 在末尾添加一个阶段
    pub fn add_phase(mut self, phase: Phase) -> Self {
        if !self.phases.contains(&phase) {
            self.phases.push(phase.clone());
            self.slots.entry(phase);
        } else {
            tracing::warn!("阶段 '{:?}' 已存在，忽略重复添加", phase);
        }
        self
    }

    /// 在目标阶段之前插入一个新阶段
    pub fn insert_phase_before(mut self, target: &Phase, phase: Phase) -> Self {
        if let Some(pos) = self.phases.iter().position(|p| p == target) {
            if !self.phases.contains(&phase) {
                self.phases.insert(pos, phase.clone());
            }
        }
        self
    }

    /// 在目标阶段之后插入一个新阶段
    pub fn insert_phase_after(mut self, target: &Phase, phase: Phase) -> Self {
        if let Some(pos) = self.phases.iter().position(|p| p == target) {
            if !self.phases.contains(&phase) {
                self.phases.insert(pos + 1, phase.clone());
            }
        }
        self
    }

    /// 移除一个阶段（及其所有 Slot）
    pub fn remove_phase(mut self, phase: &Phase) -> Self {
        self.phases.retain(|p| p != phase);
        self.slots.remove(phase);
        self.order.remove(phase);
        self
    }

    /// 获取当前阶段列表
    pub fn phases(&self) -> &[Phase] {
        &self.phases
    }

    // ---- Slot 注册 ----

    /// 将 slot 注册到指定阶段
    pub fn add_slot(mut self, phase: Phase, slot: Box<dyn SlotPlugin>) -> Self {
        let id = self.next_id;
        self.next_id += 1;
        let entry = SlotEntry::new(slot, phase.clone());
        self.slots.entry(phase.clone()).or_default().push(entry);
        self.order.entry(phase).or_default().push(id);
        self
    }

    /// 将 slot 注册到指定阶段（与 add_slot 等价）
    pub fn register(self, phase: Phase, slot: Box<dyn SlotPlugin>) -> Self {
        self.add_slot(phase, slot)
    }

    /// 将 slot 注册到指定阶段（可变引用版，不消费 self）
    ///
    /// 用于 AgentRuntime::register_slot() 等需要在持有 Pipeline 引用时注册的场景。
    pub fn add_slot_mut(&mut self, phase: Phase, slot: Box<dyn SlotPlugin>) {
        let id = self.next_id;
        self.next_id += 1;
        let entry = SlotEntry::new(slot, phase.clone());
        self.slots.entry(phase.clone()).or_default().push(entry);
        self.order.entry(phase).or_default().push(id);
    }

    // ---- 执行 ----

    /// 运行 Pipeline
    ///
    /// 遍历所有阶段，每个阶段内按注册顺序执行 Slot
    /// 根据 SlotDirective 决定流程走向
    /// 支持 JumpTo 指令：跳到指定阶段重新执行
    /// 所有执行步骤均记录时间戳和耗时
    pub async fn run(&mut self, ctx: &mut StepContext) -> Result<StepResponse, AgentError> {
        let pipeline_started_at: Timestamp = Timestamp::now();
        tracing::info!(
            timestamp = %pipeline_started_at,
            session_id = %ctx.session_id,
            "[pipeline] Starting pipeline execution"
        );

        let mut phase_idx = 0;
        let mut backward_jump_count: usize = 0;
        let mut jumped = false;

        while phase_idx < self.phases.len() {
            let phase = &self.phases[phase_idx];
            let phase_started_at: Timestamp = Timestamp::now();
            tracing::debug!(
                timestamp = %phase_started_at,
                phase = %phase,
                "[pipeline] Starting phase"
            );

            // 设置当前阶段名称，供 SlotAccessPoint 读取
            ctx.phase_name = phase.to_string();

            let phase_slots = match self.slots.get_mut(phase) {
                Some(slots) => slots,
                None => {
                    phase_idx += 1;
                    continue;
                }
            };

            for entry in phase_slots.iter_mut() {
                let slot_name = entry.name().to_string();
                let slot = &mut entry.plugin;
                let slot_started_at: Timestamp = Timestamp::now();
                tracing::debug!(
                    timestamp = %slot_started_at,
                    slot_name = %slot_name,
                    phase = %phase,
                    "[pipeline] Running slot"
                );

                let directive = slot.run(ctx).await.map_err(|e| {
                    let slot_completed_at: Timestamp = Timestamp::now();
                    let duration_ms = slot_completed_at
                        .duration_since(slot_started_at)
                        .as_millis() as i64;
                    tracing::error!(
                        timestamp = %slot_completed_at,
                        duration_ms = duration_ms,
                        slot_name = %slot_name,
                        phase = %phase,
                        error = %e,
                        "[pipeline] Slot execution failed"
                    );
                    AgentError::plugin_failed(&slot_name, e)
                })?;

                let slot_completed_at: Timestamp = Timestamp::now();
                let duration_ms = slot_completed_at
                    .duration_since(slot_started_at)
                    .as_millis() as i64;
                tracing::debug!(
                    timestamp = %slot_completed_at,
                    duration_ms = duration_ms,
                    slot_name = %slot_name,
                    phase = %phase,
                    directive = ?directive,
                    "[pipeline] Slot completed"
                );

                match directive {
                    SlotDirective::Continue => {}
                    SlotDirective::BreakPhase => {
                        let phase_completed_at: Timestamp = Timestamp::now();
                        let phase_duration_ms = phase_completed_at
                            .duration_since(phase_started_at)
                            .as_millis() as i64;
                        tracing::debug!(
                            timestamp = %phase_completed_at,
                            duration_ms = phase_duration_ms,
                            phase = %phase,
                            "[pipeline] Phase broken early"
                        );
                        break;
                    }
                    SlotDirective::BreakStep => {
                        let pipeline_completed_at: Timestamp = Timestamp::now();
                        let total_duration_ms = pipeline_completed_at
                            .duration_since(pipeline_started_at)
                            .as_millis() as i64;
                        tracing::info!(
                            timestamp = %pipeline_completed_at,
                            duration_ms = total_duration_ms,
                            session_id = %ctx.session_id,
                            "[pipeline] Pipeline broken step"
                        );
                        return Ok(StepResponse::Interrupted {
                            reason: "Slot 请求中断".to_string(),
                            response: String::new(),
                            completed_at: pipeline_completed_at,
                        });
                    }
                    SlotDirective::RestartStep => {
                        let pipeline_completed_at: Timestamp = Timestamp::now();
                        let total_duration_ms = pipeline_completed_at
                            .duration_since(pipeline_started_at)
                            .as_millis() as i64;
                        tracing::info!(
                            timestamp = %pipeline_completed_at,
                            duration_ms = total_duration_ms,
                            session_id = %ctx.session_id,
                            "[pipeline] Pipeline restarting step"
                        );
                        return Ok(StepResponse::RestartRequested {
                            session_id: ctx.session_id.clone(),
                            completed_at: pipeline_completed_at,
                        });
                    }
                    SlotDirective::AbortStep => {
                        let pipeline_completed_at: Timestamp = Timestamp::now();
                        let total_duration_ms = pipeline_completed_at
                            .duration_since(pipeline_started_at)
                            .as_millis() as i64;
                        tracing::error!(
                            timestamp = %pipeline_completed_at,
                            duration_ms = total_duration_ms,
                            session_id = %ctx.session_id,
                            slot_name = %slot_name,
                            phase = %phase,
                            "[pipeline] Pipeline aborting step"
                        );
                        return Err(AgentError::PluginFailed {
                            plugin_name: slot_name,
                            message: "Slot 报告错误".to_string(),
                        });
                    }
                    SlotDirective::AbortPipeline => {
                        let pipeline_completed_at: Timestamp = Timestamp::now();
                        let total_duration_ms = pipeline_completed_at
                            .duration_since(pipeline_started_at)
                            .as_millis() as i64;
                        tracing::error!(
                            timestamp = %pipeline_completed_at,
                            duration_ms = total_duration_ms,
                            session_id = %ctx.session_id,
                            slot_name = %slot_name,
                            phase = %phase,
                            "[pipeline] Pipeline aborted"
                        );
                        return Err(AgentError::PipelineAborted {
                            reason: format!(
                                "Slot '{}' in phase '{}' 请求终止管道",
                                slot_name, phase
                            ),
                        });
                    }
                    SlotDirective::JumpTo(target_phase) => {
                        if let Some(pos) = self.phases.iter().position(|p| p == &target_phase) {
                            let jump_at: Timestamp = Timestamp::now();
                            tracing::debug!(
                                timestamp = %jump_at,
                                from_phase = %phase,
                                to_phase = %target_phase,
                                "[pipeline] JumpTo directive executed"
                            );
                            if pos < phase_idx {
                                backward_jump_count += 1;
                                ctx.current_turn += 1;
                                if self.max_backward_jumps > 0
                                    && backward_jump_count > self.max_backward_jumps
                                {
                                    return Err(AgentError::PipelineAborted {
                                        reason: format!(
                                            "超过最大后向跳转次数（{}），可能存在死循环",
                                            self.max_backward_jumps
                                        ),
                                    });
                                }
                            }
                            phase_idx = pos;
                            jumped = true;
                            break;
                        } else {
                            let error_at: Timestamp = Timestamp::now();
                            tracing::error!(
                                timestamp = %error_at,
                                target_phase = %target_phase,
                                slot_name = %slot_name,
                                phase = %phase,
                                "[pipeline] JumpTo target phase not found, aborting"
                            );
                            return Err(AgentError::PipelineAborted {
                                reason: format!(
                                    "JumpTo target phase '{}' not found in pipeline (slot '{}' in phase '{}')",
                                    target_phase, slot_name, phase
                                ),
                            });
                        }
                    }
                }
            }

            let phase_completed_at: Timestamp = Timestamp::now();
            let phase_duration_ms = phase_completed_at
                .duration_since(phase_started_at)
                .as_millis() as i64;
            tracing::debug!(
                timestamp = %phase_completed_at,
                duration_ms = phase_duration_ms,
                phase = %phase,
                "[pipeline] Phase completed"
            );

            if !jumped {
                phase_idx += 1;
            }
            jumped = false;
        }

        let pipeline_completed_at: Timestamp = Timestamp::now();
        let total_duration_ms = pipeline_completed_at
            .duration_since(pipeline_started_at)
            .as_millis() as i64;
        tracing::info!(
            timestamp = %pipeline_completed_at,
            duration_ms = total_duration_ms,
            session_id = %ctx.session_id,
            "[pipeline] Pipeline execution completed"
        );

        Ok(StepResponse::Completed {
            response: String::new(),
            completed_at: pipeline_completed_at,
        })
    }

    /// 验证 Pipeline 配置完整性
    ///
    /// 检查：阶段非空、每阶段至少注册一个 Slot。
    /// 返回 Ok(()) 或所有错误的列表。
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors: Vec<String> = Vec::new();

        if self.phases.is_empty() {
            errors.push("Pipeline 没有任何阶段".to_string());
        }

        for phase in &self.phases {
            match self.slots.get(phase) {
                None => {
                    errors.push(format!(
                        "阶段 '{}' 没有注册任何 Slot，Pipeline 在此阶段不会产生任何效果",
                        phase
                    ));
                }
                Some(slots) if slots.is_empty() => {
                    errors.push(format!(
                        "阶段 '{}' 没有注册任何 Slot，Pipeline 在此阶段不会产生任何效果",
                        phase
                    ));
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Clone for Pipeline {
    fn clone(&self) -> Self {
        // Pipeline 不支持深度克隆 Slot（SlotPlugin 不再提供 box_clone）
        // 克隆后的 Pipeline 包含相同的阶段顺序但不包含 Slot
        Self {
            phases: self.phases.clone(),
            slots: HashMap::new(),
            order: HashMap::new(),
            next_id: 0,
            max_backward_jumps: self.max_backward_jumps,
        }
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline")
            .field("phases", &self.phases)
            .field(
                "slot_count",
                &self.slots.values().map(|v| v.len()).sum::<usize>(),
            )
            .finish()
    }
}
