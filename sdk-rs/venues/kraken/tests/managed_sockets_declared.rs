//! A dropped managed-socket declaration fails here directly: the engine would
//! otherwise refuse every feed registration for this venue.
use safe_cex_engine::Venue;

#[test]
fn venue_declares_managed_sockets() {
    assert!(<safe_cex_venue_kraken::Kraken as Venue>::managed_sockets());
    assert!(<safe_cex_venue_kraken::KrakenSpot as Venue>::managed_sockets());
}
