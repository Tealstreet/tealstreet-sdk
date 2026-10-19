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
    assert_eq!(
        a.delete(&[b'0' as u16])
            .unwrap_err()
            .value
            .get("name")
            .unwrap()
            .to_display(),
        "TypeError"
    );
    assert_eq!(
        json!({
            "refused": refused, "retainedIndex": a.has_own("0"),
            "length": number(&alias.get("length").unwrap()) as usize,
            "json": String::from_utf16(&stringify_json(&alias).unwrap().unwrap()).unwrap(),
        }),
        expected("frozenBuiltins")
    );
}

#[test]
fn array_length_coercion_observes_two_conversions_and_intervening_reentry() {
    let mut outcomes = vec![];
    for mode in ["stable", "mismatch", "throws"] {
        let array = Js::array(vec![Js::Number(1.0), Js::Number(2.0), Js::Number(3.0)]);
        let token = Js::object(vec![]);
        let input = Js::object(vec![]);
        let Js::Object(receiver) = &input else {
            unreachable!()
        };
        let receiver = Rc::downgrade(receiver);
        let events = Rc::new(RefCell::new(vec![]));
        let observed = events.clone();
        let changed = array.clone();
        let thrown = token.clone();
        input
            .define(
                "valueOf",
                Js::function("valueOf", move |this, _| {
                    assert!(this.same_reference(&Js::Object(receiver.upgrade().unwrap())));
                    let call = {
                        let mut events = observed.borrow_mut();
                        events.push(number(&changed.get("length")?) as usize);
                        events.len()
                    };
                    if call == 1 {
                        changed.set("5", Js::Number(99.0))?;
                    }
                    if call == 2 {
                        if mode == "mismatch" {
                            return Ok(Js::Number(3.0));
                        }
                        if mode == "throws" {
                            return Err(Thrown::new(thrown.clone()));
                        }
                    }
                    Ok(Js::Number(2.0))
                }),
            )
            .unwrap();
        let error = array.set("length", input).err().map(|error| {
            if error.value.same_reference(&token) {
                "original".to_owned()
            } else {
                error.value.get("name").unwrap().to_display()
            }
        });
        outcomes.push(json!({
            "mode": mode, "events": &*events.borrow(), "error": error,
            "length": number(&array.get("length").unwrap()) as usize,
            "own5": array.has_own("5"),
        }));
    }
    assert_eq!(
        json!(outcomes),
        expected("lengthCoercion"),
        "ARRAY_STORAGE ToUint32 then ToNumber both observe the original input; reentry and abrupt effects remain"
    );
}

#[test]
fn sparse_array_max_index_and_named_uint32_key_stay_sparse() {
    let object = object();
    let a = Js::array(vec![]);
    define(
        &object,
        &a,
        Js::str("4294967294"),
        Js::object(vec![
            ("value", Js::Number(7.0)),
            ("writable", Js::Bool(true)),
            ("enumerable", Js::Bool(true)),
            ("configurable", Js::Bool(true)),
        ]),
    )
    .unwrap();
    a.set("4294967295", Js::str("named")).unwrap();
    let high = number(&a.get("length").unwrap()) as u64;
    a.set("length", Js::Number(0.0)).unwrap();
    assert_eq!(
        json!({"highLength":high,"length":number(&a.get("length").unwrap()) as u64,"names":keys(a.own_property_names().unwrap()),"named":a.get("4294967295").unwrap().to_display()}),
        expected("sparse")
    );
}

