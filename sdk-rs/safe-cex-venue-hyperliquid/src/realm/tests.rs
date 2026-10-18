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
                let args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
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

#[test]
fn production_seam_history_mapper_preserves_the_throw_without_inspecting_it() {
    for kind in ["ordinary", "error"] {
        let (mut r, _) = production_realm();
        let reads = Rc::new(std::cell::RefCell::new(Vec::new()));
        let original = if kind == "error" {
            r.error_of("RangeError", "authored callback throw")
        } else {
            r.new_object()
        };
        let coercions = reads.clone();
        let to_string = r.native("toString", 0, move |_, _, _| {
            coercions.borrow_mut().push("toString");
            Ok(Value::str("authored ordinary throw"))
        });
        r.define_hidden(&original, PropKey::str("toString"), to_string);
        for name in ["message", "code", "response"] {
            let getters = reads.clone();
            let getter = r.native("spy", 0, move |_, _, _| {
                getters.borrow_mut().push(name);
                Ok(if name == "message" {
                    Value::str("authored callback throw")
                } else {
                    Value::Undefined
                })
            });
            r.define_getter(&original, PropKey::str(name), getter);
        }
        let thrown = original.clone();
        let calls = Rc::new(std::cell::Cell::new(0));
        let invoked = calls.clone();
        let resolver = r.native("throwing resolver", 1, move |_, _, _| {
            invoked.set(invoked.get() + 1);
            Err(thrown.clone())
        });
        let row=r.from_serde(&serde_json::json!({"order":{"oid":1,"coin":"BTC","timestamp":123,"side":"B","origSz":"1","sz":"0","orderType":"Limit","limitPx":"10"},"status":"filled","statusTimestamp":124}));
        let mapper = r.global_binding("mapHyperliquidHistoricalOrder").unwrap();
        let error = r
            .call(&mapper, &Value::Undefined, &[row, resolver])
            .unwrap_err();
        assert!(
            error.strict_equal(&original),
            "original thrown object identity: {kind}"
        );
        assert_eq!(calls.get(), 1, "the real resolver must execute");
        assert!(
            reads.borrow().is_empty(),
            "propagating a callback throw must not coerce or inspect it: {kind}: {:?}",
            reads.borrow()
        );
    }
}

