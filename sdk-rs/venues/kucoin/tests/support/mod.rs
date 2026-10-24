//! Helpers shared by the scripted KuCoin integration tests.
#![allow(dead_code)]
pub mod credentials;
pub mod native_sign;

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
