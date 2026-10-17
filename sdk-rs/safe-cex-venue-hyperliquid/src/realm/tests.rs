use super::{Realm, test_runtime::TestRuntime, value::*};
use std::rc::Rc;

fn realm() -> (Realm, Rc<TestRuntime>) {
    let runtime = Rc::new(TestRuntime::default());
    (Realm::new(runtime.clone()), runtime)
}
fn text(r: &mut Realm, v: &Value) -> String {
    r.to_rust_string(v).unwrap()
}
fn message(r: &mut Realm, e: &Value) -> String {
    let m = r.get_str(e, "message").unwrap();
    text(r, &m)
}

#[test]
fn property_reads_follow_the_prototype_chain_and_nullish_reads_throw() {
    let (mut r, _) = realm();
    let o = r.object_from(vec![("a", Value::Number(1.0))]);
    assert!(
        r.get_str(&o, "a")
            .unwrap()
            .strict_equal(&Value::Number(1.0))
    );
    let has = r.get_str(&o, "hasOwnProperty").unwrap();
    assert!(r.call(&has, &o, &[Value::str("a")]).unwrap().truthy());
    let e = r.get_str(&Value::Undefined, "x").unwrap_err();
    assert_eq!(
        message(&mut r, &e),
        "Cannot read properties of undefined (reading 'x')"
    );
    let name = r.get_str(&e, "name").unwrap();
    assert_eq!(text(&mut r, &name), "TypeError");
}

#[test]
fn array_methods_and_exotic_length() {
    let (mut r, _) = realm();
    let a = r.new_array(vec![
        Value::Number(3.0),
        Value::Number(1.0),
        Value::Number(2.0),
    ]);
    let doubled = r.native("", 1, |r, _, args| {
        r.binary("*", &args[0], &Value::Number(2.0))
    });
    let mapped = r.call_method(&a, "map", &[doubled]).unwrap();
    let joined = r.call_method(&mapped, "join", &[Value::str("-")]).unwrap();
    assert_eq!(text(&mut r, &joined), "6-2-4");
    r.call_method(&a, "push", &[Value::Number(9.0)]).unwrap();
    let len = r.get_str(&a, "length").unwrap();
    assert_eq!(len.as_number(), Some(4.0));
    r.set_str(&a, "length", Value::Number(1.0)).unwrap();
    let s = r.call_method(&a, "toString", &[]).unwrap();
    assert_eq!(text(&mut r, &s), "3");
    let parts = r
        .call_method(
            &Value::str("15m"),
            "split",
            &[r.regexp(r"/(\d+)/").unwrap()],
        )
        .unwrap();
    let joined = r.call_method(&parts, "join", &[Value::str("|")]).unwrap();
    assert_eq!(text(&mut r, &joined), "|15|m");
}

#[test]
fn map_set_iteration_is_live_and_insertion_ordered() {
    let (mut r, _) = realm();
    let map_ctor = r.global_binding("Map").unwrap();
    let m = r.construct(&map_ctor, &[], None).unwrap();
    r.call_method(&m, "set", &[Value::str("a"), Value::Number(1.0)])
        .unwrap();
    r.call_method(&m, "set", &[Value::str("b"), Value::Number(2.0)])
        .unwrap();
    let it = r.get_iterator(&m).unwrap();
    let first = r.iterator_step(&it).unwrap().unwrap();
    r.call_method(&m, "set", &[Value::str("c"), Value::Number(3.0)])
        .unwrap();
    let k = r.get_str(&first, "0").unwrap();
    let mut keys = vec![text(&mut r, &k)];
    while let Some(e) = r.iterator_step(&it).unwrap() {
        let k = r.get_str(&e, "0").unwrap();
        keys.push(text(&mut r, &k));
    }
    assert_eq!(keys, ["a", "b", "c"]);
}

#[test]
fn json_round_trip_keeps_key_order_and_js_numbers() {
    let (mut r, _) = realm();
    let units: Vec<u16> = r#"{"b":1,"a":[1.5,null,"x"],"2":true,"1":-0}"#.encode_utf16().collect();
    let v = r.parse_json(&units).unwrap();
    let out = r.stringify(&v).unwrap().unwrap();
    // Integer keys enumerate first in ascending order, as in V8; -0 prints as 0.
    assert_eq!(
        String::from_utf16_lossy(&out),
        r#"{"1":0,"2":true,"b":1,"a":[1.5,null,"x"]}"#
    );
    let bad: Vec<u16> = "{oops".encode_utf16().collect();
    let e = r.parse_json(&bad).unwrap_err();
    let name = r.get_str(&e, "name").unwrap();
    assert_eq!(text(&mut r, &name), "SyntaxError");
}

