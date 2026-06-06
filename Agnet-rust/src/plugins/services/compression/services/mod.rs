/*! Compression 业务接口 traits（12 个服务） */
use super::types::*;
use crate::shared_types::Message;
use async_trait::async_trait;

#[async_trait]
pub trait PidService: Send + Sync {
    fn update(&mut self, error: f64, dt: f64) -> f64;
    fn phase(&self) -> PidPhase;
    fn reset(&mut self);
}
#[async_trait]
pub trait TokenCounterService: Send + Sync {
    fn count(&self, messages: &[Message]) -> usize;
}
#[async_trait]
pub trait AnchorService: Send + Sync {
    fn calculate(&self, total: usize, phase: PidPhase) -> (usize, usize);
}
#[async_trait]
pub trait EntityExtractorService: Send + Sync {
    fn extract(&self, text: &str) -> Vec<String>;
}
#[async_trait]
pub trait EntropyService: Send + Sync {
    fn calculate(&self, messages: &[Message]) -> f64;
}
#[async_trait]
pub trait ScorerService: Send + Sync {
    fn score(
        &self,
        msg: &Message,
        entropy: f64,
        entities: &[String],
        pos: usize,
        total: usize,
    ) -> f64;
}
#[async_trait]
pub trait UcbDecisionService: Send + Sync {
    fn decide(
        &mut self,
        category: CategoryRole,
        content: ContentType,
        length: LengthBucket,
        score: f64,
    ) -> bool;
}
#[async_trait]
pub trait FuzzyControlService: Send + Sync {
    fn decide(&self, keep_ratio: f64) -> FuzzyDecision;
}
#[async_trait]
pub trait CompressorService: Send + Sync {
    async fn compress(
        &self,
        session_id: &str,
        messages: &[Message],
        keep_indices: &[usize],
    ) -> Result<CompressResult, String>;
}
#[async_trait]
pub trait FeedbackService: Send + Sync {
    fn detect_loss(&self, before: &[Message], after: &[Message]) -> Vec<LossSignal>;
}
#[async_trait]
pub trait RecallService: Send + Sync {
    fn recall(&self, signals: &[LossSignal]) -> RecallAction;
}
#[async_trait]
pub trait JournalService: Send + Sync {
    fn record(&mut self, entry: JournalEntry);
    fn entries(&self) -> &[JournalEntry];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuzzyDecision {
    Keep,
    Compress,
    Borderline,
}
#[derive(Debug, Clone)]
pub enum RecallAction {
    None,
    Restore { message_ids: Vec<usize> },
    RequestFullHistory,
}
