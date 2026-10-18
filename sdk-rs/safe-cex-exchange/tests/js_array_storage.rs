//! Real installed Object/Array/iterator operations against independent Node.
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, stringify_json},
    value::{Js, Thrown},
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

fn expected(section: &str) -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_array_storage_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    value[section].clone()
}
fn object() -> Js {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    globals.get("Object").unwrap()
}
fn define(object: &Js, target: &Js, key: Js, descriptor: Js) -> Result<Js, Thrown> {
    object.call_method("defineProperty", vec![target.clone(), key, descriptor])
}
fn descriptor(object: &Js, target: &Js, key: &str) -> Js {
    object
        .call_method(
            "getOwnPropertyDescriptor",
            vec![target.clone(), Js::str(key)],
        )
        .unwrap()
}
fn keys(values: Vec<safe_cex_exchange::js_runtime::value::Str>) -> Vec<String> {
    values
        .iter()
        .map(|key| String::from_utf16(key).unwrap())
        .collect()
}
fn number(value: &Js) -> f64 {
    value.to_number().unwrap()
}

#[test]
fn array_named_descriptors_keep_holes_aliases_bits_and_key_order() {
    let object = object();
    let a = Js::holey_array(vec![
        None,
        Some(Js::Undefined),
        Some(Js::Number(-0.0)),
        Some(Js::Number(f64::NAN)),
        Some(Js::Number(f64::INFINITY)),
        Some(Js::Number(f64::NEG_INFINITY)),
    ]);
    let alias = a.clone();
    let token = Js::object(vec![]);
    a.set("tag", token.clone())
        .expect("ARRAY_STORAGE named field on original array");
    define(
        &object,
        &a,
        Js::str("hidden"),
        Js::object(vec![("value", token.clone())]),
    )
    .unwrap();
    a.set("01", Js::Number(1.0)).unwrap();
    a.set("4294967295", Js::Number(2.0)).unwrap();
    assert!(alias.get("tag").unwrap().same_reference(&token));
    assert!(alias.get("hidden").unwrap().same_reference(&token));
    assert_eq!(
        number(&alias.get("2").unwrap()).to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(number(&alias.get("3").unwrap()).is_nan());
    assert_eq!(number(&alias.get("4").unwrap()), f64::INFINITY);
    assert_eq!(number(&alias.get("5").unwrap()), f64::NEG_INFINITY);
    assert_eq!(
        json!({
            "keys": keys(a.own_keys()), "names": keys(a.own_property_names().unwrap()),
            "ownHole": a.has_own("0"), "ownUndefined": a.has_own("1"),
            "length": number(&a.get("length").unwrap()) as usize,
            "json": String::from_utf16(&stringify_json(&a).unwrap().unwrap()).unwrap(),
        }),
        expected("named")
    );
}

#[test]
fn array_accessors_freeze_keep_original_receivers_and_native_identity() {
    let object = object();
    let a = Js::array(vec![Js::Number(0.0), Js::Undefined]);
    let token = Js::object(vec![]);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let observed = calls.clone();
    let getter = Js::function("getter", move |this, _| {
        observed.borrow_mut().push((this.clone(), Js::Undefined));
        this.get("tag")
    });
    let observed = calls.clone();
    let setter = Js::function("setter", move |this, args| {
        observed.borrow_mut().push((this, args[0].clone()));
        Ok(Js::Undefined)
    });
    define(
        &object,
        &a,
        Js::str("x"),
        Js::object(vec![
            ("get", getter.clone()),
            ("set", setter),
            ("configurable", Js::Bool(true)),
        ]),
    )
    .unwrap();
    a.set("tag", token.clone()).unwrap();
    let child = Js::array(vec![]);
    child.set("tag", token.clone()).unwrap();
    child.set("__proto__", a.clone()).unwrap();
    let read = child.get("x").unwrap();
    child.set("x", token.clone()).unwrap();
    assert!(
        object
            .call_method("freeze", vec![a.clone()])
            .unwrap()
            .same_reference(&a)
    );
    a.set("x", token.clone()).unwrap();
    let refused = [
        a.set("1", Js::Number(3.0)),
        a.set("extra", Js::Number(1.0)),
        a.set("length", Js::Number(0.0)),
        a.push(Js::Number(1.0)),
    ]
    .into_iter()
    .filter(Result::is_err)
    .count();
    let calls = calls.borrow();
    assert_eq!(
        json!({
            "read": read.same_reference(&token), "childReceiver": calls[0].0.same_reference(&child),
            "childSetter": calls[1].0.same_reference(&child) && calls[1].1.same_reference(&token),
            "frozenSetter": calls[2].0.same_reference(&a) && calls[2].1.same_reference(&token),
            "getterSame": descriptor(&object, &a, "x").get("get").unwrap().same_reference(&getter),
            "frozen": object.call_method("isFrozen", vec![a]).unwrap().truthy(), "refused": refused,
        }),
        expected("accessor")
    );
}

#[test]
fn array_length_shrink_deletes_descending_and_locks_after_blocked_index() {
    let object = object();
    let a = Js::array((0..4).map(|n| Js::Number(n as f64)).collect());
    define(
        &object,
        &a,
        Js::str("2"),
        Js::object(vec![("configurable", Js::Bool(false))]),
    )
    .expect("ARRAY_STORAGE indexed descriptor");
    let shrink = define(
        &object,
        &a,
        Js::str("length"),
        Js::object(vec![
            ("value", Js::Number(1.0)),
            ("writable", Js::Bool(false)),
        ]),
    );
    assert_eq!(
        json!({
            "shrinkRefused": shrink.is_err(), "length": number(&a.get("length").unwrap()) as usize,
            "removed3": !a.has_own("3"), "kept2": a.has_own("2"),
            "writable": descriptor(&object, &a, "length").get("writable").unwrap().truthy(),
        }),
        expected("length")
    );
    assert!(
        a.set("3", Js::Null).is_err(),
        "ARRAY_STORAGE readonly length blocks index growth"
    );
    for length in [f64::NAN, f64::INFINITY, -1.0, 1.5, 4294967296.0] {
        let fresh = Js::array(vec![]);
        let error = fresh.set("length", Js::Number(length)).unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "RangeError");
        assert_eq!(number(&fresh.get("length").unwrap()), 0.0);
    }
}

