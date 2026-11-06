//! Helpers shared by the scripted KuCoin integration tests.

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
#[allow(dead_code)]
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

#[allow(dead_code)]
pub mod credentials;
#[allow(dead_code)]
pub mod native_sign;

#[allow(dead_code)]
pub fn actions<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
) -> Vec<safe_cex_engine::Action> {
    let now = safe_cex_engine::test_support::dispatch_now(engine);
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
