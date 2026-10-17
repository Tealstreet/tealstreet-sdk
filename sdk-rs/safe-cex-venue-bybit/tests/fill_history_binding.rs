//! Historical fills through the production receiver and signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{ExchangeName, OrderSide};
use serde_json::{Value, json};
use support::{harness_as, options};
fn fill(symbol: &str) -> Value {
    json!({"execId":"fill1","orderId":"order1","symbol":symbol,"execType":"Trade",
        "execQty":"2","execPrice":"100","execTime":"1700000000000",
        "execFee":"-0.2","feeCurrency":"USDT","isMaker":true,"side":"Buy"})
}
fn fills_case(name: ExchangeName, symbol: &str, category: &str) {
    let h = harness_as(name, options(), |wire| {
        let mut row = fill(symbol);
        row["side"] = json!("Buy");
        wire.ok("GET", "/v5/execution/list", json!({"list":[row]}));
    });
    let opts = serde_json::from_value(json!({"symbol":symbol,"from":1,"to":2,"limit":10})).unwrap();
    let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "fills must reach the real execution-history reader"
    );
    assert_eq!(rows[0].id.as_deref(), Some("fill1"));
    assert_eq!(rows[0].order_id.as_deref(), Some("order1"));
    assert_eq!(
        (rows[0].timestamp, rows[0].price, rows[0].qty, rows[0].side),
        (1700000000., 100., 2., OrderSide::Buy)
    );
    assert_eq!(rows[0].commission, Some(0.2));
    assert_eq!(rows[0].maker, Some(true));
    assert_eq!(rows[0].commission_asset.as_deref(), Some("USDT"));
    let calls = h.wire.calls("/v5/execution/list");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], category);
    assert_eq!(calls[0].params["startTime"], "1");
    assert_eq!(calls[0].params["endTime"], "2");
    assert_eq!(calls[0].params["limit"], "10");
    h.venue().dispose().unwrap();
}
#[test]
fn primary_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybit, "BTCUSDT", "linear");
}
#[test]
fn demo_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybitdemo, "BTCUSDT", "linear");
}
#[test]
fn inverse_fill_history_reaches_signed_http() {
    fills_case(ExchangeName::Bybitinverse, "BTCUSD", "inverse");
}
#[test]
fn spot_retains_inherited_empty_fills() {
    let h = harness_as(ExchangeName::Bybitspot, options(), |_| {});
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(h.venue().fetch_historical_fills(opts))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/execution/list").is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn fill_history_preserves_a_valid_prefix_when_a_later_row_throws() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/execution/list",
            json!({"list":[fill("BTCUSDT"),null]}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","from":1})).unwrap();
    let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(h.wire.calls("/v5/order/history").is_empty());
    assert!(h.runtime.logs.borrow().iter().any(|m| {
        m.starts_with("warning: Error fetching executions: Cannot read properties of null")
    }));
    h.venue().dispose().unwrap();
}

#[test]
fn fill_history_discards_recovery_prefix_when_a_later_page_throws() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let h = harness_as(name, options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/order/history",
                json!({"list":[{
                "orderId":"recovered", "orderStatus":"Filled", "symbol":"BTCUSD",
                "side":"Buy", "cumExecQty":"2", "avgPrice":"100", "updatedTime":"1700000000000"
            }], "nextPageCursor":"next"}),
            );
            wire.ok("GET", "/v5/order/history", json!({"list":[null]}));
        });
        let opts = serde_json::from_value(json!({"symbol":"BTCUSD", "from":1})).unwrap();
        let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
        assert!(
            rows.is_empty(),
            "failed recovery discards its partial accumulator"
        );
        assert_eq!(h.wire.calls("/v5/order/history").len(), 2);
        assert!(h.runtime.logs.borrow().iter().any(|m| m.starts_with(
            "warning: Failed order-history fallback fetch executions: Cannot read properties of null"
        )));
        h.venue().dispose().unwrap();
    }
}

#[test]
fn fill_history_uses_actual_order_history_recovery_and_fee_detail() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let h = harness_as(name, options(), |wire| {
            let mut funding = fill("BTCUSD");
            funding["execType"] = json!("Funding");
            wire.ok("GET", "/v5/execution/list", json!({"list":[funding]}));
            wire.ok("GET","/v5/order/history",json!({"list":[{
                "orderId":"recovered","orderStatus":"Filled","symbol":"BTCUSD","side":"Sell",
                "cumExecQty":"2","avgPrice":"0","cumExecValue":"200","updatedTime":"1700000001000",
                "cumFeeDetail":"{\"USDT\":\"-0.1\",\"BTC\":\"0.2\"}","feeCcy":"BTC"
            }]}));
        });
        let opts = serde_json::from_value(
            json!({"symbol":"BTCUSD","from":1700000000000u64,"to":1700000002000u64}),
        )
        .unwrap();
        let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].price, rows[0].qty, rows[0].timestamp, rows[0].side),
            (100., 2., 1700000001., OrderSide::Sell)
        );
        assert_eq!(rows[0].order_id.as_deref(), Some("recovered"));
        assert_eq!(rows[0].commission, Some(0.1 + 0.2));
        assert_eq!(rows[0].commission_asset.as_deref(), Some("BTC"));
        assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
        assert!(
            h.runtime
                .logs
                .borrow()
                .iter()
                .any(|m| m.contains("rawRows=1, windows=1, limit=50, hasOrderId=false"))
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn primary_inverse_symbols_keep_short_then_extended_recovery_but_dedicated_inverse_does_not() {
    for (name, expected) in [
        (ExchangeName::Bybit, 6),
        (ExchangeName::Bybitdemo, 6),
        (ExchangeName::Bybitinverse, 5),
    ] {
        let h = harness_as(name, options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok("GET", "/v5/order/history", json!({"list":[]}));
        });
        let opts = serde_json::from_value(json!({"symbol":"BTCUSD"})).unwrap();
        assert!(
            h.drive(h.venue().fetch_historical_fills(opts))
                .unwrap()
                .is_empty()
        );
        let calls = h.wire.calls("/v5/order/history");
        assert_eq!(calls.len(), expected);
        assert!(calls.iter().all(|r| r.params["category"] == "inverse"));
        if expected == 6 {
            assert!(
                h.runtime
                    .logs
                    .borrow()
                    .iter()
                    .any(|m| m.contains("retrying 30d order-history lookup"))
            );
        } else {
            assert!(
                h.runtime
                    .logs
                    .borrow()
                    .iter()
                    .all(|m| !m.contains("retrying 30d order-history lookup"))
            );
        }
        h.venue().dispose().unwrap();
    }
}

