//! Producer/model seam gate: the same array contains keyed real and unkeyed fake rows.
use safe_cex_types::DisplayedPosition;
use serde_json::Value;
fn equal(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect()
            );
            for (k, v) in a {
                equal(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "executes the current TypeScript displayed-positions producer"]
fn real_and_fake_producer_rows_round_trip_through_the_shared_model() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/displayed_position_producer.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows: Vec<DisplayedPosition> = serde_json::from_value(source.clone()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].key.as_deref(), Some("account:BTCUSDT:long"));
    assert_eq!(rows[1].key, None);
    let wire = serde_json::to_value(&rows).unwrap();
    equal(&wire, &source);
    assert!(!wire[1].as_object().unwrap().contains_key("key"));
}
