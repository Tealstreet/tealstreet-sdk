use safe_cex_venue_coinbase::starbase_private::*;
use serde_json::{Value, json};
use std::process::Command;

#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;

fn number(value: &Value) -> Option<f64> {
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

fn optional_number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(number)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

fn order(value: &Value) -> CoinbaseStarbasePrivateOrder {
    let price = match &value["price"] {
        Value::String(value) if value == "market_price" => {
            CoinbaseStarbaseOrderPrice::Market(CoinbaseStarbaseMarketPrice::MarketPrice)
        }
        value => CoinbaseStarbaseOrderPrice::Number(number(value).unwrap()),
    };
    CoinbaseStarbasePrivateOrder {
        id: text(value, "id").into(),
        instrument_name: text(value, "instrumentName").into(),
        legacy_saved_symbol: text(value, "legacySavedSymbol").into(),
        state: match text(value, "state") {
            "open" => CoinbaseStarbaseOrderState::Open,
            "filled" => CoinbaseStarbaseOrderState::Filled,
            "rejected" => CoinbaseStarbaseOrderState::Rejected,
            "cancelled" => CoinbaseStarbaseOrderState::Cancelled,
            "untriggered" => CoinbaseStarbaseOrderState::Untriggered,
            "triggered" => CoinbaseStarbaseOrderState::Triggered,
            other => panic!("unclassified order state {other}"),
        },
        r#type: match text(value, "type") {
            "market" => CoinbaseStarbaseOrderType::Market,
            "limit" => CoinbaseStarbaseOrderType::Limit,
            "stop_market" => CoinbaseStarbaseOrderType::StopMarket,
            "stop_limit" => CoinbaseStarbaseOrderType::StopLimit,
            "take_market" => CoinbaseStarbaseOrderType::TakeMarket,
            "take_limit" => CoinbaseStarbaseOrderType::TakeLimit,
            "trailing_stop" => CoinbaseStarbaseOrderType::TrailingStop,
            other => panic!("unclassified order type {other}"),
        },
        time_in_force: match text(value, "timeInForce") {
            "good_til_cancelled" => CoinbaseStarbaseTimeInForce::GoodTilCancelled,
            "good_til_day" => CoinbaseStarbaseTimeInForce::GoodTilDay,
            "fill_or_kill" => CoinbaseStarbaseTimeInForce::FillOrKill,
            "immediate_or_cancel" => CoinbaseStarbaseTimeInForce::ImmediateOrCancel,
            other => panic!("unclassified time-in-force {other}"),
        },
        direction: match text(value, "direction") {
            "buy" => CoinbaseStarbaseOrderDirection::Buy,
            "sell" => CoinbaseStarbaseOrderDirection::Sell,
            other => panic!("unclassified direction {other}"),
        },
        price,
        average_price: optional_number(value, "averagePrice"),
        label: text(value, "label").into(),
        post_only: value["postOnly"].as_bool().unwrap(),
        reduce_only: value.get("reduceOnly").and_then(Value::as_bool),
        creation_timestamp_ms: number(&value["creationTimestampMs"]).unwrap(),
        last_update_timestamp_ms: number(&value["lastUpdateTimestampMs"]).unwrap(),
        gateway_amount: optional_number(value, "gatewayAmount"),
        gateway_filled_amount: optional_number(value, "gatewayFilledAmount"),
        gateway_contracts: optional_number(value, "gatewayContracts"),
        trigger: value
            .get("trigger")
            .and_then(Value::as_str)
            .map(|value| match value {
                "index_price" => CoinbaseStarbaseTriggerType::IndexPrice,
                "mark_price" => CoinbaseStarbaseTriggerType::MarkPrice,
                "last_price" => CoinbaseStarbaseTriggerType::LastPrice,
                other => panic!("unclassified trigger {other}"),
            }),
        trigger_price: optional_number(value, "triggerPrice"),
        trigger_order_id: value
            .get("triggerOrderId")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
fn normalize_starbase_order_live_typescript_contract() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/normalize-order.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    capture_metadata::assert_capture_metadata(&Value::Array(cases.clone()), 30);
    assert!(cases.len() >= 30);
    let mut extra_property_diverged = false;
    let mut numeric_bits_checked = false;
    let mut stop_loss_bits_checked = false;
    let mut info_identity_diverged = false;
    for case in cases {
        let input = order(&case["input"]);
        let actual = normalize_coinbase_starbase_order(&input);
        if case["name"] == "numeric-bits" {
            let CoinbaseStarbaseParityResult::Ready { value } = &actual else {
                panic!("numeric-bits case should normalize as ready");
            };
            for (path, actual) in [
                ("ok.value.price", value.price.to_number()),
                ("ok.value.amount", value.amount),
                ("ok.value.filled", value.filled),
                ("ok.value.remaining", value.remaining),
                ("ok.value.lastUpdate", value.last_update),
                ("ok.value.takeProfit", value.take_profit.unwrap()),
                (
                    "ok.value.info.price",
                    value.info.as_ref().unwrap()["price"].as_f64().unwrap(),
                ),
                (
                    "ok.value.info.averagePrice",
                    value.info.as_ref().unwrap()["averagePrice"]
                        .as_f64()
                        .unwrap(),
                ),
                (
                    "ok.value.info.creationTimestampMs",
                    value.info.as_ref().unwrap()["creationTimestampMs"]
                        .as_f64()
                        .unwrap(),
                ),
                (
                    "ok.value.info.gatewayAmount",
                    value.info.as_ref().unwrap()["gatewayAmount"]
                        .as_f64()
                        .unwrap(),
                ),
                (
                    "ok.value.info.gatewayFilledAmount",
                    value.info.as_ref().unwrap()["gatewayFilledAmount"]
                        .as_f64()
                        .unwrap(),
                ),
                (
                    "ok.value.info.gatewayContracts",
                    value.info.as_ref().unwrap()["gatewayContracts"]
                        .as_f64()
                        .unwrap(),
                ),
                (
                    "ok.value.info.triggerPrice",
                    value.info.as_ref().unwrap()["triggerPrice"]
                        .as_f64()
                        .unwrap(),
                ),
            ] {
                let expected =
                    u64::from_str_radix(case["floatBits"][path].as_str().unwrap(), 16).unwrap();
                assert_eq!(actual.to_bits(), expected, "f64 bits at {path}");
            }
            assert_eq!(
                case["floatBits"]["ok.value.info.lastUpdateTimestampMs"],
                "7ff8000000000000"
            );
            assert!(value.info.as_ref().unwrap()["lastUpdateTimestampMs"].is_null());
            numeric_bits_checked = true;
        }
        if case["name"] == "numeric-stop-loss-bits" {
            let CoinbaseStarbaseParityResult::Ready { value } = &actual else {
                panic!("numeric-stop-loss-bits case should normalize as ready");
            };
            let actual = value
                .stop_loss
                .expect("stop-market maps triggerPrice to stop_loss");
            let expected =
                u64::from_str_radix(case["floatBits"]["ok.value.stopLoss"].as_str().unwrap(), 16)
                    .unwrap();
            assert_eq!(actual.to_bits(), expected, "f64 bits at ok.value.stopLoss");
            stop_loss_bits_checked = true;
        }
        if case["name"] == "info-identity" {
            assert_eq!(case["identityBehavior"]["sameReference"], true);
            assert_eq!(case["identityBehavior"]["orderPriceBefore"], 3.0);
            assert_eq!(case["identityBehavior"]["orderPriceAfter"], 3.0);
            assert_eq!(case["identityBehavior"]["infoPriceBefore"], 3.0);
            assert_eq!(case["identityBehavior"]["infoPriceAfter"], 11.0);
            let CoinbaseStarbaseParityResult::Ready { value } = &actual else {
                panic!("info-identity case should normalize as ready");
            };
            assert_eq!(value.price, 3.0);
            assert_eq!(value.info.as_ref().unwrap()["price"], 3.0);
            info_identity_diverged = true;
        }
        if case["name"] == "extra-own-property" {
            let ts_info = &case["result"]["ok"]["value"]["info"];
            assert_eq!(ts_info["runtimeOnly"], json!({"retained":true}));
            let CoinbaseStarbaseParityResult::Ready { value } = &actual else {
                panic!("extra property case should normalize as ready");
            };
            assert!(value.info.as_ref().unwrap().get("runtimeOnly").is_none());
            let rust_keys: Vec<_> = value
                .info
                .as_ref()
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .collect();
            assert_ne!(
                rust_keys,
                case["infoKeys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>()
            );
            extra_property_diverged = true;
        }
        if case["name"] == "extra-blocked-property" {
            assert_eq!(
                case["result"]["ok"]["gateway"]["runtimeOnly"],
                json!({"retained":true})
            );
            let CoinbaseStarbaseParityResult::Blocked { gateway, .. } = &actual else {
                panic!("stop-limit case should remain blocked");
            };
            assert!(
                serde_json::to_value(gateway)
                    .unwrap()
                    .get("runtimeOnly")
                    .is_none()
            );
            extra_property_diverged = true;
        }
        if case["name"] == "info-identity" {
            // TS Order.info aliases the gateway; compare the Rust snapshot at call time.
        }
        if case["name"] == "blocked-key-order" {
            let CoinbaseStarbaseParityResult::Blocked { gateway, .. } = &actual else {
                panic!("stop-limit case should remain blocked");
            };
            let gateway_json = serde_json::to_value(gateway).unwrap();
            let rust_keys: Vec<_> = gateway_json
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            let ts_keys: Vec<_> = case["gatewayKeys"]
                .as_array()
                .unwrap()
                .iter()
                .map(Value::as_str)
                .map(Option::unwrap)
                .collect();
            assert_ne!(rust_keys, ts_keys, "blocked gateway key order is chosen");
        }
        let mut actual = json!({"ok": serde_json::to_value(actual).unwrap()});
        let mut expected = case["result"].clone();
        if case["name"] == "info-identity" {
            // TS Order.info aliases the gateway; compare the Rust snapshot at call time.
            expected["ok"]["value"]["info"]["price"] = case["input"]["price"].clone();
        }
        if case["name"] == "extra-own-property" {
            expected["ok"]["value"]["info"]
                .as_object_mut()
                .unwrap()
                .remove("runtimeOnly");
        }
        if case["name"] == "extra-blocked-property" {
            expected["ok"]["gateway"]
                .as_object_mut()
                .unwrap()
                .remove("runtimeOnly");
        }
        sort_keys(&mut actual);
        sort_keys(&mut expected);
        comparison::compare(&actual, &expected, &case.to_string());
    }
    assert!(
        extra_property_diverged,
        "extra runtime field difference must remain pinned"
    );
    assert!(
        numeric_bits_checked,
        "signed-zero/non-finite output bits must be measured before JSON"
    );
    assert!(
        stop_loss_bits_checked,
        "stop-market stop_loss bits must be measured before JSON"
    );
    assert!(
        info_identity_diverged,
        "TS gateway alias mutation must remain pinned"
    );
}
