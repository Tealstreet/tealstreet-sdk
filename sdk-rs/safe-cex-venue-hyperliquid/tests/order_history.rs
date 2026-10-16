use safe_cex_num::{JsJsonValue, js_number_string};
use safe_cex_venue_hyperliquid::order_history::{
    collapse_hyperliquid_order_history, map_hyperliquid_historical_order,
};
use serde_json::Value;

#[test]
fn actual_history_source_pins_transitions_and_pre_serialization_hits() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-history.json")).unwrap();
    for field in ["sideType", "orderTypeType"] {
        for kind in ["function", "object"] {
            assert!(
                cases.iter().any(|case| case[field] == kind),
                "pre-serialization inherited coverage {field}/{kind}"
            );
        }
    }
    assert!(
        cases.iter().any(|case| case["filledString"] == "Infinity"),
        "derived overflow remains observable before JSON serialization"
    );
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let input: Value = serde_json::from_str(raw).unwrap();
        if case["operation"] == "collapse" {
            let result = collapse_hyperliquid_order_history(input.as_array().unwrap());
            if let Some(message) = case["error"].as_str() {
                let error = result.unwrap_err();
                assert_eq!(error.message, message, "{raw}");
                assert_eq!(error.name, case["errorName"], "{raw}");
            } else {
                assert_eq!(
                    serde_json::to_value(result.unwrap()).unwrap(),
                    case["value"],
                    "transition and first-seen order {raw}"
                );
            }
            continue;
        }
        let result = map_hyperliquid_historical_order(&input, |coin| {
            Ok((coin == "BTC").then(|| "BTC-PERP".to_owned()))
        });
        if let Some(message) = case["error"].as_str() {
            let error = result.unwrap_err();
            assert_eq!(error.message, message, "{raw}");
            assert_eq!(error.name, case["errorName"], "{raw}");
        } else if ["sideType", "orderTypeType"]
            .iter()
            .any(|field| matches!(case[*field].as_str(), Some("function" | "object")))
        {
            // TS returns a usable record: callable fields disappear, prototype objects remain {}.
            // Rust's closed shared fields instead refuse the entire record; no serialized convergence.
            let error = result.unwrap_err();
            assert!(
                error.message.contains("cannot carry inherited"),
                "closed output refusal {raw}"
            );
            assert!(case["value"].is_object());
        } else {
            let value = result.unwrap();
            if let Some(order) = value.as_ref() {
                assert_eq!(
                    js_number_string(order.amount),
                    case["amountString"],
                    "amount pre-serialization {raw}"
                );
                assert_eq!(
                    js_number_string(order.filled),
                    case["filledString"],
                    "filled pre-serialization {raw}"
                );
            }
            assert_eq!(
                JsJsonValue::from(serde_json::to_value(value).unwrap()),
                JsJsonValue::from(case["value"].clone()),
                "mapped record {raw}"
            );
        }
    }
}

