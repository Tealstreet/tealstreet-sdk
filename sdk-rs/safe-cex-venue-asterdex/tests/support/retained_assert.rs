use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::Value;

pub fn assert_value(actual: &Input, expected: &Value) {
    if let Some(number) = actual.as_f64() {
        if let Some(tag) = expected.get("number").and_then(Value::as_str) {
            if tag == "nan" || tag == "NaN" {
                assert!(number.is_nan(), "native info NaN");
            } else {
                let bits = match tag {
                    "-0" => (-0.0_f64).to_bits(),
                    "Infinity" => f64::INFINITY.to_bits(),
                    "-Infinity" => f64::NEG_INFINITY.to_bits(),
                    _ => u64::from_str_radix(tag, 16).unwrap(),
                };
                assert_eq!(number.to_bits(), bits, "native info number bits");
            }
        } else {
            assert_eq!(
                number.to_bits(),
                expected.as_f64().unwrap().to_bits(),
                "native info number bits"
            );
        }
        return;
    }
    match (actual, expected) {
        (Input::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len());
            for (a, e) in a.iter().zip(e) {
                assert_value(a, e);
            }
        }
        (Input::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len());
            for (key, e) in e {
                assert_value(actual.get(key).expect("native own field"), e);
            }
        }
        (Input::Text(a), Value::String(e)) => assert!(a.iter().copied().eq(e.encode_utf16())),
        (Input::Null, Value::Null) => (),
        (Input::Bool(a), Value::Bool(e)) => assert_eq!(a, e),
        _ => panic!("native retained mismatch {actual:?} vs {expected:?}"),
    }
}
