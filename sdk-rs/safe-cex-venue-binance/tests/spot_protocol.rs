use safe_cex_num::number_from_string;
use safe_cex_venue_binance::spot_protocol::*;
use serde_json::Value;

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn spot_auth_payload_matches_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/spot-protocol.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(corpus["userData"], BINANCE_SPOT_USER_DATA_REQUEST_ID);
    assert_eq!(
        corpus["heartbeat"].as_f64(),
        Some(BINANCE_SPOT_HEARTBEAT_REQUEST_ID)
    );
    use safe_cex_venue_binance::protocol_types::{ApiType, BinanceOrderBookApplyResult};
    check_union(&corpus["unions"]["ApiType"], |name| match name {
        "papi" => ApiType::Papi,
        "fapi" => ApiType::Fapi,
        _ => panic!("unclassified ApiType {name}"),
    });
    check_union(
        &corpus["unions"]["BinanceOrderBookApplyResult"],
        |name| match name {
            "applied" => BinanceOrderBookApplyResult::Applied,
            "ignored" => BinanceOrderBookApplyResult::Ignored,
            "gap" => BinanceOrderBookApplyResult::Gap,
            _ => panic!("unclassified orderbook result {name}"),
        },
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9450, "SOURCE-CARDINALITY spot_protocol total");
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let window = case["window"].as_str().unwrap();
        let actual = build_binance_spot_user_data_signature_payload(
            case["key"].as_str().unwrap(),
            number_from_string(case["timestamp"].as_str().unwrap()),
            (window != "undefined").then(|| number_from_string(window)),
        );
        assert_eq!(
            actual,
            case["expected"].as_str().unwrap(),
            "case {index}: {case}"
        );
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 9450,
        "NATIVE-CARDINALITY spot_protocol completed comparisons"
    );
    eprintln!("{} live Spot signature cases", cases.len());
}

fn check_union<T: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + PartialEq>(
    expected: &Value,
    native: impl Fn(&str) -> T,
) {
    use serde_json::json;
    for value in expected.as_array().unwrap() {
        let n = value.as_str().unwrap();
        let decoded = <T as serde::Deserialize>::deserialize(value.clone()).unwrap();
        assert_eq!(
            decoded,
            native(n),
            "decoded protocol union native identity {n}"
        );
        assert_eq!(serde_json::to_value(decoded).unwrap(), *value);
        for wrong in [
            json!({n:null}),
            json!([n]),
            json!(null),
            json!(false),
            json!(1),
            json!(format!(" {n}")),
            json!(format!("{n} ")),
            json!(format!("\u{feff}{n}")),
            json!(n.to_uppercase()),
        ] {
            assert!(
                <T as serde::Deserialize>::deserialize(wrong.clone()).is_err(),
                "accepted {wrong}"
            );
        }
    }
    let error = <T as serde::Deserialize>::deserialize(json!("UNKNOWN"))
        .unwrap_err()
        .to_string();
    let literals: Vec<_> = error
        .split('`')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .skip(1)
        .map(|(_, s)| s)
        .collect();
    assert_eq!(
        serde_json::to_value(literals).unwrap(),
        *expected,
        "accepted literal metadata"
    );
}
