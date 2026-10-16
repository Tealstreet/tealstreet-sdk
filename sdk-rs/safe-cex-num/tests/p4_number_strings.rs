#[test]
fn shared_js_number_strings() {
    for (number, text) in [
        (10001.0, "10001"),
        (-0.0, "0"),
        (1e20, "100000000000000000000"),
        (1e21, "1e+21"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (f64::NAN, "NaN"),
    ] {
        assert_eq!(safe_cex_num::js_number_string(number), text);
    }
}
