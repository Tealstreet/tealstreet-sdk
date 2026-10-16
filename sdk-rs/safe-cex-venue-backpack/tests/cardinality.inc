fn assert_native_cardinality(expected: &Value, native: &Value, path: &str) {
    match (expected, native) {
        (Value::Array(expected), Value::Array(native)) => {
            assert_eq!(native.len(), expected.len(), "NATIVE-CARDINALITY array {path}");
            for (i, value) in expected.iter().enumerate() {
                assert_native_cardinality(value, &native[i], &format!("{path}/{i}"));
            }
        }
        (Value::Object(expected), Value::Object(native)) => {
            assert_eq!(native.len(), expected.len(), "NATIVE-CARDINALITY object {path}");
            for (key, value) in expected {
                assert_native_cardinality(value, &native[key], &format!("{path}/{key}"));
            }
        }
        _ => {} // Full native-value equality below checks type and scalar values.
    }
}