#[test]
fn production_seam_history_imports_execute_the_maintained_helpers() {
    let (mut r, _) = production_realm();
    let make_row = |status: &str, sz: &str, time: u64| serde_json::json!({"order":{"oid":77,"coin":"BTC","timestamp":1699999998000u64,"side":"B","origSz":"0.25","sz":sz,"orderType":"Limit","limitPx":"60000"},"status":status,"statusTimestamp":time});
    let input = r.from_serde(&serde_json::json!([
        make_row("open", "0.25", 1699999998000),
        make_row("filled", "0", 1699999999000)
    ]));
    let collapse = r.global_binding("collapseHyperliquidOrderHistory").unwrap();
    let rows = r.call(&collapse, &Value::Undefined, &[input]).unwrap();
    assert_eq!(r.get_str(&rows, "length").unwrap().as_number(), Some(1.0));
    let row = r.get_str(&rows, "0").unwrap();
    let resolve = r.native("resolve symbol", 1, |_, this, args| {
        assert!(matches!(this, Value::Undefined));
        assert!(args[0].strict_equal(&Value::str("BTC")));
        Ok(Value::str("BTC-PERP"))
    });
    let mapper = r.global_binding("mapHyperliquidHistoricalOrder").unwrap();
    let mapped = r
        .call(&mapper, &Value::Undefined, &[row, resolve.clone()])
        .unwrap();
    let order: safe_cex_types::HistoricalOrder = r.to_typed(&mapped).unwrap();
    assert_eq!(order.id, "77");
    assert_eq!(order.symbol, "BTC-PERP");
    assert_eq!(order.filled, 0.25);
    assert_eq!(order.status, safe_cex_types::OrderStatus::Closed);
    let fills=r.from_serde(&serde_json::json!([{"coin":"BTC","time":1699999999000u64,"dir":"Close Long","px":"62000","sz":"0.25","closedPnl":"125","fee":"2"}]));
    let derive = r
        .global_binding("deriveHyperliquidClosedPositions")
        .unwrap();
    let derived = r
        .call(
            &derive,
            &Value::Undefined,
            &[fills, resolve, Value::str("authored-account")],
        )
        .unwrap();
    assert_eq!(
        r.get_str(&derived, "length").unwrap().as_number(),
        Some(1.0)
    );
    let entry = r.get_str(&derived, "0").unwrap();
    for (key, expected) in [
        ("notional", 15500.0),
        ("contracts", 0.25),
        ("exitPrice", 62000.0),
        ("unrealizedPnl", 125.0),
        ("fees", 2.0),
        ("closedTime", 1699999999000.0),
    ] {
        assert_eq!(
            r.get_str(&entry, key).unwrap().as_number(),
            Some(expected),
            "{key}"
        );
    }
    for (key, expected) in [
        ("provenance", "derived"),
        ("symbol", "BTC-PERP"),
        ("account", "authored-account"),
        ("side", "long"),
    ] {
        let value = r.get_str(&entry, key).unwrap();
        assert_eq!(text(&mut r, &value), expected, "{key}");
    }
    let classifier = r
        .global_binding("isOnlyHyperliquidTerminalCancelError")
        .unwrap();
    for (message, expected) in [
        (
            "Order was never placed, already canceled, or filled. asset=0",
            true,
        ),
        (
            "Order was never placed, already canceled, or filled. asset=0, denied",
            false,
        ),
        ("denied", false),
    ] {
        assert!(
            matches!(r.call(&classifier,&Value::Undefined,&[Value::str(message)]).unwrap(),Value::Bool(v) if v==expected)
        );
    }
    let object = r.new_object();
    let poison = r.native("toString", 0, |_, _, _| {
        panic!("terminal classifier must not coerce non-strings")
    });
    r.define_hidden(&object, PropKey::str("toString"), poison);
    assert!(matches!(
        r.call(&classifier, &Value::Undefined, &[object]).unwrap(),
        Value::Bool(false)
    ));
}

