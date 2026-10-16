use safe_cex_exchange::js_runtime::{
    globals::install_builtins,
    value::{Js, from_json, to_json},
};
use serde_json::{Value, json};

fn globals() -> Js {
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    globals
}

#[test]
fn collection_values_supply_next_for_hyro_ack_cleanup() {
    let pending = Js::new_map();
    pending
        .call_method("set", vec![Js::str("auth-id"), Js::str("pending-auth")])
        .unwrap();
    let iterator = pending.call_method("values", vec![]).unwrap();
    assert!(
        !iterator.is_array(),
        "Hyro pendingAcks.values must return an iterator, not a snapshot array"
    );
    let first = iterator.call_method("next", vec![]).unwrap();
    assert!(!first.get("done").unwrap().truthy());
    assert!(
        first
            .get("value")
            .unwrap()
            .strict_equals(&Js::str("pending-auth"))
    );
    assert!(
        iterator
            .call_method("next", vec![])
            .unwrap()
            .get("done")
            .unwrap()
            .truthy()
    );
}

#[test]
fn collection_iterator_live_traces_match_node() {
    let output = std::process::Command::new("node")
        .arg("tests/js_collection_iterators_oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node is required for the collection iterator oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 80, "independent engine trace cardinality");
    let globals = globals();
    for case in cases {
        let map = case["type"] == "Map";
        let collection = if map { Js::new_map() } else { Js::new_set() };
        for (key, value) in [("a", "A"), ("b", "B")] {
            collection
                .call_method(
                    if map { "set" } else { "add" },
                    vec![Js::str(key), Js::str(value)],
                )
                .unwrap();
        }
        let mode = case["mode"].as_str().unwrap();
        let iterator = if mode == "iterator" {
            collection.iterator().unwrap()
        } else {
            collection.call_method(mode, vec![]).unwrap()
        };
        assert!(
            !iterator.is_array(),
            "{}: must return an iterator",
            case["name"]
        );
        assert!(
            iterator.iterator().unwrap().same_reference(&iterator),
            "iterator must be iterable-self"
        );
        assert!(
            iterator.own_keys().is_empty(),
            "protocol methods belong to the iterator prototype"
        );
        let mut trace = vec![];
        for operation in case["operations"].as_array().unwrap() {
            let op = operation[0].as_str().unwrap();
            let key = from_json(&operation[1]);
            let value = from_json(&operation[2]);
            match op {
                "next" => {
                    let step = iterator
                        .call_method("next", vec![Js::str("ignored")])
                        .unwrap();
                    let done = step.get("done").unwrap().truthy();
                    let value = if done {
                        json!("<undefined>")
                    } else {
                        to_json(&step.get("value").unwrap()).unwrap()
                    };
                    let keys: Vec<_> = step
                        .own_keys()
                        .iter()
                        .map(|key| String::from_utf16_lossy(key))
                        .collect();
                    assert_eq!(keys, ["value", "done"]);
                    trace.push(json!({"done":done,"value":value}));
                }
                "delete" => {
                    collection.call_method("delete", vec![key]).unwrap();
                }
                "add" => {
                    collection
                        .call_method(if map { "set" } else { "add" }, vec![key, value])
                        .unwrap();
                }
                "clear" => {
                    collection.call_method("clear", vec![]).unwrap();
                }
                "spread" => {
                    trace.push(to_json(&Js::array(iterator.iterable_values().unwrap())).unwrap());
                }
                "from" => {
                    trace.push(
                        to_json(
                            &globals
                                .get("Array")
                                .unwrap()
                                .call_method("from", vec![iterator.clone()])
                                .unwrap(),
                        )
                        .unwrap(),
                    );
                }
                _ => panic!("unknown oracle operation {op}"),
            }
        }
        assert_eq!(Value::Array(trace), case["trace"], "{}", case["name"]);
    }
}

#[test]
fn collection_iterators_preserve_native_bits_references_and_aliases() {
    let map = Js::new_map();
    let values = [
        0x8000_0000_0000_0000,
        0x7ff8_0000_0000_0042,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
    ];
    for (index, bits) in values.into_iter().enumerate() {
        map.call_method(
            "set",
            vec![Js::Number(index as f64), Js::Number(f64::from_bits(bits))],
        )
        .unwrap();
    }
    let marker = Js::object(vec![]);
    map.call_method("set", vec![Js::str("object"), marker.clone()])
        .unwrap();
    let result = map
        .call_method("values", vec![])
        .unwrap()
        .iterable_values()
        .unwrap();
    for (actual, expected) in result.iter().zip(values) {
        assert_eq!(
            actual.as_f64().unwrap().to_bits(),
            expected,
            "native iterator value bits"
        );
    }
    assert!(result[4].same_reference(&marker));
    let entries = map.get("entries").unwrap();
    let symbol = globals().get("Symbol").unwrap().get("iterator").unwrap();
    assert_eq!(symbol.type_of(), "symbol");
    assert!(Js::Null.get_property(&symbol).is_err());
    assert!(Js::Undefined.get_property(&symbol).is_err());
    let symbol_owner = globals().get("Symbol").unwrap();
    assert!(symbol_owner.set("iterator", Js::str("fake")).is_err());
    assert!(symbol_owner.own_keys().is_empty());
    assert!(map.get_property(&symbol).unwrap().same_reference(&entries));
    assert!(
        map.get("Symbol.iterator").unwrap().is_undefined(),
        "well-known symbols must not alias string keys"
    );
    let set = Js::new_set();
    set.call_method("add", vec![Js::Number(-0.0)]).unwrap();
    set.call_method("add", vec![marker.clone()]).unwrap();
    let keys = set.get("keys").unwrap();
    assert!(set.get("values").unwrap().same_reference(&keys));
    assert!(set.get_property(&symbol).unwrap().same_reference(&keys));
    let values = set
        .call_method("values", vec![])
        .unwrap()
        .iterable_values()
        .unwrap();
    assert_eq!(
        values[0].as_f64().unwrap().to_bits(),
        0,
        "Set canonicalizes -0 keys"
    );
    assert!(values[1].same_reference(&marker));
    let entries = set
        .call_method("entries", vec![])
        .unwrap()
        .iterable_values()
        .unwrap();
    assert!(
        entries[1]
            .get("0")
            .unwrap()
            .same_reference(&entries[1].get("1").unwrap())
    );
}

#[test]
fn collection_iterator_methods_use_actual_receiver_and_release_completed_owner() {
    let first = Js::new_map();
    first
        .call_method("set", vec![Js::str("first"), Js::Number(1.)])
        .unwrap();
    let second = Js::new_map();
    second
        .call_method("set", vec![Js::str("second"), Js::Number(2.)])
        .unwrap();
    let factory = first.get("values").unwrap();
    let iterator = factory.call(second.clone(), vec![]).unwrap();
    assert_eq!(
        iterator
            .call_method("next", vec![])
            .unwrap()
            .get("value")
            .unwrap()
            .as_f64(),
        Some(2.)
    );
    assert!(factory.call(Js::Undefined, vec![]).is_err());
    let first_iterator = first.call_method("values", vec![]).unwrap();
    let next = first_iterator.get("next").unwrap();
    let another = first.call_method("values", vec![]).unwrap();
    assert_eq!(
        next.call(another, vec![])
            .unwrap()
            .get("value")
            .unwrap()
            .as_f64(),
        Some(1.)
    );
    assert!(
        next.call(Js::new_set().iterator().unwrap(), vec![])
            .is_err()
    );
    let weak = if let Js::Map(entries) = &second {
        std::rc::Rc::downgrade(entries)
    } else {
        unreachable!()
    };
    drop(second);
    assert!(
        weak.upgrade().is_some(),
        "suspended iterator owns its collection"
    );
    assert!(
        iterator
            .call_method("next", vec![])
            .unwrap()
            .get("done")
            .unwrap()
            .truthy()
    );
    assert!(
        weak.upgrade().is_none(),
        "terminal iterator releases its collection"
    );
}

#[test]
fn collection_iterators_feed_live_array_from_and_collection_constructors() {
    let globals = globals();
    let set = Js::new_set();
    set.call_method("add", vec![Js::str("a")]).unwrap();
    set.call_method("add", vec![Js::str("b")]).unwrap();
    let live = set.clone();
    let mapper = Js::function("mapper", move |this, args| {
        assert!(this.strict_equals(&Js::str("receiver")));
        if args[1].as_f64() == Some(0.) {
            live.call_method("add", vec![Js::str("c")])?;
        }
        Ok(args[0].clone())
    });
    let result = globals
        .get("Array")
        .unwrap()
        .call_method(
            "from",
            vec![set.iterator().unwrap(), mapper, Js::str("receiver")],
        )
        .unwrap();
    assert_eq!(
        to_json(&result).unwrap(),
        json!(["a", "b", "c"]),
        "Array.from must invoke the mapper between iterator steps"
    );
    let map = Js::new_map();
    map.call_method("set", vec![Js::str("key"), Js::Number(-0.)])
        .unwrap();
    let copy = globals
        .get("Map")
        .unwrap()
        .call(Js::Undefined, vec![map.iterator().unwrap()])
        .unwrap();
    assert_eq!(
        copy.call_method("get", vec![Js::str("key")])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    let copy = globals
        .get("Set")
        .unwrap()
        .call(Js::Undefined, vec![set.iterator().unwrap()])
        .unwrap();
    assert_eq!(
        to_json(&Js::array(copy.iterable_values().unwrap())).unwrap(),
        json!(["a", "b", "c"])
    );
}

#[test]
fn array_from_closes_iterator_on_mapper_error_preserving_original_throw() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    let globals = globals();
    let map = Js::new_map();
    map.call_method("set", vec![Js::str("key"), Js::Number(1.)])
        .unwrap();
    let iterator = map.iterator().unwrap();
    assert_eq!(iterator.to_display(), "[object Map Iterator]");
    let closed = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = closed.clone();
    iterator
        .define(
            "return",
            Js::function("return", move |_, args| {
                assert!(args.is_empty());
                observed.set(observed.get() + 1);
                Ok(Js::Number(7.))
            }),
        )
        .unwrap();
    let original = Js::object(vec![("sentinel", Js::str("original mapper error"))]);
    let thrown = original.clone();
    let mapper = Js::function("mapper", move |_, _| Err(Thrown::new(thrown.clone())));
    let result = globals
        .get("Array")
        .unwrap()
        .call_method("from", vec![iterator, mapper])
        .unwrap_err();
    assert!(
        result.value.same_reference(&original),
        "abrupt IteratorClose preserves the original throw"
    );
    assert_eq!(closed.get(), 1);
}
