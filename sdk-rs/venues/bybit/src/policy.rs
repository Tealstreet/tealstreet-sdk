//! Bybit's shared command authority policies, used by candles, options,
//! account, identity and the no-I/O answers alike.
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

/// A public read answered once, inside its native window.
pub(crate) fn read_policy() -> Policy {
    entry_policy(Operation::Read)
}

/// Command authority for an answer or refusal with no I/O. A write command's
/// refusal keeps its NotSent outcome even when admission itself is refused.
pub(crate) fn entry_policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
