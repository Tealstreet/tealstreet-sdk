#[path = "support/input_identity.rs"]
mod input_identity;
#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::OrderSide;
use safe_cex_venue_weex::{public_trade::normalize_public_trade, utils::WeexUtilityInput};
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn public_trades_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/public-trade-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["nodeMajor"].is_number());
    assert!(corpus["header"]["node"].is_string());
    assert_eq!(corpus["header"]["nanBits"], "7ff8000000000000");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 79, "public_trade::source_cases");
    let names = [
        "distinct",
        "aliases",
        "precedence",
        "fallback_id",
        "nullish",
        "empty_id",
        "zero_price",
        "zero_qty",
        "zero_time",
        "empty",
        "null",
        "price_error",
        "time_error_before_guard",
        "quote_error",
        "numeric",
    ];
    let (mut compared, mut fields, mut successes, mut nulls, mut errors) = (0, 0, 0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        let name = if index < 15 {
            names[index].to_owned()
        } else {
            format!("flags_{}", index - 15)
        };
        assert_eq!(case["id"], name, "public_trade::source_identity");
        for route in 0..4 {
            let raw = case["raw"].as_str().unwrap();
            let input: WeexUtilityInput = match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            };
            if index >= 15 {
                let matrix = [
                    serde_json::json!(true),
                    serde_json::json!(false),
                    Value::Null,
                    serde_json::json!("true"),
                ]
                .map(safe_cex_types::retained_json::RetainedJsonInput::from);
                let slot = index - 15;
                assert_eq!(
                    [
                        input.as_value().get("buyer"),
                        input.as_value().get("m"),
                        input.as_value().get("isBuyerMaker")
                    ],
                    [
                        Some(&matrix[slot / 16]),
                        Some(&matrix[(slot / 4) % 4]),
                        Some(&matrix[slot % 4])
                    ],
                    "public_trade::flag_inputs"
                );
            }
            let label = format!("public_trade::{name}::route_{route}");
            match normalize_public_trade(&input, "ETH-USDT", |v| v) {
                Err(e) => {
                    assert_eq!(
                        e.name,
                        case["error"]["name"].as_str().unwrap(),
                        "{label}::error_name"
                    );
                    assert_eq!(
                        e.message,
                        case["error"]["message"].as_str().unwrap(),
                        "{label}::error_message"
                    );
                    errors += 1;
                }
                Ok(None) => {
                    assert!(case.get("result").is_some(), "{label}::unexpected_null");
                    assert_eq!(case["result"], Value::Null, "{label}::null");
                    nulls += 1;
                }
                Ok(Some(value)) => {
                    let expected = &case["result"];
                    assert_eq!(
                        expected.as_object().unwrap().len(),
                        4,
                        "{label}::source_result_keys"
                    );
                    assert_eq!(
                        expected["fields"].as_object().unwrap().len(),
                        4,
                        "{label}::source_field_count"
                    );
                    assert_eq!(value.id, expected["id"].as_str().unwrap(), "{label}::id");
                    assert_eq!(
                        value.symbol,
                        expected["symbol"].as_str().unwrap(),
                        "{label}::symbol"
                    );
                    let side = match expected["side"].as_str().unwrap() {
                        "buy" => OrderSide::Buy,
                        "sell" => OrderSide::Sell,
                        _ => panic!("invalid source side"),
                    };
                    assert_eq!(value.side, side, "{label}::side");
                    for (field, n) in [
                        ("price", value.price),
                        ("qty", value.qty),
                        (
                            "quoteQty",
                            value.quote_qty.expect("source always emits quoteQty"),
                        ),
                        ("timestamp", value.timestamp),
                    ] {
                        assert_eq!(
                            format!("{:016x}", n.to_bits()),
                            expected["fields"][field]["bits"].as_str().unwrap(),
                            "{label}::{field}"
                        );
                        fields += 1;
                    }
                    successes += 1;
                }
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 316, "public_trade::compared");
    assert_eq!(successes, 288, "public_trade::successes");
    assert_eq!(nulls, 12, "public_trade::nulls");
    assert_eq!(errors, 16, "public_trade::errors");
    assert_eq!(fields, 1152, "public_trade::fields");
    input_identity::check(&corpus, 0x19672b6fb5b0825d, "public_trade");
    let refusals = corpus["refusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 2, "public_trade::refusal_count");
    let expected = [
        r#"{"price":7,"qty":8,"time":1700000000123,"id":"\ud800"}"#.to_owned(),
        format!(
            "{{\"price\":7,\"qty\":8,\"time\":1700000000123,\"extra\":{}0{}}}",
            "[".repeat(150),
            "]".repeat(150)
        ),
    ];
    for (index, case) in refusals.iter().enumerate() {
        assert_eq!(
            case["raw"], expected[index],
            "public_trade::refusal_identity"
        );
        assert_eq!(
            case["accepted"], true,
            "public_trade::source_refusal_acceptance"
        );
        if index == 0 {
            let native = retained_input::admit_utf16(&expected[index]);
            assert!(
                normalize_public_trade(&native, "B/Q", |value| value).is_err(),
                "UTF-8 model id refusal remains visible"
            );
        } else {
            assert!(
                js_from_str::<WeexUtilityInput>(&expected[index]).is_err(),
                "public_trade::depth_refusal"
            );
        }
    }
    assert_eq!(
        corpus["callable"]["bits"], "401d000000000000",
        "public_trade::callable"
    );
}