#[test]
fn readonly_array_length_refuses_empty_push_but_define_coerces_twice() {
    let object = object();
    let a = Js::array(vec![]);
    define(
        &object,
        &a,
        Js::str("length"),
        Js::object(vec![("writable", Js::Bool(false))]),
    )
    .unwrap();
    let refused = a.call_method("push", vec![]).is_err();
    assert!(
        a.set("0", Js::Number(1.0)).is_err(),
        "ARRAY_STORAGE readonly length blocks new indexed properties"
    );
    assert!(!a.has_own("0"));
    let count = Rc::new(std::cell::Cell::new(0));
    let observed = count.clone();
    let value = Js::object(vec![(
        "valueOf",
        Js::function("valueOf", move |_, _| {
            observed.set(observed.get() + 1);
            Ok(Js::Number(0.0))
        }),
    )]);
    define(
        &object,
        &a,
        Js::str("length"),
        Js::object(vec![("value", value)]),
    )
    .unwrap();
    assert_eq!(
        json!({"pushRefused":refused,"conversions":count.get(),"length":number(&a.get("length").unwrap()) as u64}),
        expected("readonly")
    );
}

#[test]
fn inherited_array_index_setter_observes_original_receiver_without_growth() {
    let object = object();
    let a = Js::array(vec![]);
    let parent = Js::array(vec![]);
    let calls = Rc::new(RefCell::new(vec![]));
    let out = calls.clone();
    let receiver = a.clone();
    define(
        &object,
        &parent,
        Js::str("1"),
        Js::object(vec![
            (
                "set",
                Js::function("setter", move |this, args| {
                    out.borrow_mut().push(json!([
                        this.same_reference(&receiver),
                        args[0].to_display()
                    ]));
                    Ok(Js::Undefined)
                }),
            ),
            ("configurable", Js::Bool(true)),
        ]),
    )
    .unwrap();
    object
        .call_method("setPrototypeOf", vec![a.clone(), parent])
        .unwrap();
    a.set("1", Js::str("direct")).unwrap();
    let before = number(&a.get("length").unwrap()) as u64;
    a.call_method("push", vec![Js::str("zero"), Js::str("one")])
        .unwrap();
    assert_eq!(
        json!({"beforePush":before,"length":number(&a.get("length").unwrap()) as u64,"keys":keys(a.own_keys()),"calls":*calls.borrow()}),
        expected("inheritedSetter")
    );
}

#[test]
fn array_for_in_hidden_own_index_suppresses_inherited_without_get() {
    let object = object();
    let a = Js::array(vec![]);
    let parent = Js::array(vec![Js::Number(3.0), Js::Number(4.0)]);
    let count = Rc::new(std::cell::Cell::new(0));
    let observed = count.clone();
    define(
        &object,
        &a,
        Js::str("0"),
        Js::object(vec![
            (
                "get",
                Js::function("hidden", move |_, _| {
                    observed.set(observed.get() + 1);
                    Ok(Js::Number(99.0))
                }),
            ),
            ("enumerable", Js::Bool(false)),
        ]),
    )
    .unwrap();
    object
        .call_method("setPrototypeOf", vec![a.clone(), parent])
        .unwrap();
    let mut record = safe_cex_exchange::js_runtime::for_in::ForInRecord::get(&a);
    let mut found = vec![];
    while let Some(key) = record.next_key().unwrap() {
        found.push(key);
    }
    assert_eq!(
        json!({"keys":keys(found),"names":keys(a.own_property_names().unwrap()),"reads":count.get()}),
        expected("hiddenForIn")
    );
}

