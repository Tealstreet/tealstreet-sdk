//! Bybit ticker deltas confirm unchanged fields from their own live snapshot.
mod support;

use safe_cex_engine::model::StopTriggerType::{IndexPrice, LastPrice, MarkPrice};
use safe_cex_engine::{Event, protective_amend::fresh_reference};
use support::Harness;

const PUBLIC: &str = "/v5/public/linear";
const SNAPSHOT: &str = r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","markPrice":"100","lastPrice":"100","indexPrice":"100"}}"#;
const DELTA: &str =
    r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","bid1Price":"99"}}"#;

#[test]
fn live_deltas_keep_unchanged_snapshot_references_current() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    for _ in 0..10 {
        h.now += 1_000.;
        h.message(PUBLIC, DELTA);
    }
    for reference in [MarkPrice, LastPrice, IndexPrice] {
        assert_eq!(
            fresh_reference(h.engine.store(), "BTCUSDT", reference, h.now),
            Some(100.),
            "{reference:?}"
        );
    }
}

#[test]
fn silence_still_expires_a_known_snapshot_at_five_seconds() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.now += 5_001.;
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn a_delta_without_a_stream_snapshot_cannot_renew_rest_seeded_mark() {
    let mut h = Harness::new();
    h.now += 5_001.;
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn another_symbol_cannot_renew_the_protected_symbols_mark() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.now += 5_001.;
    h.message(PUBLIC, &DELTA.replace("BTCUSDT", "ETHUSDT"));
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn a_snapshot_missing_mark_cannot_adopt_its_rest_seed() {
    let mut h = Harness::new();
    h.message(PUBLIC, r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","lastPrice":"100"}}"#);
    h.now += 5_001.;
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn reconnect_cannot_reuse_a_retired_sockets_snapshot() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    let old = h.socket(PUBLIC).unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(1_000.);
    let current = h.socket(PUBLIC).unwrap();
    assert_ne!(old, current);
    h.event(Event::WsOpened { socket: current });
    h.flush_subscriptions();
    h.now += 5_001.;
    h.event(Event::WsMessage {
        socket: old,
        text: SNAPSHOT,
    });
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn an_explicitly_unknown_mark_drops_snapshot_provenance() {
    for value in ["null", "\"\"", "\"NaN\"", "\"0\""] {
        let mut h = Harness::new();
        h.message(PUBLIC, SNAPSHOT);
        h.now += 5_001.;
        h.message(PUBLIC, &format!(r#"{{"topic":"tickers.BTCUSDT","type":"delta","data":{{"symbol":"BTCUSDT","markPrice":{value}}}}}"#));
        h.message(PUBLIC, DELTA);
        assert!(
            fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none(),
            "{value}"
        );
    }
}

#[test]
fn an_untyped_frame_does_not_confirm_omitted_fields() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.now += 5_001.;
    h.message(PUBLIC, &DELTA.replace(",\"type\":\"delta\"", ""));
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn a_malformed_frame_retires_the_snapshot_before_another_delta() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.now += 5_001.;
    h.message(
        PUBLIC,
        r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","markPrice":123}}"#,
    );
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn a_replacement_snapshot_cannot_keep_an_omitted_old_mark() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.message(PUBLIC, r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","lastPrice":"100"}}"#);
    h.now += 5_001.;
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn a_restored_store_value_does_not_inherit_the_streams_different_mark() {
    let mut h = Harness::new();
    h.message(PUBLIC, SNAPSHOT);
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |ticker| ticker.mark = 101.);
    h.now += 5_001.;
    h.message(PUBLIC, DELTA);
    assert!(fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now).is_none());
}

#[test]
fn escaped_type_names_keep_the_documented_delta_semantics() {
    let mut h = Harness::new();
    h.message(PUBLIC, &SNAPSHOT.replace("snapshot", r#"snapsh\u006ft"#));
    h.now += 5_001.;
    h.message(PUBLIC, &DELTA.replace("delta", r#"del\u0074a"#));
    assert_eq!(
        fresh_reference(h.engine.store(), "BTCUSDT", MarkPrice, h.now),
        Some(100.)
    );
}
