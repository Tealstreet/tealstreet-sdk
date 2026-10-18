use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, stringify_json},
    value::{Js, Prop, Thrown, from_json, to_json, utf16},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

fn intrinsic() -> Js {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let define = globals
        .get("Object")
        .unwrap()
        .get("defineProperty")
        .unwrap();
    assert!(
        define.is_callable(),
        "Object.defineProperty must be installed"
    );
    define
}

fn define(intrinsic: &Js, object: &Js, descriptor: Js) -> Result<Js, Thrown> {
    intrinsic.call(
        Js::Undefined,
        vec![object.clone(), Js::str("x"), descriptor],
    )
}

fn snapshot(object: &Js) -> Value {
    let property = match object {
        Js::Object(o) => o.props.borrow().get(utf16("x").as_ref()).cloned(),
        Js::Function(f) => f.props.borrow().get(utf16("x").as_ref()).cloned(),
        _ => unreachable!(),
    };
    let property = match property {
        Some(Prop::DataDescriptor {
            value,
            writable,
            enumerable,
            configurable,
        }) => {
            json!({"value": if value.is_undefined() { json!("undefined") } else { to_json(&value).unwrap() }, "writable": writable, "enumerable": enumerable, "configurable": configurable})
        }
        Some(Prop::Data(value)) => {
            json!({"value": to_json(&value).unwrap(), "writable": true, "enumerable": true, "configurable": true})
        }
        None => Value::Null,
        _ => unreachable!(),
    };
    json!({"keys": object.own_keys().iter().map(|key| String::from_utf16(key).unwrap()).collect::<Vec<_>>(), "property": property})
}

#[test]
fn data_descriptor_flags_and_redefinitions_match_native_engine() {
    let define_property = intrinsic();
    let mut cases = vec![];
    for function in [false, true] {
        for existing in [false, true] {
            for flags in 0..8 {
                cases.push(json!({"function": function, "existing": existing, "descriptors": [
                    {"value": 1, "writable": flags & 1 != 0, "enumerable": flags & 2 != 0, "configurable": flags & 4 != 0},
                    {}, {"value": 3}, {"writable": false}, {"value": 4},
                    {"enumerable": flags & 2 == 0}, {"configurable": true}, {"writable": true}
                ]}));
            }
            cases.push(json!({"function": function, "existing": existing, "descriptors": [{}, {"value": 5}, {"enumerable": "yes", "writable": 1, "configurable": []}]}));
        }
    }
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_data_descriptors_oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    // JS has one Number kind; normalize the engine JSON integers to native f64.
    let expected = to_json(&from_json(&expected)).unwrap();
    let mut actual = vec![];
    for case in &cases {
        let object = if case["function"].as_bool().unwrap() {
            Js::function("target", |_, _| Ok(Js::Undefined))
        } else {
            Js::object(vec![])
        };
        object.set("keep", Js::Number(0.0)).unwrap();
        if case["existing"].as_bool().unwrap() {
            object.set("x", Js::Number(7.0)).unwrap();
        }
        let mut steps = vec![];
        for descriptor in case["descriptors"].as_array().unwrap() {
            let result = define(&define_property, &object, from_json(descriptor));
            if let Ok(returned) = &result {
                assert!(returned.strict_equals(&object));
            }
            let mut step = snapshot(&object);
            step["error"] = result
                .err()
                .map(|e| json!(e.value.get("name").unwrap().to_display()))
                .unwrap_or(Value::Null);
            steps.push(step);
        }
        let result = object.set("x", Js::Number(99.0));
        let mut step = snapshot(&object);
        step["error"] = result
            .err()
            .map(|e| json!(e.value.get("name").unwrap().to_display()))
            .unwrap_or(Value::Null);
        steps.push(step);
        object.reflect_delete(&utf16("x")).unwrap();
        steps.push(snapshot(&object));
        actual.push(steps);
    }
    for (index, steps) in actual.iter().enumerate() {
        assert_eq!(
            json!(steps),
            expected[index],
            "native data descriptor case {index}: {}",
            cases[index]
        );
    }
}

