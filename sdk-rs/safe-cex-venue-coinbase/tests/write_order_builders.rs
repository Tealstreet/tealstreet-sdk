use safe_cex_types::OrderTimeInForce;
use safe_cex_venue_coinbase::{starbase::CoinbaseStarbaseProtocolError, starbase_orders::*};
use serde_json::{Value, json};
use std::process::Command;
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;

fn enum_text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}
fn optional_number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(number_value)
}
fn number_value(value: &Value) -> Option<f64> {
    if let Some(value) = value.as_f64() {
        return Some(value);
    }
    match value.get("$number")?.as_str()? {
        "NaN" => Some(f64::NAN),
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        "-0" => Some(-0.0),
        _ => None,
    }
}
fn optional_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}
fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn order_type(value: &str) -> CoinbaseStarbaseWriteOrderType {
    use CoinbaseStarbaseWriteOrderType as T;
    match value {
        "limit" => T::Limit,
        "stop_limit" => T::StopLimit,
        "take_limit" => T::TakeLimit,
        "market" => T::Market,
        "stop_market" => T::StopMarket,
        "take_market" => T::TakeMarket,
        "market_limit" => T::MarketLimit,
        "trailing_stop" => T::TrailingStop,
        other => panic!("unclassified order type {other}"),
    }
}
fn otoco_type(value: &str) -> CoinbaseStarbaseOtocoLegType {
    use CoinbaseStarbaseOtocoLegType as T;
    match value {
        "take_limit" => T::TakeLimit,
        "take_market" => T::TakeMarket,
        "stop_limit" => T::StopLimit,
        "stop_market" => T::StopMarket,
        other => panic!("unclassified OTOCO type {other}"),
    }
}
fn side(value: &str) -> CoinbaseStarbaseWriteSide {
    match value {
        "buy" => CoinbaseStarbaseWriteSide::Buy,
        "sell" => CoinbaseStarbaseWriteSide::Sell,
        other => panic!("unclassified side {other}"),
    }
}
fn tif(value: &str) -> OrderTimeInForce {
    match value {
        "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
        "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
        "FillOrKill" => OrderTimeInForce::FillOrKill,
        "PostOnly" => OrderTimeInForce::PostOnly,
        other => panic!("unclassified time in force {other}"),
    }
}
fn trigger(value: &str) -> safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType {
    use safe_cex_venue_coinbase::starbase_private::CoinbaseStarbaseTriggerType as T;
    match value {
        "index_price" => T::IndexPrice,
        "mark_price" => T::MarkPrice,
        "last_price" => T::LastPrice,
        other => panic!("unclassified trigger {other}"),
    }
}
fn base_size(value: &Value) -> CoinbaseStarbaseBaseAmountIntent {
    CoinbaseStarbaseBaseAmountIntent {
        total_base_amount: number_value(&value["totalBaseAmount"]).unwrap(),
        source: CoinbaseStarbaseBaseAmountSource::TechnicalMigrationGuide,
        unit: CoinbaseStarbaseBaseAmountUnit::Base,
    }
}
fn amount(value: &Value) -> CoinbaseStarbaseWriteAmountIntent {
    if value["unit"] == "contracts" {
        CoinbaseStarbaseWriteAmountIntent::Contracts(CoinbaseStarbaseContractAmountIntent {
            total_contracts: number_value(&value["totalContracts"]).unwrap(),
            source: CoinbaseStarbaseContractAmountSource::GatewayContractCount,
            unit: CoinbaseStarbaseContractAmountUnit::Contracts,
        })
    } else {
        CoinbaseStarbaseWriteAmountIntent::Base(base_size(value))
    }
}
fn leg(value: &Value) -> CoinbaseStarbaseOtocoLegIntent {
    CoinbaseStarbaseOtocoLegIntent {
        direction: side(enum_text(value, "direction")),
        label: optional_string(value, "label"),
        post_only: optional_bool(value, "postOnly"),
        price: optional_number(value, "price"),
        reduce_only: safe_cex_types::True,
        reject_post_only: optional_bool(value, "rejectPostOnly"),
        size: base_size(&value["size"]),
        time_in_force: value.get("timeInForce").and_then(Value::as_str).map(tif),
        trigger: trigger(enum_text(value, "trigger")),
        trigger_price: optional_number(value, "triggerPrice"),
        order_type: otoco_type(enum_text(value, "type")),
    }
}
fn otoco(value: &Value) -> CoinbaseStarbaseOtocoIntent {
    CoinbaseStarbaseOtocoIntent {
        legs: [leg(&value["legs"][0]), leg(&value["legs"][1])],
        linked_order_type: CoinbaseStarbaseLinkedOrderType::OneTriggersOneCancelsOther,
        trigger_fill_condition: value
            .get("triggerFillCondition")
            .and_then(Value::as_str)
            .map(|v| match v {
                "first_hit" => CoinbaseStarbaseTriggerFillCondition::FirstHit,
                "complete_fill" => CoinbaseStarbaseTriggerFillCondition::CompleteFill,
                "incremental" => CoinbaseStarbaseTriggerFillCondition::Incremental,
                other => panic!("unclassified fill condition {other}"),
            }),
    }
}
fn place_intent(value: &Value) -> CoinbaseStarbasePlaceOrderIntent {
    CoinbaseStarbasePlaceOrderIntent {
        label: optional_string(value, "label"),
        otoco: value.get("otoco").map(otoco),
        post_only: optional_bool(value, "postOnly"),
        price: optional_number(value, "price"),
        reduce_only: optional_bool(value, "reduceOnly"),
        reject_post_only: optional_bool(value, "rejectPostOnly"),
        saved_symbol: enum_text(value, "savedSymbol").to_owned(),
        side: side(enum_text(value, "side")),
        size: amount(&value["size"]),
        time_in_force: value.get("timeInForce").and_then(Value::as_str).map(tif),
        trigger: value.get("trigger").and_then(Value::as_str).map(trigger),
        trigger_offset: optional_number(value, "triggerOffset"),
        trigger_price: optional_number(value, "triggerPrice"),
        r#type: order_type(enum_text(value, "type")),
        valid_until: value
            .get("validUntil")
            .map(|v| CoinbaseStarbaseUnresolvedValidUntilIntent {
                gateway_integer_timestamp: v["gatewayIntegerTimestamp"].as_f64().unwrap(),
                unit: CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified,
            }),
    }
}
fn side_intent(value: &Value) -> CoinbaseStarbaseSideOrderIntent {
    CoinbaseStarbaseSideOrderIntent {
        label: optional_string(value, "label"),
        otoco: value.get("otoco").filter(|v| !v.is_null()).map(otoco),
        post_only: optional_bool(value, "postOnly"),
        price: optional_number(value, "price"),
        reduce_only: optional_bool(value, "reduceOnly"),
        reject_post_only: optional_bool(value, "rejectPostOnly"),
        saved_symbol: enum_text(value, "savedSymbol").to_owned(),
        size: amount(&value["size"]),
        time_in_force: value.get("timeInForce").and_then(Value::as_str).map(tif),
        trigger: value.get("trigger").and_then(Value::as_str).map(trigger),
        trigger_offset: optional_number(value, "triggerOffset"),
        trigger_price: optional_number(value, "triggerPrice"),
        r#type: order_type(enum_text(value, "type")),
        valid_until: value.get("validUntil").filter(|v| !v.is_null()).map(|v| {
            CoinbaseStarbaseUnresolvedValidUntilIntent {
                gateway_integer_timestamp: v["gatewayIntegerTimestamp"].as_f64().unwrap(),
                unit: CoinbaseStarbaseUnresolvedTimestampUnit::UpstreamUnspecified,
            }
        }),
    }
}
fn capture<T: serde::Serialize>(result: Result<T, CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error}),
    }
}
fn capture_void(result: Result<(), CoinbaseStarbaseProtocolError>) -> Value {
    match result {
        Ok(()) => json!({"undefined":true}),
        Err(error) => json!({"error":error}),
    }
}
fn sort_keys(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = std::mem::take(object).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, value) in &mut entries {
                sort_keys(value);
            }
            *object = entries.into_iter().collect();
        }
        Value::Array(values) => values.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