#[test]
fn fill_history_caps_cursor_recursion_and_inverse_coerces_ids() {
    let h = harness_as(ExchangeName::Bybitinverse, options(), |wire| {
        let mut row = fill("BTCUSD");
        row["execId"] = json!(42);
        wire.ok(
            "GET",
            "/v5/execution/list",
            json!({"list":[row],"nextPageCursor":"more"}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSD","limit":1000})).unwrap();
    let rows = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r.id.as_deref() == Some("42")));
    let calls = h.wire.calls("/v5/execution/list");
    assert_eq!(calls.len(), 5);
    assert!(calls.iter().all(|r| r.params["limit"] == "100"));
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|m| m == "warning: fetchHistoricalFills reached max recursion depth of 5")
    );
    h.venue().dispose().unwrap();
}

#[test]
fn primary_fill_pages_keep_source_over_limit_rows() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/execution/list",
            json!({"list":[fill("BTCUSDT"),fill("BTCUSDT")],"nextPageCursor":"more"}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","limit":1})).unwrap();
    assert_eq!(
        h.drive(h.venue().fetch_historical_fills(opts))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(h.wire.calls("/v5/execution/list").len(), 1);
    h.venue().dispose().unwrap();
}

#[test]
fn public_only_and_absent_symbols_skip_fill_http() {
    for public in [false, true] {
        let mut opts = options();
        opts.public_data_only = Some(public);
        let h = harness_as(ExchangeName::Bybit, opts, |_| {});
        let opts = if public {
            json!({"symbol":"BTCUSDT"})
        } else {
            json!({})
        };
        assert!(
            h.drive(
                h.venue()
                    .fetch_historical_fills(serde_json::from_value(opts).unwrap())
            )
            .unwrap()
            .is_empty()
        );
        assert!(h.wire.calls("/v5/execution/list").is_empty());
        h.venue().dispose().unwrap();
    }
}

#[test]
#[ignore = "requires installed maintained TypeScript"]
fn fill_history_routes_and_recovery_follow_live_source() {
    fn normalize(value: Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64().unwrap()),
            Value::Array(rows) => Value::Array(rows.into_iter().map(normalize).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, normalize(value)))
                    .collect(),
            ),
            other => other,
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/rest-fill-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 45);
    for (index, case) in cases.iter().enumerate() {
        let name: ExchangeName = serde_json::from_value(case["name"].clone()).unwrap();
        let mut opts = options();
        opts.public_data_only = case["publicOnly"].as_bool();
        let h = harness_as(name, opts, |wire| {
            for (key, path) in [
                ("executionPages", "/v5/execution/list"),
                ("historyPages", "/v5/order/history"),
            ] {
                let pages = case[key].as_array().unwrap();
                if pages.is_empty() {
                    wire.ok("GET", path, json!({"list":[]}));
                } else {
                    for page in pages {
                        wire.route("GET", path, 200, page.clone());
                    }
                }
            }
        });
        let result = h
            .drive(
                h.venue()
                    .fetch_historical_fills(serde_json::from_value(case["opts"].clone()).unwrap()),
            )
            .unwrap();
        assert_eq!(
            normalize(serde_json::to_value(result).unwrap()),
            normalize(case["result"].clone()),
            "source result case{index}"
        );
        let requests = h.wire.requests.borrow();
        assert_eq!(
            requests.len(),
            case["calls"].as_array().unwrap().len(),
            "source request count case{index}"
        );
        for (actual, expected) in requests.iter().zip(case["calls"].as_array().unwrap()) {
            assert_eq!(
                actual.url,
                expected["url"].as_str().unwrap(),
                "source path case{index}"
            );
            let expected: serde_json::Map<String, Value> = expected["params"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| {
                    let value = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => safe_cex_num::js_number_string(n.as_f64().unwrap()),
                        _ => panic!("unexpected query scalar"),
                    };
                    (key.clone(), Value::String(value))
                })
                .collect();
            let mut actual_params = actual.params.clone();
            assert!(
                actual_params["requestId"]
                    .as_str()
                    .unwrap()
                    .starts_with("req-")
            );
            actual_params.as_object_mut().unwrap().remove("requestId");
            assert_eq!(
                actual_params,
                Value::Object(expected),
                "source query case{index}"
            );
        }
        assert_eq!(
            serde_json::to_value(h.runtime.logs.borrow().clone()).unwrap(),
            case["logs"],
            "source diagnostics case{index}"
        );
        drop(requests);
        h.venue().dispose().unwrap();
    }
}