#[test]
fn actual_history_source_pins_live_alias_and_representation_refusal() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-history-boundaries.json")).unwrap();
    assert!(cases.iter().any(|case| case["sameReference"] == true));
    for escaped in ["\\ud800", "\\udfff"] {
        assert!(
            cases
                .iter()
                .any(|case| case["representationRefusal"] == true
                    && case["raw"].as_str().unwrap().contains(escaped)),
            "both surrogate witnesses are required {escaped}"
        );
    }
    for operation in ["map", "collapse"] {
        for escaped in ["\\ud800", "\\udfff"] {
            assert!(
                cases.iter().any(|case| case["operation"] == operation
                    && case["representationRefusal"] == true
                    && case["raw"].as_str().unwrap().contains(escaped)),
                "row-specific representation witness {operation}/{escaped}"
            );
        }
    }
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        if case["representationRefusal"] == true {
            assert!(
                serde_json::from_str::<Value>(raw).is_err(),
                "valid JSON escape exceeds the chosen UTF-8 representation"
            );
            assert!(case["sourceJson"].as_str().unwrap().contains("\\ud"));
        } else if case["operation"] == "collapse" {
            let input: Vec<Value> = serde_json::from_str(raw).unwrap();
            assert_eq!(
                serde_json::to_value(collapse_hyperliquid_order_history(&input).unwrap()).unwrap(),
                case["value"],
                "scalar Unicode, astral pairs and BOM remain representable"
            );
        } else if case["operation"] == "live-alias" {
            let mut input: Vec<Value> = serde_json::from_str(raw).unwrap();
            let output = collapse_hyperliquid_order_history(&input).unwrap();
            input[0]["status"] = Value::String("filled".into());
            assert_eq!(
                serde_json::to_value(output).unwrap(),
                case["before"],
                "Rust returns an owned snapshot"
            );
            assert_eq!(
                serde_json::to_value(input).unwrap(),
                case["after"],
                "TS live alias sees caller mutation"
            );
        } else {
            let input: Value = serde_json::from_str(raw).unwrap();
            let result = map_hyperliquid_historical_order(&input, |coin| {
                Ok((coin == "BTC").then(|| "BTC-PERP".to_owned()))
            });
            if let Some(message) = case["error"].as_str() {
                let error = result.unwrap_err();
                assert_eq!(error.name, case["errorName"]);
                assert_eq!(
                    error.message, message,
                    "source field coercion precedes closed-output refusal"
                );
            } else {
                assert_eq!(
                    JsJsonValue::from(serde_json::to_value(result.unwrap()).unwrap()),
                    JsJsonValue::from(case["value"].clone())
                );
            }
        }
    }
}

#[test]
fn live_history_oracle_and_rank_control_execute() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/order-history.cjs");
    for args in [vec![], vec!["--boundaries"], vec!["--helpers"]] {
        let output = std::process::Command::new("node")
            .arg(&script)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live history oracle must execute: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = std::process::Command::new("node")
        .arg(script)
        .arg("--control")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("actual order-history oracle drift"),
        "rank mutation must reach fixture assertion"
    );
}

#[test]
fn live_alias_source_control_cannot_hide_owned_copy() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/order-history.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .args(["--boundaries", "--alias-control"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("actual order-history oracle drift"),
        "source alias mutation must reach before/after witness"
    );
}

#[test]
fn actual_private_history_helpers_preserve_error_order() {
    use safe_cex_venue_hyperliquid::order_history::{
        is_later_state, map_status, status_rank, to_number, to_optional_number,
    };
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-history-helpers.json")).unwrap();
    assert!(
        cases
            .iter()
            .any(|case| case["raw"] == r#"[{"statusTimestamp":{"toString":null}},null]"#)
    );
    for operation in ["toNumber", "toOptionalNumber", "isLaterState"] {
        for escaped in ["\\ud800", "\\udfff"] {
            assert!(
                cases.iter().any(|case| case["operation"] == operation
                    && case["representationRefusal"] == true
                    && case["raw"].as_str().unwrap().contains(escaped)),
                "helper-specific representation witness {operation}/{escaped}"
            );
        }
    }
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        if case["representationRefusal"] == true {
            assert!(
                serde_json::from_str::<Value>(raw).is_err(),
                "shared string representation refusal"
            );
            assert!(
                case.get("error").is_none(),
                "source accepts these valid escaped strings"
            );
            continue;
        }
        let args: Vec<Value> = if case["operation"] == "map" {
            Vec::new()
        } else {
            serde_json::from_str(raw).unwrap()
        };
        let result = match case["operation"].as_str().unwrap() {
            "map" => map_hyperliquid_historical_order(
                &serde_json::from_str::<Value>(raw).unwrap(),
                |_| {
                    let mut error =
                        safe_cex_exchange::runtime::ExchangeError::new("resolver refused coin");
                    error.name = "TypeError".into();
                    Err(error)
                },
            )
            .map(|value| serde_json::to_value(value).unwrap()),
            "toNumber" => to_number(args.first()).map(|value| serde_json::to_value(value).unwrap()),
            "toOptionalNumber" => {
                to_optional_number(args.first()).map(|value| serde_json::to_value(value).unwrap())
            }
            "mapStatus" => Ok(serde_json::to_value(map_status(args[0].as_str().unwrap())).unwrap()),
            "statusRank" => Ok(Value::from(status_rank(args[0].as_str().unwrap()))),
            "isLaterState" => is_later_state(&args[0], &args[1]).map(Value::Bool),
            operation => panic!("unrecognized actual source helper {operation}"),
        };
        if let Some(message) = case["error"].as_str() {
            let error = result.unwrap_err();
            assert_eq!(error.name, case["errorName"], "{raw}");
            assert_eq!(error.message, message, "error-order {raw}");
        } else {
            assert_eq!(
                JsJsonValue::from(result.unwrap()),
                JsJsonValue::from(case.get("value").cloned().unwrap_or(Value::Null)),
                "direct helper {raw}"
            );
        }
    }
}

