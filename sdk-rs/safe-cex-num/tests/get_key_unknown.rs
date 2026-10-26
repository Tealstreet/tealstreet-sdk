#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::get_key::{GetKeyTypeError, GetKeyUnknownValue, v_unknown, vn_unknown};
use safe_cex_num::numeric_ingress::js_value_as_f64;
use serde_json::{Value, json};
fn encode(result: Result<GetKeyUnknownValue<'_>, GetKeyTypeError>) -> Value {
    let hit = match result {
        Ok(hit) => hit,
        Err(error) => return json!({"error":{"name":error.name,"message":error.message}}),
    };
    let truthy = hit.is_truthy();
    let value = match hit {
        GetKeyUnknownValue::Undefined => json!({"kind":"undefined"}),
        GetKeyUnknownValue::Json(value) if value.is_number() => {
            json!({"kind":"number","bits":format!("{:016x}",js_value_as_f64(value).unwrap().to_bits()),"truthy":truthy})
        }
        GetKeyUnknownValue::Json(value) => json!({"kind":"json","value":value,"truthy":truthy}),
        GetKeyUnknownValue::Length(length) => {
            json!({"kind":"number","bits":format!("{:016x}",(length as f64).to_bits()),"truthy":truthy})
        }
        GetKeyUnknownValue::ObjectInherited(member) => {
            json!({"kind":"object-inherited","text":member.js_string(),"truthy":truthy})
        }
        GetKeyUnknownValue::ArrayInherited(member) => {
            json!({"kind":"array-inherited","name":member.name(),"text":member.js_string(),"truthy":truthy})
        }
        GetKeyUnknownValue::ArrayPrototype => json!({"kind":"array-prototype","truthy":truthy}),
    };
    json!({"ok":value})
}
#[test]
fn unknown_rhs_matches_actual_get_key_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/get_key_unknown.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "unknown RHS oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2961, "unknown RHS corpus count");
    for row in rows {
        let input: Option<Value> = row["raw"]
            .as_str()
            .map(|raw| serde_json::from_str(raw).unwrap());
        let keys: Vec<_> = row["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        let result = if row["mode"] == "v" {
            v_unknown(input.as_ref(), keys[0])
        } else {
            vn_unknown(input.as_ref(), &keys)
        };
        assert_eq!(
            encode(result),
            row["expected"],
            "UNKNOWN RHS source identity {row}"
        );
    }
    source_cardinality::for_each(
        corpus["refusals"].as_array().unwrap(),
        2,
        "unknown-refusals",
        |witness| {
            assert!(
                serde_json::from_str::<String>(witness["raw"].as_str().unwrap()).is_err(),
                "chosen UTF-8 representation refusal"
            );
            assert!(witness["keyError"].as_str().unwrap().contains("TypeError"));
            assert!(witness["rhsError"].as_str().unwrap().contains("TypeError"));
        },
    );
    let array = json!([null, 7]);
    assert!(matches!(
        v_unknown(Some(&array), "length"),
        Ok(GetKeyUnknownValue::Length(2))
    ));
    assert!(matches!(
        v_unknown(Some(&array), "__proto__"),
        Ok(GetKeyUnknownValue::ArrayPrototype)
    ));
}
#[test]
fn primitive_missing_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "unknown_rhs_matches_actual_get_key_source",
            "--nocapture",
        ])
        .env("GET_KEY_UNKNOWN_CONTROL", "primitive-missing")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "source mutation must assertion fail"
    );
    assert!(
        text.contains("UNKNOWN RHS source identity"),
        "named primitive control: {text}"
    );
}
