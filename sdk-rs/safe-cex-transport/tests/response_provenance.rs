//! Response transforms consume current data, independently of model projections.
use safe_cex_transport::{RestResponseBody, how::big_int_as_string_json_parse};
use serde_json::{Value, json};

fn venue_parse(data: Value) -> Value {
    match data {
        Value::String(text) => big_int_as_string_json_parse(&text),
        data => data,
    }
}

#[test]
fn model_view_does_not_consume_a_response_transform() {
    let encoded = RestResponseBody::Encoded(br#""123""#.to_vec());
    let how = encoded.clone().transform(venue_parse);
    assert_eq!(encoded.json_value().unwrap(), json!("123"));
    assert_eq!(
        how.json_value().unwrap(),
        json!("123"),
        "model view must not reparse a transformed string"
    );
    assert_eq!(
        encoded.transform(venue_parse).into_data(),
        json!("123"),
        "direct parser sees the quoted wire text"
    );
    let twice = how.transform(venue_parse).into_data();
    assert!(
        twice.is_number(),
        "HOW second parser must see the decoded string"
    );
    assert_eq!(
        twice.as_f64().unwrap().to_bits(),
        123_f64.to_bits(),
        "HOW second parser must see the decoded string"
    );
}

#[test]
fn native_response_values_retain_shape_order_and_number_bits() {
    let value: Value =
        serde_json::from_str(r#"{"z":-0,"a":1e400,"nested":[null,true,"123"]}"#).unwrap();
    for body in [
        RestResponseBody::Untransformed(value.clone()),
        RestResponseBody::Transformed(value.clone()),
    ] {
        let current = body.clone().into_data();
        assert_eq!(
            current.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["z", "a", "nested"],
            "native response key order must survive without serialization"
        );
        assert_eq!(
            current["z"].as_f64().unwrap().to_bits(),
            (-0_f64).to_bits(),
            "native response signed zero must survive"
        );
        assert_eq!(
            current["a"].as_number().unwrap().to_string(),
            value["a"].as_number().unwrap().to_string(),
            "native response overflow lexeme must not become null"
        );
        assert_eq!(
            body.transform(venue_parse).into_data(),
            value,
            "object response transform must receive the native object"
        );
    }
    for value in [Value::Null, json!([1, 2]), json!({"x":1})] {
        assert_eq!(
            RestResponseBody::Untransformed(value.clone())
                .transform(venue_parse)
                .into_data(),
            value,
            "object array and null must not be over-parsed"
        );
    }
}

#[test]
fn fallible_response_transform_exposes_the_original_error() {
    let error = RestResponseBody::Transformed(json!("payload"))
        .try_transform(|data| {
            assert_eq!(
                data,
                json!("payload"),
                "fallible parser receives current data"
            );
            Err::<Value, _>(("TypeError", "parser refused"))
        })
        .unwrap_err();
    assert_eq!(
        error,
        ("TypeError", "parser refused"),
        "response parser errors must remain caller-visible"
    );
}
