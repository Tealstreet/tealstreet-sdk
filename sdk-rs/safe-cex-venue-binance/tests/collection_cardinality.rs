use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_binance::{binance_types as futures, binancepm_types as pm};
use serde::de::DeserializeOwned;
use serde_json::Value;

fn decode<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(raw: &str, route: usize) -> T {
    match route {
        0 => serde_json::from_str(raw),
        1 => serde_json::from_slice(raw.as_bytes()),
        2 => serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()),
        3 => T::deserialize(&serde_json::from_str::<Value>(raw).unwrap()),
        4 => js_from_str(raw),
        5 => js_from_value(serde_json::from_str::<Value>(raw).unwrap()),
        6 => js_from_value_ref(&serde_json::from_str::<Value>(raw).unwrap()),
        _ => unreachable!(),
    }
    .unwrap()
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn position_risk_aliases_preserve_collection_cardinality() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/field-identity.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4, "COLLECTION-CARDINALITY source records");
    let mut comparisons = 0;
    for variant in ["binance", "binancepm"] {
        let matching: Vec<_> = cases
            .iter()
            .filter(|case| {
                case["record"] == "GET_POSITION_RISK_RESPONSE" && case["variant"] == variant
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "COLLECTION-CARDINALITY source alias identity {variant}"
        );
        let case = matching[0];
        let mut first: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let mut second = first.clone();
        first["symbol"] = Value::String("first-risk-symbol".into());
        second["symbol"] = Value::String("second-risk-symbol".into());
        let raw = serde_json::to_string(&[first, second]).unwrap();
        for route in 0..7 {
            let symbols: Vec<String> = match variant {
                "binance" => decode::<futures::GetPositionRiskResponse>(&raw, route)
                    .into_iter()
                    .map(|row| row.symbol)
                    .collect(),
                "binancepm" => decode::<pm::GetPositionRiskResponse>(&raw, route)
                    .into_iter()
                    .map(|row| row.symbol)
                    .collect(),
                _ => panic!("unclassified position risk alias {variant}"),
            };
            assert_eq!(
                symbols.len(),
                2,
                "COLLECTION-CARDINALITY native length {variant} route {route}"
            );
            for index in 0..2 {
                let symbol = symbols
                    .get(index)
                    .expect("COLLECTION-CARDINALITY independent native index coverage");
                let expected = if index == 0 {
                    "first-risk-symbol"
                } else {
                    "second-risk-symbol"
                };
                assert_eq!(
                    symbol, expected,
                    "COLLECTION-CARDINALITY native identity {variant} route {route} index {index}"
                );
                comparisons += 1;
            }
        }
    }
    assert_eq!(
        comparisons, 28,
        "COLLECTION-CARDINALITY completed native comparisons"
    );
}
