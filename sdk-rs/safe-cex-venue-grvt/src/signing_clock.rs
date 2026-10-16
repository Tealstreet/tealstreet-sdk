//! Exported GRVT signing timestamps with an injected Date.now source.

/// Read the millisecond nonce exactly once, without integer narrowing.
pub fn generate_nonce(now_ms: impl FnOnce() -> f64) -> f64 {
    now_ms()
}

/// Retain the source multiplication order and JavaScript Number spelling.
pub fn generate_expiration(hours_from_now: Option<f64>, now_ms: impl FnOnce() -> f64) -> String {
    let now = now_ms();
    let expiration_ms = now + hours_from_now.unwrap_or(1.0) * 60.0 * 60.0 * 1000.0;
    safe_cex_num::js_number_string(expiration_ms * 1_000_000.0)
}
