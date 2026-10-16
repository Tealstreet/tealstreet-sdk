use serde_json::Value;

pub(crate) fn assert_json_eq(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{actual} != {expected}");
            for (key, value) in b {
                assert_json_eq(
                    a.get(key)
                        .unwrap_or_else(|| panic!("missing {key} in {actual}")),
                    value,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{actual} != {expected}");
            for (actual, expected) in a.iter().zip(b) {
                assert_json_eq(actual, expected);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
