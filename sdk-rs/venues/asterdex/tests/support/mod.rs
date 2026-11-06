//! Trusted fixture setup; credential literals never enter venue options.
#![allow(dead_code)]
use safe_cex_engine::Engine;
use safe_cex_venue_asterdex::{AsterDex, Options as VenueOptions, sign};
mod fixture;
pub use fixture::Options;

/// Trusted setup as hosts run it: signer, then identities, then address setup
/// before start, so V3 slots carry the wallet address their keys prove.
pub fn engine(options: Options) -> Engine<AsterDex> {
    let (venue, signer, identities) = options.build_installed();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(signer)).unwrap();
    engine.set_identities(identities).unwrap();
    engine.initialize_address_nonce(0.0).unwrap();
    engine
}

/// Dispatch-signed requests are released only through preparation.
pub fn drain(engine: &mut Engine<AsterDex>, now: f64) -> Vec<safe_cex_engine::Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
