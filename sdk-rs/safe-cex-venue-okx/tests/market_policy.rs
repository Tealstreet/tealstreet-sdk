#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::JsCoercionError;
use safe_cex_types::Market;
use safe_cex_venue_okx::exchange::*;
use serde_json::{Value, json};

#[test]
#[ignore = "requires the maintained TypeScript compiler; mandatory in CI"]
fn maintained_typescript_market_instrument_and_index_policy() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/market-policy-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the source oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 401);
    cardinality::assert_groups(
        &cases,
        "market_policy operations",
        |case| case["op"].as_str().unwrap().to_owned(),
        &[
            ("instruments", 1),
            ("supported", 40),
            ("type", 144),
            ("symbol", 96),
            ("index", 117),
            ("markets", 3),
        ],
    );
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let actual: Result<Value, JsCoercionError> = match case["op"].as_str().unwrap() {
            "instruments" => Ok(json!(get_instrument_types())),
            "supported" => Ok(json!(is_supported_market_instrument(
                input.as_object().unwrap()
            ))),
            "type" => get_market_instrument_type(input.as_object().unwrap()).map(|v| json!(v)),
            "symbol" => format_market_symbol(input.as_object().unwrap()).map(Value::String),
            "index" => {
                get_index_ticker_inst_id(&serde_json::from_value::<Market>(input.clone()).unwrap())
                    .map(Value::String)
            }
            "markets" => {
                let markets: Vec<Market> =
                    serde_json::from_value(input["markets"].clone()).unwrap();
                get_markets_for_index_ticker_inst_id(&markets, input["instId"].as_str().unwrap())
                    .map(|matching| {
                        // Source filter returns the original object references in input order.
                        for m in &matching {
                            assert!(markets.iter().any(|original| std::ptr::eq(original, *m)));
                        }
                        json!(matching)
                    })
            }
            op => panic!("unknown operation {op}"),
        };
        if let Some(expected) = case.get("error") {
            let error = actual.unwrap_err();
            assert_eq!(error.name, expected["name"], "case {index}");
            assert_eq!(error.message, expected["message"], "case {index}");
        } else {
            assert_eq!(
                canonical(actual.unwrap()),
                canonical(case["expected"].clone()),
                "case {index}: {case}"
            );
        }
    }
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            serde_json::from_str(&safe_cex_num::js_number_string(number.as_f64().unwrap())).unwrap()
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical(value)))
                .collect(),
        ),
        other => other,
    }
}
