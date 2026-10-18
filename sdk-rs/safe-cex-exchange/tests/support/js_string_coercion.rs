use super::*;

fn named(
    value: &Js,
    name: &'static str,
    result: Js,
    events: Rc<RefCell<Vec<String>>>,
    error: bool,
) -> Js {
    let receiver = value.clone();
    Js::function(name, move |this, _| {
        assert!(this.same_reference(&receiver), "string_hint::receiver");
        events.borrow_mut().push(name.into());
        if error {
            Err(Thrown::error("RangeError", name))
        } else {
            Ok(result.clone())
        }
    })
}
#[test]
fn string_hint_observes_source_order_live_methods_and_native_utf16() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_string_coercion_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let labels = [
        "plain",
        "string",
        "number",
        "undefined",
        "null",
        "bigint",
        "negative-zero",
        "utf16",
        "object-fallback",
        "noncallable-fallback",
        "both-objects",
        "both-noncallable",
        "undefined-shadow",
        "first-throws",
        "second-throws",
        "first-getter-throws",
        "first-getter",
        "second-getter",
        "second-getter-unreached",
        "mutate-second",
        "delete-second",
        "inherited",
        "only-valueof",
        "null-prototype",
        "function-hook",
        "array-child",
        "error-name",
        "stock-prototype",
        "stock-shadow",
        "stock-class",
        "stock-error",
    ];
    assert_eq!(expected.len(), labels.len());
    for (label, expected) in labels.into_iter().zip(expected) {
        let events: Rc<RefCell<Vec<String>>> = Rc::default();
        let value = if label == "function-hook" {
            Js::function("sample", |_, _| Ok(Js::Undefined))
        } else {
            Js::object(vec![("sentinel", Js::str("receiver"))])
        };
        let first = |result| {
            value
                .define(
                    "toString",
                    named(&value, "toString", result, events.clone(), false),
                )
                .unwrap()
        };
        let second = |result| {
            value
                .define(
                    "valueOf",
                    named(&value, "valueOf", result, events.clone(), false),
                )
                .unwrap()
        };
        match label {
            "plain" => {}
            "string" => {
                first(Js::str("hook"));
                second(Js::Number(99.0));
            }
            "number" => first(Js::Number(42.0)),
            "undefined" => first(Js::Undefined),
            "null" => first(Js::Null),
            "bigint" => first(Js::BigInt(Rc::new(45.into()))),
            "negative-zero" => first(Js::Number(-0.0)),
            "utf16" => first(Js::utf16(&[0xd800, 120])),
            "object-fallback" => {
                first(Js::object(vec![]));
                second(Js::str("fallback"));
            }
            "noncallable-fallback" => {
                value.define("toString", Js::Number(7.0)).unwrap();
                second(Js::str("fallback"));
            }
            "both-objects" => {
                first(Js::object(vec![]));
                second(Js::object(vec![]));
            }
            "both-noncallable" => {
                value.define("toString", Js::Number(7.0)).unwrap();
                value.define("valueOf", Js::Null).unwrap();
            }
            "undefined-shadow" => value.define("toString", Js::Undefined).unwrap(),
            "first-throws" => {
                value
                    .define(
                        "toString",
                        named(&value, "toString", Js::Null, events.clone(), true),
                    )
                    .unwrap();
                second(Js::str("unreached"));
            }
            "second-throws" => {
                first(Js::object(vec![]));
                value
                    .define(
                        "valueOf",
                        named(&value, "valueOf", Js::Null, events.clone(), true),
                    )
                    .unwrap();
            }
            "first-getter-throws" => {
                let events = events.clone();
                value
                    .define_accessor(
                        "toString",
                        Some(Rc::new(move |_| {
                            events.borrow_mut().push("get-toString".into());
                            Err(Thrown::error("SyntaxError", "get-toString"))
                        })),
                        None,
                    )
                    .expect("test accessor target permits definition");
            }
            "first-getter" | "second-getter" => {
                let (key, result, event) = if label == "first-getter" {
                    ("toString", Js::str("getter"), "get-toString")
                } else {
                    first(Js::object(vec![]));
                    ("valueOf", Js::Bool(true), "get-valueOf")
                };
                let method = named(&value, key, result, events.clone(), false);
                let events = events.clone();
                value
                    .define_accessor(
                        key,
                        Some(Rc::new(move |_| {
                            events.borrow_mut().push(event.into());
                            Ok(method.clone())
                        })),
                        None,
                    )
                    .expect("test accessor target permits definition");
            }
            "second-getter-unreached" => {
                first(Js::str("done"));
                value
                    .define_accessor(
                        "valueOf",
                        Some(Rc::new(|_| panic!("string_hint::unreached"))),
                        None,
                    )
                    .expect("test accessor target permits definition");
            }
            "mutate-second" | "delete-second" => {
                second(Js::str("stale"));
                let events = events.clone();
                value
                    .define(
                        "toString",
                        Js::function("toString", move |this, _| {
                            events.borrow_mut().push("toString".into());
                            if label == "mutate-second" {
                                this.set(
                                    "valueOf",
                                    named(
                                        &this,
                                        "valueOf",
                                        Js::str("mutated"),
                                        events.clone(),
                                        false,
                                    ),
                                )?;
                            } else {
                                this.delete(&utf16("valueOf"))?;
                            }
                            Ok(Js::object(vec![]))
                        }),
                    )
                    .unwrap();
            }
            "inherited" => {
                if let Js::Object(o) = &value {
                    *o.proto.borrow_mut() = Some(Js::object(vec![(
                        "toString",
                        named(
                            &value,
                            "toString",
                            Js::str("inherited"),
                            events.clone(),
                            false,
                        ),
                    )]));
                }
            }
            "only-valueof" => second(Js::Number(81.0)),
            "null-prototype" => {
                if let Js::Object(o) = &value {
                    *o.proto.borrow_mut() = None;
                }
            }
            "function-hook" => first(Js::str("function-hook")),
            "stock-prototype" | "stock-shadow" | "stock-class" | "stock-error" => {
                let realm = Rc::new(safe_cex_num::PrototypeRealm::default());
                let prototype = Js::Builtin(realm.reference(safe_cex_num::ProtoMember::Prototype));
                let prototype = if label == "stock-shadow" {
                    let shadow = Js::object(vec![("toString", Js::Undefined)]);
                    if let Js::Object(object) = &shadow {
                        *object.proto.borrow_mut() = Some(prototype);
                    }
                    shadow
                } else {
                    prototype
                };
                if let Js::Object(object) = &value {
                    *object.proto.borrow_mut() = Some(prototype);
                }
            }
            "array-child" | "error-name" => {}
            _ => unreachable!(),
        }
        let value = if label == "array-child" || label == "error-name" {
            let trace = events.clone();
            let tag = if label == "array-child" {
                "child-toString"
            } else {
                "name-toString"
            };
            let result = if label == "array-child" {
                "child"
            } else {
                "Named"
            };
            let child = Js::object(vec![(
                "toString",
                Js::function("toString", move |_, _| {
                    trace.borrow_mut().push(tag.into());
                    Ok(Js::str(result))
                }),
            )]);
            if label == "array-child" {
                Js::array(vec![Js::str("left"), child, Js::Null])
            } else {
                let error = safe_cex_exchange::js_runtime::value::error_object("Error", "detail");
                error.define("name", child).unwrap();
                error
            }
        } else if label == "stock-class" || label == "stock-error" {
            let class = if label == "stock-class" {
                "AxiosHeaders"
            } else {
                "Error"
            };
            let replacement = Js::object_with_class(class, vec![("message", Js::str("detail"))]);
            if let (Js::Object(old), Js::Object(new)) = (&value, &replacement) {
                *new.proto.borrow_mut() = old.proto.borrow().clone();
            }
            replacement
        } else {
            value
        };
        let actual = match value.to_js_string() {
            Ok(units) => json!({"label":label,"events":*events.borrow(),"units":units.as_ref()}),
            Err(error) => {
                json!({"label":label,"events":*events.borrow(),"error":error.value.get("name").unwrap().to_display(),"message":error.value.get("message").unwrap().to_display()})
            }
        };
        assert_eq!(actual, expected, "string_hint::{label}");
    }
}
