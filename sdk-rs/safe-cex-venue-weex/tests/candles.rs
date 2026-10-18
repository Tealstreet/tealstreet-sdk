#[path = "support/cardinality.rs"]
mod cardinality;
#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_weex::utils::{WeexUtilityInput, normalize_candle};
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn candles_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/candle-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "candles");
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["nodeMajor"].is_number());
    assert!(corpus["header"]["node"].is_string());
    assert_eq!(corpus["header"]["nanBits"], "7ff8000000000000");
    let cases = corpus["cases"].as_array().unwrap();
    let ids = [
        "tuple",
        "object",
        "aliases",
        "precedence",
        "nullish",
        "empty",
        "short",
        "absent",
        "null",
        "primitive",
        "zero",
        "signed_zero",
        "numeric",
        "coercion",
        "error_order",
        "open_error",
        "date",
        "time_1e308",
        "time_1e18",
        "time_1e15",
        "time_1e12",
        "time_\"not a date\"",
        "time_\"2024-01-02T03:04:05\"",
    ];
    assert_eq!(cases.len(), 23, "candle::source_cases");
    assert_eq!(
        corpus["refusals"].as_array().unwrap().len(),
        2,
        "candle::source_refusals"
    );
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["id"].as_str().unwrap(),
            ids[index],
            "candle::source_identity"
        );
        for route in 0..4 {
            compared += 1;
            let input: Option<WeexUtilityInput> = case["raw"].as_str().map(|raw| match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let label = format!("candle::{}::route_{route}", case["id"].as_str().unwrap());
            match normalize_candle(input.as_ref(), |v| v) {
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
                }
                Ok(None) => {
                    assert!(case.get("result").is_some(), "{label}::unexpected_null");
                    assert_eq!(case["result"], Value::Null, "{label}::null");
                }
                Ok(Some(c)) => {
                    assert!(case.get("error").is_none(), "{label}::unexpected_success");
                    let fields = [
                        ("timestamp", c.timestamp),
                        ("open", c.open),
                        ("high", c.high),
                        ("low", c.low),
                        ("close", c.close),
                        ("volume", c.volume),
                    ];
                    assert_eq!(
                        case["result"].as_object().unwrap().len(),
                        fields.len(),
                        "{label}::keys"
                    );
                    for (field, value) in fields {
                        assert_eq!(
                            format!("{:016x}", value.to_bits()),
                            case["result"][field]["bits"].as_str().unwrap(),
                            "{label}::{field}"
                        );
                    }
                    assert!(c.last_side.is_none(), "{label}::last_side");
                }
            }
        }
    }
    assert_eq!(compared, 92, "candle::compared");
    assert_eq!(compared, 92, "cardinality::candles.rs::compared");
    let order_input: WeexUtilityInput =
        js_from_str(corpus["order"]["raw"].as_str().unwrap()).unwrap();
    let mut calls = 0;
    let error = normalize_candle(Some(&order_input), |value| {
        calls += 1;
        value
    })
    .unwrap_err();
    assert_eq!(
        corpus["order"]["events"],
        serde_json::json!(["date_parse"]),
        "candle::source_date_order"
    );
    assert_eq!(calls, 1, "candle::error_order::date_before_open");
    assert_eq!(
        error.name,
        corpus["order"]["error"]["name"].as_str().unwrap()
    );
    assert_eq!(
        error.message,
        corpus["order"]["error"]["message"].as_str().unwrap()
    );
    let refusal_inputs = [
        r#"{"open":"\ud800"}"#.to_owned(),
        format!("{{\"open\":{}\"7\"{}}}", "[".repeat(150), "]".repeat(150)),
    ];
    for (index, case) in corpus["refusals"].as_array().unwrap().iter().enumerate() {
        let raw = case["raw"].as_str().unwrap();
        assert_eq!(raw, refusal_inputs[index], "candle::refusal_identity");
        if index == 0 {
            let native = retained_input::admit_utf16(raw);
            let candle = normalize_candle(Some(&native), |value| value)
                .unwrap()
                .unwrap();
            assert_eq!(
                [
                    candle.timestamp,
                    candle.open,
                    candle.high,
                    candle.low,
                    candle.close,
                    candle.volume
                ]
                .map(f64::to_bits),
                [0.0f64.to_bits(); 6],
                "source zero candle fields survive UTF16 input"
            );
        } else {
            assert!(
                serde_json::from_str::<WeexUtilityInput>(raw).is_err(),
                "candle depth direct refusal"
            );
            assert!(
                js_from_str::<WeexUtilityInput>(raw).is_err(),
                "candle depth numeric refusal"
            );
        }
    }
    let mut deep = serde_json::json!("7");
    for _ in 0..150 {
        deep = serde_json::json!([deep]);
    }
    let deep = serde_json::json!({"open":deep});
    assert!(
        js_from_value::<WeexUtilityInput>(deep.clone()).is_err(),
        "candle_depth_owned"
    );
    assert!(
        js_from_value_ref::<WeexUtilityInput>(&deep).is_err(),
        "candle_depth_borrowed"
    );
    assert_eq!(
        corpus["callable"]["bits"], "401d000000000000",
        "candle_callable_source"
    );
    eprintln!(
        "candle cases {} x 4 routes; header {}",
        corpus["cases"].as_array().unwrap().len(),
        corpus["header"]
    );
}
