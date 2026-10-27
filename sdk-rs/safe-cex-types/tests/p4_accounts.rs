use safe_cex_types::accounts::*;
use serde_json::{Value, json};
use std::process::Command;
#[test]
#[ignore = "requires root node dependencies; live TypeScript account oracle"]
fn current_source_accounts() {
    let root = env!("CARGO_MANIFEST_DIR");
    let policy = Command::new("node")
        .arg(format!("{root}/scripts/account-policy.cjs"))
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        policy.status.success(),
        "{}",
        String::from_utf8_lossy(&policy.stderr)
    );
    let output = Command::new("node")
        .arg(format!("{root}/tests/p4_accounts_oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let result = match row["op"].as_str().unwrap() {
            "build" => build_exchange_account_config(input),
            "normalize" => normalize_exchange_account_config(input),
            "schema" => Ok(serde_json::to_value(get_exchange_account_credential_schema(
                input.as_str().unwrap(),
            ))
            .unwrap()),
            "names" => Ok(json!(get_supported_exchange_account_names())),
            "schemas" => Ok(json!(get_supported_exchange_account_credential_schemas())),
            "parse" => Ok(
                serde_json::to_value(parse_import_account_data(input.as_str().unwrap())).unwrap(),
            ),
            "valid" => Ok(json!(is_valid_exchange_account(input))),
            _ => unreachable!(),
        };
        let actual = match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        };
        assert!(
            equal(&actual, &row["expected"]),
            "account case {index}: {row}\nactual: {actual}"
        );
    }
    eprintln!("{} current-source account cases", rows.len());
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}

#[test]
fn import_representation_boundaries_are_explicit() {
    assert_eq!(
        parse_import_account_data(
            r#"{"exchange":"bybit","name":"x","apiKey":"k","apiSecret":"s","unknown":"\ud800"}"#
        )
        .error
        .as_deref(),
        Some("Invalid JSON format")
    );
    let nested = format!(
        "{{\"exchange\":\"bybit\",\"name\":\"x\",\"apiKey\":\"k\",\"apiSecret\":\"s\",\"unknown\":{}0{}}}",
        "[".repeat(130),
        "]".repeat(130)
    );
    assert_eq!(
        parse_import_account_data(&nested).error.as_deref(),
        Some("Invalid JSON format")
    );
}

#[test]
fn raw_import_integer_keys_follow_js_enumeration_recursively() {
    let raw = r#"{"exchange":"bybit","name":"x","apiKey":"k","apiSecret":"s","2":"two","1":"one","unknown":{"x":0,"4294967295":1,"02":2,"2":3,"1":4}}"#;
    let built = build_exchange_account_config(&serde_json::from_str(raw).unwrap()).unwrap();
    assert_eq!(
        built
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "1",
            "2",
            "exchange",
            "name",
            "apiKey",
            "apiSecret",
            "unknown",
            "tealstreetProxy"
        ]
    );
    assert_eq!(
        built["unknown"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["1", "2", "x", "4294967295", "02"]
    );
}
