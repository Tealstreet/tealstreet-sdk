mod support;
use safe_cex_engine::model::StopTriggerType::MarkPrice;
use safe_cex_engine::protective_amend::fresh_reference;
use safe_cex_engine::{PublicFeedPolicy, PublicFeedUpdate};

#[test]
fn shared_unchanged_mark_refreshes_only_from_a_genuine_source_receipt() {
    let (mut e, _, _) = support::ready_with_policy("disabled", PublicFeedPolicy::Shared);
    let update = |mark, observed| {
        serde_json::from_str::<PublicFeedUpdate>(
        &format!(r#"{{"tickers":[{{"symbol":"BTC-PERP","mark":{mark},"observedAt":[null,{observed},null]}}]}}"#)
    ).unwrap()
    };
    e.apply_public_feed(2_000., update(101., 1_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(1_900.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 2_000.),
        Some(101.)
    );
    e.apply_public_feed(8_000., update(101., 7_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(7_900.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 8_000.),
        Some(101.)
    );
    e.apply_public_feed(14_000., update(101., 7_900.)).unwrap();
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(7_900.)
    );
    assert_eq!(
        fresh_reference(e.store(), "BTC-PERP", MarkPrice, 14_000.),
        None
    );
    e.snapshot_frame(20_000.);
    assert_eq!(
        e.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(7_900.)
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
