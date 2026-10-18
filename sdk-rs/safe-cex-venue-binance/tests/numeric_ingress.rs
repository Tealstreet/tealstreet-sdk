use serde_json::Value;

#[test]
fn finding_value_rebuffering_loses_negative_zero() {
    use serde::Deserialize;
    let value: Value = serde_json::from_str("-0").unwrap();
    let direct = serde_json::from_value::<f64>(value.clone()).unwrap();
    let buffered = Value::deserialize(value).unwrap();
    let indirect = serde_json::from_value::<f64>(buffered).unwrap();
    assert_eq!(direct.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(
        indirect.to_bits(),
        0.0_f64.to_bits(),
        "FINDING must flip when rebuffering retains -0"
    );
}