#[test]
fn production_seam_timeout_leads_are_actual_class_properties() {
    let (mut r, rt) = production_realm();
    let base = Rc::new(safe_cex_exchange::exchange::BaseExchange::new(
        safe_cex_types::ExchangeName::Hyperliquid,
        serde_json::from_value(serde_json::json!({"key":"","secret":""})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        rt,
        safe_cex_exchange::exchange::ExchangeSettings::default(),
    ));
    let base = super::facade_exchange::base_exchange_class(&mut r, base, None);
    let class = crate::core_exchange_class::create_core_exchange_class(&mut r, &base).unwrap();
    for (name, expected) in [
        ("HIP3_DEX_DISCOVERY_TIMEOUT_MS", 6000.0),
        ("HIP3_MARKETS_LOAD_TIMEOUT_MS", 8000.0),
        ("HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS", 15000.0),
        ("HIP3_PER_DEX_LOAD_BUDGET_MS", 2000.0),
        ("SPOT_MARKETS_LOAD_TIMEOUT_MS", 6000.0),
    ] {
        assert_eq!(
            r.get_str(&class, name).unwrap().as_number(),
            Some(expected),
            "{name}"
        );
        assert!(
            r.global_binding(name).is_err(),
            "{name} is read from the class, not an invented free global"
        );
    }
}

#[test]
fn production_seam_browser_window_guards_refuse_before_browser_globals() {
    let (mut r, rt) = production_realm();
    for name in [
        "window",
        "ethers",
        "hasEthereumWallet",
        "generateAgentWalletName",
        "switchToCorrectNetwork",
        "reportEvent",
    ] {
        assert!(
            r.global_binding(name).is_err(),
            "native does not install browser stand-ins: {name}"
        );
    }
    for mode in 0..4 {
        let mut host = r.clone();
        let output = r.spawn_value(Box::pin(async move {
            use crate::browser_approval_flows as browser;
            match mode {
                0 => browser::agent_switch_to_correct_network(&mut host, &Value::Bool(false)).await,
                1 => {
                    browser::builder_switch_to_correct_network(&mut host, &Value::Bool(false)).await
                }
                2 => {
                    browser::create_agent_wallet_with_browser_wallet(
                        &mut host,
                        &Value::Undefined,
                        &Value::Bool(false),
                        &Value::Undefined,
                    )
                    .await
                }
                _ => {
                    browser::approve_builder_fee_with_browser_wallet(
                        &mut host,
                        &Value::Undefined,
                        &Value::Bool(false),
                        &Value::str("0x1111111111111111111111111111111111111111"),
                        &Value::str("0.001%"),
                    )
                    .await
                }
            }
        }));
        rt.drain();
        let cell = output.promise().unwrap();
        let result = match &cell.borrow().state {
            PromiseState::Fulfilled(v) => v.clone(),
            _ => panic!(
                "native missing-window guard must return its explicit wallet refusal: mode{mode}"
            ),
        };
        assert!(matches!(
            r.get_str(&result, "success").unwrap(),
            Value::Bool(false)
        ));
        let field = if mode < 2 { "error" } else { "message" };
        let message = r.get_str(&result, field).unwrap();
        assert!(text(&mut r, &message).contains("No Ethereum wallet detected"));
    }
}

#[test]
fn market_host_iterator_rejects_non_callable_collection_override() {
    use crate::market_bootstrap::MarketBootstrapHost;
    let (mut r, _) = realm();
    let array = r.new_array(vec![Value::Number(1.0)]);
    let key = PropKey::Symbol(r.intrinsics().symbol_iterator.clone());
    r.define(&array, key, Value::Null).unwrap();
    assert!(
        MarketBootstrapHost::iterator(&mut r, &array).is_err(),
        "actual market host must reject a disabled collection iterator"
    );
}

#[test]
fn market_host_iterator_calls_override_even_with_builtin_function_name() {
    use crate::market_bootstrap::MarketBootstrapHost;
    let (mut r, _) = realm();
    let array = r.new_array(vec![Value::Number(1.0)]);
    let alternative = r.new_array(vec![Value::Number(42.0)]);
    let override_method = r.native("[Symbol.iterator]", 0, move |r, _, _| {
        r.get_iterator(&alternative)
    });
    let key = PropKey::Symbol(r.intrinsics().symbol_iterator.clone());
    r.define(&array, key, override_method).unwrap();
    let iterator = MarketBootstrapHost::iterator(&mut r, &array).unwrap();
    let value = MarketBootstrapHost::next(&mut r, &iterator)
        .unwrap()
        .unwrap();
    assert!(
        value.strict_equal(&Value::Number(42.0)),
        "actual market host must call the original iterator method regardless of its name"
    );
}

#[test]
fn market_host_iterator_rejects_primitive_iterator_before_first_step() {
    use crate::market_bootstrap::MarketBootstrapHost;
    let (mut r, _) = realm();
    let array = r.new_array(vec![]);
    let override_method = r.native("custom", 0, |_, _, _| Ok(Value::Number(17.0)));
    let key = PropKey::Symbol(r.intrinsics().symbol_iterator.clone());
    r.define(&array, key, override_method).unwrap();
    assert!(
        MarketBootstrapHost::iterator(&mut r, &array).is_err(),
        "actual market host must reject a primitive iterator before reading next"
    );
}

#[test]
fn production_builder_import_preserves_configured_value_and_lazy_default() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/native-builder-config.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 5);
    let (mut r, _) = production_realm();
    let imported = r.global_binding("resolveHyperliquidBuilder").unwrap();
    let address = "0x1111111111111111111111111111111111111111";
    let builders = [
        r.object_from(vec![
            ("address", Value::str(address)),
            ("feeTenthsBps", Value::str("10")),
        ]),
        r.object_from(vec![("feeTenthsBps", Value::Number(7.))]),
        Value::Bool(false),
        Value::Number(17.),
        Value::str("raw"),
    ];
    for (index, builder) in builders.into_iter().enumerate() {
        let options = r.object_from(vec![
            ("hyperliquidBuilder", builder.clone()),
            ("testnet", Value::Bool(true)),
        ]);
        let reads = Rc::new(std::cell::Cell::new(0));
        let count = reads.clone();
        let getter = r.native("get testnet", 0, move |_, _, _| {
            count.set(count.get() + 1);
            Ok(Value::Bool(true))
        });
        r.define_getter(&options, PropKey::str("testnet"), getter);
        let result = r.call(&imported, &Value::Undefined, &[options]).unwrap();
        assert_eq!(
            reads.get(),
            0,
            "configured builder must skip the native default getter"
        );
        assert!(
            result.strict_equal(&builder),
            "configured builder identity/type case {index}"
        );
        assert_eq!(
            r.to_serde(&result).unwrap(),
            source[index]["result"],
            "source case {index}"
        );
        assert_eq!(source[index]["same"], true);
        assert_eq!(source[index]["testnetReads"], 0);
    }
}

