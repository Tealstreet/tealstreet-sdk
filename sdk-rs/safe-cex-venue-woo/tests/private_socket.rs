use serde_json::{Value, json};

#[test]
fn private_timestamp_matches_source_number_coercion_and_floor_bits() {
    use safe_cex_venue_woo::private_socket::parse_timestamp_to_seconds;
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/private-timestamps.json")).unwrap();
    assert_eq!(cases.len(), 125);
    let mut native_nan = 0;
    for case in cases {
        let input = &case["input"];
        // Native NaN is outside the chosen JSON input surface, not a JSON null.
        if input.get("$number") == Some(&json!("NaN")) {
            assert_eq!(case["ok"], json!({"$undefined":true}));
            native_nan += 1;
            continue;
        }
        let raw = if input.get("$undefined") == Some(&json!(true)) {
            None
        } else {
            Some(match input.get("$number").and_then(Value::as_str) {
                Some("Infinity") => serde_json::from_str("1e400").unwrap(),
                Some("-Infinity") => serde_json::from_str("-1e400").unwrap(),
                Some("-0") => serde_json::from_str("-0").unwrap(),
                _ => input.clone(),
            })
        };
        let actual = parse_timestamp_to_seconds(raw.as_ref());
        if let Some(error) = case.get("error") {
            let actual = actual.unwrap_err();
            assert_eq!(actual.name, error["name"], "timestamp error name {input}");
            assert_eq!(
                actual.message, error["message"],
                "timestamp error message {input}"
            );
        } else if case["ok"].get("$undefined") == Some(&json!(true)) {
            assert_eq!(actual.unwrap(), None, "timestamp absence {input}");
        } else {
            assert_eq!(
                actual.unwrap().unwrap().to_bits(),
                case["ok"].as_f64().unwrap().to_bits(),
                "timestamp output bits {input}"
            );
        }
    }
    assert_eq!(native_nan, 1, "native NaN boundary stays explicit");
}
