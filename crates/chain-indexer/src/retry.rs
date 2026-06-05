use std::future::Future;
use std::time::Duration;

use crate::error::SourceError;
use crate::event::ChainEvent;
use crate::source::BlockSource;
use crate::tracker::BlockRef;

/// How an operation backs off and gives up across transient failures.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Extra attempts after the first. `0` disables retrying.
    pub max_retries: u32,
    /// Delay before the first retry; doubles each subsequent attempt.
    pub base_delay: Duration,
    /// Ceiling the exponential delay is clamped to.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    /// Backoff before the retry following attempt `attempt` (0-based):
    /// `base_delay * 2^attempt`, clamped to `max_delay`. Saturating throughout,
    /// so a large `attempt` can never overflow the multiplication.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let factor = 2u32.saturating_pow(attempt);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

/// Runs `op`, retrying while it fails with a retryable [`SourceError`] and the
/// policy's attempt budget is not spent. Sleeps `policy.delay_for(attempt)`
/// between tries. A non-retryable error returns immediately; the last error is
/// returned once retries are exhausted (never swallowed).
pub async fn with_retry<T, F, Fut>(policy: &RetryPolicy, mut op: F) -> Result<T, SourceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, SourceError>>,
{
    let mut attempt = 0;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) if err.is_retryable() && attempt < policy.max_retries => {
                tokio::time::sleep(policy.delay_for(attempt)).await;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Wraps any [`BlockSource`], retrying each call under a [`RetryPolicy`]. Keeps
/// the underlying source (e.g. `RpcBlockSource`) free of resilience concerns;
/// the poller stays generic and simply receives a `RetryingSource` on top.
pub struct RetryingSource<S> {
    inner: S,
    policy: RetryPolicy,
}

impl<S: BlockSource> RetryingSource<S> {
    pub fn new(inner: S, policy: RetryPolicy) -> Self {
        Self { inner, policy }
    }
}

impl<S: BlockSource> BlockSource for RetryingSource<S> {
    async fn head_number(&self) -> Result<u64, SourceError> {
        with_retry(&self.policy, || self.inner.head_number()).await
    }

    async fn block_ref(&self, number: u64) -> Result<BlockRef, SourceError> {
        with_retry(&self.policy, || self.inner.block_ref(number)).await
    }

    async fn transfers(&self, from: u64, to: u64) -> Result<Vec<ChainEvent>, SourceError> {
        with_retry(&self.policy, || self.inner.transfers(from, to)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::Error as DecodeError;
    use alloy::transports::TransportErrorKind;
    use std::cell::Cell;

    /// A retryable transport error, for tests.
    fn rpc_err(msg: &str) -> SourceError {
        SourceError::Rpc(TransportErrorKind::custom_str(msg))
    }

    /// A non-retryable decode error, for tests.
    fn decode_err(msg: &str) -> SourceError {
        SourceError::Decode(DecodeError::custom(msg.to_owned()))
    }

    fn instant_policy(max_retries: u32) -> RetryPolicy {
        RetryPolicy {
            max_retries,
            base_delay: Duration::ZERO, // sleeps are no-ops, so tests are fast
            max_delay: Duration::ZERO,
        }
    }

    #[test]
    fn only_rpc_errors_are_retryable() {
        assert!(rpc_err("timeout").is_retryable());
        assert!(!SourceError::InvalidUrl("bad".into()).is_retryable());
        assert!(!decode_err("garbage").is_retryable());
        assert!(!SourceError::IncompleteLog("tx_hash").is_retryable());
    }

    #[test]
    fn delay_grows_exponentially_then_clamps() {
        let policy = RetryPolicy {
            max_retries: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500),
        };
        assert_eq!(policy.delay_for(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for(2), Duration::from_millis(400));
        // 800ms would exceed the 500ms ceiling, so it clamps.
        assert_eq!(policy.delay_for(3), Duration::from_millis(500));
        // A huge attempt must not overflow the multiplication.
        assert_eq!(policy.delay_for(64), Duration::from_millis(500));
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let calls = Cell::new(0);
        let result = with_retry(&instant_policy(5), || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move {
                if n < 3 {
                    Err(rpc_err("flaky"))
                } else {
                    Ok(42u64)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.get(), 3, "two failures, then success on the third");
    }

    #[tokio::test]
    async fn non_retryable_error_does_not_retry() {
        let calls = Cell::new(0);
        let result: Result<(), _> = with_retry(&instant_policy(5), || {
            calls.set(calls.get() + 1);
            async { Err(decode_err("bad")) }
        })
        .await;

        assert!(matches!(result, Err(SourceError::Decode(_))));
        assert_eq!(calls.get(), 1, "deterministic error is not retried");
    }

    #[tokio::test]
    async fn gives_up_after_max_retries() {
        let calls = Cell::new(0);
        let result: Result<(), _> = with_retry(&instant_policy(2), || {
            calls.set(calls.get() + 1);
            async { Err(rpc_err("always down")) }
        })
        .await;

        assert!(matches!(result, Err(SourceError::Rpc(_))));
        assert_eq!(calls.get(), 3, "initial try plus max_retries (2)");
    }
}
