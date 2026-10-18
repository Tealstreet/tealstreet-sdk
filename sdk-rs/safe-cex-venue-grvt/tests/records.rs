#![cfg(not(target_arch = "wasm32"))]

use safe_cex_types::retained_json::RetainedJsonInput as Input;
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
    boundary: bool,
) -> Result<Value, serde_json::Error> {
    if boundary {
        let input = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>())?;
        let actual = input.clone().decode_native::<T>(false)?;
        actual.assert_retained(&input, std::any::type_name::<T>());
        // Keep the original direct qualified trait route as an independent
        // observation; only the eight numeric roots capture canonical text.
        let mut decoder = serde_json::Deserializer::from_str(raw);
        let bare = <T as serde::Deserialize>::deserialize(&mut decoder);
        let captured = matches!(
            std::any::type_name::<T>().rsplit("::").next().unwrap(),
            "GrvtTriggerMetadata"
                | "GrvtInstrumentLite"
                | "GrvtSignature"
                | "OrderPayload"
                | "GrvtBuilderCheckResult"
                | "GrvtBuilderInfo"
                | "GrvtWebSocketSubscription"
                | "GrvtWebSocketMessage"
        );
        assert_eq!(
            bare.is_ok(),
            captured,
            "GRVT direct Serde capture route {}",
            std::any::type_name::<T>()
        );
        if let Ok(actual) = bare {
            decoder.end()?;
            actual.assert_retained(&input, std::any::type_name::<T>());
        }
        // No wire value is claimed: all boundary fields were inspected above.
        return Ok(Value::Null);
    }
    // Qualified trait call prevents a permissive inherent decoder hiding the gate.
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let value = <T as serde::Deserialize>::deserialize(&mut decoder)?;
    decoder.end()?;
    value.assert_native(&serde_json::from_str(raw)?, std::any::type_name::<T>());
    serde_json::to_value(value)
}

fn record(name: &str, raw: &str, boundary: bool) -> Result<Value, serde_json::Error> {
    match name {
        "GrvtTriggerMetadata" => decode::<GrvtTriggerMetadata>(raw, boundary),
        "GrvtInstrument" => decode::<GrvtInstrument>(raw, boundary),
        "GrvtInstrumentLite" => decode::<GrvtInstrumentLite>(raw, boundary),
        "GrvtTicker" => decode::<GrvtTicker>(raw, boundary),
        "GrvtOrderBookLevel" => decode::<GrvtOrderBookLevel>(raw, boundary),
        "GrvtOrderBook" => decode::<GrvtOrderBook>(raw, boundary),
        "GrvtTrade" => decode::<GrvtTrade>(raw, boundary),
        "GrvtKline" => decode::<GrvtKline>(raw, boundary),
        "GrvtPosition" => decode::<GrvtPosition>(raw, boundary),
        "GrvtBalance" => decode::<GrvtBalance>(raw, boundary),
        "GrvtAccountSummary" => decode::<GrvtAccountSummary>(raw, boundary),
        "GrvtOrder" => decode::<GrvtOrder>(raw, boundary),
        "GrvtAuthRequest" => decode::<GrvtAuthRequest>(raw, boundary),
        "GrvtAuthResponse" => decode::<GrvtAuthResponse>(raw, boundary),
        "GrvtFill" => decode::<GrvtFill>(raw, boundary),
        "OrderLeg" => decode::<OrderLeg>(raw, boundary),
        "GrvtBuilderAuthorizationResult" => decode::<GrvtBuilderAuthorizationResult>(raw, boundary),
        "GrvtSignature" => decode::<GrvtSignature>(raw, boundary),
        "OrderPayload" => decode::<OrderPayload>(raw, boundary),
        "GrvtBuilderCheckResult" => decode::<GrvtBuilderCheckResult>(raw, boundary),
        "GrvtBuilderInfo" => decode::<GrvtBuilderInfo>(raw, boundary),
        "GrvtWebSocketSubscription" => decode::<GrvtWebSocketSubscription>(raw, boundary),
        "GrvtWebSocketMessage" => decode::<GrvtWebSocketMessage>(raw, boundary),
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
        let actual = record(name, raw, row["boundary"] == true);
        if row["boundary"].as_bool().unwrap() {
            boundary_count += 1;
            assert!(
                actual.is_ok(),
                "canonical typed String admission: {name} {raw}: {actual:?}"
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
    assert_eq!(
        boundary_count, 264,
        "unchanged lone-surrogate boundary corpus"
    );
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
        let actual = record(name, &serde_json::to_string(&input).unwrap(), false).unwrap();
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
            false,
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
