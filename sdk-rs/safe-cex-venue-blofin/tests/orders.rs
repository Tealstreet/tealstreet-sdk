#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{Market, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_blofin::orders::*;
use serde_json::{Value, json};
use std::process::Command;
fn bits(value: f64) -> Value {
    json!({"number":if value.is_nan(){"nan".into()}else{format!("{:016x}",value.to_bits())}})
}
fn info(value: safe_cex_types::retained_json::RetainedJsonInput) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Float(v) => bits(v),
        Input::Lexeme(v) => bits(v.parse().unwrap()),
        Input::Array(v) => Value::Array(v.into_iter().map(info).collect()),
        Input::Object(v) => Value::Object(
            v.into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), info(v)))
                .collect(),
        ),
        Input::Text(v) => Value::String(String::from_utf16(&v).unwrap()),
        Input::Bool(v) => Value::Bool(v),
        Input::Null => Value::Null,
        Input::Undefined => panic!("source corpus has no native undefined in JSON info"),
    }
}

fn native(o: Order) -> Value {
    assert!(
        o.parent_id.is_none()
            && o.close.is_none()
            && o.trigger_type.is_none()
            && o.tp_trigger_type.is_none()
            && o.sl_trigger_type.is_none()
            && o.take_profit.is_none()
            && o.stop_loss.is_none(),
        "orders_absent_fields"
    );
    let extra = o.extra.expect("orders_extra_present");
    assert!(
        extra.remaining_notional.is_none()
            && extra.conditional_order_price.is_none()
            && extra.reject_reason.is_none()
            && extra.is_from_chaser.is_none()
            && extra.chaser_id.is_none()
            && extra.chaser_attempts.is_none()
            && extra.chaser_max_attempts.is_none(),
        "orders_extra_absent"
    );
    let mut x =
        json!({"remainingNative":bits(extra.remaining_native.expect("orders_remaining_native"))});
    if let Some(v) = extra.filled_in_placement_units {
        x["filledInPlacementUnits"] = bits(v);
    }
    let status = match o.status.value().expect("present source enum in fixture") {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        other => panic!("unexpected_status {other:?}"),
    };
    let kind = match o.r#type.value().expect("present source enum in fixture") {
        OrderType::Limit => "limit",
        OrderType::Market => "market",
        OrderType::Conditional => "conditional",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::StopLoss => "stop_market",
        other => panic!("unexpected_type {other:?}"),
    };
    let side = match o.side.value().expect("present source enum in fixture") {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let mut value = json!({"id":o.id,"status":status,"symbol":o.symbol,"type":kind,"side":side,"price":bits((o.price).to_number()),"amount":bits(o.amount),"filled":bits(o.filled),"remaining":bits(o.remaining),"reduceOnly":o.reduce_only,"lastUpdate":bits(o.last_update),"extra":x,"info":info(o.info.expect("orders_info_present"))});
    if let Some(time) = o.time_in_force.into_value() {
        value["timeInForce"] = json!(match time {
            OrderTimeInForce::PostOnly => "PostOnly",
            OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
            other => panic!("unexpected_time {other:?}"),
        });
    }
    value
}
#[test]
fn live_orders_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/orders_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_orders: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("orders", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|n| n > 0),
        "capture_node_major"
    );
    eprintln!(
        "orders capture header: {header}; cases={}",
        corpus["cases"].as_array().unwrap().len()
    );
    let mut refused = 0;
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str().unwrap();
            let input: OrdersInput = match path {
                0 => js_from_str(raw),
                1 => js_from_value(serde_json::from_str(raw).unwrap()),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()),
            }
            .expect("orders_ingress_retained");
            let markets: Vec<Market> = js_from_value(case["markets"].clone()).unwrap();
            let store = DefaultStore::new();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(markets),
                    ..Default::default()
                },
                false,
            )
            .expect("orders_fixture_store_update");
            let mut clock = Vec::new();
            let result = map_orders(&input, &store, || {
                let now = 1700000000000.125 + clock.len() as f64 * 0.375;
                clock.push(bits(now));
                now
            });
            if case["invalid"] == true {
                let err = result.expect_err("orders_typed_boundary");
                assert_eq!(err.name, "RepresentationError", "orders_typed_boundary");
                assert!(
                    case["result"]["ok"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|o| !o["id"].is_string()
                            || !o["symbol"].is_string()
                            || !o["side"].is_string()
                            || !o["status"].is_string()
                            || !o["type"].is_string()),
                    "source_orders_invalid_native"
                );
                let prefix = case["refusalPrefix"].as_u64().unwrap() as usize;
                assert_eq!(
                    clock,
                    case["clock"].as_array().unwrap()[..prefix],
                    "orders_refusal_clock_boundary"
                );
                refused += 1;
                pin.compared(i, path);
                continue;
            }
            if let Ok(rows) = &result {
                pin.native(i, "/result/ok", rows.len());
                for index in 0..pin.case_count(i, "/aliases") {
                    let pair = &case["aliases"][index];
                    let a = pair[0].as_u64().unwrap() as usize;
                    let b = pair[1].as_u64().unwrap() as usize;
                    let first = rows[a].info.as_ref().unwrap();
                    let second = rows[b].info.as_ref().unwrap();
                    assert_eq!(first, second, "orders_owned_info_values");
                    assert!(!std::ptr::eq(first, second), "orders_owned_info_boundary");
                    pin.hit("alias-pairs");
                }
            }
            let actual = match result {
                Ok(rows) => json!({"ok":rows.into_iter().map(native).collect::<Vec<_>>()}),
                Err(error) => json!({"error":error.name,"message":error.message}),
            };
            assert_eq!(
                actual, case["result"],
                "orders_native_fields case={i} path={path} raw={raw}"
            );
            assert_eq!(json!(clock), case["clock"], "orders_clock_trace case={i}");
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    assert_eq!(refused, 360, "orders_boundary_census");
    pin.finish("alias-pairs", 1299);
    for index in 0..pin.count("/boundaries") {
        let raw = &corpus["boundaries"][index];
        assert!(
            js_from_str::<OrdersInput>(raw.as_str().unwrap()).is_err(),
            "orders_ingress_boundary"
        );
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
}

