use safe_cex_venue_backpack::public_control::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};

fn oracle() -> &'static Value {
    static VALUE: OnceLock<Value> = OnceLock::new();
    VALUE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_control_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn symbols() -> Vec<JsString> {
    vec![
        u("BTC_USDC"),
        vec![],
        u("λ🚀"),
        u("\"\\\0\u{8}\t\n\u{c}\r\u{1f}\u{2028}\u{2029}"),
        vec![88, 0xd800],
        vec![0xdc00, 89],
    ]
}
fn intervals() -> Vec<JsString> {
    vec![
        u("1m"),
        vec![],
        u("λ🚀"),
        u("\"\\\n"),
        vec![0xd800],
        vec![0xdc00],
    ]
}
fn state(values: &[JsString]) -> SubscriptionState {
    let result = Rc::new(RefCell::new(Subscriptions::default()));
    for value in values {
        result.borrow_mut().insert(value.clone());
    }
    result
}
fn native_strings(v: &Value) -> Vec<JsString> {
    serde_json::from_value(v.clone()).unwrap()
}
fn native_params(v: &Value) -> Option<Vec<Option<JsString>>> {
    serde_json::from_value(v.clone()).unwrap()
}
fn assert_error(
    result: Result<(), Rc<()>>,
    original: &Rc<()>,
    expected: bool,
    source: &Value,
    label: &str,
) {
    assert_eq!(
        source["sameError"], expected,
        "{label} independent TS error"
    );
    assert_eq!(result.is_err(), expected, "{label} native error");
    if let Err(error) = result {
        assert!(
            Rc::ptr_eq(&error, original),
            "{label} native error identity"
        );
    }
}
fn native_message(
    message: &Message,
    packet: &Value,
    method: &str,
    params: Option<Vec<Option<JsString>>>,
    label: &str,
) {
    assert_eq!(message.method, method, "{label} native method");
    assert_eq!(message.params, params, "{label} native params");
    assert_eq!(
        packet["method"], message.method,
        "{label} decoded TS method"
    );
    assert_eq!(
        native_params(&packet["params"]),
        message.params,
        "{label} decoded TS params"
    );
}
fn wire_checks(wires: Vec<(JsString, JsString)>) {
    for (index, (actual, expected)) in wires.into_iter().enumerate() {
        assert_eq!(
            actual, expected,
            "wire {index}, after all native assertions"
        );
    }
}
fn expected_stream(method: usize, symbol: &[u16], interval: &[u16]) -> JsString {
    let mut stream = u(["depth.", "ticker.", "trade.", "bookTicker.", "kline."][method]);
    if method == 4 {
        stream.extend_from_slice(interval);
        stream.push(46);
    }
    stream.extend_from_slice(symbol);
    stream
}
fn subscriptions(method: usize) {
    let rows = oracle()["subscribe"].as_array().unwrap();
    assert_eq!(rows.len(), 120, "SUBSCRIBE independent corpus size");
    let names = [
        "subscribeToOrderBook",
        "subscribeToTicker",
        "subscribeToTrades",
        "subscribeToBookTicker",
        "subscribeToKline",
    ];
    let mut wires = vec![];
    for i in 0..24 {
        let row = &rows[method * 24 + i];
        let symbol = &symbols()[i / 4];
        let interval = &intervals()[i / 4];
        let existing = (i % 4) / 2 == 1;
        let fail = i % 2 == 1;
        assert_eq!(
            row["method"], names[method],
            "SUBSCRIBE independent method {method}/{i}"
        );
        assert_eq!(
            row["symbol"],
            serde_json::json!(symbol),
            "SUBSCRIBE independent symbol {method}/{i}"
        );
        assert_eq!(
            row["interval"],
            serde_json::json!(interval),
            "SUBSCRIBE independent interval {method}/{i}"
        );
        assert_eq!(row["existing"], existing);
        assert_eq!(row["fail"], fail);
        let stream = expected_stream(method, symbol, interval);
        let mut initial = vec![u("keep")];
        if existing {
            initial.push(stream.clone());
        }
        let state = state(&initial);
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let label = format!("{} case {i}", names[method]);
        let send = |message: Message| {
            calls.set(calls.get() + 1);
            assert_eq!(
                state.borrow().values(),
                &[u("keep"), stream.clone()],
                "{label} native insertion before send"
            );
            assert_eq!(
                native_strings(&row["before"][0]),
                state.borrow().values(),
                "{label} decoded before send"
            );
            native_message(
                &message,
                &row["packets"][0],
                "SUBSCRIBE",
                Some(vec![Some(stream.clone())]),
                &label,
            );
            wires.push((
                message.to_json(),
                serde_json::from_value(row["wires"][0].clone()).unwrap(),
            ));
            state.borrow_mut().remove(&u("keep"));
            state.borrow_mut().insert(u("after"));
            if fail { Err(failure.clone()) } else { Ok(()) }
        };
        let result = match method {
            0 => subscribe_to_order_book(&state, symbol, send),
            1 => subscribe_to_ticker(&state, symbol, send),
            2 => subscribe_to_trades(&state, symbol, send),
            3 => subscribe_to_book_ticker(&state, symbol, send),
            4 => subscribe_to_kline(&state, symbol, interval, send),
            _ => unreachable!(),
        };
        let expected_calls = usize::from(!existing);
        assert_eq!(calls.get(), expected_calls, "{label} native send count");
        for field in ["packets", "wires", "before"] {
            assert_eq!(
                row[field].as_array().unwrap().len(),
                expected_calls,
                "{label} independent {field} size"
            );
        }
        assert_error(result, &failure, fail && !existing, row, &label);
        let after = if existing {
            initial
        } else {
            vec![stream, u("after")]
        };
        assert_eq!(state.borrow().values(), after, "{label} native state");
        assert_eq!(
            native_strings(&row["after"]),
            after,
            "{label} decoded TS state"
        );
    }
    wire_checks(wires);
}
#[test]
fn order_book_subscriptions() {
    subscriptions(0);
}
#[test]
fn ticker_subscriptions() {
    subscriptions(1);
}
#[test]
fn trade_subscriptions() {
    subscriptions(2);
}
#[test]
fn book_ticker_subscriptions() {
    subscriptions(3);
}
#[test]
fn kline_subscriptions() {
    subscriptions(4);
}

