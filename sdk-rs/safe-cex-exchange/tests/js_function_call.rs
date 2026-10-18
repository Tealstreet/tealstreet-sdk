#[path = "support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown},
    },
    runtime::ExchangeError,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn describe(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Null => json!({"kind":"null"}),
        Js::Bool(value) => json!({"kind":"boolean","value":value}),
        Js::Number(value) => json!({"kind":"number","bits":format!("{:016x}",value.to_bits())}),
        Js::String(value) => json!({"kind":"string","units":value.as_ref()}),
        Js::BigInt(value) => json!({"kind":"bigint","text":value.to_string()}),
        Js::Function(_) => json!({"kind":"function"}),
        _ => json!({"kind":"object"}),
    }
}
fn same_value(actual: &Js, expected: &Js) -> bool {
    match (actual, expected) {
        (Js::Number(actual), Js::Number(expected)) => actual.to_bits() == expected.to_bits(),
        _ => actual.strict_equals(expected) || actual.same_reference(expected),
    }
}

#[test]
fn function_call_retains_receiver_arguments_results_and_throws() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_function_call_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let receivers = [
        Js::Undefined,
        Js::Null,
        Js::Bool(false),
        Js::Number(7.0),
        Js::Number(-0.0),
        Js::Number(f64::NAN),
        Js::utf16(&[0xd800]),
        Js::object(vec![]),
        Js::function("receiver", |_, _| Ok(Js::Undefined)),
        Js::BigInt(Rc::new(123.into())),
    ];
    let mut rows = vec![];
    for receiver in receivers {
        let observed = Rc::new(RefCell::new(None));
        let sink = observed.clone();
        let target = Js::function("target", move |this, args| {
            let result = args.first().cloned().unwrap_or(Js::Undefined);
            *sink.borrow_mut() = Some((this, args));
            Ok(result)
        });
        let call = target.get("call").unwrap();
        assert!(
            call.is_callable(),
            "FUNCTION_CALL shared callable member must exist"
        );
        let object = Js::object(vec![("marker", Js::Number(1.0))]);
        let args = vec![
            object.clone(),
            Js::Number(-0.0),
            Js::utf16(&[0xd800]),
            Js::Undefined,
            Js::BigInt(Rc::new(123.into())),
        ];
        let mut call_args = vec![receiver.clone()];
        call_args.extend(args.clone());
        let result = call.call(target, call_args).unwrap();
        let observed = observed.borrow();
        let (this, values) = observed.as_ref().unwrap();
        assert!(
            same_value(this, &receiver),
            "FUNCTION_CALL original strict receiver"
        );
        assert_eq!(values.len(), 5, "FUNCTION_CALL complete argument count");
        for (actual, expected) in values.iter().zip(&args) {
            assert!(
                same_value(actual, expected),
                "FUNCTION_CALL original argument identity/bits/units"
            );
        }
        assert!(
            result.same_reference(&object),
            "FUNCTION_CALL original returned referent"
        );
        rows.push(json!({"receiver":describe(this),"arguments":values.iter().map(describe).collect::<Vec<_>>(),"resultSame":true}));
    }
    assert_eq!(rows.len(), 10, "FUNCTION_CALL native receiver case floor");
    assert_eq!(
        json!(rows),
        source["rows"],
        "FUNCTION_CALL Node receiver/argument observations"
    );
    let empty = Rc::new(RefCell::new(None));
    let sink = empty.clone();
    let target = Js::function("empty", move |this, args| {
        *sink.borrow_mut() = Some(json!({"receiver":describe(&this),"arity":args.len()}));
        Ok(Js::Undefined)
    });
    let call = target.get("call").unwrap();
    assert!(call.call(target, vec![]).unwrap().is_undefined());
    assert_eq!(empty.borrow().as_ref().unwrap(), &source["empty"]);
    let mut refusals = vec![];
    for invalid in [
        Js::Undefined,
        Js::Null,
        Js::Bool(false),
        Js::Number(7.0),
        Js::str("x"),
        Js::object(vec![]),
        Js::array(vec![]),
    ] {
        let error = call.call(invalid.clone(), vec![]).unwrap_err();
        refusals.push(json!({"receiver":describe(&invalid),"error":error.value.get("name").unwrap().to_display()}));
    }
    assert_eq!(
        refusals.len(),
        7,
        "FUNCTION_CALL invalid receiver case floor"
    );
    assert_eq!(json!(refusals), source["refusals"]);
    let reads = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let target = Js::function("shadowed", |_, _| {
        panic!("FUNCTION_CALL original body must not run")
    });
    let receiver = Js::object(vec![]);
    let sentinel = Js::object(vec![]);
    let expected = target.clone();
    let this = receiver.clone();
    let result = sentinel.clone();
    let count = calls.clone();
    let override_call = Js::function("override", move |actual, args| {
        assert!(actual.same_reference(&expected));
        assert_eq!(args.len(), 1);
        assert!(args[0].same_reference(&this));
        count.set(count.get() + 1);
        Ok(result.clone())
    });
    let count = reads.clone();
    target.define_accessor(
        "call",
        Some(Rc::new(move |_| {
            count.set(count.get() + 1);
            Ok(override_call.clone())
        })),
        None,
    );
    assert!(
        target
            .call_method("call", vec![receiver.clone()])
            .unwrap()
            .same_reference(&sentinel)
    );
    assert_eq!(
        json!({"reads":reads.get(),"calls":calls.get()}),
        source["ownCall"]
    );
    let mut throws = 0;
    for error in [
        Thrown::new(Js::object(vec![])),
        Thrown::from(ExchangeError::new("original")),
    ] {
        let original = error.clone();
        let target = Js::function("thrower", move |_, _| Err(error.clone()));
        let thrown = target
            .get("call")
            .unwrap()
            .call(target, vec![receiver.clone()])
            .unwrap_err();
        assert!(
            thrown.value.same_reference(&original.value),
            "FUNCTION_CALL original throw referent"
        );
        match (&thrown.native, &original.native) {
            (Some(actual), Some(expected)) => assert!(
                Rc::ptr_eq(actual, expected),
                "FUNCTION_CALL original native error metadata"
            ),
            (None, None) => {}
            _ => panic!("FUNCTION_CALL native cause presence"),
        }
        throws += 1;
    }
    assert_eq!(throws, 2);
    assert_eq!(json!(throws), source["throws"]);
    let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
        Rc::new(runtime::Runtime::default());
    let promise = Js::Promise(Promise::settled(&runtime, Ok(Js::Number(7.0))));
    let original = promise.clone();
    let target = Js::function("promise", move |_, _| Ok(promise.clone()));
    assert!(
        target
            .get("call")
            .unwrap()
            .call(target, vec![receiver])
            .unwrap()
            .same_reference(&original),
        "FUNCTION_CALL no implicit await or promise replacement"
    );
    assert_eq!(source["promiseSame"], true);
}
