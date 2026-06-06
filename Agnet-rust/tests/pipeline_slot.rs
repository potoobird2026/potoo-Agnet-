use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use aagnet::core::access::SlotAccessPoint;
use aagnet::core::context::StepContext;
use aagnet::core::phase::Phase;
use aagnet::core::pipeline::Pipeline;
use aagnet::core::slot::{SlotDirective, SlotPlugin};
use aagnet::core::types::error::PluginError;
use aagnet::core::types::plugin::{AgentConfig, PluginInitContext};
use aagnet::core::types::Timestamp;
use aagnet::shared_types::{ContentBlock, Message, MessageRole, StepResponse};

#[allow(dead_code)]
fn make_msg(text: &str) -> Message {
    Message {
        role: MessageRole::User,
        content: vec![ContentBlock::Text(text.to_string())],
        tool_calls: None,
        tool_call_id: None,
        reasoning: None,
        metadata: None,
        created_at: Timestamp::now(),
    }
}

/// A mock slot that records lifecycle calls and returns a configurable directive.
struct MockSlot {
    name: String,
    directive: SlotDirective,
    init_called: Arc<Mutex<bool>>,
    run_called: Arc<Mutex<i32>>,
    shutdown_called: Arc<Mutex<bool>>,
    fail_init: bool,
}

#[allow(dead_code)]
impl MockSlot {
    fn new(name: &str, directive: SlotDirective) -> Self {
        Self {
            name: name.to_string(),
            directive,
            init_called: Arc::new(Mutex::new(false)),
            run_called: Arc::new(Mutex::new(0)),
            shutdown_called: Arc::new(Mutex::new(false)),
            fail_init: false,
        }
    }

    fn new_failing(name: &str) -> Self {
        Self {
            name: name.to_string(),
            directive: SlotDirective::Continue,
            init_called: Arc::new(Mutex::new(false)),
            run_called: Arc::new(Mutex::new(0)),
            shutdown_called: Arc::new(Mutex::new(false)),
            fail_init: true,
        }
    }

    fn init_was_called(&self) -> bool {
        *self.init_called.lock().unwrap()
    }

    fn run_count(&self) -> i32 {
        *self.run_called.lock().unwrap()
    }

    fn shutdown_was_called(&self) -> bool {
        *self.shutdown_called.lock().unwrap()
    }
}

#[async_trait]
impl SlotPlugin for MockSlot {
    fn name(&self) -> &str {
        &self.name
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        *self.init_called.lock().unwrap() = true;
        if self.fail_init {
            Err(PluginError::InitFailed("mock init failure".to_string()))
        } else {
            Ok(())
        }
    }

    async fn run(&mut self, _ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        *self.run_called.lock().unwrap() += 1;
        Ok(self.directive.clone())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        *self.shutdown_called.lock().unwrap() = true;
        Ok(())
    }
}

#[allow(dead_code)]
fn make_init_ctx(name: &str) -> PluginInitContext {
    PluginInitContext::new(
        name,
        json!({}),
        AgentConfig::default(),
        PathBuf::from("/tmp"),
    )
}

#[tokio::test]
async fn test_pipeline_empty() {
    let mut pipeline = Pipeline::new().add_phase(Phase::init());
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), StepResponse::Completed { .. }));
}

#[tokio::test]
async fn test_pipeline_single_slot_continue() {
    let slot = Box::new(MockSlot::new("test-slot", SlotDirective::Continue));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), StepResponse::Completed { .. }));
}

#[tokio::test]
async fn test_pipeline_break_phase() {
    let slot1 = Box::new(MockSlot::new("break-slot", SlotDirective::BreakPhase));
    let slot2 = Box::new(MockSlot::new("should-not-run", SlotDirective::Continue));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot1)
        .add_slot(Phase::init(), slot2);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_break_step() {
    let slot = Box::new(MockSlot::new("break-step", SlotDirective::BreakStep));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), StepResponse::Interrupted { .. }));
}

#[tokio::test]
async fn test_pipeline_restart_step() {
    let slot = Box::new(MockSlot::new("restart", SlotDirective::RestartStep));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        StepResponse::RestartRequested { .. }
    ));
}

#[tokio::test]
async fn test_pipeline_abort_step() {
    let slot = Box::new(MockSlot::new("abort-step", SlotDirective::AbortStep));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pipeline_abort_pipeline() {
    let slot = Box::new(MockSlot::new(
        "abort-pipeline",
        SlotDirective::AbortPipeline,
    ));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pipeline_jump_to() {
    let jump_slot = Box::new(MockSlot::new(
        "jumper",
        SlotDirective::JumpTo(Phase::new("target")),
    ));
    let target_slot = Box::new(MockSlot::new("target-slot", SlotDirective::Continue));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_phase(Phase::new("target"))
        .add_slot(Phase::init(), jump_slot)
        .add_slot(Phase::new("target"), target_slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_jump_to_nonexistent() {
    let slot = Box::new(MockSlot::new(
        "bad-jump",
        SlotDirective::JumpTo(Phase::new("nowhere")),
    ));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pipeline_multiple_phases() {
    let slot_a = Box::new(MockSlot::new("phase-a", SlotDirective::Continue));
    let slot_b = Box::new(MockSlot::new("phase-b", SlotDirective::Continue));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_slot(Phase::init(), slot_a)
        .add_phase(Phase::context())
        .add_slot(Phase::context(), slot_b);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_with_recommended_phases() {
    let mut pipeline = Pipeline::with_recommended_phases();
    // Add at least one slot per phase so validation passes at runtime
    let phases = pipeline.phases().to_vec();
    for phase in &phases {
        pipeline = pipeline.add_slot(
            phase.clone(),
            Box::new(MockSlot::new(
                &format!("slot-{phase}"),
                SlotDirective::Continue,
            )),
        );
    }
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_validate_empty() {
    let pipeline = Pipeline::new();
    let result = pipeline.validate();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pipeline_validate_ok() {
    let pipeline = Pipeline::new().add_phase(Phase::init()).add_slot(
        Phase::init(),
        Box::new(MockSlot::new("s", SlotDirective::Continue)),
    );
    let result = pipeline.validate();
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_max_backward_jumps() {
    // Jumper in a later phase jumping back to an earlier phase triggers the backward counter
    let jumper = Box::new(MockSlot::new(
        "jumper",
        SlotDirective::JumpTo(Phase::init()),
    ));
    let mut pipeline = Pipeline::new()
        .add_phase(Phase::init())
        .add_phase(Phase::context())
        .add_slot(
            Phase::init(),
            Box::new(MockSlot::new("normal", SlotDirective::Continue)),
        )
        .add_slot(Phase::context(), jumper)
        .with_max_backward_jumps(2);
    let mut ctx = StepContext::new("s1".to_string(), vec![], 10);
    let result = pipeline.run(&mut ctx).await;
    assert!(result.is_err());
}
