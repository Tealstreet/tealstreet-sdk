#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::json;

fn reconciliation(name: ExchangeName, expect_probes: bool) {
    let inverse = name == ExchangeName::Bybitinverse;
    let symbol = if inverse { "BTCUSD" } else { "BTCUSDT" };
    let mut options = support::options();
    options.bybit_site_id = Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl);
    let h = support::harness_as(name, options, |wire| {
        wire.ok("GET", "/v5/position/list", json!({"list":[]}));
        wire.ok(
            "POST",
            "/v5/order/create",
            json!({"orderId":"reduce-limit"}),
        );
        wire.ok(
            "GET",
            "/v5/execution/list",
            json!({"list":[],"nextPageCursor":""}),
        );
    });
    let market = serde_json::from_value(json!({
        "id":symbol,"symbol":symbol,"base":"BTC","quote":if inverse {"USD"}else{"USDT"},
        "active":true,"isInverse":inverse,"precision":{"amount":1,"price":0.1},
        "limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}
    }))
    .unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let opts: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":symbol,"type":"limit","side":"sell","amount":1,"price":100,"reduceOnly":true
    }))
    .unwrap();
    let venue = h.venue();
    assert_eq!(
        h.drive(Box::pin(venue.base().place_order(venue, opts)))
            .unwrap(),
        ["reduce-limit"]
    );
    assert_eq!(
        h.store.orders().len(),
        1,
        "Brazil fallback publishes the placed order"
    );
    assert!(h.wire.calls("/v5/execution/list").is_empty());
    h.advance(9_000.);
    let probes = h.wire.calls("/v5/execution/list");
    if expect_probes {
        assert!(
            !probes.is_empty(),
            "primary source still reconciles reduce-only limits"
        );
    } else {
        assert!(
            probes.is_empty(),
            "variant source never schedules primary reconciliation: {probes:?}"
        );
        assert_eq!(h.store.orders().len(), 1);
    }
    h.venue().dispose().unwrap();
}
#[test]
fn spot_limit_recording_does_not_schedule_primary_reconciliation() {
    reconciliation(ExchangeName::Bybitspot, false);
}
#[test]
fn inverse_limit_recording_does_not_schedule_primary_reconciliation() {
    reconciliation(ExchangeName::Bybitinverse, false);
}
#[test]
fn primary_and_demo_keep_source_reconciliation() {
    reconciliation(ExchangeName::Bybit, true);
    reconciliation(ExchangeName::Bybitdemo, true);
}