#[test]
fn promises_adopt_and_then_chains_settle_on_the_runtime() {
    let (mut r, rt) = realm();
    let promise = r.global_binding("Promise").unwrap();
    let inner = r
        .call_method(&promise, "resolve", &[Value::Number(2.0)])
        .unwrap();
    let add = r.native("", 1, |r, _, args| {
        r.binary("+", &args[0], &Value::Number(40.0))
    });
    let chained = r.call_method(&inner, "then", &[add]).unwrap();
    let outer = r.spawn_value(r.await_value(chained));
    rt.drain();
    let cell = outer.promise().unwrap();
    match &cell.borrow().state {
        PromiseState::Fulfilled(v) => assert_eq!(v.as_number(), Some(42.0)),
        _ => panic!("promise did not fulfil"),
    };
}

#[test]
fn timers_fire_on_the_virtual_clock_and_clear_cancels() {
    let (mut r, rt) = realm();
    let hits = Rc::new(std::cell::Cell::new(0));
    let h = hits.clone();
    let cb = r.native("", 0, move |_, _, _| {
        h.set(h.get() + 1);
        Ok(Value::Undefined)
    });
    let set_timeout = r.global_binding("setTimeout").unwrap();
    let clear = r.global_binding("clearTimeout").unwrap();
    r.call(
        &set_timeout,
        &Value::Undefined,
        &[cb.clone(), Value::Number(100.0)],
    )
    .unwrap();
    let id = r
        .call(&set_timeout, &Value::Undefined, &[cb, Value::Number(50.0)])
        .unwrap();
    r.call(&clear, &Value::Undefined, &[id]).unwrap();
    rt.drain();
    assert_eq!(hits.get(), 1);
    assert_eq!(rt.now.get(), 100.0);
}

#[test]
fn operators_follow_abstract_semantics() {
    let (mut r, _) = realm();
    let sum = r
        .binary("+", &Value::str("a"), &Value::Number(1.0))
        .unwrap();
    assert_eq!(text(&mut r, &sum), "a1");
    assert!(
        r.binary("<", &Value::str("10"), &Value::Number(9.0))
            .unwrap()
            .strict_equal(&Value::Bool(false))
    );
    assert!(
        r.binary(">=", &Value::Number(f64::NAN), &Value::Number(1.0))
            .unwrap()
            .strict_equal(&Value::Bool(false))
    );
    assert!(
        r.binary("==", &Value::Null, &Value::Undefined)
            .unwrap()
            .truthy()
    );
    let e = r.error("boom");
    let s = r.to_string(&e).unwrap();
    assert_eq!(String::from_utf16_lossy(&s), "Error: boom");
}

#[test]
fn number_to_string_with_radix_matches_v8() {
    use super::builtins::radix_string;
    // Values printed by Node v24.20.0 (`v.toString(r)`).
    for (v, radix, expected) in [
        (0.5, 36, "0.i"),
        (255.0, 16, "ff"),
        (-255.5, 2, "-11111111.1"),
        (
            0.1,
            2,
            "0.0001100110011001100110011001100110011001100110011001101",
        ),
        (0.123456789, 36, "0.4fzzzxjylrx"),
        (1e21, 36, "5v1j4f4ds7c000"),
        (123.456, 7, "234.312256641535441"),
        (std::f64::consts::PI, 16, "3.243f6a8885a3"),
    ] {
        assert_eq!(radix_string(v, radix), expected, "{v}.toString({radix})");
    }
}

fn production_realm() -> (Realm, Rc<TestRuntime>) {
    let (realm, runtime) = realm();
    super::imports::bind(&realm);
    (realm, runtime)
}

