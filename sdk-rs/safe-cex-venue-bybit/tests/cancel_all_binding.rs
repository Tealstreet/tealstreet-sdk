//! Cancel-all must reach Bybit even when the local order cache is empty.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order};
use serde_json::json;

fn order(id: &str, symbol: &str) -> Order {
    serde_json::from_value(json!({
        "id": id, "symbol": symbol, "status": "open", "type": "limit", "side": "buy",
        "price": 25000, "amount": 1, "filled": 0, "remaining": 1, "reduceOnly": false,
        "timeInForce": "GoodTillCancel", "lastUpdate": 1
    }))
    .unwrap()
}

fn check_cancel_symbol(name: ExchangeName, symbol: &str, category: &str) {
    let h = support::harness_as(name, support::options(), |wire| {
        wire.ok("POST", "/v5/order/cancel-all", json!({}));
    });
    h.drive(h.venue().cancel_symbol_orders(symbol.into()))
        .unwrap();
    let calls = h.wire.calls("/v5/order/cancel-all");
    assert_eq!(
        calls.len(),
        1,
        "empty store still requires venue-side cancellation"
    );
    assert_eq!(
        calls[0].body,
        json!({"category": category, "symbol": symbol})
    );
    assert!(h.wire.calls("/v5/order/cancel-batch").is_empty());
}

#[test]
fn primary_cancel_symbol_with_empty_store() {
    check_cancel_symbol(ExchangeName::Bybit, "BTCUSDT", "linear");
}
#[test]
fn demo_cancel_symbol_with_empty_store() {
    check_cancel_symbol(ExchangeName::Bybitdemo, "BTCUSDT", "linear");
}
#[test]
fn spot_cancel_symbol_with_empty_store() {
    check_cancel_symbol(ExchangeName::Bybitspot, "BTCUSDT", "spot");
}
#[test]
fn inverse_cancel_symbol_with_empty_store() {
    check_cancel_symbol(ExchangeName::Bybitinverse, "BTCUSD", "inverse");
}

#[test]
fn cancel_all_dispatches_once_per_symbol_and_retires_snapshot_rows() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let mut opts = support::options();
        opts.bybit_site_id = Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl);
        let h = support::harness_as(name.clone(), opts, |wire| {
            wire.ok("POST", "/v5/order/cancel-all", json!({}));
        });
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                orders: Some(vec![
                    order("first", "BTCUSDT"),
                    order("second", "BTCUSDT"),
                    order("third", "ETHUSDT"),
                ]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        h.drive(h.venue().cancel_all_orders()).unwrap();
        let calls = h.wire.calls("/v5/order/cancel-all");
        assert_eq!(
            calls.len(),
            2,
            "{name:?}: one cancel-all per distinct symbol"
        );
        assert_eq!(calls[0].body["symbol"], "BTCUSDT");
        assert_eq!(calls[1].body["symbol"], "ETHUSDT");
        assert!(
            h.store.orders().unwrap().is_empty(),
            "{name:?}: partial id/symbol retirement rows must be accepted"
        );
    }
}

#[test]
fn cancel_symbol_reports_venue_rejection_without_throwing_or_retiring() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitspot,
        ExchangeName::Bybitinverse,
    ] {
        let mut opts = support::options();
        opts.bybit_site_id = Some(safe_cex_types::ExchangeOptionsBybitSiteId::BraBtl);
        let h = support::harness_as(name.clone(), opts, |wire| {
            // A resolved response with a non-OK message reaches the source member's
            // reporting branch without the HTTP interceptor rejecting retCode.
            wire.route(
                "POST",
                "/v5/order/cancel-all",
                200,
                json!({"retCode": 0, "retMsg": "cancel refused", "result": {}}),
            );
        });
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                orders: Some(vec![order("keep", "BTCUSDT")]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = errors.clone();
        let _cleanup = h.venue().base().on_args(
            "error",
            std::rc::Rc::new(move |_, message| {
                let message: Vec<serde_json::Value> = message
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
                captured.borrow_mut().push(json!(message));
                Ok(())
            }),
        );
        h.drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
            .unwrap();
        assert_eq!(h.wire.calls("/v5/order/cancel-all").len(), 1);
        assert_eq!(h.store.orders().unwrap()[0].id, "keep");
        assert_eq!(
            *errors.borrow(),
            vec![json!(["Exchange Error: cancel refused", {"suppressConsoleLog": false}])]
        );
    }
}