#[test]
fn array_callbacks_slice_sort_and_json_read_live_properties() {
    let object = object();
    let a = Js::array(vec![Js::Number(1.0), Js::Number(2.0), Js::Number(3.0)]);
    let receiver = a.clone();
    let mapped = a
        .call_method(
            "map",
            vec![Js::function("map", move |_, args| {
                if number(&args[1]) == 0.0 {
                    receiver.set("1", Js::Number(8.0))?;
                    receiver.delete(&"2".encode_utf16().collect::<Vec<_>>())?;
                }
                Ok(Js::Number(number(&args[0]) * 2.0))
            })],
        )
        .unwrap();
    let sliced_array = Js::array(vec![Js::Number(0.0), Js::Number(1.0), Js::Number(2.0)]);
    let receiver = sliced_array.clone();
    let start = Js::object(vec![(
        "valueOf",
        Js::function("valueOf", move |_, _| {
            receiver.set("1", Js::Number(9.0))?;
            Ok(Js::Number(0.0))
        }),
    )]);
    let sliced = sliced_array
        .call_method("slice", vec![start, Js::Number(2.0)])
        .unwrap();
    let sorted = Js::array(vec![Js::Number(2.0), Js::Number(1.0)]);
    define(
        &object,
        &sorted,
        Js::str("0"),
        Js::object(vec![
            (
                "get",
                Js::function("get", |this, _| {
                    this.set("1", Js::Number(5.0))?;
                    Ok(Js::Number(3.0))
                }),
            ),
            (
                "set",
                Js::function("set", |this, args| {
                    this.set("finalFirst", args[0].clone())?;
                    Ok(Js::Undefined)
                }),
            ),
            ("configurable", Js::Bool(true)),
            ("enumerable", Js::Bool(true)),
        ]),
    )
    .unwrap();
    sorted
        .call_method(
            "sort",
            vec![Js::function("compare", |_, args| {
                Ok(Js::Number(number(&args[0]) - number(&args[1])))
            })],
        )
        .unwrap();
    let json_array = Js::array(vec![Js::Number(0.0), Js::Number(1.0)]);
    define(
        &object,
        &json_array,
        Js::str("0"),
        Js::object(vec![(
            "get",
            Js::function("get", |this, _| {
                this.delete(&"1".encode_utf16().collect::<Vec<_>>())?;
                Ok(Js::Number(-0.0))
            }),
        )]),
    )
    .unwrap();
    let wire = |value: &Js| String::from_utf16(&stringify_json(value).unwrap().unwrap()).unwrap();
    assert_eq!(
        json!({"mapped":wire(&mapped),"mappedKeys":keys(mapped.own_keys()),"sliced":[number(&sliced.get("0").unwrap()) as i64,number(&sliced.get("1").unwrap()) as i64],"sorted":[number(&sorted.get("finalFirst").unwrap()) as i64,number(&sorted.get("1").unwrap()) as i64],"json":wire(&json_array)}),
        expected("liveReads")
    );
    let token = Js::object(vec![]);
    let thrown = token.clone();
    define(
        &object,
        &a,
        Js::str("1"),
        Js::object(vec![(
            "get",
            Js::function("throw", move |_, _| Err(Thrown::new(thrown.clone()))),
        )]),
    )
    .unwrap();
    let error = a
        .call_method(
            "map",
            vec![Js::function("identity", |_, args| Ok(args[0].clone()))],
        )
        .unwrap_err();
    assert!(
        error.value.same_reference(&token),
        "original accessor throw must escape map"
    );
}

#[test]
fn array_prototype_method_alias_uses_actual_call_receiver() {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let prototype = globals.get("Array").unwrap().get("prototype").unwrap();
    let a = Js::array(vec![]);
    let b = Js::array(vec![]);
    let callback = Js::function("map", |_, args| Ok(Js::Number(number(&args[0]) + 1.0)));
    let result = prototype
        .get("map")
        .unwrap()
        .call(
            Js::object(vec![("0", Js::Number(4.0)), ("length", Js::Number(1.0))]),
            vec![callback],
        )
        .unwrap();
    let to_string = prototype.get("toString").unwrap();
    let mut borrowed_tags = Vec::new();
    for receiver in [
        Js::object(vec![("join", Js::Number(0.0))]),
        Js::function("borrowed", |_, _| Ok(Js::Undefined)),
        Js::Number(7.0),
        Js::Bool(false),
        Js::str("x"),
        Js::BigInt(Rc::new(1.into())),
    ] {
        let tag = to_string.call(receiver, vec![]).unwrap();
        borrowed_tags.push(String::from_utf16(tag.as_str().unwrap()).unwrap());
    }
    a.set("join", Js::Number(0.0)).unwrap();
    let array_tag = to_string.call(a.clone(), vec![]).unwrap();
    let nullish_errors: Vec<_> = [Js::Null, Js::Undefined]
        .into_iter()
        .map(|receiver| {
            let error = to_string.call(receiver, vec![]).unwrap_err().value;
            ["name", "message"]
                .map(|key| String::from_utf16(error.get(key).unwrap().as_str().unwrap()).unwrap())
        })
        .collect();
    assert_eq!(
        json!({"samePrototype":a.get("__proto__").unwrap().same_reference(&prototype),"sameMap":a.get("map").unwrap().same_reference(&b.get("map").unwrap()),"mapBorrow":[number(&result.get("0").unwrap()) as i64], "toStringBorrow":borrowed_tags,"toStringArray":String::from_utf16(array_tag.as_str().unwrap()).unwrap(),"toStringNullish":nullish_errors}),
        expected("prototypeAliases")
    );
}

