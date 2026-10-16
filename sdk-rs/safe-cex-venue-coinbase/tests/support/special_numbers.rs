use serde_json::{Value, json};

// Preserve runtime identity before JSON collapses non-finite values and -0.
fn number(value: f64) -> Value {
    if value.is_nan() {
        json!({"$number":"NaN"})
    } else if value.is_infinite() {
        json!({"$number":if value.is_sign_positive(){"Infinity"}else{"-Infinity"}})
    } else if value == 0.0 && value.is_sign_negative() {
        json!({"$number":"-0"})
    } else {
        json!(value)
    }
}

// A projection must never recreate a skipped field or undo altered serialization.
pub fn project_number(object: &mut Value, field: &str, value: f64) {
    let serialized = object
        .get_mut(field)
        .unwrap_or_else(|| panic!("missing serialized numeric field {field}"));
    assert_eq!(
        *serialized,
        json!(value),
        "altered serialized numeric field {field}"
    );
    *serialized = number(value);
}