#[test]
fn production_emitter_preserves_empty_array_and_multiple_arguments() {
    use std::cell::RefCell;

    let (mut r, rt) = production_realm();
    let options = serde_json::from_value(serde_json::json!({"key":"","secret":""})).unwrap();
    let base = Rc::new(safe_cex_exchange::exchange::BaseExchange::new(
        safe_cex_types::ExchangeName::Hyperliquid,
        options,
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        rt,
        safe_cex_exchange::exchange::ExchangeSettings::default(),
    ));
    let seen = Rc::new(RefCell::new(Vec::new()));
    for event in ["arity", "warning", "log"] {
        let seen = seen.clone();
        base.on_args(
            event,
            Rc::new(move |event, args| {
                seen.borrow_mut().push((event.to_owned(), args.to_vec()));
                Ok(())
            }),
        );
    }
    let class = super::facade_exchange::base_exchange_class(&mut r, base, None);
    let options = r.new_object();
    let store = r.new_object();
    let instance = r
        .construct(&class, &[Value::str("hyperliquid"), options, store], None)
        .unwrap();
    let emitter = r.get_str(&instance, "emitter").unwrap();
    r.call_method(&emitter, "emit", &[Value::str("arity")])
        .unwrap();
    let array = r.new_array(vec![Value::Number(7.0), Value::str("eight")]);
    r.call_method(&emitter, "emit", &[Value::str("arity"), array])
        .unwrap();
    let metadata = r.object_from(vec![("code", Value::Number(42.0))]);
    r.call_method(
        &emitter,
        "emit",
        &[Value::str("warning"), Value::str("venue warning"), metadata],
    )
    .unwrap();
    r.call_method(
        &instance,
        "log",
        &[Value::str("native log"), Value::str("Warning")],
    )
    .unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![
            ("arity".into(), vec![]),
            ("arity".into(), vec![serde_json::json!([7, "eight"])]),
            (
                "warning".into(),
                vec![
                    serde_json::json!("venue warning"),
                    serde_json::json!({"code":42})
                ]
            ),
            (
                "log".into(),
                vec![
                    serde_json::json!("[hyperliquid] native log"),
                    serde_json::json!("Warning")
                ]
            ),
        ],
        "the production facade must retain the emitter's actual argument list"
    );
}

#[test]
fn production_import_registry_uses_real_helpers_and_rejects_unknown_names() {
    let (mut r, _) = production_realm();
    for name in [
        "hasSeenFill",
        "markFillSeen",
        "isFinitePositiveNumber",
        "mapSeries",
        "hyperliquidOrderResponseHasDeferredOrderStatus",
    ] {
        assert!(r.global_binding(name).unwrap().is_callable(), "{name}");
    }
    let error = r
        .global_binding("__unknown_hyperliquid_import")
        .unwrap_err();
    assert_eq!(
        message(&mut r, &error),
        "__unknown_hyperliquid_import is not defined"
    );
    let seen = r.global_binding("hasSeenFill").unwrap();
    let mark = r.global_binding("markFillSeen").unwrap();
    let id = Value::str("authored-fill-id");
    assert!(matches!(
        r.call(&seen, &Value::Undefined, std::slice::from_ref(&id))
            .unwrap(),
        Value::Bool(false)
    ));
    assert!(matches!(
        r.call(&mark, &Value::Undefined, std::slice::from_ref(&id))
            .unwrap(),
        Value::Undefined
    ));
    let factory = r.global_binding("getCrossCache").unwrap();
    let cache = r.call(&factory, &Value::Undefined, &[]).unwrap();
    assert!(matches!(
        r.call_method(&cache, "hasSeenFill", std::slice::from_ref(&id))
            .unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        r.call(&seen, &Value::Undefined, &[id]).unwrap(),
        Value::Bool(true)
    ));
    let predicate = r.global_binding("isFinitePositiveNumber").unwrap();
    for (number, expected) in [
        (f64::NAN, false),
        (f64::INFINITY, false),
        (-f64::INFINITY, false),
        (-0.0, false),
        (0.0, false),
        (-1.0, false),
        (1.25, true),
    ] {
        assert!(
            matches!(r.call(&predicate, &Value::Undefined, &[Value::Number(number)]).unwrap(), Value::Bool(value) if value==expected)
        );
    }
    let object = r.new_object();
    let poison = r.native("toJSON", 0, |_, _, _| {
        panic!("strict numeric predicate must not inspect an object's toJSON")
    });
    r.define_hidden(&object, PropKey::str("toJSON"), poison);
    for value in [
        Value::Undefined,
        Value::Null,
        Value::Bool(true),
        Value::str("1.25"),
        object,
    ] {
        assert!(matches!(
            r.call(&predicate, &Value::Undefined, &[value]).unwrap(),
            Value::Bool(false)
        ));
    }
}

