use serde::Serialize;
use serde_json::{Value, json};

#[path = "../../../parity/evidence/retained_model_values.rs"]
#[expect(
    dead_code,
    reason = "Shared observer also exports a separate protocol fixture adapter"
)]
pub mod retained_model_values;

pub fn encode(value: &impl Serialize) -> Value {
    retained_model_values::observe(value, numeric)
}
pub fn numeric(n: f64) -> Value {
    if n.is_nan() {
        json!({"$number":"7ff8000000000000"})
    } else {
        json!({"$number":format!("{:016x}",n.to_bits())})
    }
}
pub fn decode_number(v: &Value) -> f64 {
    if let Some(n) = v.as_f64() {
        return n;
    }
    let s = v["$number"].as_str().unwrap();
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
