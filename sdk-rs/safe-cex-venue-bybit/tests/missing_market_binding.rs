#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::json;

fn missing_market(kind: &str, full_position: bool) {
    let h = support::harness_as(ExchangeName::Bybitspot, support::options(), |_| {});
    if full_position {
        let position = serde_json::from_value(json!({
            "id":"FOOUSDT","symbol":"FOOUSDT","side":"long","contracts":1,"entryPrice":10,
            "notional":10,"unrealizedPnl":0,"leverage":1,"liquidationPrice":0
        }))
        .unwrap();
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                positions: Some(vec![position]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    }
    let opts: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol":"FOOUSDT","type":kind,"side":"sell","amount":1,
        "price":10,"triggerPrice":9
    }))
    .unwrap();
    let venue = h.venue();
    let error = h
        .drive(Box::pin(venue.base().place_order(venue, opts)))
        .unwrap_err();
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, "Market FOOUSDT not found");
    assert!(h.wire.requests.borrow().is_empty());
}

#[test]
fn spot_missing_limit_market_uses_source_error() {
    missing_market("limit", false);
}
#[test]
fn spot_missing_stop_market_uses_source_error() {
    missing_market("stop_market", true);
}
#[test]
fn spot_missing_take_profit_market_uses_source_error() {
    missing_market("take_profit_market", true);
}