#[test]
fn enhanced_error_message_is_readonly_enumerable_and_nonconfigurable() {
    let define_property = intrinsic();
    let error = safe_cex_exchange::js_runtime::value::error_object("Error", "original");
    let enhanced = Js::object(vec![]);
    if let Js::Object(object) = &enhanced {
        *object.proto.borrow_mut() = Some(error);
    }
    let returned = define_property
        .call(
            Js::Undefined,
            vec![
                enhanced.clone(),
                Js::str("message"),
                Js::object(vec![
                    ("value", Js::str("Polymarket Error: try later")),
                    ("writable", Js::Bool(false)),
                    ("enumerable", Js::Bool(true)),
                ]),
            ],
        )
        .unwrap();
    assert!(returned.strict_equals(&enhanced));
    assert_eq!(enhanced.get("name").unwrap().to_display(), "Error");
    assert_eq!(
        Thrown::new(enhanced.clone()).message(),
        "Polymarket Error: try later"
    );
    assert!(
        enhanced.set("message", Js::str("lost")).is_err(),
        "frozen error message must reject assignment"
    );
    assert_eq!(
        to_json(&enhanced.spread_object().unwrap()).unwrap(),
        json!({"message": "Polymarket Error: try later"})
    );
    let error = enhanced.delete(&utf16("message")).unwrap_err();
    assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    assert!(
        enhanced.has_own("message"),
        "nonconfigurable message must survive deletion"
    );
    assert!(enhanced.define("message", Js::str("lost")).is_err());
}

#[test]
fn frozen_values_use_same_value_and_preserve_explicit_undefined() {
    let define_property = intrinsic();
    for value in [
        Js::Number(f64::NAN),
        Js::Number(-0.0),
        Js::object(vec![]),
        Js::Undefined,
    ] {
        let object = Js::object(vec![]);
        define(
            &define_property,
            &object,
            Js::object(vec![("value", value.clone())]),
        )
        .unwrap();
        define(
            &define_property,
            &object,
            Js::object(vec![("value", value.clone())]),
        )
        .unwrap();
        assert!(object.has_own("x"));
        assert!(object.own_keys().is_empty());
        let different = if matches!(value, Js::Number(_)) {
            Js::Number(0.0)
        } else {
            Js::object(vec![])
        };
        assert!(
            define(
                &define_property,
                &object,
                Js::object(vec![("value", different)])
            )
            .is_err()
        );
    }
    let object = Js::object(vec![("x", Js::Number(8.0))]);
    define(&define_property, &object, Js::object(vec![])).unwrap();
    assert_eq!(object.get("x").unwrap().to_display(), "8");
    define(
        &define_property,
        &object,
        Js::object(vec![("value", Js::Undefined)]),
    )
    .unwrap();
    assert!(object.get("x").unwrap().is_undefined());
}

#[test]
fn inherited_descriptor_fields_getter_order_and_readonly_prototypes() {
    let define_property = intrinsic();
    let reads = Rc::new(RefCell::new(vec![]));
    let descriptor_proto = Js::object(vec![]);
    for (name, value) in [
        ("enumerable", Js::Bool(true)),
        ("configurable", Js::Bool(false)),
        ("value", Js::Number(42.0)),
        ("writable", Js::Bool(false)),
    ] {
        let reads = reads.clone();
        descriptor_proto
            .define_accessor(
                name,
                Some(Rc::new(move |receiver| {
                    reads.borrow_mut().push(name);
                    assert!(
                        receiver.has_own("receiver"),
                        "inherited getter receives original descriptor"
                    );
                    Ok(value.clone())
                })),
                None,
            )
            .expect("test accessor target permits definition");
    }
    let descriptor = Js::object(vec![("receiver", Js::Bool(true))]);
    if let Js::Object(object) = &descriptor {
        *object.proto.borrow_mut() = Some(descriptor_proto);
    }
    let object = Js::object(vec![]);
    define(&define_property, &object, descriptor).unwrap();
    assert_eq!(
        *reads.borrow(),
        ["enumerable", "configurable", "value", "writable"]
    );
    let child = Js::object(vec![]);
    if let Js::Object(child) = &child {
        *child.proto.borrow_mut() = Some(object);
    }
    assert!(child.set("x", Js::Number(3.0)).is_err());
    assert!(!child.has_own("x"));
    assert_eq!(child.get("x").unwrap().to_display(), "42");
}

