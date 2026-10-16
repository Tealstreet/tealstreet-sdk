use safe_cex_venue_bitget::{numeric::FiniteNumberInput, ticker::*};
use serde_json::{Value, json};

fn number(value: &Value) -> f64 {
    match value["$number"].as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        text => text.parse().unwrap(),
    }
}
fn decode(value: &Value) -> Option<Value> {
    if value.get("$undefined").is_some() {
        return None;
    }
    if let Some(text) = value.get("$number").and_then(Value::as_str) {
        // Wire-representable infinities retain the raw overflow lexeme. NaN has
        // no JSON spelling and must stay in the separate domain witness set.
        return Some(match text {
            "Infinity" => serde_json::from_str("1e400").unwrap(),
            "-Infinity" => serde_json::from_str("-1e400").unwrap(),
            "NaN" => panic!("NaN must remain an explicit JSON input-domain witness"),
            text => serde_json::from_str(text).unwrap(),
        });
    }
    Some(match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| decode(v).unwrap_or(Value::Null))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        value => value.clone(),
    })
}
fn check(oracle: &Value) {
    for source in oracle.as_array().unwrap() {
        let file = source["file"].as_str().unwrap();
        for case in source["scalar"].as_array().unwrap() {
            let v = &case["input"];
            let input = if v.get("$number").is_some() {
                FiniteNumberInput::Number(number(v))
            } else if let Some(s) = v.as_str() {
                FiniteNumberInput::String(s)
            } else {
                FiniteNumberInput::Other
            };
            let actual = to_finite_number(input);
            if case["output"].get("$undefined").is_some() {
                assert!(actual.is_none(), "{file} toFiniteNumber {}", case["name"]);
            } else {
                assert_eq!(
                    actual.unwrap().to_bits(),
                    number(&case["output"]).to_bits(),
                    "{file} toFiniteNumber bits {}",
                    case["name"]
                );
            }
        }
        for case in source["cases"].as_array().unwrap() {
            let input = decode(&case["input"]);
            let actual = build_ticker_update(input.as_ref());
            if let Some(error) = case["output"].get("error") {
                let actual = actual.unwrap_err();
                assert_eq!(
                    json!({"name":actual.name,"message":actual.message}),
                    *error,
                    "{file} buildTickerUpdate error {}",
                    case["name"]
                );
            } else {
                let actual = actual.unwrap();
                let expected = case["output"]["value"].as_object().unwrap();
                assert_eq!(
                    actual.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    expected.keys().map(String::as_str).collect::<Vec<_>>(),
                    "{file} buildTickerUpdate fields {}",
                    case["name"]
                );
                for (key, value) in actual {
                    assert_eq!(
                        value.to_bits(),
                        number(&expected[key]).to_bits(),
                        "{file} buildTickerUpdate bits {} {key}",
                        case["name"]
                    );
                }
            }
        }
        assert_eq!(
            source["excluded"].as_array().unwrap().len(),
            73,
            "ticker NaN input-domain witness inventory"
        );
        for case in source["excluded"].as_array().unwrap() {
            assert_eq!(
                case["output"]["value"],
                json!({}),
                "CHOSEN unrepresentable NaN blocks later aliases: {} {}",
                file,
                case["name"]
            );
        }
        for witness in source["utf16"].as_array().unwrap() {
            assert!(
                serde_json::from_str::<Value>(witness["json"].as_str().unwrap()).is_err(),
                "chosen UTF8 ticker ingress"
            );
            assert_eq!(
                witness["scalar"],
                if witness["json"].as_str().unwrap().starts_with("\"12") {
                    json!({"$number":"12"})
                } else {
                    json!({"$undefined":true})
                },
                "TS accepts surrogate string then applies parseFloat"
            );
            assert_eq!(
                witness["ticker"],
                if witness["json"].as_str().unwrap().starts_with("\"12") {
                    json!({"bid":{"$number":"12"}})
                } else {
                    json!({})
                },
                "TS ticker retains the numeric prefix before a lone surrogate"
            );
        }
    }
}
#[test]
fn frozen_ticker_helpers() {
    check(&serde_json::from_str(include_str!("ticker-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_ticker_helpers() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ticker source failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    check(&oracle);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("ticker-oracle.json")).unwrap(),
        "ticker source snapshot drift"
    );
}
