use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_delay: Duration,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, initial_delay_ms: u64) -> Self {
        Self {
            max_attempts,
            initial_delay: Duration::from_millis(initial_delay_ms),
        }
    }

    /// 执行带重试的异步操作
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let mut last_error = None;

        for attempt in 1..=self.max_attempts {
            match operation().await {
                Ok(result) => {
                    if attempt > 1 {
                        tracing::info!(attempt = attempt, "Operation succeeded after retry");
                    }
                    return Ok(result);
                }
                Err(err) => {
                    last_error = Some(err);

                    if attempt < self.max_attempts {
                        let delay = self.calculate_delay(attempt);
                        tracing::warn!(
                            attempt = attempt,
                            max_attempts = self.max_attempts,
                            delay_ms = delay.as_millis(),
                            error = %last_error.as_ref().unwrap(),
                            "Operation failed, retrying..."
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        // 所有重试都失败了
        let final_error = last_error.unwrap();
        tracing::error!(
            max_attempts = self.max_attempts,
            error = %final_error,
            "Operation failed after all retry attempts"
        );
        Err(final_error)
    }

    /// 计算指数退避延迟
    fn calculate_delay(&self, attempt: u32) -> Duration {
        // 指数退避: initial_delay * 2^(attempt-1)
        let multiplier = 2u64.pow(attempt - 1);
        let delay = self.initial_delay.as_millis() as u64 * multiplier;

        // 限制最大延迟为 30 秒
        let max_delay = Duration::from_secs(30);
        let calculated_delay = Duration::from_millis(delay);

        if calculated_delay > max_delay {
            max_delay
        } else {
            calculated_delay
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_success_on_first_attempt() {
        let policy = RetryPolicy::new(3, 100);
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = policy
            .execute(|| async {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, anyhow::Error>(42)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_on_second_attempt() {
        let policy = RetryPolicy::new(3, 10);
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = policy
            .execute(|| async {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Err(anyhow::anyhow!("First attempt fails"))
                } else {
                    Ok(42)
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_all_attempts_fail() {
        let policy = RetryPolicy::new(3, 10);
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = policy
            .execute(|| async {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(anyhow::anyhow!("Always fails"))
            })
            .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_calculate_delay() {
        let policy = RetryPolicy::new(5, 1000);

        assert_eq!(policy.calculate_delay(1), Duration::from_millis(1000));
        assert_eq!(policy.calculate_delay(2), Duration::from_millis(2000));
        assert_eq!(policy.calculate_delay(3), Duration::from_millis(4000));
        assert_eq!(policy.calculate_delay(4), Duration::from_millis(8000));
        assert_eq!(policy.calculate_delay(5), Duration::from_millis(16000));
    }

    #[test]
    fn test_max_delay_cap() {
        let policy = RetryPolicy::new(10, 10000);
        let delay = policy.calculate_delay(10); // Would be 5120 seconds without cap
        assert_eq!(delay, Duration::from_secs(30));
    }
}
