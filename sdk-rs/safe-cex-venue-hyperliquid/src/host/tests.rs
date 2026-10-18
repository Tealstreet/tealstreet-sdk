use super::{Host, test_runtime::TestRuntime};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

fn realm() -> (Host, Rc<TestRuntime>) {
    let runtime = Rc::new(TestRuntime::default());
    (crate::host::test_runtime::host(runtime.clone()), runtime)
}
fn text(r: &mut Host, v: &Js) -> String {
    r.to_rust_string(v).unwrap()
}
fn message(r: &mut Host, e: &Thrown) -> String {
    let m = r.get_str(&e.value, "message").unwrap();
    text(r, &m)
}

#[test]
fn property_reads_follow_the_prototype_chain_and_nullish_reads_throw() {
    let (mut r, _) = realm();
    let o = r.object_from(vec![("a", Js::Number(1.0))]);
    assert!(r.get_str(&o, "a").unwrap().strict_equals(&Js::Number(1.0)));
    let e = r.get_str(&Js::Undefined, "x").unwrap_err();
    assert_eq!(
        message(&mut r, &e),
        "Cannot read properties of undefined (reading 'x')"
    );
    let name = r.get_str(&e.value, "name").unwrap();
    assert_eq!(text(&mut r, &name), "TypeError");
}

#[test]
fn array_methods_and_exotic_length() {
    let (mut r, _) = realm();
    let a = r.new_array(vec![Js::Number(3.0), Js::Number(1.0), Js::Number(2.0)]);
    let doubled = r.native("", 1, |r, _, args| {
        r.binary("*", &args[0], &Js::Number(2.0))
    });
    let mapped = r.call_method(&a, "map", &[doubled]).unwrap();
    let joined = r.call_method(&mapped, "join", &[Js::str("-")]).unwrap();
    assert_eq!(text(&mut r, &joined), "6-2-4");
    r.call_method(&a, "push", &[Js::Number(9.0)]).unwrap();
    let len = r.get_str(&a, "length").unwrap();
    assert_eq!(len.as_f64(), Some(4.0));
    r.set_str(&a, "length", Js::Number(1.0)).unwrap();
    let s = r.call_method(&a, "toString", &[]).unwrap();
    assert_eq!(text(&mut r, &s), "3");
    let parts = r
        .call_method(&Js::str("15m"), "split", &[r.regexp(r"/(\d+)/").unwrap()])
        .unwrap();
    let joined = r.call_method(&parts, "join", &[Js::str("|")]).unwrap();
    assert_eq!(text(&mut r, &joined), "|15|m");
}

#[test]
fn map_set_iteration_is_live_and_insertion_ordered() {
    let (mut r, _) = realm();
    let map_ctor = r.global_binding("Map").unwrap();
    let m = r.construct(&map_ctor, &[], None).unwrap();
    r.call_method(&m, "set", &[Js::str("a"), Js::Number(1.0)])
        .unwrap();
    r.call_method(&m, "set", &[Js::str("b"), Js::Number(2.0)])
        .unwrap();
    let it = r.get_iterator(&m).unwrap();
    let first = r.iterator_step(&it).unwrap().unwrap();
    r.call_method(&m, "set", &[Js::str("c"), Js::Number(3.0)])
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
    let name = r.get_str(&e.value, "name").unwrap();
    assert_eq!(text(&mut r, &name), "SyntaxError");
}

#[test]
fn promises_adopt_and_then_chains_settle_on_the_runtime() {
    let (mut r, rt) = realm();
    let promise = r.global_binding("Promise").unwrap();
    let inner = r
        .call_method(&promise, "resolve", &[Js::Number(2.0)])
        .unwrap();
    let add = r.native("", 1, |r, _, args| {
        r.binary("+", &args[0], &Js::Number(40.0))
    });
    let chained = r.call_method(&inner, "then", &[add]).unwrap();
    let outer = r.spawn_value(r.await_value(chained));
    rt.drain();
    let Js::Promise(cell) = outer else {
        panic!("expected promise")
    };
    match cell.result() {
        Some(Ok(v)) => assert_eq!(v.as_f64(), Some(42.0)),
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
        Ok(Js::Undefined)
    });
    let set_timeout = r.global_binding("setTimeout").unwrap();
    let clear = r.global_binding("clearTimeout").unwrap();
    r.call(
        &set_timeout,
        &Js::Undefined,
        &[cb.clone(), Js::Number(100.0)],
    )
    .unwrap();
    let id = r
        .call(&set_timeout, &Js::Undefined, &[cb, Js::Number(50.0)])
        .unwrap();
    r.call(&clear, &Js::Undefined, &[id]).unwrap();
    rt.drain();
    assert_eq!(hits.get(), 1);
    assert_eq!(rt.now.get(), 100.0);
}

