use super::{globals::install_builtins, value::{Js, Prop, Thrown}};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

#[test]
fn installed_array_from_does_not_close_on_iterator_value_failure() {
    let output = Command::new("node").arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_iterator_records_source.cjs")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    for case in cases.as_array().unwrap() {
        let fault = case["fault"].as_str().unwrap().to_owned();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let original = Js::object(vec![]);
        let cleanup = Js::object(vec![]);
        let events = trace.clone();
        let token = original.clone();
        let mode = fault.clone();
        let next = Js::function("next", move |_, _| {
            events.borrow_mut().push("next");
            if mode == "next" { return Err(Thrown::new(token.clone())); }
            let step = Js::object(vec![]);
            for field in ["done", "value"] {
                let events = events.clone();
                let token = token.clone();
                let mode = mode.clone();
                step.define_accessor(field, Some(Rc::new(move |_| {
                    events.borrow_mut().push(field);
                    if mode == field { return Err(Thrown::new(token.clone())); }
                    Ok(if field == "done" { Js::Bool(false) } else { Js::Number(1.0) })
                })), None);
            }
            Ok(step)
        });
        let iterator = Js::object(vec![]);
        let events = trace.clone();
        iterator.define_accessor("next", Some(Rc::new(move |_| {
            events.borrow_mut().push("getNext");
            Ok(next.clone())
        })), None);
        let events = trace.clone();
        let receiver = iterator.clone();
        iterator.define("return", Js::function("return", move |this, _| {
            assert!(this.same_reference(&receiver));
            events.borrow_mut().push("return");
            Err(Thrown::new(cleanup.clone()))
        })).unwrap();
        let iterable = Js::object(vec![]);
        let Js::Object(data) = &iterable else { unreachable!() };
        *data.symbol_iterator.borrow_mut() = Some(Prop::Data(Js::function("[Symbol.iterator]", move |_, _| Ok(iterator.clone()))));
        let events = trace.clone();
        let token = original.clone();
        let mapper = Js::function("mapper", move |_, _| {
            events.borrow_mut().push("body");
            Err(Thrown::new(token.clone()))
        });
        let error = globals.get("Array").unwrap().call_method("from", vec![iterable, mapper]).unwrap_err();
        trace.borrow_mut().push(if error.value.same_reference(&original) { "original" } else { "other" });
        assert_eq!(json!(*trace.borrow()), case["trace"], "JS_ITERATOR_RECORD {fault} abrupt-close trace");
    }
}
