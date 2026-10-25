//! Trusted fixture setup; credential literals never enter venue options.
#![allow(dead_code)]
use safe_cex_engine::Engine;
use safe_cex_venue_asterdex::{AsterDex, Options as VenueOptions, sign};
mod fixture;
pub use fixture::Options;

pub fn engine(options: Options) -> Engine<AsterDex> {
    let (venue, signer) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(signer)).unwrap();
    engine
}

/// Dispatch-signed requests are released only through preparation.
pub fn drain(engine: &mut Engine<AsterDex>, now: f64) -> Vec<safe_cex_engine::Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
