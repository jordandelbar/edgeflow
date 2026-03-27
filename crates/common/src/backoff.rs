use std::future::Future;
use std::time::Duration;

use rand::Rng as _;

const MAX_BACKOFF: Duration = Duration::from_secs(30);

fn compute_backoff(attempt: u64) -> Duration {
    Duration::from_secs(1u64.checked_shl(attempt as u32).unwrap_or(u64::MAX)).min(MAX_BACKOFF)
}

fn jitter(base: Duration) -> Duration {
    let max_ms = base.as_millis() as u64;
    Duration::from_millis(rand::rng().random_range(0..=max_ms))
}

/// Retries `f` indefinitely with exponential backoff and full jitter until it succeeds.
pub async fn retry_forever<F, Fut, T, E>(description: &str, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0u32;
    loop {
        match f().await {
            Ok(val) => return val,
            Err(e) => {
                attempt += 1;
                let sleep = jitter(compute_backoff(attempt as u64));
                tracing::warn!(
                    attempt,
                    backoff_ms = sleep.as_millis(),
                    error = %e,
                    "{description} failed, retrying",
                );
                tokio::time::sleep(sleep).await;
            }
        }
    }
}