#[test]
fn production_builder_import_uses_fresh_nullish_defaults() {
    let output = std::process::Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/native-builder-config.cjs"
            ),
            "--defaults",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 5);
    let (mut r, _) = production_realm();
    let imported = r.global_binding("resolveHyperliquidBuilder").unwrap();
    let inputs = [
        Value::Undefined,
        Value::Null,
        r.new_object(),
        r.object_from(vec![("testnet", Value::Bool(true))]),
        r.object_from(vec![
            ("testnet", Value::Bool(true)),
            ("hyperliquidBuilder", Value::Null),
        ]),
    ];
    for (index, options) in inputs.into_iter().enumerate() {
        let result = r
            .call(&imported, &Value::Undefined, std::slice::from_ref(&options))
            .unwrap();
        let second = r.call(&imported, &Value::Undefined, &[options]).unwrap();
        assert!(!result.strict_equal(&second), "fresh default case {index}");
        let fee = r.get_str(&result, "feeTenthsBps").unwrap();
        assert_eq!(fee.as_number().unwrap().to_bits(), 10f64.to_bits());
        assert_eq!(r.to_serde(&result).unwrap(), source[index]["result"]);
        assert_eq!(source[index]["fresh"], true);
    }
}

#[test]
fn production_builder_consumer_keeps_the_fee_type_in_the_action() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let (mut r, _) = production_realm();
    let builder = r.object_from(vec![
        (
            "address",
            Value::str("0xABCDEF1111111111111111111111111111111111"),
        ),
        ("feeTenthsBps", Value::str("10")),
    ]);
    let options = r.object_from(vec![
        ("applicationId", Value::str("account")),
        ("hyperliquidBuilder", builder),
    ]);
    let instance = r.object_from(vec![
        ("options", options),
        ("_builderFeeApproved", Value::Bool(true)),
    ]);
    let mut future = Box::pin(crate::order_request_leaves::get_builder_info(
        &mut r, &instance,
    ));
    let Poll::Ready(Ok(info)) = future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("approved builder consumer should complete immediately");
    };
    drop(future);
    let fee = r.get_str(&info, "f").unwrap();
    assert!(
        fee.strict_equal(&Value::str("10")),
        "actual builder action must retain the source string fee"
    );
    let address = r.get_str(&info, "b").unwrap();
    assert_eq!(
        text(&mut r, &address),
        "0xabcdef1111111111111111111111111111111111"
    );
    let action_factory = r.global_binding("orderWiresToOrderAction").unwrap();
    let wires = r.new_array(vec![]);
    let action = r
        .call(&action_factory, &Value::Undefined, &[wires, info.clone()])
        .unwrap();
    let actual = r.get_str(&action, "builder").unwrap();
    assert!(actual.strict_equal(&info));
    let encoded = r.to_js_json(&action).unwrap();
    let safe_cex_num::JsJsonValue::Object(fields) = encoded else {
        panic!("action object");
    };
    let builder = fields.iter().find(|(k, _)| k == "builder").unwrap();
    let safe_cex_num::JsJsonValue::Object(fields) = &builder.1 else {
        panic!("builder object");
    };
    assert!(
        matches!(&fields.iter().find(|(k,_)|k=="f").unwrap().1,safe_cex_num::JsJsonValue::String(fee) if fee=="10")
    );
}