#[test]
fn operators_follow_abstract_semantics() {
    let (mut r, _) = realm();
    let sum = r.binary("+", &Js::str("a"), &Js::Number(1.0)).unwrap();
    assert_eq!(text(&mut r, &sum), "a1");
    assert!(
        r.binary("<", &Js::str("10"), &Js::Number(9.0))
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    assert!(
        r.binary(">=", &Js::Number(f64::NAN), &Js::Number(1.0))
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    let e = r.error("boom");
    let s = r.to_string(&e.value).unwrap();
    assert_eq!(String::from_utf16_lossy(&s), "Error: boom");
}

#[test]
fn number_to_string_with_radix_matches_v8() {
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
        let mut host = realm().0;
        let value = host
            .call_method(&Js::Number(v), "toString", &[Js::Number(radix as f64)])
            .unwrap();
        assert_eq!(text(&mut host, &value), expected, "{v}.toString({radix})");
    }
}

fn production_realm() -> (Host, Rc<TestRuntime>) {
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
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        rt,
        safe_cex_exchange::exchange::ExchangeSettings::default(),
    ));
    use safe_cex_exchange::js_runtime::{globals::stringify_json, value::Js};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/realm_emitter_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 5);
    assert_eq!(source["sourceIdentity"], true);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let second = Rc::new(RefCell::new(Vec::new()));
    for event in ["arity", "warning", "log", "numbers"] {
        let seen = seen.clone();
        base.on_args(
            event,
            Rc::new(move |event, args| {
                seen.borrow_mut().push((event.to_owned(), args.to_vec()));
                Ok(())
            }),
        );
        let second = second.clone();
        base.on_args(
            event,
            Rc::new(move |_, args| {
                second.borrow_mut().push(args.to_vec());
                Ok(())
            }),
        );
    }
    let class = super::facade_exchange::base_exchange_class(&mut r, base, None);
    let options = r.new_object();
    let store = r.new_object();
    let instance = r
        .construct(&class, &[Js::str("hyperliquid"), options, store], None)
        .unwrap();
    let emitter = r.get_str(&instance, "emitter").unwrap();
    r.call_method(&emitter, "emit", &[Js::str("arity")])
        .unwrap();
    let array = r.new_array(vec![Js::Number(7.0), Js::str("eight")]);
    r.call_method(&emitter, "emit", &[Js::str("arity"), array.clone()])
        .unwrap();
    let metadata = r.object_from(vec![("code", Js::Number(42.0))]);
    r.call_method(
        &emitter,
        "emit",
        &[Js::str("warning"), Js::str("venue warning"), metadata],
    )
    .unwrap();
    r.call_method(
        &instance,
        "log",
        &[Js::str("native log"), Js::str("Warning")],
    )
    .unwrap();
    let numbers = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 7.0, 42.0];
    let mut args = vec![Js::str("numbers")];
    args.extend(numbers.into_iter().map(Js::Number));
    r.call_method(&emitter, "emit", &args).unwrap();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 5, "production emitter event count");
    assert_eq!(second.borrow().len(), 5, "second native listener count");
    assert_eq!(
        seen.iter().map(|(_, args)| args.len()).collect::<Vec<_>>(),
        vec![0, 1, 2, 2, 6]
    );
    let array_number = seen[1].1[0].get("0").unwrap();
    let code = seen[2].1[1].get("code").unwrap();
    let numeric_values = [array_number, code]
        .into_iter()
        .chain(seen[4].1.iter().cloned());
    assert_eq!(source["bits"].as_array().unwrap().len(), 8);
    for (index, value) in numeric_values.enumerate() {
        let Js::Number(value) = value else {
            panic!("native emitter numeric type {index}");
        };
        assert_eq!(
            format!("{:016x}", value.to_bits()),
            source["bits"][index].as_str().unwrap(),
            "native emitter bits {index}"
        );
    }
    for (index, (event, args)) in seen.iter().enumerate() {
        assert_eq!(event, source["rows"][index]["event"].as_str().unwrap());
        for (left, right) in args.iter().zip(&second.borrow()[index]) {
            assert!(
                left.same_reference(right)
                    || left.strict_equals(right)
                    || matches!((left,right),(Js::Number(a),Js::Number(b)) if a.to_bits()==b.to_bits()),
                "both listeners receive original native argument {index}"
            );
        }
        let units = stringify_json(&Js::array(args.clone())).unwrap().unwrap();
        assert_eq!(
            String::from_utf16(&units).unwrap(),
            source["rows"][index]["json"].as_str().unwrap(),
            "real JS stringify emitter arguments {index}"
        );
    }

    r.set_str(&array, "0", Js::Number(8.0)).unwrap();
    let Js::Number(number) = seen[1].1[0].get("0").unwrap() else {
        panic!("HL_REALM_ORIGINAL_ARRAY_IDENTITY native number type");
    };
    assert_eq!(
        number.to_bits(),
        8.0_f64.to_bits(),
        "HL_REALM_ORIGINAL_ARRAY_IDENTITY source mutation remains visible"
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
    let id = Js::str("authored-fill-id");
    assert!(matches!(
        r.call(&seen, &Js::Undefined, std::slice::from_ref(&id))
            .unwrap(),
        Js::Bool(false)
    ));
    assert!(matches!(
        r.call(&mark, &Js::Undefined, std::slice::from_ref(&id))
            .unwrap(),
        Js::Undefined
    ));
    let factory = r.global_binding("getCrossCache").unwrap();
    let cache = r.call(&factory, &Js::Undefined, &[]).unwrap();
    assert!(matches!(
        r.call_method(&cache, "hasSeenFill", std::slice::from_ref(&id))
            .unwrap(),
        Js::Bool(true)
    ));
    assert!(matches!(
        r.call(&seen, &Js::Undefined, &[id]).unwrap(),
        Js::Bool(true)
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
            matches!(r.call(&predicate, &Js::Undefined, &[Js::Number(number)]).unwrap(), Js::Bool(value) if value==expected)
        );
    }
    let object = r.new_object();
    let poison = r.native("toJSON", 0, |_, _, _| {
        panic!("strict numeric predicate must not inspect an object's toJSON")
    });
    r.define_hidden(&object, Js::str("toJSON"), poison);
    for value in [
        Js::Undefined,
        Js::Null,
        Js::Bool(true),
        Js::str("1.25"),
        object,
    ] {
        assert!(matches!(
            r.call(&predicate, &Js::Undefined, &[value]).unwrap(),
            Js::Bool(false)
        ));
    }
}