#[test]
fn audit_order_refusal_preempts_source_evaluation() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/orders_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "BLOFIN-AUDIT source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["audit"].as_array().unwrap();
    assert_eq!(cases.len(), 2, "BLOFIN-AUDIT independent count");
    let messages = [
        "Cannot read properties of null (reading 'instType')",
        "Cannot convert object to primitive value",
    ];
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["result"]["error"], "TypeError",
            "BLOFIN-AUDIT source error"
        );
        assert_eq!(
            case["result"]["message"], messages[index],
            "BLOFIN-AUDIT source precedence"
        );
        let expected_clock = if index == 0 {
            json!([{"number":"4278bcfe56800200"}])
        } else {
            json!([])
        };
        assert_eq!(case["clock"], expected_clock, "BLOFIN-AUDIT source clock");
        let input: OrdersInput = js_from_str(case["raw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(js_from_value(case["markets"].clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut calls = 0;
        let error = map_orders(&input, &store, || {
            calls += 1;
            1700000000000.125
        })
        .unwrap_err();
        assert_eq!(
            error.name, "RepresentationError",
            "BLOFIN-AUDIT native early refusal"
        );
        assert_eq!(
            error.message, "Source order is outside the shared typed Order model",
            "BLOFIN-AUDIT native refusal message"
        );
        assert_eq!(calls, 0, "BLOFIN-AUDIT native skipped clock");
    }
}

#[test]
fn audit_order_source_control_discriminates() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "audit_order_refusal_preempts_source_evaluation",
            "--nocapture",
        ])
        .env("SAFE_CEX_BLOFIN_AUDIT_SOURCE_CLOCK", "1")
        .output()
        .unwrap();
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "BLOFIN-AUDIT source control exit: {diagnostic}"
    );
    assert!(
        diagnostic.contains("BLOFIN-AUDIT source clock"),
        "BLOFIN-AUDIT named source control: {diagnostic}"
    );
}
