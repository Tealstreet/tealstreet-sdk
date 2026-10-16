#![cfg(not(target_arch = "wasm32"))]
use safe_cex_num::numeric_ingress::JsNumberOutput;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_grvt::records::*;
use serde::Deserialize;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

#[test]
fn public_numeric_records_preserve_node_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/numeric-records.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "numeric oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 2354, "GRVT numeric record corpus");
    for row in rows {
        match row["name"].as_str().unwrap() {
            "GrvtInstrumentLite" => {
                check_numeric::<GrvtInstrumentLite>(&row, |r, field| match field {
                    "bd" => r.bd.unwrap(),
                    "qd" => r.qd.unwrap(),
                    "fi" => r.fi.unwrap(),
                    _ => panic!("unknown field"),
                })
            }
            "GrvtTriggerMetadata" => {
                check_numeric::<GrvtTriggerMetadata>(&row, |r, _| r.trigger_price.unwrap())
            }
            "GrvtSignature" => check_numeric::<GrvtSignature>(&row, |r, field| match field {
                "v" => r.v,
                "nonce" => r.nonce,
                _ => panic!("unknown field"),
            }),
            "OrderPayload" => check_numeric::<OrderPayload>(&row, |r, _| r.nonce),
            "GrvtBuilderCheckResult" => {
                check_numeric::<GrvtBuilderCheckResult>(&row, |r, _| r.max_fee_rate_raw.unwrap())
            }
            "GrvtBuilderInfo" => check_numeric::<GrvtBuilderInfo>(&row, |r, _| r.builder_fee_int),
            "GrvtWebSocketSubscription" => {
                check_numeric::<GrvtWebSocketSubscription>(&row, |r, _| r.id)
            }
            "GrvtWebSocketMessage" => check_numeric::<GrvtWebSocketMessage>(&row, |r, _| {
                let error = r.error.as_ref().unwrap();
                let params = r
                    .params
                    .as_ref()
                    .unwrap()
                    .clone()
                    .decode::<std::collections::BTreeMap<String, f64>>()
                    .unwrap();
                let result = r
                    .result
                    .as_ref()
                    .unwrap()
                    .clone()
                    .decode::<Vec<f64>>()
                    .unwrap();
                let data = error
                    .data
                    .as_ref()
                    .unwrap()
                    .clone()
                    .decode::<std::collections::BTreeMap<String, f64>>()
                    .unwrap();
                for number in [params["n"], result[0], data["n"]] {
                    assert_eq!(
                        number.to_bits(),
                        error.code.to_bits(),
                        "WebSocket opaque payload retains Number bits"
                    );
                }
                error.code
            }),
            name => panic!("unclassified numeric record {name}"),
        }
    }
}
fn check_numeric<T: DeserializeOwned + Serialize + JsNumberOutput>(
    row: &Value,
    number: impl Fn(&T, &str) -> f64,
) {
    let raw = row["raw"].as_str().unwrap();
    let value: Value = serde_json::from_str(raw).unwrap();
    let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
    let entries = [
        serde_json::from_str::<T>(raw),
        serde_json::from_value(value.clone()),
        T::deserialize(&value),
        js_from_str(raw),
        js_from_value(value.clone()),
        js_from_value_ref(&value),
    ];
    for (path, record) in entries.into_iter().enumerate() {
        let record = record
            .unwrap_or_else(|error| panic!("GRVT numeric decode path {path}: {raw}: {error}"));
        assert_eq!(
            number(&record, row["field"].as_str().unwrap()).to_bits(),
            expected,
            "GRVT numeric bits path {path}: {raw}"
        );
    }
}

#[test]
fn reverted_candidate_fixture_is_not_a_shipped_regression_guard() {
    // The earlier map-only candidate was never committed. Pin its clean numeric
    // leaf behavior alongside the Value-first rebuffer that rejected that candidate.
    #[derive(Deserialize)]
    struct Candidate {
        bd: f64,
        qd: f64,
        fi: f64,
    }
    let raw = r#"{"bd":-0,"qd":-0,"fi":-0}"#;
    let candidate: Candidate = serde_json::from_str(raw).unwrap();
    for n in [candidate.bd, candidate.qd, candidate.fi] {
        assert_eq!(
            n.to_bits(),
            (-0.0_f64).to_bits(),
            "pre-Value-first candidate fixture"
        );
    }
    let value: Value = serde_json::from_str(raw).unwrap();
    let rebuffered = Value::deserialize(value).unwrap();
    let rejected: Candidate = serde_json::from_value(rebuffered).unwrap();
    for n in [rejected.bd, rejected.qd, rejected.fi] {
        assert_eq!(
            n.to_bits(),
            0.0_f64.to_bits(),
            "negative control: rejected Value-first candidate loses integer -0"
        );
    }
}
