use serde_json::Value;
// JSON numeric values compare as binary64; DTO tests do not assert signed-byte spelling.
pub fn compare(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{context}"),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{context}: keys/order"
            );
            for (key, value) in a {
                compare(value, &b[key], &format!("{context}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(
                a.len(),
                b.len(),
                "{context}: expected/native collection cardinality"
            );
            for (a, b) in a.iter().zip(b) {
                compare(a, b, context);
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}