#[test]
fn array_holes_use_inherited_index_getters_in_iteration_and_map() {
    let object = object();
    let a = Js::holey_array(vec![None, Some(Js::Undefined)]);
    let token = Js::object(vec![]);
    let proto = Js::array(vec![]);
    define(
        &object,
        &proto,
        Js::str("0"),
        Js::object(vec![
            ("get", Js::function("get", |this, _| this.get("tag"))),
            ("configurable", Js::Bool(true)),
        ]),
    )
    .unwrap();
    a.set("tag", token.clone())
        .expect("ARRAY_STORAGE inherited receiver metadata");
    a.set("__proto__", proto).unwrap();
    let iterated = a.iterable_values().unwrap();
    let mapped = a
        .call_method(
            "map",
            vec![Js::function("identity", |_, args| Ok(args[0].clone()))],
        )
        .unwrap();
    assert_eq!(
        json!({
            "own": a.has_own("0"), "receiver": a.get("0").unwrap().same_reference(&token),
            "iterated": iterated[0].same_reference(&token),
            "mapped": mapped.get("0").unwrap().same_reference(&token),
        }),
        expected("inherited")
    );
    assert!(a.has_own("1"));
    assert!(!a.has_own("0"));
}

#[test]
fn array_symbol_iterator_accessor_retains_receiver_alias_and_negative_zero() {
    let object = object();
    let a = Js::array(vec![Js::Null]);
    let token = Js::object(vec![]);
    let captured = Rc::new(RefCell::new(Js::Undefined));
    let seen = captured.clone();
    let values = Js::array(vec![token.clone(), Js::Number(-0.0)]);
    let iterator = Js::function("values", move |_, _| values.iterator());
    let getter = Js::function("get iterator", move |this, _| {
        *seen.borrow_mut() = this;
        Ok(iterator.clone())
    });
    define(
        &object,
        &a,
        Js::SymbolIterator,
        Js::object(vec![("get", getter), ("configurable", Js::Bool(true))]),
    )
    .expect("ARRAY_STORAGE array symbol descriptor");
    let alias = a.clone();
    let values = alias.iterable_values().unwrap();
    assert_eq!(
        json!({
            "receiver": captured.borrow().same_reference(&a), "token": values[0].same_reference(&token),
            "minusZero": number(&values[1]).to_bits() == (-0.0f64).to_bits(),
        }),
        expected("symbol")
    );
    assert_eq!(number(&a.get("length").unwrap()), 1.0);
}

#[test]
fn array_existing_holes_aliases_and_native_number_bits_remain_intact() {
    let payload = f64::from_bits(0x7ff8_0000_0000_0123);
    let token = Js::object(vec![]);
    let a = Js::holey_array(vec![
        None,
        Some(Js::Undefined),
        Some(Js::Number(payload)),
        Some(Js::Number(-0.0)),
        Some(token.clone()),
    ]);
    let alias = a.clone();
    assert!(!alias.has_own("0"));
    assert!(alias.has_own("1"));
    assert_eq!(
        number(&alias.get("2").unwrap()).to_bits(),
        payload.to_bits()
    );
    assert_eq!(
        number(&alias.get("3").unwrap()).to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(alias.get("4").unwrap().same_reference(&token));
    alias.set("0", Js::Undefined).unwrap();
    assert!(a.has_own("0"));
    assert!(a.get("0").unwrap().is_undefined());
    assert_eq!(number(&a.get("length").unwrap()), 5.0);
    assert_eq!(
        String::from_utf16(&stringify_json(&a).unwrap().unwrap()).unwrap(),
        expected("baseline")["json"].as_str().unwrap()
    );
}

#[test]
fn array_freeze_blocks_builtin_mutations_without_detaching_original_alias() {
    let object = object();
    let a = Js::array(vec![Js::Number(2.0), Js::Number(1.0)]);
    let alias = a.clone();
    object
        .call_method("freeze", vec![a.clone()])
        .expect("ARRAY_STORAGE freeze original array referent");
    let mut refused = 0;
    for method in ["push", "pop", "reverse", "sort"] {
        let args = if method == "push" {
            vec![Js::Null]
        } else {
            vec![]
        };
        let error = alias.call_method(method, args).unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        refused += 1;
    }
    a.delete(&[b'0' as u16]);
    assert_eq!(
        json!({
            "refused": refused, "retainedIndex": a.has_own("0"),
            "length": number(&alias.get("length").unwrap()) as usize,
            "json": String::from_utf16(&stringify_json(&alias).unwrap().unwrap()).unwrap(),
        }),
        expected("frozenBuiltins")
    );
}