#[test]
fn write_order_builders_live_typescript_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/write-order-builders.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    capture_metadata::assert_capture_metadata(&Value::Array(cases.clone()), 36);
    assert!(cases.len() >= 30);
    let mut typed_intent_order_diverged = false;
    let mut extra_runtime_field_diverged = false;
    let mut tagged_numeric_blocked_intent = false;
    for case in cases {
        let value = &case["input"];
        if value["size"]["totalContracts"]["$number"] == "NaN" {
            let input_intent = place_intent(value);
            let gate = build_coinbase_starbase_place_order(&input_intent).unwrap();
            let CoinbaseStarbaseWriteGate::Blocked { intent, .. } = gate else {
                panic!("contracts intent must remain blocked before numeric validation");
            };
            let CoinbaseStarbaseWriteAmountIntent::Contracts(size) = &intent.size else {
                panic!("contracts intent must retain the contract amount");
            };
            for (path, actual) in [
                ("ok.intent.price", intent.price.unwrap()),
                ("ok.intent.size.totalContracts", size.total_contracts),
            ] {
                let expected =
                    u64::from_str_radix(case["floatBits"][path].as_str().unwrap(), 16).unwrap();
                assert_eq!(
                    actual.to_bits(),
                    expected,
                    "blocked intent f64 bits at {path}"
                );
            }
            tagged_numeric_blocked_intent = true;
        }
        let actual = match enum_text(&case, "name") {
            "buildCoinbaseStarbaseBuy" => capture(build_coinbase_starbase_buy(&side_intent(value))),
            "buildCoinbaseStarbaseSell" => {
                capture(build_coinbase_starbase_sell(&side_intent(value)))
            }
            "buildCoinbaseStarbasePlaceOrder" => {
                capture(build_coinbase_starbase_place_order(&place_intent(value)))
            }
            "buildOtoco" => capture(build_otoco(&otoco(value))),
            "buildOtocoLeg" => capture(build_otoco_leg(&leg(value))),
            "validateOtocoLegShape" => capture_void(validate_otoco_leg_shape(&leg(value))),
            "validateOrderShape" => capture_void(validate_order_shape(
                order_type(enum_text(value, "type")),
                optional_number(value, "price"),
                value.get("trigger").and_then(Value::as_str).map(trigger),
                optional_number(value, "triggerOffset"),
                optional_number(value, "triggerPrice"),
            )),
            other => panic!("unclassified builder {other}"),
        };
        if case["result"]["ok"]["status"] == "blocked" {
            if case["input"].get("unmodeledRuntimeField").is_some() {
                assert_eq!(
                    case["result"]["ok"]["intent"]["unmodeledRuntimeField"],
                    json!({"retained":true}),
                    "TypeScript retains caller-added own fields in blocked intent"
                );
                assert!(
                    actual["ok"]["intent"]
                        .get("unmodeledRuntimeField")
                        .is_none()
                );
                extra_runtime_field_diverged = true;
            }
            let rust_keys: Vec<_> = actual["ok"]["intent"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            let ts_keys: Vec<_> = case["result"]["ok"]["intent"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            typed_intent_order_diverged |= rust_keys != ts_keys;
            if case["name"] == "buildCoinbaseStarbasePlaceOrder"
                && case["input"]["size"]["unit"] == "contracts"
                && case["input"].get("unmodeledRuntimeField").is_none()
            {
                assert_eq!(ts_keys, ["savedSymbol", "side", "size", "type", "price"]);
                assert_eq!(rust_keys, ["price", "savedSymbol", "side", "size", "type"]);
            }
            let mut rust = actual;
            let mut ts = case["result"].clone();
            if case["input"].get("unmodeledRuntimeField").is_some() {
                ts["ok"]["intent"]
                    .as_object_mut()
                    .unwrap()
                    .remove("unmodeledRuntimeField");
            }
            sort_keys(&mut rust);
            sort_keys(&mut ts);
            comparison::compare(&rust, &ts, &case.to_string());
        } else {
            comparison::compare(&actual, &case["result"], &case.to_string());
        }
    }
    assert!(
        typed_intent_order_diverged,
        "blocked intent key-order choice must remain visible in the corpus"
    );
    assert!(
        extra_runtime_field_diverged,
        "blocked intent caller-added field difference must remain visible in the corpus"
    );
    assert!(
        tagged_numeric_blocked_intent,
        "blocked raw numeric preservation must be pinned by tagged f64 bits"
    );
}
