use safe_cex_store::ws_diagnostics::{redact_json_value, redact_structured_json};
use serde_json::{Value, json};

#[test]
fn redaction_ordinary_assignment_omits_prototype_key() {
    let mut cases = 0;
    for prototype in [
        json!("primitive"),
        Value::Null,
        json!({"hidden":1}),
        json!([]),
    ] {
        let input = json!({"__proto__":prototype,"constructor":"own","toString":"own","ok":2});
        for depth in [0, 12, 13] {
            let result = redact_json_value(&input, depth, false);
            assert_eq!(
                result.as_object().unwrap().len(),
                if depth > 12 { 4 } else { 3 },
                "REDACTOR native own-key count/{depth}"
            );
            assert_eq!(
                result.get("__proto__").is_some(),
                depth > 12,
                "REDACTOR native setter own-key/{depth}"
            );
            assert_eq!(result["constructor"], "own");
            assert_eq!(result["toString"], "own");
            assert_eq!(result["ok"], 2);
            cases += 1;
        }
    }
    assert_eq!(cases, 12, "REDACTOR independent value/depth floor");
    for prototype in [
        json!("primitive"),
        Value::Null,
        json!({"hidden":1}),
        json!([]),
    ] {
        let input = json!({"__proto__":prototype,"constructor":"own","toString":"own","ok":2});
        let rendered = redact_structured_json(&input.to_string()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&rendered).unwrap(),
            json!({"constructor":"own","toString":"own","ok":2}),
            "REDACTOR public wire effect"
        );
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/redaction-behavior-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "REDACTOR actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