#[test]
fn production_order_wire_import_keeps_native_nonfinite_fields() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/native-order-wire.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 6);
    let (mut r, _) = production_realm();
    let imported = r.global_binding("orderRequestToOrderWire").unwrap();
    for (index, number) in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        60000.125,
        1e30,
    ]
    .into_iter()
    .enumerate()
    {
        let trigger = r.object_from(vec![
            ("isMarket", Value::Bool(true)),
            ("triggerPx", Value::Number(number)),
            ("tpsl", Value::str("sl")),
        ]);
        let order_type = r.object_from(vec![("trigger", trigger)]);
        let order = r.object_from(vec![
            ("is_buy", Value::Bool(true)),
            ("limit_px", Value::Number(number)),
            ("sz", Value::Number(number)),
            ("reduce_only", Value::Bool(false)),
            ("order_type", order_type),
        ]);
        let wire = r
            .call(&imported, &Value::Undefined, &[order, Value::Number(7.)])
            .unwrap_or_else(|error| {
                panic!("native wire case {index}: {}", message(&mut r, &error))
            });
        assert_eq!(r.get_str(&wire, "a").unwrap().as_number(), Some(7.));
        assert!(
            r.get_str(&wire, "b")
                .unwrap()
                .strict_equal(&Value::Bool(true))
        );
        assert!(
            r.get_str(&wire, "r")
                .unwrap()
                .strict_equal(&Value::Bool(false))
        );
        let expected = ["NaN", "Infinity", "-Infinity", "0", "60000.125", "1e+3"][index];
        for field in ["p", "s"] {
            let value = r.get_str(&wire, field).unwrap();
            assert_eq!(
                text(&mut r, &value),
                expected,
                "native {field} case {index}"
            );
        }
        assert_eq!(
            r.to_serde(&wire).unwrap(),
            source[index],
            "maintained source case {index}"
        );
    }
}

#[test]
fn production_order_wire_import_retains_undefined_and_limit_alias() {
    let (mut r, _) = production_realm();
    let imported = r.global_binding("orderRequestToOrderWire").unwrap();
    let limit = r.object_from(vec![("tif", Value::str("Gtc"))]);
    let order_type = r.object_from(vec![("limit", limit.clone())]);
    let order = r.object_from(vec![
        ("limit_px", Value::Number(11.25)),
        ("sz", Value::Number(3.5)),
        ("order_type", order_type),
        ("cloid", Value::Null),
    ]);
    let wire = r
        .call(&imported, &Value::Undefined, &[order, Value::Undefined])
        .unwrap();
    assert_eq!(r.own_entries(&wire).unwrap().len(), 7);
    for key in ["a", "b", "r"] {
        assert!(
            matches!(r.get_str(&wire, key).unwrap(), Value::Undefined),
            "native own-undefined {key}"
        );
    }
    assert!(matches!(r.get_str(&wire, "c").unwrap(), Value::Null));
    let order_type = r.get_str(&wire, "t").unwrap();
    let retained = r.get_str(&order_type, "limit").unwrap();
    assert!(retained.strict_equal(&limit));
    r.set_str(&limit, "tif", Value::str("Alo")).unwrap();
    let observed = r.get_str(&retained, "tif").unwrap();
    assert_eq!(text(&mut r, &observed), "Alo");
}