#[test]
fn resubscribe_ordered_snapshot() {
    let rows = oracle()["resubscribe"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "RESUBSCRIBE independent size");
    let seeds = [
        vec![],
        vec![u("a")],
        vec![u("b"), u("a"), u("b")],
        vec![vec![0xd800], vec![0xdc00], u("🚀")],
        vec![u("a"), u("b"), u("a")],
        vec![u(""), u("\"\\\n")],
    ];
    let ordered = [
        vec![],
        vec![u("a")],
        vec![u("b"), u("a")],
        vec![vec![0xd800], vec![0xdc00], u("🚀")],
        vec![u("b"), u("a")],
        vec![u(""), u("\"\\\n")],
    ];
    let mut wires = vec![];
    for (i, row) in rows.iter().enumerate() {
        let seed = &seeds[i / 2];
        let expected = &ordered[i / 2];
        let fail = i % 2 == 1;
        assert_eq!(
            native_strings(&row["seed"]),
            *seed,
            "RESUBSCRIBE independent seed {i}"
        );
        assert_eq!(row["reinsert"], i / 2 == 4);
        assert_eq!(row["fail"], fail);
        let state = state(seed);
        if i / 2 == 4 {
            state.borrow_mut().remove(&u("a"));
            state.borrow_mut().insert(u("a"));
        }
        assert_eq!(
            state.borrow().values(),
            expected,
            "RESUBSCRIBE native set order {i}"
        );
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let label = format!("RESUBSCRIBE case {i}");
        let result = resubscribe(&state, |message| {
            calls.set(calls.get() + 1);
            native_message(
                &message,
                &row["packets"][0],
                "SUBSCRIBE",
                Some(expected.iter().cloned().map(Some).collect()),
                &label,
            );
            wires.push((
                message.to_json(),
                serde_json::from_value(row["wires"][0].clone()).unwrap(),
            ));
            state.borrow_mut().insert(u("after"));
            if fail { Err(failure.clone()) } else { Ok(()) }
        });
        let count = usize::from(!expected.is_empty());
        assert_eq!(calls.get(), count, "{label} native count");
        assert_eq!(row["packets"].as_array().unwrap().len(), count);
        assert_eq!(row["wires"].as_array().unwrap().len(), count);
        assert_error(result, &failure, fail && count != 0, row, &label);
        let mut after = expected.clone();
        if count != 0 {
            after.push(u("after"));
        }
        assert_eq!(state.borrow().values(), after, "{label} native state");
        assert_eq!(native_strings(&row["after"]), after);
    }
    wire_checks(wires);
}

