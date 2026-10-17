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
