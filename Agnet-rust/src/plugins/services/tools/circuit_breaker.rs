/*! CircuitBreaker —— 熔断器 */
use std::time::{Duration, Instant};
const _DEFAULT_MAX_FAILURES: u32 = 5;
const _DEFAULT_COOLDOWN_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    max_failures: u32,
    cooldown: Duration,
    failures: u32,
    last_failure: Option<Instant>,
    state: CircuitBreakerState,
    opened_at: Option<Instant>,
}
impl CircuitBreaker {
    pub fn new(max_failures: u32, cooldown_secs: u64) -> Self {
        Self {
            max_failures,
            cooldown: Duration::from_secs(cooldown_secs),
            failures: 0,
            last_failure: None,
            state: CircuitBreakerState::Closed,
            opened_at: None,
        }
    }
    pub fn is_open(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => false,
            CircuitBreakerState::Open => {
                if let Some(opened) = self.opened_at {
                    if opened.elapsed() >= self.cooldown {
                        self.state = CircuitBreakerState::HalfOpen;
                        false
                    } else {
                        true
                    }
                } else {
                    self.state = CircuitBreakerState::Closed;
                    false
                }
            }
            CircuitBreakerState::HalfOpen => false,
        }
    }
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.state = CircuitBreakerState::Closed;
        self.opened_at = None;
    }
    pub fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
        if self.failures >= self.max_failures {
            self.state = CircuitBreakerState::Open;
            self.opened_at = Some(Instant::now());
        }
    }
    pub fn state(&self) -> CircuitBreakerState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_closed() {
        let mut cb = CircuitBreaker::new(3, 60);
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_record_failure_below_threshold() {
        let mut cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_record_failure_reaches_threshold_opens() {
        let mut cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);
        assert!(cb.is_open());
    }

    #[test]
    fn test_record_success_resets() {
        let mut cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_cooldown_transitions_to_half_open() {
        let mut cb = CircuitBreaker::new(2, 0); // 0 秒冷却，立即过期
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitBreakerState::Open);
        // cooldown = 0，is_open() 应转为 HalfOpen 并返回 false
        assert!(!cb.is_open());
        assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);
    }

    #[test]
    fn test_half_open_allows_check() {
        let mut cb = CircuitBreaker::new(2, 0);
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_open()); // → HalfOpen
        assert!(!cb.is_open()); // HalfOpen → still false
    }

    #[test]
    fn test_success_after_half_open_resets() {
        let mut cb = CircuitBreaker::new(2, 0);
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_open()); // → HalfOpen
        cb.record_success();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }
}
