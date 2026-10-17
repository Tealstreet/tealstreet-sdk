#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::json;

fn poisoned_failure(name: ExchangeName, split: bool, client_id: bool) {
    let h = support::harness_as(name, support::options(), |wire| {
        wire.reject_http_errors.set(true);
        wire.ok("GET", "/v5/position/list", json!({"list":[]}));
        wire.route(
            "POST",
            "/v5/order/create",
            400,
            json!({
                "retCode":10001,"retMsg":"first lot rejected",
                "code":{"toString":null,"valueOf":null}
            }),
        );
        wire.ok("POST", "/v5/order/create", json!({"orderId":"second-lot"}));
        wire.ok("POST", "/v5/order/create", json!({"orderId":"third-lot"}));
    });
    let market = serde_json::from_value(json!({
        "id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":1},"leverage":{"min":1,"max":100}}
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
    let mut opts = json!({"symbol":"BTCUSDT","type":"limit","side":"buy",
        "amount":if split {3.} else {0.5},"price":10});
    if client_id {
        opts["clientOrderId"] = json!("caller-order");
    }
    let opts: PlaceOrderOpts = serde_json::from_value(opts).unwrap();
    let venue = h.venue();
    let result = h.drive(Box::pin(venue.base().place_order(venue, opts)));
    if client_id {
        let error = result.unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert!(
            error
                .message
                .contains("Cannot convert object to primitive value"),
            "{error:?}"
        );
    } else {
        let ids = result.expect("source short circuit must skip unnecessary classifier");
        if split {
            assert_eq!(ids, ["second-lot", "third-lot"]);
        } else {
            assert!(ids.is_empty());
        }
    }
    assert_eq!(
        h.wire.calls("/v5/order/create").len(),
        if split { 3 } else { 1 }
    );
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| line.contains("first lot rejected"))
    );
}
#[test]
fn primary_split_order_failure_does_not_evaluate_unknown_outcome() {
    poisoned_failure(ExchangeName::Bybit, true, false);
}
#[test]
fn demo_split_order_failure_does_not_evaluate_unknown_outcome() {
    poisoned_failure(ExchangeName::Bybitdemo, true, false);
}
#[test]
fn primary_single_order_without_client_id_skips_unknown_outcome() {
    poisoned_failure(ExchangeName::Bybit, false, false);
}
#[test]
fn demo_single_order_without_client_id_skips_unknown_outcome() {
    poisoned_failure(ExchangeName::Bybitdemo, false, false);
}
#[test]
fn caller_client_id_still_evaluates_unknown_outcome() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        poisoned_failure(name, false, true);
    }
}
