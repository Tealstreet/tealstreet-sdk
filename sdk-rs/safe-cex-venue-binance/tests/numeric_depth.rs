use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_binance::{binance_types as futures, binancepm_types as pm};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(case: &Value) {
    let depth = case["depth"].as_u64().unwrap();
    let raw = case["raw"].as_str().unwrap();
    // Construct the existing Value independently: raw Value parsing itself has
    // a depth budget and must not mask the public typed decoder's boundary.
    let mut value = case["base"].clone();
    let mut extra = Value::from(0);
    for _ in 0..depth {
        extra = Value::Array(vec![extra]);
    }
    value["unknown"] = extra;
    for route in 0..7 {
        let result: Result<T, serde_json::Error> = match route {
            0 => serde_json::from_str(raw),
            1 => serde_json::from_slice(raw.as_bytes()),
            2 => serde_json::from_value(value.clone()),
            3 => T::deserialize(&value),
            4 => js_from_str(raw),
            5 => js_from_value(value.clone()),
            6 => js_from_value_ref(&value),
            _ => unreachable!(),
        };
        let label = format!(
            "{}:{} depth={depth} route={route}",
            case["variant"], case["record"]
        );
        if depth == 140 {
            assert!(
                result.is_err(),
                "CHOSEN deep unknown field refusal: {label}"
            );
            assert!(
                result
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("recursion limit exceeded"),
                "depth error identity: {label}"
            );
        } else {
            let result =
                result.unwrap_or_else(|e| panic!("shallow unknown field accepted: {label}: {e}"));
            let output = serde_json::to_value(result).unwrap();
            assert_eq!(
                output[case["key"].as_str().unwrap()]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                1.0_f64.to_bits(),
                "known field retained: {label}"
            );
            assert!(
                output.get("unknown").is_none(),
                "typed record ignores unknown field: {label}"
            );
        }
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn numeric_capture_depth_cost_on_unknown_record_fields() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-depth.mjs"
        ))
        .output()
        .expect("Node actual-schema depth oracle");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        match (
            case["variant"].as_str().unwrap(),
            case["record"].as_str().unwrap(),
        ) {
            ("binance", "GET_POSITION_RISK_RESPONSE") => {
                check::<futures::GetPositionRiskResponseRow>(&case)
            }
            ("binancepm", "GET_POSITION_RISK_RESPONSE") => {
                check::<pm::GetPositionRiskResponseRow>(&case)
            }
            ("binance", "BinanceIncomeRecord") => check::<futures::BinanceIncomeRecord>(&case),
            ("binance", "BinanceIncomeHistoryOptions") => {
                check::<futures::BinanceIncomeHistoryOptions>(&case)
            }
            _ => panic!("unclassified source record {case}"),
        }
    }
}
