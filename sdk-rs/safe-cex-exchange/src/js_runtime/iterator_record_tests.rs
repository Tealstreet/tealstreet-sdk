use super::{
    globals::install_builtins,
    value::{Js, Prop, Thrown},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};

#[test]
fn installed_array_from_does_not_close_on_iterator_value_failure() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_iterator_records_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
            if mode == "next" {
                return Err(Thrown::new(token.clone()));
            }
            let step = Js::object(vec![]);
            for field in ["done", "value"] {
                let events = events.clone();
                let token = token.clone();
                let mode = mode.clone();
                step.define_accessor(
                    field,
                    Some(Rc::new(move |_| {
                        events.borrow_mut().push(field);
                        if mode == field {
                            return Err(Thrown::new(token.clone()));
                        }
                        Ok(if field == "done" {
                            Js::Bool(false)
                        } else {
                            Js::Number(1.0)
                        })
                    })),
                    None,
                );
            }
            Ok(step)
        });
        let iterator = Js::object(vec![]);
        let events = trace.clone();
        iterator.define_accessor(
            "next",
            Some(Rc::new(move |_| {
                events.borrow_mut().push("getNext");
                Ok(next.clone())
            })),
            None,
        );
        let events = trace.clone();
        let Js::Object(data) = &iterator else {
            unreachable!()
        };
        let receiver = Rc::downgrade(data);
        iterator
            .define(
                "return",
                Js::function("return", move |this, _| {
                    assert!(this.same_reference(&Js::Object(receiver.upgrade().unwrap())));
                    events.borrow_mut().push("return");
                    Err(Thrown::new(cleanup.clone()))
                }),
            )
            .unwrap();
        let iterable = Js::object(vec![]);
        let Js::Object(data) = &iterable else {
            unreachable!()
        };
        *data.symbol_iterator.borrow_mut() = Some(Prop::Data(Js::function(
            "[Symbol.iterator]",
            move |_, _| Ok(iterator.clone()),
        )));
        let events = trace.clone();
        let token = original.clone();
        let mapper = Js::function("mapper", move |_, _| {
            events.borrow_mut().push("body");
            Err(Thrown::new(token.clone()))
        });
        let error = globals
            .get("Array")
            .unwrap()
            .call_method("from", vec![iterable, mapper])
            .unwrap_err();
        trace
            .borrow_mut()
            .push(if error.value.same_reference(&original) {
                "original"
            } else {
                "other"
            });
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "JS_ITERATOR_RECORD {fault} abrupt-close trace"
        );
    }
}

#[test]
fn record_elisions_cached_next_and_normal_close_match_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_iterator_records_source.cjs"
        ))
        .arg("normal")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in cases.as_array().unwrap() {
        let mode = case["mode"].as_str().unwrap().to_owned();
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let token = Js::object(vec![]);
        let cleanup = Js::object(vec![]);
        let iterator = Js::object(vec![]);
        let Js::Object(data) = &iterator else {
            unreachable!()
        };
        let receiver = Rc::downgrade(data);
        let events = trace.clone();
        let payload = token.clone();
        let index = Cell::new(0usize);
        let next = Js::function("next", move |this, _| {
            assert!(this.same_reference(&Js::Object(receiver.upgrade().unwrap())));
            let at = index.get();
            index.set(at + 1);
            events.borrow_mut().push(format!("next{at}"));
            if at == 0 {
                this.define(
                    "next",
                    Js::function("bad", |_, _| Err(Thrown::error("Error", "uncached next"))),
                )?;
            }
            let step = Js::object(vec![]);
            for field in ["done", "value"] {
                let events = events.clone();
                let payload = payload.clone();
                step.define_accessor(
                    field,
                    Some(Rc::new(move |_| {
                        events.borrow_mut().push(format!("{field}{at}"));
                        if field == "done" {
                            Ok(Js::Bool(false))
                        } else if at == 0 {
                            Err(Thrown::error("Error", "elision read value"))
                        } else {
                            Ok(payload.clone())
                        }
                    })),
                    None,
                );
            }
            Ok(step)
        });
        let events = trace.clone();
        iterator.define_accessor(
            "next",
            Some(Rc::new(move |_| {
                events.borrow_mut().push("getNext".into());
                Ok(next.clone())
            })),
            None,
        );
        let events = trace.clone();
        let close_mode = mode.clone();
        let failure = cleanup.clone();
        iterator.define_accessor(
            "return",
            Some(Rc::new(move |this| {
                events.borrow_mut().push("getReturn".into());
                match close_mode.as_str() {
                    "getThrow" => Err(Thrown::new(failure.clone())),
                    "undefined" => Ok(Js::Undefined),
                    "number" => Ok(Js::Number(7.0)),
                    "class" => Ok(Js::constructor("Return", None, |_, _| {
                        Ok(Js::object(vec![]))
                    })),
                    _ => {
                        let Js::Object(data) = this else {
                            unreachable!()
                        };
                        let receiver = Rc::downgrade(data);
                        let events = events.clone();
                        let close_mode = close_mode.clone();
                        let failure = failure.clone();
                        Ok(Js::function("return", move |this, _| {
                            assert!(this.same_reference(&Js::Object(receiver.upgrade().unwrap())));
                            events.borrow_mut().push("return".into());
                            match close_mode.as_str() {
                                "throw" => Err(Thrown::new(failure.clone())),
                                "object" => Ok(Js::object(vec![])),
                                _ => Ok(Js::Number(7.0)),
                            }
                        }))
                    }
                }
            })),
            None,
        );
        let iterable = Js::object(vec![]);
        let Js::Object(data) = &iterable else {
            unreachable!()
        };
        let events = trace.clone();
        *data.symbol_iterator.borrow_mut() = Some(Prop::Data(Js::function(
            "[Symbol.iterator]",
            move |_, _| {
                events.borrow_mut().push("getIterator".into());
                Ok(iterator.clone())
            },
        )));
        let record = super::iterator::IteratorRecord::get(&iterable).unwrap();
        assert!(record.step().unwrap().is_some());
        let bound = record.step_value().unwrap().unwrap();
        assert!(bound.same_reference(&token), "JS_ITERATOR_RECORD original bound value");
        let result = record.close();
        let succeeded = result.is_ok();
        let failure = match result {
            Ok(()) => "none".into(),
            Err(error) if error.value.same_reference(&cleanup) => "cleanup".into(),
            Err(error) => error.value.get("name").unwrap().to_display(),
        };
        assert_eq!(
            json!({"mode":mode,"trace":*trace.borrow(),"boundSame":succeeded && bound.same_reference(&token),"failure":failure}),
            *case,
            "JS_ITERATOR_RECORD normal close and elision"
        );
    }
}
