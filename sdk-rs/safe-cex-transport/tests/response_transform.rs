use safe_cex_transport::{RestResponseBody, TransportError};
use serde_json::{Value, json};

#[test]
fn axios_response_transform_observes_current_value_and_physical_decoding() {
    let cases = [
        (
            RestResponseBody::Encoded(br#"{"ok":19}"#.to_vec()),
            json!({"ok":19}),
        ),
        (
            RestResponseBody::Encoded("\u{feff}{\"ok\":23}".as_bytes().to_vec()),
            json!({"ok":23}),
        ),
        (
            RestResponseBody::Untransformed(json!("\u{feff}{\"ok\":29}")),
            json!("\u{feff}{\"ok\":29}"),
        ),
        (
            RestResponseBody::Untransformed(json!("{\"ok\":31}")),
            json!({"ok":31}),
        ),
        (
            RestResponseBody::Transformed(json!("{\"ok\":37}")),
            json!({"ok":37}),
        ),
        (
            RestResponseBody::Untransformed(json!("\"{\\\"ok\\\":41}\"")),
            json!("{\"ok\":41}"),
        ),
        (
            RestResponseBody::Untransformed(json!("{broken")),
            json!("{broken"),
        ),
        (RestResponseBody::Untransformed(json!("")), json!("")),
        (
            RestResponseBody::Untransformed(json!("\"\\u0000\"")),
            json!("\u{0}"),
        ),
        (
            RestResponseBody::Transformed(json!([43, false, null])),
            json!([43, false, null]),
        ),
        (RestResponseBody::Untransformed(json!(false)), json!(false)),
        (RestResponseBody::Untransformed(Value::Null), Value::Null),
        (
            RestResponseBody::Encoded(vec![b'"', 0xff, b'"']),
            json!("\u{fffd}"),
        ),
        (
            RestResponseBody::Encoded("\u{feff}\u{feff}47".as_bytes().to_vec()),
            json!("\u{feff}47"),
        ),
    ];
    assert_eq!(cases.len(), 14, "axios_transform_native_declared_cases");
    let mut observed = 0;
    for (index, (input, expected)) in cases.into_iter().enumerate() {
        let output = input.transform_axios_response().unwrap();
        let RestResponseBody::Transformed(actual) = output else {
            panic!("axios_transform_native_phase {index}");
        };
        assert_eq!(actual, expected, "axios_transform_native_value {index}");
        observed += 1;
    }
    assert_eq!(observed, 14, "axios_transform_native_observed_cases");
}

#[test]
fn axios_response_transform_retains_number_bits_and_own_key_order() {
    let negative = RestResponseBody::Untransformed(json!("-0"))
        .transform_axios_response()
        .unwrap()
        .into_data();
    assert_eq!(
        negative.as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits(),
        "axios_transform_native_negative_zero"
    );
    let overflow = RestResponseBody::Untransformed(json!("1e400"))
        .transform_axios_response()
        .unwrap()
        .into_data();
    assert!(overflow.is_number(), "axios_transform_native_overflow_kind");
    assert_eq!(
        safe_cex_num::number_from_string(&overflow.to_string()).to_bits(),
        f64::INFINITY.to_bits(),
        "axios_transform_native_overflow_bits"
    );
    let rounded = RestResponseBody::Untransformed(json!("9007199254740993"))
        .transform_axios_response()
        .unwrap()
        .into_data();
    assert_eq!(
        rounded.to_string(),
        "9007199254740992",
        "axios_transform_native_binary64_rounding"
    );
    let object = RestResponseBody::Untransformed(json!("{\"b\":1,\"11\":2,\"2\":3,\"a\":4}"))
        .transform_axios_response()
        .unwrap()
        .into_data();
    assert_eq!(
        object
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["2", "11", "b", "a"],
        "axios_transform_native_key_order"
    );
}

#[test]
fn axios_response_transform_refuses_valid_unrepresentable_json() {
    for input in [
        "\"\\ud800\"".to_owned(),
        format!("{}0{}", "[".repeat(150), "]".repeat(150)),
    ] {
        assert!(
            serde_json::from_str::<Box<serde_json::value::RawValue>>(&input).is_ok(),
            "axios_transform_native_valid_json_boundary"
        );
        let error = RestResponseBody::Untransformed(Value::String(input))
            .transform_axios_response()
            .unwrap_err();
        let TransportError::Host(message) = error else {
            panic!("axios_transform_native_representation_error_kind");
        };
        assert!(
            message.starts_with("Axios response representation refused: "),
            "axios_transform_native_representation_cause"
        );
    }
}
