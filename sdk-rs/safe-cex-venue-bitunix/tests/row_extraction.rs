use safe_cex_num::{
    ProtoMember, TableHit,
    numeric_ingress::{js_deserialize_passthrough, js_value_as_f64},
};
use safe_cex_venue_bitunix::row_extraction::{
    extract_account_row, extract_position_rows, get_first_value,
};
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

fn first_observe(hit: TableHit<&Value>) -> Value {
    match hit {
        TableHit::Missing => json!({"type":"undefined"}),
        TableHit::Mapped(v) => observe(v),
        TableHit::Inherited(p) => {
            json!({"type":if p==ProtoMember::Prototype {"object"} else {"function"},"string":p.js_string()})
        }
    }
}
#[test]
fn actual_typescript_row_extraction() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/row-extraction-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ROWS actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        corpus["first"].as_array().unwrap().len(),
        96,
        "FIRST source total"
    );
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        68,
        "ROWS source total"
    );
    let mut first_count = 0;
    let mut first_refused = 0;
    for (index, row) in corpus["first"].as_array().unwrap().iter().enumerate() {
        let raw = row["raw"].as_str().unwrap();
        let value = match js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)) {
            Ok(v) => v,
            Err(_) => {
                assert_eq!(raw, r#"{"a":"\ud800"}"#, "FIRST UTF8 refusal");
                if index % 8 < 2 {
                    assert_eq!(
                        row["result"],
                        json!({"type":"string","units":[55296]}),
                        "FIRST source surrogate value"
                    );
                }
                first_refused += 1;
                continue;
            }
        };
        let keys = row["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            first_observe(get_first_value(value.as_object().unwrap(), &keys)),
            row["result"],
            "FIRST native selected value {index}"
        );
        first_count += 1;
    }
    assert_eq!(first_count, 88, "FIRST native comparisons");
    assert_eq!(first_refused, 8, "FIRST refusal count");
    let expected_positions = [0, 0, 0, 0, 0, 2, 1, 1, 2, 0, 1, 2, 0, 0, 2, 2, 1];
    let mut account_count = 0;
    let mut position_cases = 0;
    let mut position_count = 0;
    let mut refused = 0;
    for (index, row) in corpus["rows"].as_array().unwrap().iter().enumerate() {
        let expected = expected_positions[index / 4];
        assert_eq!(
            row["positions"]["items"].as_array().unwrap().len(),
            expected,
            "ROWS source position count {index}"
        );
        let value = match row["raw"].as_str() {
            None => None,
            Some(raw) => {
                match js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        assert_eq!(
                            raw, r#"{"data":[{"marginCoin":"USDT","symbol":"\ud800"}]}"#,
                            "ROWS UTF8 refusal"
                        );
                        assert_eq!(
                            row["positions"]["items"][0]["entries"][1][1]["units"],
                            json!([55296]),
                            "ROWS source surrogate value"
                        );
                        assert_eq!(
                            row["account"]["ok"], row["positions"]["items"][0],
                            "ROWS source surrogate account"
                        );
                        refused += 1;
                        continue;
                    }
                }
            }
        };
        let account = match extract_account_row(value.as_ref(), row["coin"].as_str().unwrap()) {
            Ok(Some(v)) => json!({"ok":observe(v)}),
            Ok(None) => json!({"ok":{"type":"object","entries":[]}}),
            Err(_) => json!({"error":"TypeError"}),
        };
        assert_eq!(
            account, row["account"],
            "ACCOUNT native selected row {index}"
        );
        account_count += 1;
        let positions = extract_position_rows(value.as_ref());
        assert_eq!(
            positions.len(),
            expected,
            "POSITIONS independent native length {index}"
        );
        assert_eq!(
            positions.len(),
            row["positions"]["items"].as_array().unwrap().len(),
            "POSITIONS expected native length {index}"
        );
        for (i, p) in positions.iter().enumerate() {
            assert_eq!(
                observe(p),
                row["positions"]["items"][i],
                "POSITIONS native selected row {index}:{i}"
            );
            position_count += 1;
        }
        position_cases += 1;
    }
    assert_eq!(account_count, 64, "ACCOUNT native comparisons");
    assert_eq!(position_cases, 64, "POSITIONS native case comparisons");
    assert_eq!(position_count, 52, "POSITIONS native row comparisons");
    assert_eq!(refused, 4, "ROWS refusal count");
}
