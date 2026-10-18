use safe_cex_exchange::{
    js_runtime::value::{Thrown, from_json, to_json},
    runtime::ExchangeError,
};
use serde_json::{Value, json};
use std::rc::Rc;

#[test]
fn thrown_error_codes_preserve_falsy_and_structured_values() {
    for code in [
        Value::Null,
        json!(false),
        json!(0.0),
        json!(""),
        json!({"reason":"denied"}),
        json!([42.0]),
    ] {
        let thrown = Thrown::error("AxiosError", "request failed");
        thrown.value.define("code", from_json(&code)).unwrap();
        let error = thrown.to_exchange_error();
        assert_eq!(error.name, "AxiosError");
        assert_eq!(error.message, "request failed");
        assert_eq!(
            error.code.as_ref().map(to_json).transpose().unwrap(),
            Some(code)
        );
    }
    let thrown = Thrown::error("Error", "no code");
    assert!(thrown.to_exchange_error().code.is_none());
}

#[test]
fn retained_native_errors_keep_their_complete_metadata() {
    let mut error = ExchangeError::new("native failure");
    error.code = Some(from_json(&json!({"reason":"denied"})));
    error.context = Some(json!({"source":"native"}));
    assert_eq!(Thrown::from(error.clone()).to_exchange_error(), error);
}

#[test]
fn throwing_code_getters_report_their_cause_without_recursing() {
    let thrown = Thrown::error("Error", "original failure");
    let cause = Thrown::type_error("code getter failed");
    cause.value.define_accessor(
        "code",
        Some(Rc::new(|_| panic!("cause code must not be read"))),
        None,
    );
    thrown
        .value
        .define_accessor("code", Some(Rc::new(move |_| Err(cause.clone()))), None);
    let error = thrown.to_exchange_error();
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "code getter failed");
}

#[test]
fn retained_callback_metadata_preserves_utf16_without_undoing_typed_edits() {
    use safe_cex_exchange::js_runtime::value::{Js, OwnedJsValue, owned::OwnedCallbackError};
    use safe_cex_transport::{
        RestFailure, RestRequest, TransportError, provider::TransportErrorOrigin,
    };

    let original = OwnedJsValue::object([
        ("name", OwnedJsValue::String(vec![0xd800])),
        ("message", OwnedJsValue::String(vec![0xdc00])),
    ]);
    let mut failure = RestFailure::pre_transmit("callback rejected".into(), RestRequest::default());
    failure.transport_error = Some(Box::new(TransportError::Rejected(
        TransportErrorOrigin::new(OwnedCallbackError::new(original)),
    )));
    let error = ExchangeError::from(failure);
    let thrown = Thrown::from(error.clone());
    assert_eq!(
        thrown.value.get("name").unwrap().as_str().unwrap().as_ref(),
        &[0xd800]
    );
    assert_eq!(
        thrown
            .value
            .get("message")
            .unwrap()
            .as_str()
            .unwrap()
            .as_ref(),
        &[0xdc00]
    );
    assert!(matches!(&thrown.value, Js::Object(object) if object.class == "Error"));
    assert_eq!(thrown.to_exchange_error(), error);

    let mut edited = error;
    edited.name = "TypeError".into();
    edited.message = "venue supplied context".into();
    let thrown = Thrown::from(edited.clone());
    assert!(
        thrown
            .value
            .get("name")
            .unwrap()
            .strict_equals(&Js::str("TypeError"))
    );
    assert!(
        thrown
            .value
            .get("message")
            .unwrap()
            .strict_equals(&Js::str("venue supplied context"))
    );
    assert!(!thrown.value.has_own("name"));
    assert!(thrown.value.own_keys().is_empty());
    assert_eq!(thrown.to_exchange_error(), edited);
}
