use super::*;

#[test]
fn facade_emitter_retains_numeric_bits_and_own_undefined() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    let emitter = assembled.this.get("emitter").unwrap();
    let received = Rc::new(RefCell::new(Vec::<Vec<Js>>::new()));
    let sink = received.clone();
    emitter
        .call_method(
            "on",
            vec![
                Js::str("retained"),
                Js::function("observe", move |_, args| {
                    sink.borrow_mut().push(args);
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
    let object = Js::object(vec![("present", Js::Undefined), ("null", Js::Null)]);
    let values = vec![
        Js::Undefined,
        Js::Null,
        Js::Number(f64::from_bits(0x7ff8_0000_0000_0042)),
        Js::Number(f64::INFINITY),
        Js::Number(f64::NEG_INFINITY),
        Js::Number(-0.0),
        object,
    ];
    let mut args = vec![Js::str("retained")];
    args.extend(values.clone());
    emitter.call_method("emit", args).unwrap();
    emitter
        .call_method("emit", vec![Js::str("retained")])
        .unwrap();
    let received = received.borrow();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].len(), 7, "EMITTER-RETAINED argument count");
    assert!(
        received[1].is_empty(),
        "EMITTER-RETAINED missing argument stays missing"
    );
    assert!(
        received[0][0].is_undefined(),
        "EMITTER-RETAINED undefined differs from null"
    );
    assert!(matches!(received[0][1], Js::Null));
    for index in 2..6 {
        let (Js::Number(actual), Js::Number(expected)) = (&received[0][index], &values[index])
        else {
            panic!("EMITTER-RETAINED native number at {index}");
        };
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "EMITTER-RETAINED number bits at {index}"
        );
    }
    assert!(received[0][6].has_own("present"));
    assert!(received[0][6].get("present").unwrap().is_undefined());
    assert!(!received[0][6].has_own("absent"));
}

