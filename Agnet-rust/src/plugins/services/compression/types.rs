/*! Compression 公共类型 */
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 服务状态机（只有两态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Sleep,
    Compressing,
}

/// 钩子事件（从 Slot 发给 Service）
#[derive(Debug, Clone)]
pub enum HookEvent {
    NewMessagesArrived {
        session_id: String,
    },
    RoundComplete {
        session_id: String,
        round_id: usize,
        interval_ms: u64,
    },
}

/// 对话阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationPhase {
    Idle,
    Busy,
    ToolHeavy,
}

/// PID 阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidPhase {
    ColdStart,
    Normal,
    PreBurst,
    PostCompress,
}

/// UCB 分类（9×3×3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CategoryRole {
    System,
    User,
    Assistant,
    ToolCall,
    ToolResult,
    Summary,
    Code,
    Data,
    Other,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Text,
    Code,
    Data,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthBucket {
    Short,
    Medium,
    Long,
}

/// 压缩结果
#[derive(Debug, Clone)]
pub struct CompressResult {
    pub session_id: String,
    pub compressed_count: usize,
    pub summary: String,
    pub token_saved: usize,
    pub elapsed: Duration,
}

/// 损失信号
#[derive(Debug, Clone)]
pub struct LossSignal {
    pub session_id: String,
    pub missing_info: Vec<String>,
    pub severity: f64,
}

/// 压缩日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: i64,
    pub session_id: String,
    pub compressed_count: usize,
    pub token_saved: usize,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_state_variants() {
        assert_ne!(ServiceState::Sleep, ServiceState::Compressing);
    }

    #[test]
    fn test_hook_event_new_messages() {
        let e = HookEvent::NewMessagesArrived {
            session_id: "s1".into(),
        };
        match e {
            HookEvent::NewMessagesArrived { session_id } => assert_eq!(session_id, "s1"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_hook_event_round_complete() {
        let e = HookEvent::RoundComplete {
            session_id: "s2".into(),
            round_id: 3,
            interval_ms: 1500,
        };
        match e {
            HookEvent::RoundComplete {
                session_id,
                round_id,
                interval_ms,
            } => {
                assert_eq!(session_id, "s2");
                assert_eq!(round_id, 3);
                assert_eq!(interval_ms, 1500);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_conversation_phase_variants() {
        assert_ne!(ConversationPhase::Idle, ConversationPhase::Busy);
        assert_ne!(ConversationPhase::Busy, ConversationPhase::ToolHeavy);
    }

    #[test]
    fn test_pid_phase_variants() {
        assert_ne!(PidPhase::ColdStart, PidPhase::Normal);
        assert_ne!(PidPhase::PreBurst, PidPhase::PostCompress);
    }

    #[test]
    fn test_compress_result() {
        let r = CompressResult {
            session_id: "s".into(),
            compressed_count: 5,
            summary: "ok".into(),
            token_saved: 100,
            elapsed: Duration::from_millis(50),
        };
        assert_eq!(r.compressed_count, 5);
        assert_eq!(r.token_saved, 100);
    }

    #[test]
    fn test_journal_entry_serialize() {
        let e = JournalEntry {
            timestamp: 123,
            session_id: "s".into(),
            compressed_count: 2,
            token_saved: 50,
            success: true,
        };
        let json = serde_json::to_string(&e).expect("JournalEntry 序列化应成功");
        assert!(json.contains("\"session_id\":\"s\""));
    }
}
