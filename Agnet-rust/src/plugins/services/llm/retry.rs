//! RetryManager — 带退避的重试管理器
//!
//! 设计文档 §4.6：原 RetryManager（llm_thinker/components/retry_manager.rs），
//! 去掉 Component trait 后降级为普通 struct。
//!
//! 策略：
//! - Fixed(delay) — 固定间隔重试
//! - Exponential{initial, max} — 指数退避：delay(n) = min(initial * 2^n, max)
//! - 只重试 LlmError::is_retryable() = true 的错误

use std::future::Future;
use std::time::Duration;

use crate::shared_types::llm::{LlmConfig, LlmError, RetryBackoff};

/// 无状态重试管理器（设计文档 §4.6）
pub struct RetryManager;

impl RetryManager {
    /// 创建新的重试管理器
    pub fn new() -> Self {
        Self
    }

    /// 添加 ±25% 随机 jitter 避免 thundering herd
    fn add_jitter(delay: Duration, attempt: u32) -> Duration {
        let nanos = delay.as_nanos();
        if nanos == 0 {
            return delay;
        }
        // 基于 attempt 和 nanos 的简单伪随机：0..=50 百分比
        let seed = (nanos.wrapping_mul(1103515245).wrapping_add(12345)
            ^ (attempt as u128).wrapping_mul(6364136223846793005)) as u64;
        let pct = (seed % 51) as f64; // 0..50
        let jitter_factor = 0.75 + (pct / 100.0); // 0.75..1.25
        Duration::from_nanos((nanos as f64 * jitter_factor) as u64)
    }

    /// 带退避的重试调用（设计文档 §3.5）
    ///
    /// 调用 `call_fn` 最多 `config.max_retries + 1` 次，
    /// 根据 `config.retry_backoff` 在重试间退避。
    /// 只有可重试错误（LlmError::is_retryable()）触发重试。
    pub async fn call_with_retry<F, Fut, T>(
        &self,
        config: &LlmConfig,
        call_fn: F,
    ) -> Result<T, LlmError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, LlmError>> + Send,
        T: Send,
    {
        let max_retries = config.max_retries;
        let backoff = &config.retry_backoff;
        let mut last_error: Option<LlmError> = None;
        let mut attempt_log: Vec<String> = Vec::new();

        for attempt in 0..=max_retries {
            match call_fn().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    let retryable = err.is_retryable();
                    attempt_log.push(format!("attempt={}: {}", attempt, err.suggestion()));
                    last_error = Some(err);

                    if !retryable || attempt >= max_retries {
                        if attempt >= max_retries && max_retries > 0 && last_error.is_some() {
                            tracing::error!(
                                "retry exhausted after {} attempts: {}",
                                attempt + 1,
                                attempt_log.join("; ")
                            );
                        }
                        return Err(last_error.unwrap_or_else(|| {
                            LlmError::ConfigError("retry: no attempts made (max_retries=0?)".into())
                        }));
                    }

                    let delay = match backoff {
                        // 设计文档 §3.5: Fixed 固定间隔退避 + jitter
                        RetryBackoff::Fixed(d) => Self::add_jitter(*d, attempt),
                        // 设计文档 §3.5: Exponential: delay(n) = min(initial * 2^n, max) + jitter
                        RetryBackoff::Exponential { initial, max } => {
                            let multiplier = 1u64 << attempt;
                            let secs = initial.as_secs_f64() * multiplier as f64;
                            let base = Duration::from_secs_f64(secs.min(max.as_secs_f64()));
                            Self::add_jitter(base, attempt)
                        }
                    };

                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::StreamError {
            trace_id: String::new(),
            message: "retry loop exited unexpectedly".into(),
        }))
    }
}

impl Default for RetryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use super::*;

    fn retrier() -> RetryManager {
        RetryManager::new()
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let retrier = retrier();
        let config = LlmConfig {
            max_retries: 3,
            retry_backoff: RetryBackoff::Fixed(Duration::from_millis(10)),
            ..LlmConfig::default()
        };
        let result: Result<i32, LlmError> = retrier
            .call_with_retry(&config, || async { Ok::<_, LlmError>(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_success_after_retries() {
        let retrier = retrier();
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();
        let config = LlmConfig {
            max_retries: 3,
            retry_backoff: RetryBackoff::Fixed(Duration::from_millis(5)),
            ..LlmConfig::default()
        };
        let result = retrier
            .call_with_retry(&config, || {
                let a = attempt_clone.clone();
                async move {
                    let prev = a.fetch_add(1, Ordering::SeqCst);
                    if prev < 2 {
                        Err(LlmError::Timeout {
                            trace_id: "test".into(),
                            timeout: Duration::from_secs(1),
                        })
                    } else {
                        Ok(99)
                    }
                }
            })
            .await;
        assert_eq!(result.unwrap(), 99);
        assert_eq!(attempt.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error_no_retry() {
        let retrier = retrier();
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();
        let config = LlmConfig {
            max_retries: 3,
            retry_backoff: RetryBackoff::Fixed(Duration::from_millis(5)),
            ..LlmConfig::default()
        };
        let result: Result<(), LlmError> = retrier
            .call_with_retry(&config, || {
                let a = attempt_clone.clone();
                async move {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err(LlmError::ConfigError("bad input".into()))
                }
            })
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ConfigError(_)));
        assert_eq!(attempt.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_exponential_backoff_delay_increases() {
        let retrier = retrier();
        let backoff = RetryBackoff::Exponential {
            initial: Duration::from_millis(10),
            max: Duration::from_secs(5),
        };
        let config = LlmConfig {
            max_retries: 2,
            retry_backoff: backoff,
            ..LlmConfig::default()
        };
        let attempt = Arc::new(AtomicU32::new(0));
        let attempt_clone = attempt.clone();
        let result: Result<(), LlmError> = retrier
            .call_with_retry(&config, || {
                let a = attempt_clone.clone();
                async move {
                    a.fetch_add(1, Ordering::SeqCst);
                    Err(LlmError::Timeout {
                        trace_id: "test".into(),
                        timeout: Duration::from_secs(1),
                    })
                }
            })
            .await;
        assert!(result.is_err());
    }
}