#[test]
fn production_import_map_series_awaits_values_and_callbacks_and_reads_live_length() {
    let (mut r, rt) = production_realm();
    let series = r.global_binding("mapSeries").unwrap();
    let promised = r.settled_promise(Ok(Js::Number(3.0)));
    let input = r.new_array(vec![Js::Number(2.0), promised]);
    let expected_input = input.clone();
    let receiver = r.new_object();
    let expected_receiver = receiver.clone();
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = trace.clone();
    let callback = r.native("serial-callback", 3, move |r, this, args| {
        assert!(this.strict_equals(&expected_receiver));
        assert!(args[2].strict_equals(&expected_input));
        let value = args[0].as_f64().unwrap();
        let index = args[1].as_f64().unwrap();
        recorded.borrow_mut().push(("start", value));
        if index == 0.0 {
            r.call_method(&expected_input, "push", &[Js::Number(4.0)])?;
        }
        let sleep = r.runtime().sleep(10);
        let ended = recorded.clone();
        Ok(r.spawn_value(Box::pin(async move {
            sleep.await.unwrap();
            ended.borrow_mut().push(("end", value));
            Ok(Js::Number(value * 10.0))
        })))
    });
    let output = r
        .call(&series, &Js::Undefined, &[input, callback, receiver])
        .unwrap();
    rt.drain();
    let Js::Promise(cell) = output else {
        panic!("expected promise")
    };
    let result = match cell.result() {
        Some(Ok(v)) => v.clone(),
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
    assert_eq!(r.get_str(&result, "length").unwrap().as_f64(), Some(3.0));
    for (i, n) in [20.0, 30.0, 40.0].into_iter().enumerate() {
        assert_eq!(
            r.get_str(&result, &i.to_string()).unwrap().as_f64(),
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
        assert!(
            r.get_str(this, "mapSeries")?
                .strict_equals(&expected_series)
        );
        observed.set(observed.get() + 1);
        Ok(r.settled_promise(Err(thrown.clone())))
    });
    let input = r.new_array(vec![Js::Number(1.0), Js::Number(2.0)]);
    let output = r.call(&series, &Js::Undefined, &[input, callback]).unwrap();
    rt.drain();
    let Js::Promise(cell) = output else {
        panic!("expected promise")
    };
    match cell.result() {
        Some(Err(error)) => assert!(error.value.strict_equals(&original.value)),
        _ => panic!("serial error must reject"),
    };
    assert_eq!(calls.get(), 1, "no later callback after rejection");
}

#[test]
fn production_import_map_series_resolves_callback_call_before_awaited_item() {
    let (mut r, rt) = production_realm();
    let series = r.global_binding("mapSeries").unwrap();
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let input = r.object_from(vec![("length", Js::Number(1.0))]);
    let item_trace = trace.clone();
    let item = r.native("item", 0, move |r, _, _| {
        item_trace.borrow_mut().push("get item");
        Ok(r.settled_promise(Ok(Js::Number(2.0))))
    });
    r.define_getter(&input, Js::str("0"), item);
    let callback = r.native("callback", 1, |_, _, _| {
        panic!("own call must replace direct invocation")
    });
    let expected_callback = callback.clone();
    let receiver = r.new_object();
    let expected_receiver = receiver.clone();
    let call_trace = trace.clone();
    let own_call = r.native("own call", 4, move |_, this, args| {
        assert!(this.strict_equals(&expected_callback));
        assert!(args[0].strict_equals(&expected_receiver));
        assert_eq!(args[1].as_f64(), Some(2.0));
        assert_eq!(args[2].as_f64(), Some(0.0));
        call_trace.borrow_mut().push("own call");
        Ok(Js::Number(20.0))
    });
    let getter_trace = trace.clone();
    let getter = r.native("get call", 0, move |_, _, _| {
        getter_trace.borrow_mut().push("get callback.call");
        Ok(own_call.clone())
    });
    r.define_getter(&callback, Js::str("call"), getter);
    let output = r
        .call(&series, &Js::Undefined, &[input, callback, receiver])
        .unwrap();
    rt.drain();
    assert_eq!(
        *trace.borrow(),
        ["get callback.call", "get item", "own call"]
    );
    let Js::Promise(cell) = output else {
        panic!("expected promise")
    };
    let result = match cell.result() {
        Some(Ok(v)) => v.clone(),
        _ => panic!("own call result must fulfill"),
    };
    assert_eq!(r.get_str(&result, "0").unwrap().as_f64(), Some(20.0));
}

#[test]
fn production_seam_history_mapper_preserves_the_throw_without_inspecting_it() {
    for kind in ["ordinary", "error"] {
        let (mut r, _) = production_realm();
        let reads = Rc::new(std::cell::RefCell::new(Vec::new()));
        let original = if kind == "error" {
            r.error_of("RangeError", "authored callback throw").value
        } else {
            r.new_object()
        };
        let coercions = reads.clone();
        let to_string = r.native("toString", 0, move |_, _, _| {
            coercions.borrow_mut().push("toString");
            Ok(Js::str("authored ordinary throw"))
        });
        r.define_hidden(&original, Js::str("toString"), to_string);
        for name in ["message", "code", "response"] {
            let getters = reads.clone();
            let getter = r.native("spy", 0, move |_, _, _| {
                getters.borrow_mut().push(name);
                Ok(if name == "message" {
                    Js::str("authored callback throw")
                } else {
                    Js::Undefined
                })
            });
            r.define_getter(&original, Js::str(name), getter);
        }
        let thrown = original.clone();
        let calls = Rc::new(std::cell::Cell::new(0));
        let invoked = calls.clone();
        let resolver = r.native("throwing resolver", 1, move |_, _, _| {
            invoked.set(invoked.get() + 1);
            Err(Thrown::new(thrown.clone()))
        });
        let row=r.encode_json(&serde_json::json!({"order":{"oid":1,"coin":"BTC","timestamp":123,"side":"B","origSz":"1","sz":"0","orderType":"Limit","limitPx":"10"},"status":"filled","statusTimestamp":124}));
        let mapper = r.global_binding("mapHyperliquidHistoricalOrder").unwrap();
        let error = r
            .call(&mapper, &Js::Undefined, &[row, resolver])
            .unwrap_err();
        assert!(
            error.value.strict_equals(&original),
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
    let input = r.encode_json(&serde_json::json!([
        make_row("open", "0.25", 1699999998000),
        make_row("filled", "0", 1699999999000)
    ]));
    let collapse = r.global_binding("collapseHyperliquidOrderHistory").unwrap();
    let rows = r.call(&collapse, &Js::Undefined, &[input]).unwrap();
    assert_eq!(r.get_str(&rows, "length").unwrap().as_f64(), Some(1.0));
    let row = r.get_str(&rows, "0").unwrap();
    let resolve = r.native("resolve symbol", 1, |_, this, args| {
        assert!(matches!(this, Js::Undefined));
        assert!(args[0].strict_equals(&Js::str("BTC")));
        Ok(Js::str("BTC-PERP"))
    });
    let mapper = r.global_binding("mapHyperliquidHistoricalOrder").unwrap();
    let mapped = r
        .call(&mapper, &Js::Undefined, &[row, resolve.clone()])
        .unwrap();
    let order: safe_cex_types::HistoricalOrder = r.to_typed(&mapped).unwrap();
    assert_eq!(order.id, "77");
    assert_eq!(order.symbol, "BTC-PERP");
    assert_eq!(order.filled, 0.25);
    assert_eq!(order.status, safe_cex_types::OrderStatus::Closed);
    let fills=r.encode_json(&serde_json::json!([{"coin":"BTC","time":1699999999000u64,"dir":"Close Long","px":"62000","sz":"0.25","closedPnl":"125","fee":"2"}]));
    let derive = r
        .global_binding("deriveHyperliquidClosedPositions")
        .unwrap();
    let derived = r
        .call(
            &derive,
            &Js::Undefined,
            &[fills, resolve, Js::str("authored-account")],
        )
        .unwrap();
    assert_eq!(r.get_str(&derived, "length").unwrap().as_f64(), Some(1.0));
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
            r.get_str(&entry, key).unwrap().as_f64(),
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
            matches!(r.call(&classifier,&Js::Undefined,&[Js::str(message)]).unwrap(),Js::Bool(v) if v==expected)
        );
    }
    let object = r.new_object();
    let poison = r.native("toString", 0, |_, _, _| {
        panic!("terminal classifier must not coerce non-strings")
    });
    r.define_hidden(&object, Js::str("toString"), poison);
    assert!(matches!(
        r.call(&classifier, &Js::Undefined, &[object]).unwrap(),
        Js::Bool(false)
    ));
}

#[test]
fn production_seam_timeout_leads_are_actual_class_properties() {
    let (mut r, rt) = production_realm();
    let base = Rc::new(safe_cex_exchange::exchange::BaseExchange::new(
        safe_cex_types::ExchangeName::Hyperliquid,
        serde_json::from_value(serde_json::json!({"key":"","secret":""})).unwrap(),
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
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
            r.get_str(&class, name).unwrap().as_f64(),
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
    for name in ["window", "switchToCorrectNetwork"] {
        assert!(
            r.global_binding(name).is_err(),
            "native does not invent an ambient browser/module binding: {name}"
        );
    }
    for mode in 0..4 {
        let mut host = r.clone();
        let output = r.spawn_value(Box::pin(async move {
            use crate::browser_approval_flows as browser;
            match mode {
                0 => browser::agent_switch_to_correct_network(&mut host, &Js::Bool(false)).await,
                1 => browser::builder_switch_to_correct_network(&mut host, &Js::Bool(false)).await,
                2 => {
                    browser::create_agent_wallet_with_browser_wallet(
                        &mut host,
                        &Js::Undefined,
                        &Js::Bool(false),
                        &Js::Undefined,
                    )
                    .await
                }
                _ => {
                    browser::approve_builder_fee_with_browser_wallet(
                        &mut host,
                        &Js::Undefined,
                        &Js::Bool(false),
                        &Js::str("0x1111111111111111111111111111111111111111"),
                        &Js::str("0.001%"),
                    )
                    .await
                }
            }
        }));
        rt.drain();
        let Js::Promise(cell) = output else {
            panic!("expected promise")
        };
        let result = match cell.result() {
            Some(Ok(v)) => v.clone(),
            _ => panic!(
                "native missing-window guard must return its explicit wallet refusal: mode{mode}"
            ),
        };
        assert!(matches!(
            r.get_str(&result, "success").unwrap(),
            Js::Bool(false)
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
    let array = r.new_array(vec![Js::Number(1.0)]);
    let key = Js::SymbolIterator;
    r.define(&array, key, Js::Null).unwrap();
    assert!(
        MarketBootstrapHost::iterator(&mut r, &array).is_err(),
        "actual market host must reject a disabled collection iterator"
    );
}

#[test]
fn market_host_iterator_calls_override_even_with_builtin_function_name() {
    use crate::market_bootstrap::MarketBootstrapHost;
    let (mut r, _) = realm();
    let array = r.new_array(vec![Js::Number(1.0)]);
    let alternative = r.new_array(vec![Js::Number(42.0)]);
    let override_method = r.native("[Symbol.iterator]", 0, move |_, _, _| {
        alternative.iterator()
    });
    let key = Js::SymbolIterator;
    r.define(&array, key, override_method).unwrap();
    let iterator = MarketBootstrapHost::iterator(&mut r, &array).unwrap();
    let value = MarketBootstrapHost::next(&mut r, &iterator)
        .unwrap()
        .unwrap();
    assert!(
        value.strict_equals(&Js::Number(42.0)),
        "actual market host must call the original iterator method regardless of its name"
    );
}

#[test]
fn market_host_iterator_rejects_primitive_iterator_before_first_step() {
    use crate::market_bootstrap::MarketBootstrapHost;
    let (mut r, _) = realm();
    let array = r.new_array(vec![]);
    let override_method = r.native("custom", 0, |_, _, _| Ok(Js::Number(17.0)));
    let key = Js::SymbolIterator;
    r.define(&array, key, override_method).unwrap();
    assert!(
        MarketBootstrapHost::iterator(&mut r, &array).is_err(),
        "actual market host must reject a primitive iterator before reading next"
    );
}

#[test]
fn native_module_imports_match_the_maintained_browser_guard_and_signature_bodies() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg("sdk-rs/safe-cex-venue-hyperliquid/tests/native-imports-source.cjs")
        .current_dir(root)
        .output()
        .expect("maintained module source");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let (mut host, runtime) = production_realm();
    for (name, field) in [
        ("TEALSTREET_BUILDER_FEE", "fee"),
        ("DEFAULT_BUILDER_FEE_TENTHS_BPS", "tenths"),
        ("DEFAULT_BUILDER_FEE_RATE", "rate"),
    ] {
        let value = host.global_binding(name).unwrap();
        assert_eq!(source_json(&mut host, &value), source["constants"][field]);
    }
    let ethers = host.global_binding("ethers").unwrap();
    assert!(ethers.get("BrowserProvider").unwrap().is_undefined());
    let signature = ethers.get("Signature").unwrap();
    for (index, row) in source["signatureCases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let input = host.encode_json(&row["input"]);
        let parsed = signature.call_method("from", vec![input]);
        let projected = parsed.and_then(|value| {
            Ok(Js::object(vec![
                ("r", value.get("r")?),
                ("s", value.get("s")?),
                ("v", value.get("v")?),
            ]))
        });
        match projected {
            Ok(value) => assert_eq!(
                source_json(&mut host, &value),
                row["result"],
                "Signature.from case {index}"
            ),
            Err(error) => {
                let fields = Js::object(vec![]);
                for name in [
                    "name",
                    "message",
                    "code",
                    "shortMessage",
                    "argument",
                    "value",
                ] {
                    let mut value = error.value.get(name).unwrap();
                    if matches!(value, Js::BigInt(_)) {
                        value = Js::String(value.to_js_string().unwrap());
                    }
                    fields.define(name, value).unwrap();
                }
                assert_eq!(
                    source_json(&mut host, &fields),
                    row["error"],
                    "Signature.from case {index}"
                );
                let descriptors = Js::object(vec![]);
                let object = host.global_binding("Object").unwrap();
                for name in ["message", "code", "shortMessage", "argument", "value"] {
                    let descriptor = object
                        .call_method(
                            "getOwnPropertyDescriptor",
                            vec![error.value.clone(), Js::str(name)],
                        )
                        .unwrap();
                    descriptor
                        .delete(&"value".encode_utf16().collect::<Vec<_>>())
                        .unwrap();
                    descriptors.define(name, descriptor).unwrap();
                }
                assert_eq!(
                    source_json(&mut host, &descriptors),
                    row["descriptors"],
                    "Signature.from error descriptors {index}"
                );
            }
        }
    }
    let supplied = Js::object(vec![]);
    let receiver = supplied.clone();
    let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = calls.clone();
    supplied
        .define(
            "signTypedData",
            host.native("signTypedData", 3, move |_, this, args| {
                assert!(this.same_reference(&receiver));
                seen.borrow_mut().push(args[2].clone());
                Ok(Js::str(&format!(
                    "0x{}{}1b",
                    "11".repeat(32),
                    "22".repeat(32)
                )))
            }),
        )
        .unwrap();
    let sign = host.global_binding("signBuilderFeeApproval").unwrap();
    let pending = sign
        .call(
            Js::Undefined,
            vec![
                supplied,
                Js::str(crate::constants::TEALSTREET_BUILDER_ADDRESS_MAINNET),
                Js::Undefined,
                Js::Bool(false),
                Js::Number(42.0),
            ],
        )
        .unwrap();
    runtime.drain();
    let Js::Promise(pending) = pending else {
        panic!("async module helper")
    };
    let result = pending
        .result()
        .expect("supplied native signer completes")
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(source_json(&mut host, &result), source["signature"]);
    assert_eq!(
        calls.borrow()[0].get("maxFeeRate").unwrap().to_display(),
        "0.01%"
    );
    assert!(host.global_binding("window").is_err());
    let math = host.global_binding("Math").unwrap();
    let original_math = math.clone();
    let draws = Rc::new(std::cell::Cell::new(0));
    let reads = Rc::new(std::cell::Cell::new(0));
    let count = draws.clone();
    let observed = reads.clone();
    math.define_accessor(
        "random",
        Some(Rc::new(move |receiver| {
            assert!(receiver.same_reference(&original_math));
            observed.set(observed.get() + 1);
            let count = count.clone();
            let original_math = original_math.clone();
            Ok(Js::function("random", move |receiver, _| {
                assert!(receiver.same_reference(&original_math));
                let index = count.get();
                count.set(index + 1);
                Ok(Js::Number([0.0, 0.5, 1.0 - f64::EPSILON, f64::NAN][index]))
            }))
        })),
        None,
    )
    .unwrap();
    let name = host
        .global_binding("generateAgentWalletName")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    assert_eq!(source_json(&mut host, &name), source["name"]);
    assert_eq!(reads.get(), 4);
    assert_eq!(draws.get(), 4);
    let available = host
        .global_binding("hasEthereumWallet")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    assert!(available.strict_equals(&Js::Bool(false)));
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let thrown = cause.clone();
    let window = Js::object(vec![]);
    window
        .define_accessor(
            "ethereum",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .unwrap();
    host.set_global("window", window);
    let error = host
        .global_binding("hasEthereumWallet")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap_err();
    assert!(error.value.same_reference(&cause));
    host.shared
        .globals
        .delete(&"window".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    let report = host.global_binding("reportEvent").unwrap();
    let payload = Js::object(vec![("amount", Js::Number(-0.0))]);
    assert!(
        report
            .call(Js::Undefined, vec![Js::str("no-sink"), payload.clone()])
            .unwrap()
            .is_undefined()
    );
    struct Sink {
        observed: Rc<std::cell::RefCell<Option<Js>>>,
        cause: Js,
    }
    impl safe_cex_types::telemetry::TelemetrySink for Sink {
        fn event(&self, _: &str, data: Rc<dyn std::any::Any>) -> Result<(), Rc<dyn std::any::Any>> {
            *self.observed.borrow_mut() = Some(
                data.downcast_ref::<Js>()
                    .expect("original native payload")
                    .clone(),
            );
            Err(Rc::new(self.cause.clone()))
        }
    }
    let observed = Rc::new(std::cell::RefCell::new(None));
    *host.shared.extra.telemetry.borrow_mut() = Some(Rc::new(Sink {
        observed: observed.clone(),
        cause: cause.clone(),
    }));
    let error = report
        .call(Js::Undefined, vec![Js::str("event"), payload.clone()])
        .unwrap_err();
    assert!(error.value.same_reference(&cause));
    assert!(observed.borrow().as_ref().unwrap().same_reference(&payload));
}

#[test]
fn installed_substr_uses_utf16_indices_and_source_coercion_order() {
    let (mut host, _) = realm();
    let text = Js::utf16(&[65, 0xd800, 66, 0xdc00]);
    let cases = vec![
        (vec![], vec![65, 0xd800, 66, 0xdc00]),
        (vec![Js::Number(1.0), Js::Number(1.0)], vec![0xd800]),
        (vec![Js::Number(-2.0)], vec![66, 0xdc00]),
        (vec![Js::Number(f64::INFINITY), Js::Number(1.0)], vec![]),
        (
            vec![Js::Number(f64::NEG_INFINITY), Js::Number(1.0)],
            vec![65],
        ),
        (
            vec![Js::Number(f64::NAN), Js::Number(2.0)],
            vec![65, 0xd800],
        ),
        (
            vec![Js::Number(1.0), Js::Number(f64::INFINITY)],
            vec![0xd800, 66, 0xdc00],
        ),
        (vec![Js::Number(1.0), Js::Number(f64::NEG_INFINITY)], vec![]),
    ];
    let script = "const s=String.fromCharCode(65,0xd800,66,0xdc00);const rows=[[],[1,1],[-2],[Infinity,1],[-Infinity,1],[NaN,2],[1,Infinity],[1,-Infinity]];console.log(JSON.stringify(rows.map(args=>Array.from({length:s.substr(...args).length},(_,i)=>s.substr(...args).charCodeAt(i)))));";
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .output()
        .unwrap();
    assert!(output.status.success());
    let source: Vec<Vec<u16>> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), cases.len());
    for ((args, expected), source) in cases.into_iter().zip(source) {
        assert_eq!(expected, source);
        assert_eq!(
            host.call_method(&text, "substr", &args)
                .unwrap()
                .as_str()
                .map(|units| units.as_ref()),
            Some(expected.as_slice())
        );
    }
    let order = Rc::new(std::cell::RefCell::new(vec![]));
    let start = Js::object(vec![]);
    let count = Js::object(vec![]);
    let first = order.clone();
    start
        .define(
            "valueOf",
            Js::function("start", move |_, _| {
                first.borrow_mut().push("start");
                Ok(Js::Number(1.0))
            }),
        )
        .unwrap();
    let second = order.clone();
    let cause = Js::object(vec![]);
    let thrown = cause.clone();
    count
        .define(
            "valueOf",
            Js::function("count", move |_, _| {
                second.borrow_mut().push("count");
                Err(Thrown::new(thrown.clone()))
            }),
        )
        .unwrap();
    let error = host
        .call_method(&text, "substr", &[start, count])
        .unwrap_err();
    assert!(error.value.same_reference(&cause));
    assert_eq!(*order.borrow(), ["start", "count"]);
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
            ("address", Js::str(address)),
            ("feeTenthsBps", Js::str("10")),
        ]),
        r.object_from(vec![("feeTenthsBps", Js::Number(7.))]),
        Js::Bool(false),
        Js::Number(17.),
        Js::str("raw"),
    ];
    for (index, builder) in builders.into_iter().enumerate() {
        let options = r.object_from(vec![
            ("hyperliquidBuilder", builder.clone()),
            ("testnet", Js::Bool(true)),
        ]);
        let reads = Rc::new(std::cell::Cell::new(0));
        let count = reads.clone();
        let getter = r.native("get testnet", 0, move |_, _, _| {
            count.set(count.get() + 1);
            Ok(Js::Bool(true))
        });
        r.define_getter(&options, Js::str("testnet"), getter);
        let result = r.call(&imported, &Js::Undefined, &[options]).unwrap();
        assert_eq!(
            reads.get(),
            0,
            "configured builder must skip the native default getter"
        );
        assert!(
            result.strict_equals(&builder),
            "configured builder identity/type case {index}"
        );
        assert_eq!(
            source_json(&mut r, &result),
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
        Js::Undefined,
        Js::Null,
        r.new_object(),
        r.object_from(vec![("testnet", Js::Bool(true))]),
        r.object_from(vec![
            ("testnet", Js::Bool(true)),
            ("hyperliquidBuilder", Js::Null),
        ]),
    ];
    for (index, options) in inputs.into_iter().enumerate() {
        let result = r
            .call(&imported, &Js::Undefined, std::slice::from_ref(&options))
            .unwrap();
        let second = r.call(&imported, &Js::Undefined, &[options]).unwrap();
        assert!(!result.strict_equals(&second), "fresh default case {index}");
        let fee = r.get_str(&result, "feeTenthsBps").unwrap();
        assert_eq!(fee.as_f64().unwrap().to_bits(), 10f64.to_bits());
        assert_eq!(source_json(&mut r, &result), source[index]["result"]);
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
            Js::str("0xABCDEF1111111111111111111111111111111111"),
        ),
        ("feeTenthsBps", Js::str("10")),
    ]);
    let options = r.object_from(vec![
        ("applicationId", Js::str("account")),
        ("hyperliquidBuilder", builder),
    ]);
    let instance = r.object_from(vec![
        ("options", options),
        ("_builderFeeApproved", Js::Bool(true)),
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
        fee.strict_equals(&Js::str("10")),
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
        .call(&action_factory, &Js::Undefined, &[wires, info.clone()])
        .unwrap();
    let actual = r.get_str(&action, "builder").unwrap();
    assert!(actual.strict_equals(&info));
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
            ("isMarket", Js::Bool(true)),
            ("triggerPx", Js::Number(number)),
            ("tpsl", Js::str("sl")),
        ]);
        let order_type = r.object_from(vec![("trigger", trigger)]);
        let order = r.object_from(vec![
            ("is_buy", Js::Bool(true)),
            ("limit_px", Js::Number(number)),
            ("sz", Js::Number(number)),
            ("reduce_only", Js::Bool(false)),
            ("order_type", order_type),
        ]);
        let wire = r
            .call(&imported, &Js::Undefined, &[order, Js::Number(7.)])
            .unwrap_or_else(|error| {
                panic!("native wire case {index}: {}", message(&mut r, &error))
            });
        assert_eq!(r.get_str(&wire, "a").unwrap().as_f64(), Some(7.));
        assert!(
            r.get_str(&wire, "b")
                .unwrap()
                .strict_equals(&Js::Bool(true))
        );
        assert!(
            r.get_str(&wire, "r")
                .unwrap()
                .strict_equals(&Js::Bool(false))
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
            source_json(&mut r, &wire),
            source[index],
            "maintained source case {index}"
        );
    }
}

