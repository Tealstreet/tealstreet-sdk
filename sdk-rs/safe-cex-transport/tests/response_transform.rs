use safe_cex_transport::RestResponseBody;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
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
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(
                    "\u{feff}{\"ok\":29}"
                ))
                .unwrap(),
            ),
            json!("\u{feff}{\"ok\":29}"),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("{\"ok\":31}"))
                    .unwrap(),
            ),
            json!({"ok":31}),
        ),
        (
            RestResponseBody::Transformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("{\"ok\":37}"))
                    .unwrap(),
            ),
            json!({"ok":37}),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(
                    "\"{\\\"ok\\\":41}\""
                ))
                .unwrap(),
            ),
            json!("{\"ok\":41}"),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("{broken"))
                    .unwrap(),
            ),
            json!("{broken"),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("")).unwrap(),
            ),
            json!(""),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("\"\\u0000\""))
                    .unwrap(),
            ),
            json!("\u{0}"),
        ),
        (
            RestResponseBody::Transformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([
                    43, false, null
                ]))
                .unwrap(),
            ),
            json!([43, false, null]),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(false)).unwrap(),
            ),
            json!(false),
        ),
        (
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(Value::Null).unwrap(),
            ),
            Value::Null,
        ),
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
        assert_eq!(
            RestResponseBody::Transformed(actual).json_value().unwrap(),
            expected,
            "axios_transform_native_value {index}"
        );
        observed += 1;
    }
    assert_eq!(observed, 14, "axios_transform_native_observed_cases");
}

#[test]
fn axios_response_transform_retains_number_bits_and_own_key_order() {
    let negative = RestResponseBody::Untransformed(
        safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("-0")).unwrap(),
    )
    .transform_axios_response()
    .unwrap()
    .into_data();
    assert_eq!(
        match negative {
            Input::Float(number) => number.to_bits(),
            _ => panic!("native negative number"),
        },
        (-0.0_f64).to_bits(),
        "axios_transform_native_negative_zero"
    );
    let overflow = RestResponseBody::Untransformed(
        safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("1e400")).unwrap(),
    )
    .transform_axios_response()
    .unwrap()
    .into_data();
    assert!(
        matches!(overflow, Input::Float(_)),
        "axios_transform_native_overflow_kind"
    );
    assert_eq!(
        match overflow {
            Input::Float(number) => number.to_bits(),
            _ => panic!("native overflow number"),
        },
        f64::INFINITY.to_bits(),
        "axios_transform_native_overflow_bits"
    );
    let rounded = RestResponseBody::Untransformed(
        safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("9007199254740993"))
            .unwrap(),
    )
    .transform_axios_response()
    .unwrap()
    .into_data();
    assert_eq!(
        match rounded {
            Input::Float(number) => safe_cex_num::js_number_string(number),
            _ => panic!("native rounded number"),
        },
        "9007199254740992",
        "axios_transform_native_binary64_rounding"
    );
    let object = RestResponseBody::Untransformed(
        safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(
            "{\"b\":1,\"11\":2,\"2\":3,\"a\":4}"
        ))
        .unwrap(),
    )
    .transform_axios_response()
    .unwrap()
    .into_data();
    assert_eq!(
        match &object {
            Input::Object(fields) => safe_cex_types::js_coercion::js_own_keys_utf16(
                fields.iter().map(|(key, _)| key.as_slice())
            )
            .into_iter()
            .map(|key| String::from_utf16(key).unwrap())
            .collect::<Vec<_>>(),
            _ => panic!("native ordered object"),
        },
        ["2", "11", "b", "a"],
        "axios_transform_native_key_order"
    );
}

#[test]
fn axios_response_transform_retains_valid_utf16_and_deep_native_json() {
    for input in [
        "\"\\ud800\"".to_owned(),
        format!("{}0{}", "[".repeat(150), "]".repeat(150)),
    ] {
        assert!(
            serde_json::from_str::<Box<serde_json::value::RawValue>>(&input).is_ok(),
            "axios_transform_native_valid_json_boundary"
        );
        let output = RestResponseBody::Untransformed(Input::Text(input.encode_utf16().collect()))
            .transform_axios_response()
            .unwrap();
        let RestResponseBody::Transformed(value) = output else {
            panic!("actual parser phase")
        };
        assert_eq!(
            safe_cex_num::js_retained_json_stringify(&value)
                .unwrap()
                .unwrap(),
            input,
            "native parser preserves source-accepted JSON instead of refusing before consumer"
        );
        if input.starts_with('"') {
            assert_eq!(value, Input::Text(vec![0xd800]));
        } else {
            let mut value = &value;
            let mut depth = 0;
            while let Input::Array(items) = value {
                assert_eq!(items.len(), 1);
                value = &items[0];
                depth += 1;
            }
            assert_eq!(depth, 150);
            assert!(matches!(value,Input::Float(number) if *number==0.0));
        }
    }
}
