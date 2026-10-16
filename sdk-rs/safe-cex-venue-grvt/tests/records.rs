#![cfg(not(target_arch = "wasm32"))]

use safe_cex_venue_grvt::records::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
#[path = "support/enum_identity.rs"]
mod enum_identity;
#[path = "support/record_identity.rs"]
mod record_identity;
use record_identity::NativeIdentity;

fn decode<T: DeserializeOwned + Serialize + NativeIdentity>(
    raw: &str,
) -> Result<Value, serde_json::Error> {
    // Qualified trait call prevents a permissive inherent decoder hiding the gate.
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = <T as serde::Deserialize>::deserialize(&mut decoder)?;
    decoder.end()?;
    value.assert_native(&serde_json::from_str(raw)?, std::any::type_name::<T>());
    serde_json::to_value(value)
}

fn record(name: &str, raw: &str) -> Result<Value, serde_json::Error> {
    match name {
        "GrvtTriggerMetadata" => decode::<GrvtTriggerMetadata>(raw),
        "GrvtInstrument" => decode::<GrvtInstrument>(raw),
        "GrvtInstrumentLite" => decode::<GrvtInstrumentLite>(raw),
        "GrvtTicker" => decode::<GrvtTicker>(raw),
        "GrvtOrderBookLevel" => decode::<GrvtOrderBookLevel>(raw),
        "GrvtOrderBook" => decode::<GrvtOrderBook>(raw),
        "GrvtTrade" => decode::<GrvtTrade>(raw),
        "GrvtKline" => decode::<GrvtKline>(raw),
        "GrvtPosition" => decode::<GrvtPosition>(raw),
        "GrvtBalance" => decode::<GrvtBalance>(raw),
        "GrvtAccountSummary" => decode::<GrvtAccountSummary>(raw),
        "GrvtOrder" => decode::<GrvtOrder>(raw),
        "GrvtAuthRequest" => decode::<GrvtAuthRequest>(raw),
        "GrvtAuthResponse" => decode::<GrvtAuthResponse>(raw),
        "GrvtFill" => decode::<GrvtFill>(raw),
        "OrderLeg" => decode::<OrderLeg>(raw),
        "GrvtBuilderAuthorizationResult" => decode::<GrvtBuilderAuthorizationResult>(raw),
        "GrvtSignature" => decode::<GrvtSignature>(raw),
        "OrderPayload" => decode::<OrderPayload>(raw),
        "GrvtBuilderCheckResult" => decode::<GrvtBuilderCheckResult>(raw),
        "GrvtBuilderInfo" => decode::<GrvtBuilderInfo>(raw),
        "GrvtWebSocketSubscription" => decode::<GrvtWebSocketSubscription>(raw),
        "GrvtWebSocketMessage" => decode::<GrvtWebSocketMessage>(raw),
        _ => panic!("unclassified record {name}"),
    }
}

#[test]
fn protocol_records_match_typescript_accepted_sets() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/records.cjs"))
        .output()
        .expect("run TypeScript record oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let count = rows.len();
    let mut boundary_count = 0;
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let raw = row["raw"].as_str().unwrap();
        let actual = record(name, raw);
        if row["boundary"].as_bool().unwrap() {
            boundary_count += 1;
            assert!(
                actual.is_err(),
                "shared UTF-8 representation refusal: {name} {raw}"
            );
            continue;
        }
        assert_eq!(
            actual.is_ok(),
            row["accepted"].as_bool().unwrap(),
            "GRVT record accepted set: {name} {} {raw}: {actual:?}",
            row["label"]
        );
        if let Ok(value) = actual {
            assert_eq!(
                value,
                serde_json::from_str::<Value>(raw).unwrap(),
                "GRVT record field preservation: {name}"
            );
        }
    }
    assert_eq!(count, 1782, "record corpus census");
    assert_eq!(boundary_count, 264, "lone-surrogate refusal corpus");
}

#[test]
fn actual_source_numeric_type_mutation_is_detected() {
    for (mutation, record) in [
        ("trigger-number", "GrvtTriggerMetadata"),
        ("lite-number", "GrvtInstrumentLite"),
        ("signature-number", "GrvtSignature"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "protocol_records_match_typescript_accepted_sets",
                "--nocapture",
            ])
            .env("GRVT_RECORD_MUTATION", mutation)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(101),
            "numeric source control must assertion-fail"
        );
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            log.contains(&format!("GRVT record accepted set: {record}")),
            "numeric source mutation failed for wrong reason: {log}"
        );
    }
}

#[test]
fn collection_cardinality_supplement() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/records.cjs"))
        .output()
        .expect("run TypeScript record oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 1782, "GRVT-CARDINALITY source total");
    let mut compared = 0;
    for (name, field) in [
        ("GrvtOrderBook", "bids"),
        ("GrvtOrderBook", "asks"),
        ("GrvtAccountSummary", "balances"),
        ("GrvtInstrumentLite", "v"),
        ("OrderPayload", "legs"),
    ] {
        let sources: Vec<_> = rows
            .iter()
            .filter(|row| row["name"] == name && row["label"] == "all-fields")
            .collect();
        assert_eq!(sources.len(), 1, "GRVT-CARDINALITY source record {name}");
        assert_eq!(
            sources[0]["accepted"], true,
            "GRVT-CARDINALITY source admission {name}"
        );
        let mut input: Value = serde_json::from_str(sources[0]["raw"].as_str().unwrap()).unwrap();
        let values = input[field].as_array_mut().unwrap();
        assert_eq!(
            values.len(),
            1,
            "GRVT-CARDINALITY source field {name}.{field}"
        );
        values.push(values[0].clone());
        let actual = record(name, &serde_json::to_string(&input).unwrap()).unwrap();
        assert_eq!(
            actual[field].as_array().unwrap().len(),
            2,
            "GRVT-CARDINALITY retained field {name}.{field}"
        );
        compared += 1;
    }
    assert_eq!(compared, 5, "GRVT-CARDINALITY compared fields");
}

#[test]
fn opaque_collection_cardinality_supplement() {
    let mut compared = 0;
    for payload in [
        serde_json::json!(["first", "second"]),
        serde_json::json!({"first": "left", "second": "right"}),
    ] {
        let input = serde_json::json!({
            "jsonrpc": "2.0", "params": payload, "result": payload,
            "error": {"code": 1, "message": "error", "data": payload}
        });
        let actual = record(
            "GrvtWebSocketMessage",
            &serde_json::to_string(&input).unwrap(),
        )
        .unwrap();
        for pointer in ["/params", "/result", "/error/data"] {
            let value = actual.pointer(pointer).unwrap();
            let len = match value {
                Value::Array(values) => values.len(),
                Value::Object(values) => values.len(),
                _ => panic!("GRVT-OPAQUE-CARDINALITY collection {pointer}"),
            };
            assert_eq!(len, 2, "GRVT-OPAQUE-CARDINALITY returned count {pointer}");
            assert_eq!(
                value, &payload,
                "GRVT-OPAQUE-CARDINALITY entire payload {pointer}"
            );
            compared += len;
        }
    }
    assert_eq!(compared, 12, "GRVT-OPAQUE-CARDINALITY compared members");
}