#[test]
fn production_order_wire_import_retains_undefined_and_limit_alias() {
    let (mut r, _) = production_realm();
    let imported = r.global_binding("orderRequestToOrderWire").unwrap();
    let limit = r.object_from(vec![("tif", Js::str("Gtc"))]);
    let order_type = r.object_from(vec![("limit", limit.clone())]);
    let order = r.object_from(vec![
        ("limit_px", Js::Number(11.25)),
        ("sz", Js::Number(3.5)),
        ("order_type", order_type),
        ("cloid", Js::Null),
    ]);
    let wire = r
        .call(&imported, &Js::Undefined, &[order, Js::Undefined])
        .unwrap();
    assert_eq!(r.own_entries(&wire).unwrap().len(), 7);
    for key in ["a", "b", "r"] {
        assert!(
            matches!(r.get_str(&wire, key).unwrap(), Js::Undefined),
            "native own-undefined {key}"
        );
    }
    assert!(matches!(r.get_str(&wire, "c").unwrap(), Js::Null));
    let order_type = r.get_str(&wire, "t").unwrap();
    let retained = r.get_str(&order_type, "limit").unwrap();
    assert!(retained.strict_equals(&limit));
    r.set_str(&limit, "tif", Js::str("Alo")).unwrap();
    let observed = r.get_str(&retained, "tif").unwrap();
    assert_eq!(text(&mut r, &observed), "Alo");
}

