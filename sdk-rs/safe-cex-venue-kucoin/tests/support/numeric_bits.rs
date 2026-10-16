pub fn bits(number: f64) -> String {
    let bits = if number.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        number.to_bits()
    };
    format!("{bits:016x}")
}
