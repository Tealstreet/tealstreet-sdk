#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_store::Store;
use safe_cex_types::*;
use serde_json::{Value, json};
use support::{Harness, harness, options};
fn raw_order() -> Value {
    json!({"id":"order-1","symbol":"BTC-USDC","type":"LIMIT","side":"BUY","status":"OPEN","timeInForce":"GOOD_TIL_CANCEL","price":"100.12","size":"1.234","cumSuccessFillSize":"0","reduceOnly":false,"triggerPrice":"0","openSlParams":{"triggerPrice":"0"},"openTpParams":{"triggerPrice":"0"}})
}
fn contract() -> Value {
    json!({"symbol":"BTC-USDC","crossSymbolName":"BTCUSDC","baseTokenId":"BTC","settleAssetId":"USDC","l2PairId":"11","enableTrade":true,"stepSize":"0.001","tickSize":"0.01","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"50","initialMarginRate":"0.1"})
}
fn fixture() -> Harness {
    fixture_with_tick_interval(None)
}
fn fixture_with_tick_interval(tick_interval: Option<f64>) -> Harness {
    let mut opts = options();
    opts.extra = tick_interval
        .map(|interval| serde_json::from_value(json!({"tickInterval": interval})).unwrap());
    opts.omni_key_seed = Some(format!("0x{}1b", "01".repeat(64)));
    opts.passphrase = Some("fixture-passphrase".into());
    let h = harness(ExchangeName::Apexomni, opts, |http| {
        http.ok(
            "GET",
            "/v3/symbols",
            json!({"contractConfig":{"perpetualContract":[contract()]}}),
        );
        http.ok("GET", "/v3/account", json!({"id":"16909060","l2key":"fixture-l2","positions":[],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
        http.ok(
            "GET",
            "/v3/account-balance",
            json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"}),
        );
        http.ok("GET", "/v3/user", json!({"id":"16909060"}));
        http.ok("GET", "/v3/open-orders", json!([raw_order()]));
        http.ok("POST", "/v3/order", json!({"id":"9007199254740993"}));
        http.ok("POST", "/v3/delete-order", json!({}));
        http.ok("POST", "/v3/set-initial-margin-rate", json!({}));
        http.ok("POST", "/v3/delete-open-orders", json!({}));
    });
    Store::update(h.store.as_ref(), serde_json::from_value(json!({"markets":[{"id":"BTC-USDC","symbol":"BTCUSDC","base":"BTC","quote":"USDC","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":50}},"info":contract()}]})).unwrap(), false).unwrap();
    h.store.update_zk_link_config(Some(json!({"id":"16909060","l2Key":"fixture-l2","contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"},"contractConfig":{"perpetualContract":[contract()]}}))).unwrap();
    h
}

fn check_native_field(order: &Order, field: &str) {
    let undefined = match field {
        "type" => order.r#type.is_undefined(),
        "status" => order.status.is_undefined(),
        "side" => order.side.is_undefined(),
        _ => unreachable!(),
    };
    assert!(
        undefined,
        "unmapped {field} remains an own undefined property"
    );
    let native = safe_cex_exchange::js_runtime::value::from_typed(order).unwrap();
    assert!(native.has_own(field));
    assert!(matches!(
        native.get(field).unwrap(),
        safe_cex_exchange::js_runtime::value::Js::Undefined
    ));
    assert_eq!(
        order
            .info
            .value()
            .unwrap()
            .get(field)
            .unwrap()
            .text_equals("UNMAPPED_NATIVE"),
        true
    );
}
fn unmapped_case(field: &str) {
    let h = fixture();
    let rows = |prefix: &str| {
        let mut before = raw_order();
        before["id"] = json!(format!("{prefix}-before"));
        let mut native = raw_order();
        native["id"] = json!(format!("{prefix}-native"));
        native[field] = json!("UNMAPPED_NATIVE");
        let mut after = raw_order();
        after["id"] = json!(format!("{prefix}-after"));
        json!([before, native, after])
    };
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/v3/open-orders");
    h.http.ok("GET", "/v3/open-orders", rows("rest"));
    let mut errors = Vec::new();
    match h.drive(h.venue().fetch_orders(None)) {
        Ok(orders) => {
            assert_eq!(
                orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                ["rest-before", "rest-native", "rest-after"]
            );
            check_native_field(&orders[1], field);
        }
        Err(error) => errors.push(format!("REST {field}: {}", error.message)),
    }
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone();
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"topic":"ws_zk_accounts_v3","contents":{"orders":rows("ws")}}).to_string(),
            ),
        ));
    h.runtime.pump();
    let orders = h.store.orders();
    let expected = if field == "status" {
        assert!(
            !orders.iter().any(|order| order.id == "ws-native"),
            "source ignores an unrecognized private status after mapping it"
        );
        vec!["ws-before", "ws-after"]
    } else {
        vec!["ws-before", "ws-native", "ws-after"]
    };
    for id in expected {
        if let Some(order) = orders.iter().find(|o| o.id == id) {
            if id == "ws-native" {
                check_native_field(order, field);
            }
        } else {
            errors.push(format!("private frame {field} lost {id}"));
        }
    }
    drop(orders);
    h.venue().dispose().unwrap();
    h.runtime.pump();
    assert!(
        errors.is_empty(),
        "unmapped native order enums must not abort a batch: {}",
        errors.join("; ")
    );
}
#[test]
fn unmapped_order_type_keeps_rest_and_private_batches() {
    unmapped_case("type");
}
#[test]
fn unmapped_order_status_keeps_rest_and_private_batches() {
    unmapped_case("status");
}
#[test]
fn unmapped_order_side_keeps_rest_and_private_batches() {
    unmapped_case("side");
}

