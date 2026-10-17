#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};

fn fixture(name: ExchangeName) -> support::Harness {
    let h = support::harness_as(name, support::options(), |wire| {
        wire.ok("GET", "/v5/position/list", json!({"list":[]}));
        wire.ok(
            "POST",
            "/v5/order/create",
            json!({"orderId":"adjusted-order"}),
        );
    });
    let market = serde_json::from_value(json!({
        "id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.5},
        "limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}}
    }))
    .unwrap();
    let ticker = serde_json::from_value(json!({
        "id":"BTCUSDT","symbol":"BTCUSDT","bid":10,"ask":10,"last":10,"mark":10,"index":10,
        "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0
    }))
    .unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h
}

fn place(
    h: &support::Harness,
    fields: Value,
) -> Result<Vec<String>, safe_cex_exchange::runtime::ExchangeError> {
    let mut opts = json!({"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":0.01});
    opts.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let opts: PlaceOrderOpts = serde_json::from_value(opts).unwrap();
    let venue = h.venue();
    h.drive(Box::pin(venue.base().place_order(venue, opts)))
}

fn rounded_trigger(name: ExchangeName) {
    let h = fixture(name);
    let error = place(&h, json!({"price":0.2,"triggerPrice":0.2})).unwrap_err();
    assert_eq!(error.name, "Error");
    assert_eq!(
        error.message,
        "Trigger price is required for conditional Bybit orders (BTCUSDT)"
    );
    assert!(h.wire.calls("/v5/order/create").is_empty());
}
fn rounded_price(name: ExchangeName) {
    let h = fixture(name);
    assert_eq!(
        place(&h, json!({"price":0.2,"triggerPrice":20})).unwrap(),
        ["adjusted-order"]
    );
    let calls = h.wire.calls("/v5/order/create");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].body["orderType"], "Market");
    assert!(calls[0].body.get("price").is_none());
    assert_eq!(calls[0].body["triggerPrice"], "20");
    assert_eq!(calls[0].body["triggerDirection"], "1");
    assert_eq!(calls[0].body["reduceOnly"], false);
    assert!(calls[0].body.get("timeInForce").is_none());
}
#[test]
fn primary_conditional_rounded_trigger_is_required() {
    rounded_trigger(ExchangeName::Bybit);
}
#[test]
fn demo_conditional_rounded_trigger_is_required() {
    rounded_trigger(ExchangeName::Bybitdemo);
}
#[test]
fn primary_conditional_rounded_price_uses_market() {
    rounded_price(ExchangeName::Bybit);
}
#[test]
fn demo_conditional_rounded_price_uses_market() {
    rounded_price(ExchangeName::Bybitdemo);
}

#[test]
fn adjusted_zero_payloads_preserve_source_raw_option_gates() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        for kind in ["limit", "stop_market", "take_profit_market"] {
            let h = fixture(name.clone());
            place(
                &h,
                json!({"type":kind,"price":0.2,"triggerPrice":0.2,
                "stopLoss":0.2,"takeProfit":0.2}),
            )
            .unwrap();
            let calls = h.wire.calls("/v5/order/create");
            assert_eq!(calls.len(), 1);
            let body = &calls[0].body;
            assert_eq!(
                body["orderType"],
                match kind {
                    "limit" => "Limit",
                    "stop_market" => "PartialStopLoss",
                    _ => "PartialTakeProfit",
                }
            );
            assert_eq!(body["triggerPrice"], "0");
            assert_eq!(body["stopLoss"], "0");
            assert_eq!(body["takeProfit"], "0");
            assert!(body.get("triggerDirection").is_none());
            if kind == "limit" {
                assert_eq!(body["price"], "0");
            } else {
                assert!(body.get("price").is_none());
            }
        }
        let h = fixture(name.clone());
        place(
            &h,
            json!({"type":"conditional","price":20,"triggerPrice":25}),
        )
        .unwrap();
        let body = &h.wire.calls("/v5/order/create")[0].body;
        assert_eq!(body["orderType"], "Limit");
        assert_eq!(body["price"], "20");
        assert_eq!(body["timeInForce"], "GTC");
    }
}

#[test]
fn nan_adjusted_trigger_omits_direction_and_preserves_source_error_priority() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        for kind in ["limit", "conditional", "stop_market", "take_profit_market"] {
            let h = fixture(name.clone());
            let mut markets = h.store.markets().to_vec();
            markets[0].precision.price = 0.;
            Store::update(
                h.store.as_ref(),
                StoreUpdate {
                    markets: Some(markets),
                    tickers: Some(vec![]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let result = place(&h, json!({"type":kind,"price":20,"triggerPrice":25}));
            if kind == "conditional" {
                let error = result.unwrap_err();
                assert_eq!(error.name, "Error");
                assert_eq!(
                    error.message,
                    "Trigger price is required for conditional Bybit orders (BTCUSDT)"
                );
                assert!(h.wire.calls("/v5/order/create").is_empty());
            } else {
                assert_eq!(result.unwrap(), ["adjusted-order"]);
                let body = &h.wire.calls("/v5/order/create")[0].body;
                assert_eq!(body["triggerPrice"], "NaN");
                assert!(body.get("triggerDirection").is_none());
                assert_eq!(
                    body["orderType"],
                    match kind {
                        "limit" => "Limit",
                        "stop_market" => "PartialStopLoss",
                        _ => "PartialTakeProfit",
                    }
                );
            }
        }
    }
}
