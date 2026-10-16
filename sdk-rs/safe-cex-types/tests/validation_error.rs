use safe_cex_types::{AccountValidationError, ValidateAccountResult};
use serde_json::{Value, json};

fn tagged(value: &Value) -> Value {
    match value {
        Value::Number(number) => {
            json!({"$bits":format!("{:016x}", number.to_string().parse::<f64>().unwrap().to_bits())})
        }
        Value::Array(values) => Value::Array(values.iter().map(tagged).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), tagged(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
#[test]
fn actual_validation_method_retains_non_string_error_values() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/validation-error.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 32);
    for row in rows {
        let msg: Value = serde_json::from_str(row["input"].as_str().unwrap()).unwrap();
        // Select the source truthiness result through its observed identity; no coercion.
        let expected = if row["sameValue"].as_bool().unwrap() {
            AccountValidationError::from_value(msg)
        } else if row["caught"].as_bool().unwrap() {
            AccountValidationError::from("outer error")
        } else {
            AccountValidationError::from("Failed to validate KuCoin Spot account")
        };
        assert_eq!(
            tagged(expected.as_value()),
            row["result"]["error"],
            "VALIDATION-ERROR actual method retains runtime value"
        );
        let data = json!({"error":expected.as_value()});
        let result: ValidateAccountResult = serde_json::from_value(data).unwrap();
        assert_eq!(
            tagged(result.error.as_value()),
            row["result"]["error"],
            "VALIDATION-ERROR typed result retains runtime value"
        );
        assert!(result.balance.is_none());
        assert_eq!(row["balanceCalls"], 0);
    }
    for invalid in [json!([]), json!(null), json!(false), json!(1), json!({})] {
        assert!(
            serde_json::from_value::<ValidateAccountResult>(invalid).is_err(),
            "VALIDATION-ERROR record remains map-only and requires error"
        );
    }
    assert!(
        serde_json::from_value::<ValidateAccountResult>(json!({"error":"x","balance":null}))
            .is_err(),
        "VALIDATION-ERROR optional balance remains non-null"
    );
}