fn malformed_reduce_only_case(private_frame: bool) {
    for (bad, displayed) in [
        (json!(17), "Number(17.0)"),
        (json!("true"), "String(\"true\")"),
        (json!({}), "Object([])"),
        (json!([]), "Array([])"),
    ] {
        let h = fixture();
        let rows = |prefix: &str| {
            let mut before = raw_order();
            before["id"] = json!(format!("{prefix}-before"));
            let mut malformed = raw_order();
            malformed["id"] = json!(format!("{prefix}-bad"));
            malformed["reduceOnly"] = bad.clone();
            let mut after = raw_order();
            after["id"] = json!(format!("{prefix}-after"));
            json!([before, malformed, after])
        };
        h.http
            .routes
            .borrow_mut()
            .retain(|(_, path, _)| path != "/v3/open-orders");
        h.http.ok("GET", "/v3/open-orders", rows("rest"));
        if !private_frame {
            let orders = h
                .drive(h.venue().fetch_orders(None))
                .expect("one malformed reduceOnly must not reject the REST snapshot");
            assert_eq!(
                orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
                ["rest-before", "rest-after"]
            );
            assert!(orders.iter().all(|order| order.reduce_only == false));
            let warnings = h.runtime.console.borrow().clone();
            assert_eq!(
                warnings
                    .iter()
                    .filter(|line| line.contains("Order.reduceOnly"))
                    .count(),
                1
            );
            assert!(
                warnings.iter().any(|line| line.contains(displayed)),
                "refusal must report the original value: {warnings:?}"
            );
            h.venue().dispose().unwrap();
            continue;
        }
        h.drive(h.venue().start()).unwrap();
        h.runtime.pump();
        h.runtime.console.borrow_mut().clear();
        let private = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("private"))
            .unwrap()
            .1
            .clone();
        private
            .events
            .borrow_mut()
            .push_back(safe_cex_exchange::websocket::WsEvent::Message(
                safe_cex_transport::socket::SocketFrame::Text(
                    json!({"topic":"ws_zk_accounts_v3","contents":{"orders":rows("ws")}})
                        .to_string(),
                ),
            ));
        h.runtime.pump();
        let orders = h.store.orders();
        for id in ["ws-before", "ws-after"] {
            assert!(
                orders.iter().any(|order| order.id == id),
                "private row lost {id}"
            );
        }
        assert!(!orders.iter().any(|order| order.id == "ws-bad"));
        drop(orders);
        let warnings = h.runtime.console.borrow();
        assert_eq!(
            warnings
                .iter()
                .filter(|line| line.contains("Order.reduceOnly"))
                .count(),
            1
        );
        assert!(
            warnings.iter().any(|line| line.contains(displayed)),
            "private refusal must report field and original value: {warnings:?}"
        );
        assert!(
            h.runtime.reports.borrow().is_empty(),
            "row refusal must not abort dispatch"
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn malformed_reduce_only_refuses_only_its_rest_row() {
    malformed_reduce_only_case(false);
}
#[test]
fn malformed_reduce_only_refuses_only_its_private_row() {
    malformed_reduce_only_case(true);
}
