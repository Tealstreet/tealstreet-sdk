use safe_cex_types::{ModelField, Order};
use safe_cex_webworker::chaser::placement_filled;
use serde_json::{Value, json};

fn number(value: &Value) -> f64 {
    match value.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => value.as_f64().unwrap(),
    }
}
#[test]
fn placement_fills_match_live_source_including_nonfinite_fallback_and_negative_zero() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_fill_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 91);
    for row in rows {
        let mut order: Order = serde_json::from_value(json!({
            "id":"x","status":"open","symbol":"BTC","type":"limit","side":"buy",
            "price":100,"amount":10,"filled":0,"remaining":10,"lastUpdate":0,"reduceOnly":false,"extra":{}
        }))
        .unwrap();
        order.filled = number(&row["filled"]);
        if row["noExtra"] == true {
            order.extra = ModelField::Absent;
        } else {
            order.extra.as_mut().unwrap().filled_in_placement_units =
                row.get("placement").map(number);
        }
        let actual = placement_filled(&order);
        let expected = number(&row["result"]);
        assert!(
            if expected.is_nan() {
                actual.is_nan()
            } else {
                actual.to_bits() == expected.to_bits()
            },
            "placement units selection differs: {row}, actual={actual}"
        );
    }
}
