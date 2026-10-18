//! Regressions run the assembled class, including the concrete HTTP clients.
#[path = "../../tests/support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::js_runtime::{promise::await_js, value::from_json};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use serde_json::json;
use support::{Harness, harness, options};

fn venue() -> Harness {
    harness(ExchangeName::Phemex, options(), |_| {})
}
fn orders(count: usize) -> Vec<Order> {
    (0..count)
        .map(|i| {
            serde_json::from_value(json!({
                "id":format!("order-{i}"), "symbol":"BTCUSDT", "side":"buy", "type":"limit",
                "status":"open", "amount":1, "filled":0, "remaining":1, "price":100,
                "reduceOnly":false, "lastUpdate":0, "info":{"posSide":"Long"}
            }))
            .unwrap()
        })
        .collect()
}
fn seed(h: &Harness, orders: &[Order]) {
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(orders.to_vec()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

#[test]
fn batch_cancel_success_checks_envelope_not_order_rows() {
    let h = venue();
    let orders = orders(12);
    seed(&h, &orders);
    h.http.ok(
        "DELETE",
        "/g-orders",
        json!({"code":0,"data":[{"orderID":"order-0"}]}),
    );
    h.drive(h.venue().cancel_orders(orders))
        .expect("successful envelope cancels both lots");
    assert_eq!(h.http.calls("/g-orders").len(), 2);
    assert!(
        h.store
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Canceled)
    );
}

#[test]
fn symbol_cancel_uses_the_live_store_filter_and_real_cancellation_client() {
    let h = venue();
    let mut orders = orders(3);
    orders[1].symbol = "ETHUSDT".into();
    seed(&h, &orders);
    h.http.ok("DELETE", "/g-orders/cancel", json!({"code":0}));
    h.http
        .ok("DELETE", "/g-orders", json!({"code":0,"data":[]}));
    // Invoke the member installed on the actual PhemexExchange receiver.
    h.drive(
        h.exchange
            .invoke_unit("cancelSymbolOrders", vec![from_json(&json!("BTCUSDT"))]),
    )
    .expect("symbol cancellation completes");
    let requests = h.http.calls("/g-orders");
    assert_eq!(requests.len(), 1, "one real matching-symbol batch");
    assert_eq!(requests[0].method.to_uppercase(), "DELETE");
    assert_eq!(requests[0].params["symbol"], "BTCUSDT");
    assert_eq!(requests[0].params["orderID"], "order-0,order-2");
    let stored = h.store.orders();
    assert_eq!(stored.len(), 3);
    assert_eq!(stored[0].status, OrderStatus::Canceled);
    assert_eq!(
        stored[1].status,
        OrderStatus::Open,
        "other symbol stays open"
    );
    assert_eq!(stored[2].status, OrderStatus::Canceled);
}

#[test]
fn batch_cancel_rejection_preserves_live_orders_and_message() {
    for data in [json!([]), json!(null), json!([{"code":0}])] {
        let h = venue();
        let orders = orders(2);
        seed(&h, &orders);
        h.http.ok(
            "DELETE",
            "/g-orders",
            json!({"code":10002,"msg":"batch rejected","data":data}),
        );
        let error = h
            .drive(h.venue().cancel_orders(orders))
            .expect_err("rejected envelope");
        assert_eq!(error.message, "batch rejected");
        assert!(
            h.store
                .orders()
                .iter()
                .all(|o| o.status == OrderStatus::Open)
        );
    }
}

#[test]
fn single_and_batch_cancels_bypass_the_read_rate_limiter() {
    for count in [1, 2] {
        let h = venue();
        h.http.ok("DELETE", "/g-orders/cancel", json!({"code":0}));
        h.http
            .ok("DELETE", "/g-orders", json!({"code":0,"data":[]}));
        // Fill the actual xhr client's 3-RPS window without advancing time.
        let xhr = h.exchange.rt.this.get("xhr").unwrap();
        h.http.ok("GET", "/read-window", json!({}));
        for _ in 0..3 {
            xhr.call_method(
                "get",
                vec![safe_cex_exchange::js_runtime::value::Js::str(
                    "/read-window",
                )],
            )
            .unwrap();
        }
        h.runtime.pump();
        assert_eq!(h.http.calls("/read-window").len(), 3);
        let future = h
            .exchange
            .rt
            .this
            .call_method(
                "cancelOrders",
                vec![from_json(&serde_json::to_value(orders(count)).unwrap())],
            )
            .unwrap();
        h.runtime.pump();
        assert_eq!(
            h.http
                .calls(if count == 1 {
                    "/g-orders/cancel"
                } else {
                    "/g-orders"
                })
                .len(),
            1,
            "cancel must reach HTTP while the read window is full"
        );
        h.drive(async {
            await_js(
                &(h.runtime.clone()
                    as std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
                future,
            )
            .await
            .map_err(|e| e.to_exchange_error())
        })
        .unwrap();
    }
}

#[test]
fn binding_place_orders_rejects_duplicate_ids_before_any_http() {
    let h = venue();
    // Allow an unchecked batch to place real requests, rather than fail on a
    // missing market or an unscripted endpoint before exposing the ID defect.
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(json!({"markets":[{
            "id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
            "precision":{"amount":0.001,"price":0.1},
            "limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":50}}
        }]}))
        .unwrap(),
        false,
    )
    .unwrap();
    h.http.ok(
        "GET",
        "/g-accounts/accountPositions",
        json!({"code":0,"data":{
            "positions":[{"symbol":"BTCUSDT","size":"2","avgEntryPriceRp":"90","markPriceRp":"100",
            "leverageRr":"-10","posSide":"Long","side":"Buy","posMode":"Hedged",
            "crossMargin":true,"liquidationPriceRp":"50"}]
        }}),
    );
    h.http.ok(
        "PUT",
        "/g-orders/create",
        json!({"code":0,"data":{"orderID":"created"}}),
    );
    let order = json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":1,"price":100,"clientOrderId":"same-id"});
    let future = h
        .exchange
        .rt
        .this
        .call_method("placeOrders", vec![from_json(&json!([order, order]))])
        .unwrap();
    let error = h
        .drive(async {
            await_js(
                &(h.runtime.clone()
                    as std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
                future,
            )
            .await
            .map_err(|e| e.to_exchange_error())
        })
        .unwrap_err();
    assert_eq!(
        error.message,
        "Phemex clientOrderId same-id is duplicated in one batch"
    );
    assert!(
        h.http.requests.borrow().is_empty(),
        "whole batch must reject before any request"
    );
}

