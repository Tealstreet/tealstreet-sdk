pub use safe_cex_venue_weex::{Options as NativeOptions, WeexFutures};
pub mod managed;
pub mod setup;

/// Native fixtures release managed work at their explicit event clock.
#[allow(dead_code)]
pub trait PreparedActions {
    fn prepared_actions(&mut self, now: f64) -> std::vec::IntoIter<safe_cex_engine::Action>;
}
impl<V: safe_cex_engine::Venue> PreparedActions for safe_cex_engine::Engine<V> {
    fn prepared_actions(&mut self, now: f64) -> std::vec::IntoIter<safe_cex_engine::Action> {
        safe_cex_engine::test_support::prepared_actions(self, now).into_iter()
    }
}
