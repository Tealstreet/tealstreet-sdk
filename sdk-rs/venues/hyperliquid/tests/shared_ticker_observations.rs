mod support;
use safe_cex_engine::model::StopTriggerType::MarkPrice;
use safe_cex_engine::protective_amend::fresh_reference;
use safe_cex_engine::{PublicFeedPolicy, PublicFeedUpdate};

#[test]
fn shared_source_provenance_cannot_replace_local_mark_receipts() {
    let (mut e, _, _) = support::ready_with_policy("disabled", PublicFeedPolicy::Shared);
    let update = |mark, observed| {
        serde_json::from_str::<PublicFeedUpdate>(
        &format!(r#"{{"tickers":[{{"symbol":"BTC-PERP","mark":{mark},"observedAt":[null,{observed},null]}}]}}"#)
    ).unwrap()
    };
    e.apply_public_feed(2_000., update(101., 1_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 2_000.),
        Some(101.)
    );
    assert_eq!(source_mark(&mut e, 2_000.), 1_900.);
    e.apply_public_feed(8_000., update(101., 7_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 8_000.),
        None
    );
    assert_eq!(source_mark(&mut e, 8_000.), 7_900.);
    e.apply_public_feed(14_000., update(101., 7_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 14_000.),
        None
    );
    assert_eq!(source_mark(&mut e, 20_000.), 7_900.);
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(2_000.)
    );
    e.apply_public_feed(
        20_000.,
        serde_json::from_str(r#"{"tickers":[{"symbol":"BTC-PERP","mark":102}]}"#).unwrap(),
    )
    .unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(20_000.),
        "an older producer retains changed-value dating"
    );
    e.apply_public_feed(
        26_000.,
        serde_json::from_str(r#"{"tickers":[{"symbol":"BTC-PERP","mark":102}]}"#).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 26_000.),
        None,
        "missing time never refreshes an unchanged value"
    );
}

fn source_mark(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_hyperliquid::Hyperliquid>,
    now: f64,
) -> f64 {
    let row = e
        .store()
        .tickers()
        .iter()
        .position(|ticker| ticker.symbol == "BTC-PERP")
        .unwrap();
    let frame = e.snapshot_frame(now);
    let (_, bytes) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::TICKER_OBSERVATIONS)
        .unwrap();
    f64::from_le_bytes(bytes[row * 32 + 16..row * 32 + 24].try_into().unwrap())
}
