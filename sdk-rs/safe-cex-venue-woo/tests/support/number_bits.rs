pub fn number_bits(value: f64) -> String {
    if value.is_nan() {
        "7ff8000000000000".to_owned()
    } else {
        format!("{:016x}", value.to_bits())
    }
}
