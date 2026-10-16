use safe_cex_types::{AccountValidationError, StoreSubscriberError};
use std::{cell::Cell, error::Error, fmt, rc::Rc};

#[test]
fn decoded_validation_error_clone_preserves_native_values_and_owned_storage() {
    let original: AccountValidationError = serde_json::from_str(
        r#"{"message":"rejected","detail":{"code":409,"tags":["first","second"]}}"#,
    )
    .unwrap();
    let expected = serde_json::json!({
        "message": "rejected", "detail": {"code": 409, "tags": ["first", "second"]}
    });
    assert_eq!(
        original.as_value(),
        &expected,
        "decoded native validation error"
    );
    let mut cloned = original.clone().into_value();
    assert_eq!(cloned, expected, "cloned native validation error");
    cloned["detail"]["code"] = serde_json::json!(503);
    cloned["detail"]["tags"][0] = serde_json::json!("changed");
    assert_eq!(cloned["detail"]["code"], 503);
    assert_eq!(cloned["detail"]["tags"][0], "changed");
    assert_eq!(
        original.as_value(),
        &expected,
        "original validation error remains owned"
    );
}

#[derive(Debug)]
struct SharedCause(Cell<u32>);
impl fmt::Display for SharedCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cause {}", self.0.get())
    }
}
impl Error for SharedCause {}

#[test]
fn subscriber_error_clone_copies_response_and_retains_native_cause_identity() {
    let response: serde_json::Value =
        serde_json::from_str(r#"{"status":409,"detail":["callback","failed"]}"#).unwrap();
    assert_eq!(response["status"], 409, "decoded native callback status");
    assert_eq!(
        response["detail"],
        serde_json::json!(["callback", "failed"])
    );
    let cause = Rc::new(SharedCause(Cell::new(17)));
    let original =
        StoreSubscriberError::with_response("listener failure", response).with_cause(cause.clone());
    let cloned = original.clone();
    assert_eq!(
        original.message(),
        "listener failure",
        "original callback message"
    );
    assert_eq!(
        cloned.message(),
        "listener failure",
        "cloned callback message"
    );
    let expected = serde_json::json!({"status":409,"detail":["callback","failed"]});
    assert_eq!(
        original.response(),
        Some(&expected),
        "original callback response"
    );
    assert_eq!(
        cloned.response(),
        Some(&expected),
        "cloned callback response"
    );
    assert!(
        !std::ptr::eq(original.response().unwrap(), cloned.response().unwrap()),
        "callback response storage is separate"
    );
    assert!(
        std::ptr::eq(original.source().unwrap(), cloned.source().unwrap()),
        "callback cause identity is retained"
    );
    assert!(
        std::ptr::eq(
            cloned
                .source()
                .unwrap()
                .downcast_ref::<SharedCause>()
                .unwrap(),
            Rc::as_ref(&cause)
        ),
        "clone retains original native cause"
    );
    cause.0.set(37);
    assert_eq!(
        original
            .source()
            .unwrap()
            .downcast_ref::<SharedCause>()
            .unwrap()
            .0
            .get(),
        37
    );
    assert_eq!(
        cloned
            .source()
            .unwrap()
            .downcast_ref::<SharedCause>()
            .unwrap()
            .0
            .get(),
        37
    );
    drop(cloned);
    assert_eq!(
        original.response(),
        Some(&expected),
        "response survives cloned error drop"
    );
    assert!(
        StoreSubscriberError::new("plain")
            .clone()
            .source()
            .is_none()
    );
}