#[test]
fn invalid_descriptors_leave_target_unchanged() {
    let define_property = intrinsic();
    let object = Js::object(vec![]);
    for descriptor in [
        Js::Undefined,
        Js::Null,
        Js::Number(1.0),
        Js::object(vec![("get", Js::Number(1.0))]),
    ] {
        assert!(define(&define_property, &object, descriptor).is_err());
        assert!(!object.has_own("x"));
    }
    for target in [Js::Undefined, Js::Null, Js::Number(1.0)] {
        assert!(define(&define_property, &target, Js::object(vec![])).is_err());
    }
}

#[test]
fn array_target_descriptors_preserve_slots_and_flags_like_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_data_descriptors_oracle.cjs"
        ))
        .arg("--array-targets")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let object = globals.get("Object").unwrap();
    let define_property = object.get("defineProperty").unwrap();
    let mut actual = vec![];
    for target in [
        Js::array(vec![]),
        Js::holey_array(vec![Some(Js::Number(-0.0)), None, Some(Js::Undefined)]),
    ] {
        let before = stringify_json(&target).unwrap();
        let keys = target.own_keys();
        for descriptor in [
            Js::Undefined,
            Js::Null,
            Js::Number(1.0),
            Js::object(vec![("get", Js::Number(1.0))]),
        ] {
            assert!(define(&define_property, &target, descriptor).is_err());
            assert!(!target.has_own("x"));
            assert_eq!(target.own_keys(), keys);
            assert_eq!(stringify_json(&target).unwrap(), before);
        }
        let returned = define(&define_property, &target, Js::object(vec![])).unwrap();
        let descriptor = object
            .call_method(
                "getOwnPropertyDescriptor",
                vec![target.clone(), Js::str("x")],
            )
            .unwrap();
        // Observe native presence and numeric bits before the explicit JSON boundary.
        let negative_zero = match target.get("0").unwrap() {
            Js::Number(value) => value.to_bits() == (-0.0f64).to_bits(),
            _ => false,
        };
        let flags: Vec<bool> = ["writable", "enumerable", "configurable"]
            .into_iter()
            .map(|key| descriptor.get(key).unwrap().truthy())
            .collect();
        let write = target.set("x", Js::Number(1.0)).unwrap_err();
        let delete = target.delete(&utf16("x")).unwrap_err();
        let redefine = define(
            &define_property,
            &target,
            Js::object(vec![("value", Js::Number(1.0))]),
        )
        .unwrap_err();
        let errors: Vec<_> = [write, delete, redefine]
            .iter()
            .map(|error| error.value.get("name").unwrap().to_display())
            .collect();
        actual.push(json!({
            "same": returned.same_reference(&target),
            "keys": target.own_keys().iter().map(|key| String::from_utf16(key).unwrap()).collect::<Vec<_>>(),
            "flags": flags,
            "own": target.has_own("x"),
            "undefined": descriptor.get("value").unwrap().is_undefined() && target.get("x").unwrap().is_undefined(),
            "length": target.get("length").unwrap().to_number().unwrap() as u32,
            "hole": !target.has_own("1"),
            "ownUndefined": target.has_own("2") && target.get("2").unwrap().is_undefined(),
            "negativeZero": negative_zero,
            "errors": errors,
            "wire": String::from_utf16(&stringify_json(&target).unwrap().unwrap()).unwrap(),
        }));
    }
    assert_eq!(json!(actual), expected);
}