#[test]
fn facade_emitter_retains_reference_identity_and_utf16() {
    let this = base_exchange(json!({"key":"k", "secret":"s"})).unwrap();
    let emitter = this.get("emitter").unwrap();
    let received = Rc::new(RefCell::new(Vec::<Vec<Js>>::new()));
    let sink = received.clone();
    emitter
        .call_method(
            "on",
            vec![
                Js::str("identity"),
                Js::function("observe", move |_, args| {
                    sink.borrow_mut().push(args);
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
    let shared = Js::object(vec![("value", Js::Number(17.0))]);
    emitter
        .call_method("emit", vec![Js::str("identity"), shared.clone()])
        .unwrap();
    assert!(
        received.borrow()[0][0].same_reference(&shared),
        "EMITTER-RETAINED same object referent"
    );
    shared.set("value", Js::Number(23.0)).unwrap();
    assert!(
        received.borrow()[0][0]
            .get("value")
            .unwrap()
            .strict_equals(&Js::Number(23.0))
    );
    let text = Js::String(Rc::from([0xd800, 0x0061]));
    emitter
        .call_method("emit", vec![Js::str("identity"), text.clone()])
        .unwrap();
    assert!(
        received.borrow()[1][0].strict_equals(&text),
        "EMITTER-RETAINED UTF16 units"
    );
}

#[test]
fn thrown_error_code_roundtrip_retains_native_payload() {
    let object = Js::object(vec![
        ("present", Js::Undefined),
        ("nan", Js::Number(f64::NAN)),
    ]);
    for code in [
        Js::Undefined,
        Js::Null,
        Js::Number(f64::NAN),
        Js::Number(f64::INFINITY),
        Js::Number(f64::NEG_INFINITY),
        Js::Number(-0.0),
        Js::String(Rc::from([0xd800])),
        object,
    ] {
        let original = Thrown::error("AxiosError", "retained code");
        original.value.define("code", code.clone()).unwrap();
        let restored = Thrown::from(original.to_exchange_error());
        assert!(
            restored.value.has_own("code"),
            "ERROR-CODE-RETAINED own code property"
        );
        let actual = restored.value.get("code").unwrap();
        if let (Js::Number(actual), Js::Number(expected)) = (&actual, &code) {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "ERROR-CODE-RETAINED numeric bits"
            );
        } else {
            assert!(
                actual.strict_equals(&code),
                "ERROR-CODE-RETAINED primitive/reference identity"
            );
        }
    }
}

#[test]
fn formatter_retains_code_identity_and_undefined_fields() {
    use safe_cex_exchange::error_reporting::format_error_for_sentry;
    let code = Js::object(vec![("native", Js::Number(f64::INFINITY))]);
    let mut error = ExchangeError::new("formatter");
    error.code = Some(code.clone());
    error.context = Some(json!({"errorCode":0}));
    let output = format_error_for_sentry(&error);
    let tags = output.get("tags").unwrap();
    let extra = output.get("extra").unwrap();
    assert!(tags.get("errorCode").unwrap().same_reference(&code));
    assert_eq!(
        tags.own_keys()
            .iter()
            .map(|key| String::from_utf16_lossy(key))
            .collect::<Vec<_>>(),
        [
            "errorType",
            "errorCode",
            "statusCode",
            "isTimeout",
            "isCORS"
        ]
    );
    assert_eq!(
        extra
            .own_keys()
            .iter()
            .map(|key| String::from_utf16_lossy(key))
            .collect::<Vec<_>>(),
        [
            "url",
            "method",
            "statusText",
            "enhancedMessage",
            "originalMessage",
            "timestamp",
            "config"
        ]
    );
    for key in ["statusCode", "isTimeout", "isCORS"] {
        assert!(tags.get(key).unwrap().is_undefined());
    }
    for key in [
        "url",
        "method",
        "statusText",
        "enhancedMessage",
        "originalMessage",
        "timestamp",
        "config",
    ] {
        assert!(extra.get(key).unwrap().is_undefined());
    }
    code.set("native", Js::Number(-0.0)).unwrap();
    assert_eq!(
        tags.get("errorCode")
            .unwrap()
            .get("native")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
}
#[test]
fn native_error_code_writes_override_only_code_metadata() {
    let mut error = ExchangeError::new("native");
    error.context = Some(json!({"producer":"native"}));
    let thrown = Thrown::from(error);
    thrown.value.set("code", Js::Number(f64::INFINITY)).unwrap();
    let restored = thrown.to_exchange_error();
    assert_eq!(restored.context, Some(json!({"producer":"native"})));
    assert_eq!(
        restored.code.as_ref().unwrap().as_f64().unwrap(),
        f64::INFINITY
    );
}

#[test]
fn native_emitter_once_reentry_and_throw_order_retain_payload() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    let base = assembled.base.clone();
    let shared = Js::object(vec![("present", Js::Undefined)]);
    let trace = Rc::new(RefCell::new(Vec::new()));
    let weak = Rc::downgrade(&base);
    let sink = trace.clone();
    base.once(
        "reentry",
        Rc::new(move |_, data| {
            sink.borrow_mut().push(data.clone());
            weak.upgrade().unwrap().emit("reentry", data.clone())
        }),
    );
    base.emit("reentry", shared.clone()).unwrap();
    base.emit("reentry", shared.clone()).unwrap();
    assert_eq!(trace.borrow().len(), 1);
    assert!(trace.borrow()[0].same_reference(&shared));
    let marker = ExchangeError::new("listener sentinel");
    let cause = marker.clone();
    base.once("throw", Rc::new(move |_, _| Err(cause.clone())));
    let tail = trace.clone();
    base.on(
        "throw",
        Rc::new(move |_, data| {
            tail.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    assert_eq!(base.emit("throw", shared.clone()).unwrap_err(), marker);
    assert_eq!(
        trace.borrow().len(),
        1,
        "EMITTER-RETAINED stop on first throw"
    );
    base.emit("throw", shared.clone()).unwrap();
    assert_eq!(
        trace.borrow().len(),
        2,
        "EMITTER-RETAINED once removed before throw"
    );
    assert!(trace.borrow()[1].same_reference(&shared));
    let missing = Rc::new(RefCell::new(Vec::new()));
    let sink = missing.clone();
    base.on(
        "missing",
        Rc::new(move |_, data| {
            sink.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    base.emit_args("missing", &[]).unwrap();
    assert!(
        missing.borrow()[0].is_undefined(),
        "EMITTER-RETAINED missing scalar is undefined"
    );
}