fn source_json(host: &mut Host, value: &Js) -> serde_json::Value {
    let units = host
        .stringify(value)
        .unwrap()
        .expect("source output has a JSON value");
    let text = String::from_utf16(&units).unwrap();
    let result: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        text,
        serde_json::to_string(&result).unwrap(),
        "canonical JS stringify uses the source numeric spelling"
    );
    result
}

#[test]
fn shared_spread_rechecks_descriptors_and_keeps_iterator_symbol_aliases() {
    let output = std::process::Command::new("node").args(["-e", r#"
const assert=require('node:assert/strict'), token={}, source={};
Object.defineProperty(source,'first',{enumerable:true,get(){assert.equal(this,source);Object.defineProperty(source,'second',{enumerable:false});return -0;}});
source.second=2;source[Symbol.iterator]=token;const result={...source};assert.equal(result[Symbol.iterator],token);assert(Object.is(result.first,-0));console.log(JSON.stringify(Object.keys(result)));
"#]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let (mut context, _) = production_realm();
    let source = context.new_object();
    let receiver = source.clone();
    source.define_accessor("first", Some(Rc::new(move |this| {
        assert!(this.same_reference(&receiver));
        this.define_property(&"second".encode_utf16().collect::<Vec<_>>(), &Js::object(vec![("enumerable", Js::Bool(false))]))?;
        Ok(Js::Number(-0.0))
    })), None).unwrap();
    source.define("second", Js::Number(2.0)).unwrap();
    let token = context.new_object();
    source.define_property_for(&Js::SymbolIterator, &Js::object(vec![("value",token.clone()),("enumerable",Js::Bool(true))])).unwrap();
    let result = context.new_object();
    context.copy_data_properties(&result, &source).unwrap();
    assert_eq!(result.own_keys().iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>(), expected, "HL_SHARED_SPREAD live descriptor check");
    assert_eq!(result.get("first").unwrap().as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert!(result.get_property(&Js::SymbolIterator).unwrap().same_reference(&token), "HL_SHARED_SPREAD original iterator-symbol value");
}
