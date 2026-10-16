use safe_cex_num::numeric_ingress::js_deserialize_passthrough;
use serde::de::value::F64Deserializer;

#[test]
fn passthrough_accepts_stock_f64_events_and_refuses_nan_projection() {
    for expected in [
        0x3ff4_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
    ] {
        let value = js_deserialize_passthrough(F64Deserializer::<serde_json::Error>::new(
            f64::from_bits(expected),
        ))
        .expect("public passthrough accepts stock native floating-point events");
        assert_eq!(
            value
                .as_number()
                .unwrap()
                .to_string()
                .parse::<f64>()
                .unwrap()
                .to_bits(),
            expected
        );
    }
    let error = js_deserialize_passthrough(F64Deserializer::<serde_json::Error>::new(f64::NAN))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("numeric passthrough refuses NaN JSON")
    );
}
