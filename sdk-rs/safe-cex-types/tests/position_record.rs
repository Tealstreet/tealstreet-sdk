use safe_cex_types::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSourceAxis,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
fn compare(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect::<std::collections::BTreeSet<_>>()
            );
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
fn record() -> Value {
    json!({"key":"BTC:long:1","account":"a","symbol":"BTC","settleCurrency":"USDT","side":"long","positionSide":"both","state":"closed","closedSize":2,"avgEntryPrice":100,"realizedPnl":4,"fees":1,"funding":-1,"fundingAttributed":false,"provenance":"reported","sourceAxis":"closed-positions","closeCount":3})
}
fn union<T: for<'de> Deserialize<'de> + Serialize>(values: &Value) {
    let values = values.as_array().unwrap();
    for value in values {
        let item: T = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(item).unwrap(), *value);
        let literal = value.as_str().unwrap();
        for bad in [
            json!({literal:null}),
            json!([literal]),
            json!(null),
            json!(true),
            json!(1),
            json!(format!(" {literal}")),
            json!(format!("{literal} ")),
            json!(format!("\u{feff}{literal}")),
            json!("unknown"),
        ] {
            assert!(
                <T as Deserialize>::deserialize(bad).is_err(),
                "qualified string decoder rejects extra shape/spelling"
            );
        }
        let upper = json!(literal.to_uppercase());
        if !values.contains(&upper) {
            assert!(serde_json::from_value::<T>(upper).is_err());
        }
    }
}
#[test]
fn canonical_position_contract_matches_typescript_fields_and_closed_unions() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position_record_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let spec: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(spec["fields"].as_array().unwrap().len(), 24);
    union::<PositionCloseReason>(&spec["unions"]["PositionCloseReason"]);
    union::<PositionProvenance>(&spec["unions"]["PositionProvenance"]);
    union::<PositionSourceAxis>(&spec["unions"]["PositionSourceAxis"]);
    union::<PositionRecordLeg>(&spec["unions"]["PositionRecordLeg"]);
    union::<PositionRecordState>(&spec["unions"]["PositionRecordState"]);
    let minimal = record();
    let decoded: PositionRecord = serde_json::from_value(minimal.clone()).unwrap();
    compare(&serde_json::to_value(decoded).unwrap(), &minimal);
    let mut complete = minimal.clone();
    for field in spec["fields"].as_array().unwrap() {
        let name = field["name"].as_str().unwrap();
        if field["optional"] == true {
            complete[name] = match field["type"].as_str().unwrap() {
                "number" => json!(1.5),
                "string" => json!("venue"),
                "PositionCloseReason" => json!("user"),
                other => panic!("unexpected source optional {other}"),
            };
            let mut bad = minimal.clone();
            bad[name] = Value::Null;
            assert!(
                <PositionRecord as Deserialize>::deserialize(bad).is_err(),
                "optional non-null field {name}"
            );
        } else {
            let mut bad = minimal.clone();
            bad.as_object_mut().unwrap().remove(name);
            assert!(
                serde_json::from_value::<PositionRecord>(bad).is_err(),
                "required field {name}"
            );
        }
    }
    let native: PositionRecord = serde_json::from_value(complete.clone()).unwrap();
    compare(&serde_json::to_value(native).unwrap(), &complete);
    let positional = Value::Array(
        spec["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| complete[f["name"].as_str().unwrap()].clone())
            .collect(),
    );
    for bad in [
        positional,
        json!([]),
        Value::Null,
        json!(true),
        json!(1),
        json!("record"),
    ] {
        assert!(
            PositionRecord::deserialize(bad.clone()).is_err(),
            "public inherent-call record regression must remain strict"
        );
        assert!(
            <PositionRecord as Deserialize>::deserialize(bad).is_err(),
            "qualified record decoder rejects non-map"
        );
    }
}