#[test]
fn historical_order_native_field_identity() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/history-field-identities.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8, "HISTORY-IDENTITY source total");
    let mut comparisons = 0;
    for index in 0..8 {
        let case = cases.get(index).expect("HISTORY-IDENTITY index coverage");
        let input: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let order = map_hyperliquid_historical_order(&input, |_| Ok(Some("BTC-PERP".into())))
            .unwrap()
            .unwrap();
        let expected = &case["value"];
        assert_eq!(
            order.id,
            expected["id"].as_str().unwrap(),
            "HISTORY native id"
        );
        assert_eq!(
            order.symbol,
            expected["symbol"].as_str().unwrap(),
            "HISTORY native symbol"
        );
        let side = match expected["side"].as_str().unwrap() {
            "buy" => safe_cex_types::OrderSide::Buy,
            "sell" => safe_cex_types::OrderSide::Sell,
            _ => panic!("HISTORY unexpected source side"),
        };
        let kind = match expected["type"].as_str().unwrap() {
            "limit" => safe_cex_types::OrderType::Limit,
            "market" => safe_cex_types::OrderType::Market,
            "stop_market" => safe_cex_types::OrderType::StopLoss,
            "take_profit_market" => safe_cex_types::OrderType::TakeProfit,
            _ => panic!("HISTORY unexpected source type"),
        };
        assert_eq!(
            order.side, side,
            "HISTORY native side enum before serialization"
        );
        assert_eq!(
            order.r#type, kind,
            "HISTORY native order type enum before serialization"
        );
        assert_eq!(
            order.status,
            safe_cex_types::OrderStatus::Closed,
            "HISTORY native status enum"
        );
        assert_eq!(expected["status"], "closed");
        assert_eq!(
            order.price.map(f64::to_bits),
            expected["price"].as_f64().map(f64::to_bits),
            "HISTORY native price"
        );
        assert_eq!(
            order.average_price.map(f64::to_bits),
            expected["averagePrice"].as_f64().map(f64::to_bits),
            "HISTORY native average price"
        );
        assert_eq!(
            order.amount.to_bits(),
            expected["amount"].as_f64().unwrap().to_bits(),
            "HISTORY native amount"
        );
        assert_eq!(
            order.filled.to_bits(),
            expected["filled"].as_f64().unwrap().to_bits(),
            "HISTORY native filled"
        );
        assert_eq!(
            order.reduce_only,
            expected["reduceOnly"].as_bool(),
            "HISTORY native reduce only"
        );
        assert_eq!(
            order.created_time.to_bits(),
            expected["createdTime"].as_f64().unwrap().to_bits(),
            "HISTORY native created time"
        );
        assert_eq!(
            order.updated_time.map(f64::to_bits),
            expected["updatedTime"].as_f64().map(f64::to_bits),
            "HISTORY native updated time"
        );
        comparisons += 12;
    }
    let mut input = vec![serde_json::json!({"order":{"oid":7},"status":"open"})];
    let owned = collapse_hyperliquid_order_history(&input).unwrap();
    input[0]["status"] = "filled".into();
    assert_eq!(
        owned[0]["status"].as_str(),
        Some("open"),
        "HISTORY native returned winner stays an owned snapshot"
    );
    assert_eq!(input[0]["status"].as_str(), Some("filled"));
    assert_eq!(comparisons, 96, "HISTORY-IDENTITY native comparison total");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-history.cjs"
        ))
        .arg("--field-identities")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source field identity supplement {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-history-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "HISTORY actual returned-unit/alias source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let receipt: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(receipt["cases"], 3);
}
