mod support;

use support::Harness;

const SNAPSHOT: &str = r#"{"lastUpdateId":100,"bids":[["99.0","5"]],"asks":[["101.0","5"]]}"#;

#[test]
fn first_diff_after_snapshot_is_checked_by_id_range_not_pu() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    assert_eq!(h.snapshot_requests(), 1);
    h.respond("/fapi/v1/depth", 200, SNAPSHOT);
    assert_eq!(h.best_bid(), Some(99.0));

    // Its `pu` predates the snapshot; only U <= 101 <= u matters.
    h.diff(98, 105, 97, "99.5");
    h.diff(106, 110, 105, "99.7");
    assert_eq!(h.best_bid(), Some(99.7));
    assert_eq!(h.snapshot_requests(), 0, "no resync while continuous");
}

#[test]
fn a_gap_resyncs_once_from_the_next_diff() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    h.respond("/fapi/v1/depth", 200, SNAPSHOT);
    h.diff(98, 105, 97, "99.5");
    h.diff(120, 125, 119, "99.6");
    assert_eq!(h.best_bid(), None, "book dropped on a gap");
    h.diff(126, 130, 125, "99.6");
    assert_eq!(h.snapshot_requests(), 1);
}

#[test]
fn rate_limits_pause_snapshots_instead_of_retrying() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    h.respond("/fapi/v1/depth", 418, r#"{"code":-1003}"#);
    for update in 0..50 {
        h.diff(200 + update, 200 + update, 199 + update, "98.0");
    }
    assert_eq!(h.snapshot_requests(), 0, "no requests during the ban");
}

#[test]
fn a_snapshot_frame_carries_the_whole_state_without_draining_changes() {
    let mut h = Harness::new();
    h.diff(90, 95, 89, "98.0");
    h.respond("/fapi/v1/depth", 200, SNAPSHOT);
    h.diff(101, 102, 100, "98.5");
    let kinds = |frame: &[u8]| -> Vec<u16> {
        safe_cex_engine::wire::sections(frame)
            .map(|(kind, _)| kind)
            .collect()
    };
    let snapshot = h.engine.snapshot_frame(h.now).to_vec();
    use safe_cex_engine::wire::section::*;
    assert_eq!(
        kinds(&snapshot),
        [
            MARKETS,
            TICKERS,
            ORDERS,
            POSITIONS,
            BALANCE,
            SETTINGS,
            STATUS,
            VENUE_STATE,
            CAPABILITIES,
            BOOK,
            STRINGS
        ]
    );
    let frame = h
        .engine
        .take_frame(h.now)
        .expect("pending changes survive the snapshot")
        .to_vec();
    assert!(kinds(&frame).contains(&BOOK));
}
