use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown},
    runtime::ExchangeError,
};
use std::{cell::Cell, rc::Rc};

#[test]
fn local_origin_recovers_original_without_getter_replay_or_reference_cycle() {
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let original = Js::object(vec![]);
    original
        .define_accessor(
            "message",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(Js::utf16(&[0xd800]))
            })),
            None,
        )
        .unwrap();
    let weak = match &original {
        Js::Object(object) => Rc::downgrade(object),
        _ => unreachable!(),
    };
    let error = Thrown::new(original.clone()).to_exchange_error();
    assert_eq!(reads.get(), 1);
    let recovered = Thrown::from(error.clone());
    assert!(recovered.value.same_reference(&original));
    assert_eq!(
        reads.get(),
        1,
        "recovery must not read the original message"
    );
    let mut source = std::error::Error::source(&error);
    let mut depth = 0;
    while let Some(cause) = source {
        let _ = cause.to_string();
        source = cause.source();
        depth += 1;
        assert!(depth < 8, "local origin chain must be acyclic");
    }
    assert_eq!(
        reads.get(),
        1,
        "origin diagnostics use the captured projection"
    );
    drop(original);
    drop(recovered);
    assert!(
        weak.upgrade().is_some(),
        "typed error retains its original throw"
    );
    drop(error);
    assert!(
        weak.upgrade().is_none(),
        "dropping the last origin releases the throw"
    );
}

#[test]
fn native_metadata_and_explicit_diagnostic_edits_remain_authoritative() {
    let mut native = ExchangeError::new("native");
    native.code = Some(Js::Number(-0.0));
    native.context = Some(serde_json::json!({"original":true}));
    let thrown = Thrown::from(native.clone());
    assert_eq!(thrown.to_exchange_error(), native);

    let original = Js::object(vec![
        ("name", Js::str("Original")),
        ("message", Js::str("before")),
    ]);
    let mut error = Thrown::new(original.clone()).to_exchange_error();
    error.name = "Edited".into();
    error.message = "after".into();
    error.code = Some(Js::Number(7.0));
    let recovered = Thrown::from(error);
    assert!(!recovered.value.same_reference(&original));
    assert!(
        recovered
            .value
            .get("name")
            .unwrap()
            .strict_equals(&Js::str("Edited"))
    );
    assert!(
        recovered
            .value
            .get("message")
            .unwrap()
            .strict_equals(&Js::str("after"))
    );
    assert!(
        recovered
            .value
            .get("code")
            .unwrap()
            .strict_equals(&Js::Number(7.0))
    );
    assert!(
        original
            .get("message")
            .unwrap()
            .strict_equals(&Js::str("before"))
    );
}

#[test]
fn a_throwing_code_getter_recovers_its_cause_without_reading_cause_code() {
    let cause = Js::object(vec![("message", Js::str("getter cause"))]);
    cause
        .define_accessor(
            "code",
            Some(Rc::new(|_| panic!("cause code must not be read"))),
            None,
        )
        .unwrap();
    let saved = cause.clone();
    let original = Js::object(vec![]);
    original
        .define_accessor(
            "code",
            Some(Rc::new(move |_| Err(Thrown::new(saved.clone())))),
            None,
        )
        .unwrap();
    let error = Thrown::new(original).to_exchange_error();
    assert_eq!(error.message, "getter cause");
    assert!(Thrown::from(error).value.same_reference(&cause));
}