#[test]
fn unsubscribe_live_streams_and_failure() {
    let rows = oracle()["unsubscribe"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "UNSUBSCRIBE independent size");
    let cases = [
        vec![],
        vec![Some(u("a"))],
        vec![Some(u("a")), Some(u("b")), Some(u("a"))],
        vec![Some(u("absent"))],
        vec![Some(vec![0xd800]), None, Some(u("c"))],
        vec![Some(u("a"))],
        vec![Some(u("a"))],
        vec![Some(u("a"))],
    ];
    let removed = [
        vec![],
        vec![u("a")],
        vec![u("a"), u("b")],
        vec![],
        vec![vec![0xd800], u("c")],
        vec![u("a"), u("b")],
        vec![],
        vec![u("c")],
    ];
    let mut wires = vec![];
    for (i, row) in rows.iter().enumerate() {
        let mode = i / 2;
        let fail = i % 2 == 1;
        let params = &cases[mode];
        assert_eq!(row["mode"], mode);
        assert_eq!(row["fail"], fail);
        assert_eq!(
            native_params(&row["streams"]),
            Some(params.clone()),
            "UNSUBSCRIBE independent content {i}"
        );
        let initial = vec![u("a"), u("b"), u("c"), u("keep"), vec![0xd800]];
        let state = state(&initial);
        let streams = Rc::new(RefCell::new(params.clone()));
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let label = format!("UNSUBSCRIBE case {i}");
        let result = unsubscribe(&state, &streams, |message| {
            calls.set(calls.get() + 1);
            assert_eq!(
                state.borrow().values(),
                initial,
                "{label} native send before deletion"
            );
            assert_eq!(native_strings(&row["before"][0]), initial);
            native_message(
                &message,
                &row["packets"][0],
                "UNSUBSCRIBE",
                Some(params.clone()),
                &label,
            );
            wires.push((
                message.to_json(),
                serde_json::from_value(row["wires"][0].clone()).unwrap(),
            ));
            match mode {
                5 => streams.borrow_mut().push(Some(u("b"))),
                6 => streams.borrow_mut().clear(),
                7 => streams.borrow_mut()[0] = Some(u("c")),
                _ => {}
            }
            if fail { Err(failure.clone()) } else { Ok(()) }
        });
        let count = usize::from(!params.is_empty());
        assert_eq!(calls.get(), count, "{label} native send count");
        for field in ["packets", "wires", "before"] {
            assert_eq!(row[field].as_array().unwrap().len(), count);
        }
        assert_error(result, &failure, fail && count != 0, row, &label);
        let after: Vec<_> = initial
            .into_iter()
            .filter(|v| fail || !removed[mode].contains(v))
            .collect();
        assert_eq!(state.borrow().values(), after, "{label} native state");
        assert_eq!(
            native_strings(&row["after"]),
            after,
            "{label} decoded TS state"
        );
    }
    wire_checks(wires);
}

#[test]
fn unsubscribe_kline_delegate() {
    let rows = oracle()["kline"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "KLINE-UNSUBSCRIBE independent size");
    for (i, row) in rows.iter().enumerate() {
        let symbol = &symbols()[i / 2];
        let interval = &intervals()[i / 2];
        let fail = i % 2 == 1;
        assert_eq!(row["symbol"], serde_json::json!(symbol));
        assert_eq!(row["interval"], serde_json::json!(interval));
        assert_eq!(row["fail"], fail);
        let expected = expected_stream(4, symbol, interval);
        let failure = Rc::new(());
        let calls = Cell::new(0);
        let label = format!("KLINE-UNSUBSCRIBE case {i}");
        let result = unsubscribe_from_kline(symbol, interval, |streams| {
            calls.set(calls.get() + 1);
            assert_eq!(
                *streams.borrow(),
                vec![Some(expected.clone())],
                "{label} native delegate params"
            );
            assert_eq!(
                native_strings(&row["forwarded"][0]),
                vec![expected.clone()],
                "{label} decoded TS delegate params"
            );
            if fail { Err(failure.clone()) } else { Ok(()) }
        });
        assert_eq!(calls.get(), 1, "{label} native count");
        assert_eq!(row["forwarded"].as_array().unwrap().len(), 1);
        assert_error(result, &failure, fail, row, &label);
    }
}

