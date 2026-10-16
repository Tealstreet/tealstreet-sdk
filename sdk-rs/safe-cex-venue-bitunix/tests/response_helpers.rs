use safe_cex_num::numeric_ingress::{js_deserialize_passthrough, js_value_as_f64};
use safe_cex_venue_bitunix::response_helpers::{get_response_candidates, is_record};
use serde_json::{Value, json};
use std::process::Command;

fn observe(value: &Value) -> Value {
    match value {
        Value::Null => json!({"type":"null"}),
        Value::Bool(v) => json!({"type":"boolean","value":v}),
        Value::String(v) => json!({"type":"string","units":v.encode_utf16().collect::<Vec<_>>()}),
        Value::Number(_) => {
            let n = js_value_as_f64(value).unwrap();
            json!({"type":"number","bits":format!("{:016x}", if n.is_nan() { 0x7ff8_0000_0000_0000 } else { n.to_bits() })})
        }
        Value::Array(rows) => {
            json!({"type":"array","items":rows.iter().map(observe).collect::<Vec<_>>()})
        }
        Value::Object(rows) => {
            let mut keys = rows.keys().collect::<Vec<_>>();
            keys.sort();
            json!({"type":"object","entries":keys.into_iter().map(|k|json!([k,observe(&rows[k])])).collect::<Vec<_>>()})
        }
    }
}

fn location(target: &Value, current: &Value, at: String) -> Option<String> {
    if std::ptr::eq(target, current) {
        return Some(at);
    }
    match current {
        Value::Object(values) => values
            .iter()
            .find_map(|(k, v)| location(target, v, format!("{at}/{k}"))),
        Value::Array(values) => values
            .iter()
            .enumerate()
            .find_map(|(k, v)| location(target, v, format!("{at}/{k}"))),
        _ => None,
    }
}

#[test]
fn actual_typescript_response_helpers() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/response-helpers-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "RESPONSE actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 64, "RESPONSE source total");
    let mut expected_counts = vec![
        0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 8, 3, 3, 3, 5, 1, 4, 1,
    ];
    for _ in 0..7 {
        for depth in 1..=6 {
            expected_counts.push((depth + 1).min(5));
        }
    }
    let mut cases = 0;
    let mut comparisons = 0;
    let mut refused = 0;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row["outputs"].as_array().unwrap().len(),
            expected_counts[index],
            "RESPONSE source per-case count {index}"
        );
        assert_eq!(
            row["paths"].as_array().unwrap().len(),
            expected_counts[index],
            "RESPONSE source identity count {index}"
        );
        let input = match row["raw"].as_str() {
            None => None,
            Some(raw) => {
                match js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        assert_eq!(raw, "\"\\ud800\"", "RESPONSE only declared refusal");
                        assert_eq!(row["record"], false, "RESPONSE surrogate is not record");
                        // Native UTF-8 cannot represent the source candidate; the
                        // raw JSON escape is retained and the boundary is explicit.
                        assert_eq!(
                            row["outputs"],
                            json!([{ "type":"string", "units":[55296] }]),
                            "RESPONSE source surrogate candidate"
                        );
                        refused += 1;
                        continue;
                    }
                }
            }
        };
        assert_eq!(
            is_record(input.as_ref()),
            row["record"].as_bool().unwrap(),
            "RESPONSE native isRecord {index}"
        );
        let native = get_response_candidates(input.as_ref());
        assert_eq!(
            native.len(),
            expected_counts[index],
            "RESPONSE independent native length {index}"
        );
        assert_eq!(
            native.len(),
            row["outputs"].as_array().unwrap().len(),
            "RESPONSE expected/native length {index}"
        );
        for (i, value) in native.iter().enumerate() {
            assert_eq!(
                observe(value),
                row["outputs"][i],
                "RESPONSE native candidate {index}:{i}"
            );
            let path = if value.is_object() || value.is_array() {
                location(value, input.as_ref().unwrap(), "$".into())
            } else {
                None
            };
            assert_eq!(
                json!(path),
                row["paths"][i],
                "RESPONSE native object identity {index}:{i}"
            );
            comparisons += 1;
        }
        cases += 1;
    }
    assert_eq!(cases, 63, "RESPONSE native case comparisons");
    assert_eq!(comparisons, 207, "RESPONSE native output comparisons");
    assert_eq!(refused, 1, "RESPONSE UTF8 refusals");
}