#[test]
fn daily_candles_normalize_real_offset_dates_before_utc_midnight() {
    use safe_cex_exchange::js_runtime::value::Js;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let cases = json!([
        {"to":1700000000123u64,"offset":null,"interval":"1d"},
        {"to":1700000000123u64,"offset":8,"interval":"1d"},
        {"to":1700000000123u64,"offset":-5,"interval":"1d"},
        {"to":1700000000123u64,"offset":5.75,"interval":"1d"},
        {"to":1700000000123u64,"offset":-9.5,"interval":"1d"},
        {"to":1700000000123u64,"offset":8,"interval":"1w"},
        {"to":1700000000123u64,"offset":8,"interval":"1M"},
        {"to":1700000000123u64,"offset":8,"interval":"1m"}
    ]);
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/daily-candle-source.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 8);
    for index in 0..8 {
        let input = &cases[index];
        let h = venue();
        h.http.ok(
            "GET",
            "/exchange/public/md/v2/kline/list",
            json!({"data":{"rows":[[1,86400,0,"10","15","9","12","3"]]}}),
        );
        let to = if input["offset"].is_null() {
            from_json(&input["to"])
        } else {
            h.exchange
                .rt
                .global("dayjs")
                .unwrap()
                .call(Js::Undefined, vec![from_json(&input["to"])])
                .unwrap()
                .call_method("utcOffset", vec![from_json(&input["offset"])])
                .unwrap()
        };
        // Call the assembled receiver with a real shared Dayjs instance; no date stub.
        let opts = Js::object(vec![
            ("symbol", Js::str("BTCUSDT")),
            ("interval", from_json(&input["interval"])),
            ("limit", Js::Number(2.0)),
            ("to", to),
        ]);
        let candles: Vec<Candle> = h
            .drive(h.exchange.invoke("fetchOHLCV", vec![opts]))
            .unwrap();
        let calls = h.http.calls("/exchange/public/md/v2/kline/list");
        assert_eq!(calls.len(), 1, "daily case {index} reaches real HTTP");
        let wanted = &expected[index]["requests"][0]["params"];
        assert_eq!(calls[0].params.as_object().unwrap().len(), 4);
        for field in ["symbol", "resolution"] {
            assert_eq!(
                calls[0].params[field], wanted[field],
                "case {index} {field}"
            );
        }
        for field in ["from", "to"] {
            assert_eq!(
                calls[0].params[field].as_f64(),
                wanted[field].as_f64(),
                "daily UTC request must match actual TypeScript case {index} {field}: {input}"
            );
        }
        assert_eq!(candles.len(), 1);
        for (field, value) in [
            ("timestamp", candles[0].timestamp),
            ("open", candles[0].open),
            ("high", candles[0].high),
            ("low", candles[0].low),
            ("close", candles[0].close),
            ("volume", candles[0].volume),
        ] {
            assert_eq!(
                Some(value),
                expected[index]["candles"][0][field].as_f64(),
                "daily mapped candles case {index} {field}"
            );
        }
    }
}