#[test]
fn ping_guards_time_and_failure() {
    let rows = oracle()["ping"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "PING independent size");
    let mut cases = vec![];
    for now in [
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        123.25,
    ] {
        cases.push((false, true, "none", now));
    }
    for disposed in [true, false] {
        for connected in [true, false] {
            for fail in ["none", "connected"] {
                cases.push((disposed, connected, fail, 7.0));
            }
        }
    }
    for fail in ["now", "send"] {
        for connected in [true, false] {
            cases.push((false, connected, fail, 7.0));
        }
    }
    assert_eq!(cases.len(), 18);
    let mut wires = vec![];
    for (i, (disposed, connected, fail, now)) in cases.into_iter().enumerate() {
        let row = &rows[i];
        assert_eq!(row["disposed"], disposed);
        assert_eq!(row["connected"], connected);
        assert_eq!(row["failure"], fail);
        assert_eq!(
            row["now"],
            format!("{:016x}", now.to_bits()),
            "PING independent time content {i}"
        );
        let at = Cell::new(99.0);
        let failure = Rc::new(());
        let calls = RefCell::new(vec![]);
        let label = format!("PING case {i}");
        let result = ping(
            disposed,
            || {
                calls.borrow_mut().push("connected");
                if fail == "connected" {
                    Err(failure.clone())
                } else {
                    Ok(connected)
                }
            },
            &at,
            || {
                calls.borrow_mut().push("now");
                if fail == "now" {
                    Err(failure.clone())
                } else {
                    Ok(now)
                }
            },
            |message| {
                calls.borrow_mut().push("send");
                assert_eq!(
                    at.get().to_bits(),
                    now.to_bits(),
                    "{label} native timestamp before send"
                );
                native_message(&message, &row["packets"][0], "PING", None, &label);
                wires.push((
                    message.to_json(),
                    serde_json::from_value(row["wires"][0].clone()).unwrap(),
                ));
                if fail == "send" {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            },
        );
        let mut expected_calls = vec![];
        if !disposed {
            expected_calls.push("connected");
            if fail != "connected" && connected {
                expected_calls.push("now");
                if fail != "now" {
                    expected_calls.push("send");
                }
            }
        }
        assert_eq!(*calls.borrow(), expected_calls, "{label} native call order");
        assert_eq!(row["calls"], serde_json::json!(expected_calls));
        let sends = usize::from(expected_calls.contains(&"send"));
        assert_eq!(row["packets"].as_array().unwrap().len(), sends);
        assert_eq!(row["wires"].as_array().unwrap().len(), sends);
        let expected_at = if sends == 1 { now } else { 99.0 };
        assert_eq!(
            at.get().to_bits(),
            expected_at.to_bits(),
            "{label} native timestamp"
        );
        assert_eq!(row["pingAt"], format!("{:016x}", expected_at.to_bits()));
        let expected_error = (!disposed && fail == "connected")
            || (!disposed && connected && ["now", "send"].contains(&fail));
        assert_error(result, &failure, expected_error, row, &label);
    }
    wire_checks(wires);
}

#[test]
fn open_callback_order_and_failure() {
    let rows = oracle()["open"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "OPEN independent size");
    for (i, row) in rows.iter().enumerate() {
        let disposed = i / 5 == 1;
        let fail = ["none", "track", "reset", "resubscribe", "ping"][i % 5];
        assert_eq!(row["disposed"], disposed);
        assert_eq!(row["fail"], fail);
        let failure = Rc::new(());
        let calls = RefCell::new(vec![]);
        let changed_disposed = Cell::new(disposed);
        let label = format!("OPEN case {i}");
        let callback = |name| {
            calls.borrow_mut().push(name);
            changed_disposed.set(true);
            if fail == name {
                Err(failure.clone())
            } else {
                Ok(())
            }
        };
        let result = on_open(
            changed_disposed.get(),
            || callback("track"),
            || callback("reset"),
            || callback("resubscribe"),
            || callback("ping"),
        );
        let expected = if disposed {
            vec![]
        } else {
            let length = if fail == "none" { 4 } else { i % 5 };
            ["track", "reset", "resubscribe", "ping"][..length].to_vec()
        };
        assert_eq!(*calls.borrow(), expected, "{label} native callback order");
        assert_eq!(row["calls"], serde_json::json!(expected));
        assert_error(result, &failure, !disposed && fail != "none", row, &label);
    }
}
