#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{
    js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
};
use safe_cex_types::{
    Order, OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
    OrderType,
};
use safe_cex_venue_blofin::inline_tpsl::*;
use serde_json::{Value, json};
use std::process::Command;
fn bits(value: f64) -> Value {
    json!({"number":if value.is_nan(){"nan".into()}else{format!("{:016x}",value.to_bits())}})
}
fn info(value: Value) -> Value {
    match value {
        Value::Number(_) => bits(js_value_as_f64(&value).unwrap()),
        Value::Array(v) => Value::Array(v.into_iter().map(info).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, info(v))).collect()),
        other => other,
    }
}
fn trigger(t: OrderStopTriggerType) -> &'static str {
    match t {
        OrderStopTriggerType::LastPrice => "last_price",
        OrderStopTriggerType::MarkPrice => "mark_price",
        OrderStopTriggerType::IndexPrice => "index_price",
    }
}
fn native(o: Order) -> Value {
    let status = match o.status.value().expect("present source enum in fixture") {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        OrderStatus::Creating => "creating",
        OrderStatus::Updating => "updating",
        OrderStatus::Canceling => "canceling",
    };
    let kind = match o.r#type.value().expect("present source enum in fixture") {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    };
    let mut value = json!({"id":o.id,"status":status,"symbol":o.symbol,"type":kind,"side":match o.side.value().expect("present source enum in fixture"){OrderSide::Buy=>"buy",OrderSide::Sell=>"sell"},"price":bits((o.price).to_number()),"amount":bits(o.amount),"filled":bits(o.filled),"remaining":bits(o.remaining),"reduceOnly":o.reduce_only,"lastUpdate":bits(o.last_update)});
    if let Some(v) = o.parent_id.into_value() {
        value["parentId"] = json!(v);
    }
    if let Some(v) = o.close.into_value() {
        value["close"] = json!(v);
    }
    if let Some(v) = o.info.into_value() {
        value["info"] = info(v);
    }
    if let Some(v) = o.time_in_force.into_value() {
        value["timeInForce"] = json!(match v {
            OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
            OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
            OrderTimeInForce::FillOrKill => "FillOrKill",
            OrderTimeInForce::PostOnly => "PostOnly",
        });
    }
    for (key, v) in [
        ("triggerType", o.trigger_type),
        ("tpTriggerType", o.tp_trigger_type),
        ("slTriggerType", o.sl_trigger_type),
    ] {
        if let Some(v) = v.into_value() {
            value[key] = json!(trigger(v));
        }
    }
    for (key, v) in [("takeProfit", o.take_profit), ("stopLoss", o.stop_loss)] {
        if let Some(v) = v.into_value() {
            value[key] = bits(v);
        }
    }
    if let Some(e) = o.extra.into_value() {
        let mut extra = json!({});
        for (key, v) in [
            ("remainingNative", e.remaining_native),
            ("filledInPlacementUnits", e.filled_in_placement_units),
            ("remainingNotional", e.remaining_notional),
            ("conditionalOrderPrice", e.conditional_order_price),
            ("chaserAttempts", e.chaser_attempts),
            ("chaserMaxAttempts", e.chaser_max_attempts),
        ] {
            if let Some(v) = v {
                extra[key] = bits(v);
            }
        }
        if let Some(v) = e.reject_reason {
            extra["rejectReason"] = json!(match v {
                OrderRejectReason::PostOnly => "PostOnly",
                OrderRejectReason::InsufficientBalance => "InsufficientBalance",
                OrderRejectReason::ReduceOnly => "ReduceOnly",
            });
        }
        if let Some(v) = e.is_from_chaser {
            extra["isFromChaser"] = json!(v);
        }
        if let Some(v) = e.chaser_id {
            extra["chaserId"] = json!(v);
        }
        value["extra"] = extra;
    }
    value
}
#[test]
fn live_inline_tpsl_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/inline_tpsl_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_inline: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("inline_tpsl", &corpus);
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
        "inline capture header: {header}; cases={}",
        corpus["cases"].as_array().unwrap().len()
    );
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str().unwrap();
            let input: Vec<Order> = match path {
                0 => js_from_str(raw),
                1 => js_from_value(serde_json::from_str(raw).unwrap()),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()),
            }
            .expect("inline_shared_order_ingress");
            let mut clock = Vec::new();
            let mut result = add_inline_tpsl_fallback_orders(&input, || {
                let now = 1700000000000.125 + clock.len() as f64 * 0.375;
                clock.push(bits(now));
                now
            });
            if let Ok(rows) = &mut result {
                pin.native(i, "/result/ok", rows.len());
                assert_eq!(
                    rows.len(),
                    case["result"]["ok"].as_array().unwrap().len(),
                    "inline_order_count"
                );
                assert_eq!(
                    case["prefixAliases"],
                    json!(vec![true; input.len()]),
                    "source_inline_original_order_aliases"
                );
                for n in input.len()..pin.case_count(i, "/result/ok") {
                    let row = &rows[n];
                    let expected = &case["result"]["ok"][n];
                    match row.r#type.value().expect("present source enum in fixture") {
                        OrderType::TakeProfit => assert_eq!(
                            bits((row.price).to_number()),
                            expected["price"],
                            "inline_take_profit_price"
                        ),
                        OrderType::StopLoss => {
                            assert_eq!(
                                bits((row.price).to_number()),
                                expected["price"],
                                "inline_stop_loss_price"
                            )
                        }
                        _ => panic!("inline_fallback_type"),
                    }
                    assert_eq!(
                        bits(row.amount),
                        expected["amount"],
                        "inline_fallback_amount"
                    );
                    assert_eq!(
                        bits(row.extra.as_ref().unwrap().remaining_native.unwrap()),
                        expected["extra"]["remainingNative"],
                        "inline_fallback_native_quantity"
                    );
                    pin.hit("generated-fields");
                }
                for n in 0..pin.case_count(i, "/prefixAliases") {
                    let v = &input[n];
                    assert!(!std::ptr::eq(&rows[n], v), "inline_owned_order_boundary");
                    pin.hit("prefix-ownership");
                }
                for n in 0..pin.case_count(i, "/infoAliases") {
                    let index = &case["infoAliases"][n];
                    let parent = index.as_u64().expect("source_inline_info_alias") as usize;
                    assert!(
                        !std::ptr::eq(
                            rows[input.len() + n].info.as_ref().unwrap(),
                            input[parent].info.as_ref().unwrap()
                        ),
                        "inline_owned_info_boundary"
                    );
                    pin.hit("info-aliases");
                }
                if let Some(first) = rows.first_mut() {
                    let original = input[0].id.clone();
                    let saved = first.id.clone();
                    first.id = "owned-output-mutation".into();
                    assert_eq!(input[0].id, original, "inline_detached_input");
                    first.id = saved;
                }
            }
            let actual = match result {
                Ok(rows) => json!({"ok":rows.into_iter().map(native).collect::<Vec<_>>()}),
                Err(e) => json!({"error":e.name,"message":e.message}),
            };
            assert_eq!(
                actual, case["result"],
                "inline_native_fields case={i} path={path} raw={raw}"
            );
            assert_eq!(json!(clock), case["clock"], "inline_clock_trace case={i}");
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("generated-fields", 951);
    pin.finish("prefix-ownership", 957);
    pin.finish("info-aliases", 951);
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    let projection = &corpus["projection"];
    assert_eq!(
        projection["source"][0]["unknownOrder"], "source-keeps-order",
        "source_inline_unknown_order"
    );
    assert_eq!(
        projection["source"][0]["extra"]["unknownExtra"], "source-keeps-extra",
        "source_inline_unknown_extra"
    );
    let typed: Vec<Order> = js_from_str(projection["raw"].as_str().unwrap()).unwrap();
    let projected = add_inline_tpsl_fallback_orders(&typed, || 1.0).unwrap();
    let first = native(projected[0].clone());
    assert!(
        first.get("unknownOrder").is_none() && first["extra"].get("unknownExtra").is_none(),
        "inline_shared_model_projection"
    );
    for i in 0..pin.count("/boundaries") {
        let boundary = &corpus["boundaries"][i];
        assert!(
            boundary["result"].get("ok").is_some(),
            "source_inline_ingress_admission case={i}"
        );
        assert!(
            js_from_str::<Vec<Order>>(boundary["raw"].as_str().unwrap()).is_err(),
            "inline_typed_ingress_boundary case={i}"
        );
        pin.hit("boundaries");
    }
    pin.finish("boundaries", pin.count("/boundaries"));
}