#[test]
fn array_coercion_uses_live_hooks_original_receivers_and_throw_identity() {
    use safe_cex_exchange::js_runtime::ops;
    let array = Js::array(vec![Js::Number(2.0)]);
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured = events.clone();
    let original = array.clone();
    array
        .set(
            "toString",
            Js::function("toString", move |this, _| {
                assert!(
                    this.same_reference(&original),
                    "ARRAY_COERCION original string receiver"
                );
                captured.borrow_mut().push("string");
                Ok(Js::utf16(&[0xd800, 33]))
            }),
        )
        .unwrap();
    let captured = events.clone();
    let original = array.clone();
    array
        .set(
            "valueOf",
            Js::function("valueOf", move |this, _| {
                assert!(
                    this.same_reference(&original),
                    "ARRAY_COERCION original numeric receiver"
                );
                captured.borrow_mut().push("number");
                Ok(Js::Number(-0.0))
            }),
        )
        .unwrap();
    let units = array.to_js_string().unwrap().to_vec();
    let bits = format!("{:016x}", array.to_number().unwrap().to_bits());
    let plus = ops::add(&array, &Js::str("!"))
        .unwrap()
        .to_js_string()
        .unwrap()
        .to_vec();
    let token = Js::object(vec![]);
    let thrown = token.clone();
    let parent = Js::object(vec![(
        "toString",
        Js::function("throw", move |_, _| Err(Thrown::new(thrown.clone()))),
    )]);
    array
        .delete(&"toString".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    object()
        .call_method("setPrototypeOf", vec![array.clone(), parent])
        .unwrap();
    let error = array
        .to_js_string()
        .expect_err("ARRAY_COERCION inherited hook throws");
    assert!(
        error.value.same_reference(&token),
        "ARRAY_COERCION exact thrown referent"
    );
    assert_eq!(
        json!({"units":units,"bits":bits,"plus":plus,"events":*events.borrow(),"sameError":true}),
        expected("coercionHooks")
    );
}

#[test]
fn array_prototype_constructor_identity_matches_node() {
    let realm = Rc::new(safe_cex_num::PrototypeRealm::default());
    let _binding = realm.bind();
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let array = globals.get("Array").unwrap();
    let object = globals.get("Object").unwrap();
    let prototype = array.get("prototype").unwrap();
    let instance = Js::array(vec![]);
    assert!(
        prototype.get("constructor").unwrap().same_reference(&array),
        "ARRAY_CONSTRUCTOR prototype backlink retains the installed Array referent"
    );
    let original = descriptor(&object, &prototype, "constructor");
    let mut observed = json!({
        "prototype": prototype.get("constructor").unwrap().same_reference(&array),
        "instance": instance.get("constructor").unwrap().same_reference(&array),
        "descriptor": [original.get("value").unwrap().same_reference(&array), original.get("writable").unwrap().truthy(), original.get("enumerable").unwrap().truthy(), original.get("configurable").unwrap().truthy()],
        "enumerated": keys(prototype.own_keys()).contains(&"constructor".to_owned()),
    });
    let replacement = Js::object(vec![]);
    prototype.set("constructor", replacement.clone()).unwrap();
    observed["replaced"] = json!(
        instance
            .get("constructor")
            .unwrap()
            .same_reference(&replacement)
    );
    instance.set("constructor", array.clone()).unwrap();
    observed["shadowed"] = json!(
        instance.get("constructor").unwrap().same_reference(&array)
            && prototype
                .get("constructor")
                .unwrap()
                .same_reference(&replacement)
    );
    instance
        .delete(&"constructor".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    observed["unshadowed"] = json!(
        instance
            .get("constructor")
            .unwrap()
            .same_reference(&replacement)
    );
    define(&object, &prototype, Js::str("constructor"), original).unwrap();
    observed["restored"] = json!(instance.get("constructor").unwrap().same_reference(&array));
    assert_eq!(observed, expected("constructorIdentity"));
}

#[test]
fn array_constructor_retention_is_bounded_to_one_pair_per_realm() {
    let mut retained = Vec::new();
    for _ in 0..8 {
        let realm = Rc::new(safe_cex_num::PrototypeRealm::default());
        let binding = realm.bind();
        let mut previous = None;
        let mut discarded = Vec::new();
        for _ in 0..32 {
            let globals = Js::object(vec![]);
            install_builtins::<()>(&globals, std::rc::Weak::new());
            let constructor = globals.get("Array").unwrap();
            let prototype = constructor.get("prototype").unwrap();
            let Js::Object(owner) = &constructor else {
                panic!("Array facade");
            };
            let Js::Array(storage) = &prototype else {
                panic!("Array prototype");
            };
            if let Some((old_constructor, old_prototype)) = previous.take() {
                let old_constructor: std::rc::Weak<
                    safe_cex_exchange::js_runtime::value::ObjectData,
                > = old_constructor;
                let old_prototype: std::rc::Weak<safe_cex_exchange::js_runtime::value::ArrayData> =
                    old_prototype;
                assert!(
                    old_constructor.upgrade().is_none(),
                    "ARRAY_CONSTRUCTOR reinstall releases the previous facade"
                );
                assert!(
                    Rc::ptr_eq(&old_prototype.upgrade().unwrap(), storage),
                    "ARRAY_CONSTRUCTOR one prototype per realm"
                );
            }
            let before = (Rc::strong_count(owner), Rc::strong_count(storage));
            for _ in 0..128 {
                let value = Js::array(vec![Js::Number(-0.0)]);
                assert!(
                    value
                        .get("constructor")
                        .unwrap()
                        .same_reference(&constructor)
                );
                let Js::Array(value) = value else {
                    unreachable!()
                };
                discarded.push(Rc::downgrade(&value));
            }
            assert_eq!(
                (Rc::strong_count(owner), Rc::strong_count(storage)),
                before,
                "ARRAY_CONSTRUCTOR operations do not retain extra owners"
            );
            previous = Some((Rc::downgrade(owner), Rc::downgrade(storage)));
        }
        assert!(
            discarded.iter().all(|value| value.upgrade().is_none()),
            "ARRAY_CONSTRUCTOR all operation arrays release"
        );
        drop(binding);
        drop(realm);
        let pair = previous.unwrap();
        assert_eq!(
            (pair.0.strong_count(), pair.1.strong_count()),
            (1, 1),
            "ARRAY_CONSTRUCTOR one retained pair after realm disposal"
        );
        retained.push(pair);
    }
    assert_eq!(retained.len(), 8);
    for (constructor, prototype) in retained {
        assert_eq!(
            (constructor.strong_count(), prototype.strong_count()),
            (1, 1)
        );
        let prototype = Js::Array(prototype.upgrade().unwrap());
        prototype
            .delete(&"constructor".encode_utf16().collect::<Vec<_>>())
            .unwrap();
        drop(prototype);
        assert!(constructor.upgrade().is_none());
    }
}
