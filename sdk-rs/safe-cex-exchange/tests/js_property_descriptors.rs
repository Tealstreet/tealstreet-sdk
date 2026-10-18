use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, stringify_json},
    value::{Js, Thrown, from_json},
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

fn object_builtin() -> Js {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    globals.get("Object").unwrap()
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_property_descriptors_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn define(object: &Js, target: &Js, key: &str, descriptor: Js) -> Result<Js, Thrown> {
    object.call_method(
        "defineProperty",
        vec![target.clone(), Js::str(key), descriptor],
    )
}
fn descriptor(object: &Js, target: &Js, key: &str) -> Js {
    object
        .call_method(
            "getOwnPropertyDescriptor",
            vec![target.clone(), Js::str(key)],
        )
        .unwrap()
}

#[test]
fn source_accessor_flags_identity_and_frozen_setter_match_node() {
    let expected = source();
    let object = object_builtin();
    let target = Js::object(vec![("tag", Js::Number(7.0))]);
    let getter = Js::function("getter", |this, _| this.get("tag"));
    let writes = Rc::new(RefCell::new(Vec::new()));
    let observed = writes.clone();
    let setter = Js::function("setter", move |this, args| {
        observed.borrow_mut().push((this, args[0].clone()));
        Ok(Js::Undefined)
    });
    let installed = define(
        &object,
        &target,
        "x",
        Js::object(vec![
            ("get", getter.clone()),
            ("set", setter.clone()),
            ("configurable", Js::Bool(true)),
        ]),
    );
    assert!(
        installed.is_ok(),
        "JS_PROPERTIES source accessor definition is admitted"
    );
    let before = descriptor(&object, &target, "x");
    assert!(before.get("get").unwrap().same_reference(&getter));
    assert!(before.get("set").unwrap().same_reference(&setter));
    assert_eq!(
        target.get("x").unwrap().to_number().unwrap().to_bits(),
        7.0f64.to_bits()
    );
    let frozen = object.call_method("freeze", vec![target.clone()]).unwrap();
    assert!(frozen.same_reference(&target));
    let token = Js::object(vec![]);
    target.set("x", token.clone()).unwrap();
    assert!(writes.borrow()[0].0.same_reference(&target));
    assert!(writes.borrow()[0].1.same_reference(&token));
    let same = define(&object, &target, "x", Js::object(vec![("get", getter)])).is_ok();
    let changed = define(
        &object,
        &target,
        "x",
        Js::object(vec![("get", Js::function("other", |_, _| Ok(Js::Null)))]),
    )
    .is_err();
    target.delete(&"x".encode_utf16().collect::<Vec<_>>());
    let after = descriptor(&object, &target, "x");
    let child = Js::object(vec![("tag", Js::Number(22.0))]);
    let Js::Object(data) = &child else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(target.clone());
    object
        .call_method("preventExtensions", vec![child.clone()])
        .unwrap();
    child.set("x", token.clone()).unwrap();
    assert_eq!(
        json!({
            "getterReceiver": child.get("x").unwrap().to_number().unwrap().to_bits() == 22.0f64.to_bits(),
            "setterReceiver": writes.borrow()[1].0.same_reference(&child),
            "setterValue": writes.borrow()[1].1.same_reference(&token),
            "own": child.has_own("x"),
        }),
        expected["inherited"],
        "JS_PROPERTIES inherited accessor receiver through non-extensible child"
    );

    assert_eq!(
        json!({
            "getterSame": before.get("get").unwrap().same_reference(&after.get("get").unwrap()),
            "setterSame": before.get("set").unwrap().same_reference(&after.get("set").unwrap()),
            "enumerable": after.get("enumerable").unwrap().truthy(),
            "configurable": after.get("configurable").unwrap().truthy(),
            "sameRedefinition": same, "changedRefused": changed, "ownAfterDelete": target.has_own("x"),
            "setterReceiver": writes.borrow()[0].0.same_reference(&target),
            "setterValue": writes.borrow()[0].1.same_reference(&token),
            "frozen": object.call_method("isFrozen", vec![target.clone()]).unwrap().truthy(),
            "extensible": object.call_method("isExtensible", vec![target]).unwrap().truthy(),
        }),
        expected["accessor"]
    );
}

#[test]
fn maintained_hyperliquid_frozen_configs_keep_identity_and_refuse_writes() {
    let expected = source();
    let object = object_builtin();
    for row in expected["configs"].as_array().unwrap() {
        let config = from_json(&row["value"]);
        let frozen = object.call_method("freeze", vec![config.clone()]);
        assert!(
            frozen.is_ok(),
            "JS_PROPERTIES maintained frozen config operation exists"
        );
        assert!(frozen.unwrap().same_reference(&config));
        assert!(
            config.set("leverage", Js::Number(-0.0)).is_err(),
            "JS_PROPERTIES frozen source value refuses overwrite"
        );
        assert!(config.set("extra", Js::Bool(true)).is_err());
        assert!(
            define(
                &object,
                &config,
                "extra",
                Js::object(vec![("value", Js::Null)])
            )
            .is_err()
        );
        assert_eq!(
            config.get("marginType").unwrap().to_display(),
            row["value"]["marginType"].as_str().unwrap()
        );
        assert_eq!(
            config.get("positionMode").unwrap().to_display(),
            row["value"]["positionMode"].as_str().unwrap()
        );
        assert_eq!(
            String::from_utf16(&stringify_json(&config).unwrap().unwrap()).unwrap(),
            row["json"].as_str().unwrap()
        );
        let leverage = descriptor(&object, &config, "leverage");
        assert!(!leverage.get("writable").unwrap().truthy());
        assert!(!leverage.get("configurable").unwrap().truthy());
        assert!(leverage.get("enumerable").unwrap().truthy());
        assert_eq!(
            config
                .get("leverage")
                .unwrap()
                .to_number()
                .unwrap()
                .to_bits(),
            row["value"]["leverage"].as_f64().unwrap().to_bits()
        );
    }
}

#[test]
fn descriptor_admission_observes_getter_validation_before_setter() {
    let expected = source();
    let object = object_builtin();
    let description = Js::object(vec![]);
    let reads = Rc::new(RefCell::new(Vec::new()));
    for key in [
        "enumerable",
        "configurable",
        "value",
        "writable",
        "get",
        "set",
    ] {
        let events = reads.clone();
        description.define_accessor(
            key,
            Some(Rc::new(move |_| {
                events.borrow_mut().push(key);
                Ok(if key == "get" {
                    Js::Number(7.0)
                } else {
                    Js::Undefined
                })
            })),
            None,
        );
    }
    let target = Js::object(vec![]);
    let failure = define(&object, &target, "x", description).unwrap_err();
    assert!(!target.has_own("x"));
    assert_eq!(
        json!({"reads": *reads.borrow(), "error": failure.value.get("name").unwrap().to_display()}),
        expected["readOrder"]
    );
    let token = Js::object(vec![]);
    let original = token.clone();
    let description = Js::object(vec![]);
    description.define_accessor(
        "get",
        Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
        None,
    );
    assert!(
        define(&object, &target, "x", description)
            .unwrap_err()
            .value
            .same_reference(&token)
    );
}