#[test]
fn production_import_map_series_awaits_values_and_callbacks_and_reads_live_length() {
    let (mut r, rt) = production_realm();
    let series = r.global_binding("mapSeries").unwrap();
    let promised = r.settled_promise(Ok(Value::Number(3.0)));
    let input = r.new_array(vec![Value::Number(2.0), promised]);
    let expected_input = input.clone();
    let receiver = r.new_object();
    let expected_receiver = receiver.clone();
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = trace.clone();
    let callback = r.native("serial-callback", 3, move |r, this, args| {
        assert!(this.strict_equal(&expected_receiver));
        assert!(args[2].strict_equal(&expected_input));
        let value = args[0].as_number().unwrap();
        let index = args[1].as_number().unwrap();
        recorded.borrow_mut().push(("start", value));
        if index == 0.0 {
            r.call_method(&expected_input, "push", &[Value::Number(4.0)])?;
        }
        let sleep = r.runtime().sleep(10);
        let ended = recorded.clone();
        Ok(r.spawn_value(Box::pin(async move {
            sleep.await.unwrap();
            ended.borrow_mut().push(("end", value));
            Ok(Value::Number(value * 10.0))
        })))
    });
    let output = r
        .call(&series, &Value::Undefined, &[input, callback, receiver])
        .unwrap();
    rt.drain();
    let cell = output.promise().unwrap();
    let result = match &cell.borrow().state {
        PromiseState::Fulfilled(v) => v.clone(),
        _ => panic!("serial result must fulfill"),
    };
    assert_eq!(
        *trace.borrow(),
        [
            ("start", 2.0),
            ("end", 2.0),
            ("start", 3.0),
            ("end", 3.0),
            ("start", 4.0),
            ("end", 4.0)
        ]
    );
    assert_eq!(r.get_str(&result, "length").unwrap().as_number(), Some(3.0));
    for (i, n) in [20.0, 30.0, 40.0].into_iter().enumerate() {
        assert_eq!(
            r.get_str(&result, &i.to_string()).unwrap().as_number(),
            Some(n)
        );
    }
}

#[test]
fn production_import_map_series_retains_module_receiver_and_original_rejection() {
    let (mut r, rt) = production_realm();
    let series = r.global_binding("mapSeries").unwrap();
    let expected_series = series.clone();
    let original = r.error_of("RangeError", "authored serial rejection");
    let thrown = original.clone();
    let calls = Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    let callback = r.native("reject", 1, move |r, this, _| {
        assert!(r.get_str(this, "mapSeries")?.strict_equal(&expected_series));
        observed.set(observed.get() + 1);
        Ok(r.settled_promise(Err(thrown.clone())))
    });
    let input = r.new_array(vec![Value::Number(1.0), Value::Number(2.0)]);
    let output = r
        .call(&series, &Value::Undefined, &[input, callback])
        .unwrap();
    rt.drain();
    let cell = output.promise().unwrap();
    match &cell.borrow().state {
        PromiseState::Rejected(error) => assert!(error.strict_equal(&original)),
        _ => panic!("serial error must reject"),
    };
    assert_eq!(calls.get(), 1, "no later callback after rejection");
}

#[test]
fn production_import_map_series_resolves_callback_call_before_awaited_item() {
    let (mut r, rt) = production_realm();
    let series = r.global_binding("mapSeries").unwrap();
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let input = r.object_from(vec![("length", Value::Number(1.0))]);
    let item_trace = trace.clone();
    let item = r.native("item", 0, move |r, _, _| {
        item_trace.borrow_mut().push("get item");
        Ok(r.settled_promise(Ok(Value::Number(2.0))))
    });
    r.define_getter(&input, PropKey::str("0"), item);
    let callback = r.native("callback", 1, |_, _, _| {
        panic!("own call must replace direct invocation")
    });
    let expected_callback = callback.clone();
    let receiver = r.new_object();
    let expected_receiver = receiver.clone();
    let call_trace = trace.clone();
    let own_call = r.native("own call", 4, move |_, this, args| {
        assert!(this.strict_equal(&expected_callback));
        assert!(args[0].strict_equal(&expected_receiver));
        assert_eq!(args[1].as_number(), Some(2.0));
        assert_eq!(args[2].as_number(), Some(0.0));
        call_trace.borrow_mut().push("own call");
        Ok(Value::Number(20.0))
    });
    let getter_trace = trace.clone();
    let getter = r.native("get call", 0, move |_, _, _| {
        getter_trace.borrow_mut().push("get callback.call");
        Ok(own_call.clone())
    });
    r.define_getter(&callback, PropKey::str("call"), getter);
    let output = r
        .call(&series, &Value::Undefined, &[input, callback, receiver])
        .unwrap();
    rt.drain();
    assert_eq!(
        *trace.borrow(),
        ["get callback.call", "get item", "own call"]
    );
    let cell = output.promise().unwrap();
    let result = match &cell.borrow().state {
        PromiseState::Fulfilled(v) => v.clone(),
        _ => panic!("own call result must fulfill"),
    };
    assert_eq!(r.get_str(&result, "0").unwrap().as_number(), Some(20.0));
}
