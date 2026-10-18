use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::{private_event_value::*, public_dispatch::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
fn list(trace: Rc<RefCell<Vec<String>>>, name: &str) -> CallbackList {
    let name = name.to_owned();
    Rc::new(RefCell::new(vec![Rc::new(move |_| {
        trace.borrow_mut().push(name.clone());
        Ok(())
    })]))
}
#[test]
fn dispatch_native_selection_before_source() {
    let mut inputs = vec![];
    let mut actual = vec![];
    for event in ["kline", "constructor", "toString", "__proto__"] {
        for broadcast in [false, true] {
            for exact in [false, true] {
                for prefix in [false, true] {
                    for malformed in [false, true] {
                        let handlers = Handlers::default();
                        let trace = Rc::new(RefCell::new(vec![]));
                        if broadcast {
                            handlers.own.borrow_mut().insert(
                                event.into(),
                                Handler::List(list(trace.clone(), "broadcast")),
                            );
                        }
                        if exact {
                            handlers.own.borrow_mut().insert(
                                format!("{event}.BTC"),
                                Handler::List(list(trace.clone(), "exact")),
                            );
                        }
                        if prefix {
                            handlers.own.borrow_mut().insert(
                                format!("{event}.1m.ETH"),
                                Handler::List(list(trace.clone(), "prefix")),
                            );
                        }
                        let mut data = format!(r#"{{"e":"{event}","s":"BTC"}}"#);
                        if malformed {
                            data.pop();
                        }
                        let result = dispatch_binance_message(&handlers, &data, |topic| {
                            trace.borrow_mut().push(format!("stats:{topic}"));
                            Ok(())
                        });
                        let inherited_nonzero = !broadcast && event == "constructor";
                        let selected = broadcast || inherited_nonzero || exact || prefix;
                        let mut expected = vec![];
                        if selected {
                            expected.push(format!("stats:{event}"));
                        }
                        if !malformed && selected && !inherited_nonzero {
                            if broadcast {
                                expected.push("broadcast".into())
                            }
                            if exact {
                                expected.push("exact".into())
                            }
                            if !broadcast && !exact && prefix {
                                expected.push("prefix".into())
                            }
                        }
                        assert_eq!(
                            *trace.borrow(),
                            expected,
                            "native dispatch selected/order {event} {broadcast} {exact} {prefix} {malformed}"
                        );
                        if inherited_nonzero && !malformed {
                            let e = result.as_ref().unwrap_err();
                            assert_eq!(
                                e.name, "TypeError",
                                "native inherited arity selects iteration"
                            );
                            assert_eq!(e.message, "broadcast is not iterable");
                        } else {
                            assert!(result.is_ok(), "native dispatch outcome");
                        }
                        inputs.push(json!({"event":event,"broadcast":broadcast,"exact":exact,"prefix":prefix,"data":data}));
                        actual.push(json!({"trace":*trace.borrow(),"failure":result.err().map(|e|json!({"name":e.name,"message":e.message}))}));
                    }
                }
            }
        }
    }
    assert_eq!(inputs.len(), 64, "independent dispatch floor 4*2^4");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-dispatch.mjs"
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
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 64, "independent source floor");
    assert_eq!(
        actual, source,
        "actual source dispatch after all native assertions"
    );
}
#[test]
fn dispatch_native_live_lists_payload_and_errors() {
    let handlers = Handlers::default();
    let trace = Rc::new(RefCell::new(vec![]));
    let first: CallbackList = Rc::new(RefCell::new(vec![]));
    let weak = Rc::downgrade(&first);
    let t = trace.clone();
    first.borrow_mut().push(Rc::new(move |events: EventValue| {
        t.borrow_mut().push("broadcast".into());
        let x = events.read("0")?;
        x.write("native", number(4.2));
        let t = t.clone();
        weak.upgrade().unwrap().borrow_mut().push(Rc::new(move |_| {
            t.borrow_mut().push("appended".into());
            Ok(())
        }));
        Ok(())
    }) as safe_cex_venue_binance::public_listeners::Parser);
    handlers
        .own
        .borrow_mut()
        .insert("kline".into(), Handler::List(first.clone()));
    let checking = Rc::new(std::cell::Cell::new(true));
    let check = checking.clone();
    let t = trace.clone();
    handlers.own.borrow_mut().insert(
        "kline.BTC".into(),
        Handler::List(Rc::new(RefCell::new(vec![
            Rc::new(move |events: EventValue| {
                let value = events.read("0")?.read("native")?;
                if check.get() {
                    assert!(
                        matches!(value,EventValue::Number(v)if v==4.2),
                        "native exact observes original payload mutation"
                    );
                } else {
                    assert!(
                        matches!(value, EventValue::Undefined),
                        "native new dispatch payload is fresh"
                    );
                }
                t.borrow_mut().push("exact".into());
                Ok(())
            }) as safe_cex_venue_binance::public_listeners::Parser,
        ]))),
    );
    dispatch_binance_message(&handlers, r#"{"e":"kline","s":"BTC"}"#, |_| {
        trace.borrow_mut().push("stats:kline".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        *trace.borrow(),
        ["stats:kline", "broadcast", "appended", "exact"],
        "native live appended callback before exact"
    );
    assert_eq!(first.borrow().len(), 2, "independent live callback count");
    let error = Rc::new(ExchangeError::new("stats fixture"));
    let returned = dispatch_binance_message(&handlers, r#"{"e":"kline","s":"BTC"}"#, |_| {
        Err(error.clone())
    })
    .unwrap_err();
    assert!(
        Rc::ptr_eq(&error, &returned),
        "native statistics failure identity before parsing"
    );
    checking.set(false);
    trace.borrow_mut().clear();
    dispatch_binance_message(&handlers, r#"{"e":"kline","s":"BTC"}"#, |_| {
        first.borrow_mut().clear();
        Ok(())
    })
    .unwrap();
    assert_eq!(
        *trace.borrow(),
        ["exact"],
        "native current length reread after stats clears broadcast"
    );
    let h = Handlers::default();
    h.prototype.borrow_mut().insert(
        "kline.1m.ETH".into(),
        Handler::List(list(trace.clone(), "prefix")),
    );
    trace.borrow_mut().clear();
    dispatch_binance_message(&h, r#"{"e":"kline","s":"BTC"}"#, |_| Ok(())).unwrap();
    assert_eq!(
        *trace.borrow(),
        ["prefix"],
        "native enumerable inherited prefix"
    );
    h.own
        .borrow_mut()
        .insert("kline.bad".into(), Handler::Value(EventValue::Null));
    let mut stats = false;
    let e = dispatch_binance_message(&h, r#"{"e":"kline","s":"BTC"}"#, |_| {
        stats = true;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(
        e.name, "TypeError",
        "native prefix null throws before stats"
    );
    assert!(!stats);
}

#[test]
fn dispatch_native_overflow_signed_zero_and_deep_frame() {
    let handlers = Handlers::default();
    let seen = Rc::new(std::cell::Cell::new(0));
    let count = seen.clone();
    let callback: safe_cex_venue_binance::public_listeners::Parser = Rc::new(move |events| {
        let first = events.read("0")?;
        let infinity = first.read("n")?;
        assert!(
            matches!(infinity,EventValue::Number(v)if v==f64::INFINITY),
            "native JSON overflow is Infinity"
        );
        let zero = first.read("zero")?;
        assert!(
            matches!(zero,EventValue::Number(v)if v==0.0&&v.is_sign_negative()),
            "native JSON negative zero sign"
        );
        let mut node = first.read("deep")?;
        for _ in 0..150 {
            node = node.read("0")?;
        }
        assert!(
            matches!(node,EventValue::Number(v)if v==4.2),
            "native decoded depth150 leaf"
        );
        count.set(count.get() + 1);
        Ok(())
    });
    handlers.own.borrow_mut().insert(
        "kline".into(),
        Handler::List(Rc::new(RefCell::new(vec![callback]))),
    );
    let deep = format!("{}4.2{}", "[".repeat(150), "]".repeat(150));
    let data = format!(r#"{{"e":"kline","n":1e400,"zero":-0,"deep":{deep}}}"#);
    dispatch_binance_message(&handlers, &data, |_| Ok(())).unwrap();
    assert_eq!(seen.get(), 1, "independent overflow/depth callback count");
}
