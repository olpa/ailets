pub const POLL_INTERVAL_MS: u64 = 10;

/// Poll until `predicate` holds, checking every `POLL_INTERVAL_MS` up to `timeout_ms`.
#[allow(clippy::disallowed_methods)]
pub async fn poll_until<F>(mut predicate: F, timeout_ms: u64, msg: &str)
where
    F: FnMut() -> bool,
{
    let max_iters = timeout_ms / POLL_INTERVAL_MS;
    for _ in 0..max_iters {
        if predicate() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
    panic!("timed out after {timeout_ms}ms: {msg}");
}
