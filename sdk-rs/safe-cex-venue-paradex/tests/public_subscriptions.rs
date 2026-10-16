use safe_cex_venue_paradex::public_subscriptions::{
    self as api, PublicSubscription, SubscriptionMethod as Method,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
struct Marker;
type Error = Rc<Marker>;
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn number(v: &Value) -> Option<f64> {
    v.as_str()
        .map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn decode(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn symbol(mode: &str) -> Vec<u16> {
    match mode {
        "empty" => vec![],
        "surrogate" => vec![88, 0xd800],
        _ => vec![88],
    }
}
fn method(m: Method) -> &'static str {
    match m {
        Method::Subscribe => "subscribe",
        Method::Unsubscribe => "unsubscribe",
    }
}
fn outcome(actual: &Result<(), Error>, expected: &Value, error: &Error, op: &str) {
    match actual {
        Ok(()) => {
            assert_eq!(expected["ok"].as_bool(), Some(true), "{op}_native_success");
            assert_eq!(
                expected["undefined"].as_bool(),
                Some(true),
                "{op}_source_void"
            );
        }
        Err(e) => {
            assert_eq!(expected["ok"].as_bool(), Some(false), "{op}_native_error");
            assert!(Rc::ptr_eq(e, error), "{op}_native_error_identity");
            assert_eq!(expected["same"].as_bool(), Some(true));
        }
    }
}
fn admission(mut input: Value) -> Value {
    for name in ["id", "attempt", "count", "version", "force"] {
        if input.get(name).is_none() {
            input[name] = Value::Null;
        }
    }
    input
}
fn simple(op: &str, input: Value, row: &Value) {
    assert_eq!(row["op"], op, "subscriptions_live_operation");
    assert_eq!(row["input"], input, "subscriptions_live_input");
    let s = Rc::new(symbol(input["symbol"].as_str().unwrap_or("normal")));
    assert_eq!(*s, decode(&row["symbol"]), "subscriptions_live_symbol");
    let key = vec![88, 0xd800, 58, 49, 109];
    assert_eq!(key, decode(&row["key"]), "subscriptions_live_key");
    let trace = RefCell::new(Vec::<String>::new());
    let error = Rc::new(Marker);
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if input["failure"] == name {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let id = Cell::new(number(&input["id"]).unwrap_or(0.0));
    let count = Cell::new(number(&input["count"]));
    let version = Cell::new(number(&input["version"]));
    let desired = Cell::new(false);
    let sent = Cell::new(input["sent"].as_bool().unwrap_or(false));
    let active = Cell::new(true);
    let timer = Rc::new(Marker);
    let timeout = RefCell::new(if input["hasTimer"].as_bool().unwrap_or(false) {
        Some(timer.clone())
    } else {
        None
    });
    let fields = [
        "ohlcvCallbacks",
        "ohlcvIntervals",
        "ohlcvLastTradeAt",
        "ohlcvLastRecoveryAt",
        "lastCandleCache",
    ];
    let retained = RefCell::new([true; 5]);
    let deleted = RefCell::new(Vec::new());
    let calls = RefCell::new(Vec::<Value>::new());
    let pending = RefCell::new(Vec::<Value>::new());
    let requests = RefCell::new(Vec::<Value>::new());
    let writes = RefCell::new(Vec::new());
    let assert_symbol = |given: &[u16]| assert_eq!(given, s.as_slice(), "{op}_native_symbol");
    let assert_key = |given: &[u16]| assert_eq!(given, key.as_slice(), "cleanup_native_key");
    let read_count = |given: &[u16]| {
        assert_symbol(given);
        touch("count-get")?;
        Ok(count.get())
    };
    let write_count = |given: &[u16], value| {
        assert_symbol(given);
        touch("count-set")?;
        count.set(Some(value));
        writes.borrow_mut().push(bits(value));
        Ok(())
    };
    let rpc = |m: Method, channel: Vec<u16>, attempt: Option<f64>| {
        touch("rpc")?;
        let source = &row["calls"][calls.borrow().len()];
        assert_eq!(
            method(m),
            source["method"].as_str().unwrap(),
            "{op}_native_method"
        );
        assert_eq!(channel, decode(&source["channel"]), "{op}_native_channel");
        assert_eq!(
            attempt.map(bits),
            source["attempt"].as_str().map(str::to_owned),
            "{op}_native_attempt"
        );
        calls.borrow_mut().push(json!({"method":method(m),"channel":channel,"attempt":attempt.map(bits),"argc":if attempt.is_some(){3}else{2}}));
        Ok(())
    };
    let result = match op {
        "rpc" => api::send_json_rpc_subscription(
            if input["method"] == "subscribe" {
                Method::Subscribe
            } else {
                Method::Unsubscribe
            },
            s.clone(),
            number(&input["attempt"]),
            || {
                touch("connected")?;
                Ok(input["connected"].as_bool().unwrap())
            },
            || {
                touch("id-read")?;
                Ok(id.get())
            },
            |value| {
                touch("id-write")?;
                id.set(value);
                Ok(())
            },
            |given_id, channel: &Rc<Vec<u16>>, attempt| {
                touch("pending")?;
                assert!(Rc::ptr_eq(channel, &s), "rpc_native_pending_identity");
                let expected = &row["pending"][pending.borrow().len()];
                assert_eq!(bits(given_id), expected["id"], "rpc_native_pending_id");
                assert_eq!(
                    channel.as_slice(),
                    decode(&expected["channel"]),
                    "rpc_native_pending_channel"
                );
                assert_eq!(
                    bits(attempt),
                    expected["attempt"],
                    "rpc_native_pending_attempt"
                );
                pending
                    .borrow_mut()
                    .push(json!({"id":bits(given_id),"channel":**channel,"attempt":bits(attempt)}));
                Ok(())
            },
            |request: PublicSubscription<Rc<Vec<u16>>>| {
                let source = &row["requests"][requests.borrow().len()];
                assert_eq!(bits(request.id), source["id"], "rpc_native_request_id");
                assert_eq!(
                    request.jsonrpc,
                    source["jsonrpc"].as_str().unwrap(),
                    "rpc_native_jsonrpc"
                );
                assert_eq!(
                    method(request.method),
                    source["method"].as_str().unwrap(),
                    "rpc_native_method"
                );
                assert!(
                    Rc::ptr_eq(&request.channel, &s),
                    "rpc_native_request_channel_identity"
                );
                assert_eq!(
                    *request.channel,
                    decode(&source["channel"]),
                    "rpc_native_channel"
                );
                requests.borrow_mut().push(json!({"id":bits(request.id),"jsonrpc":request.jsonrpc,"method":method(request.method),"channel":*request.channel,"keys":["id","jsonrpc","method","params"],"paramsKeys":["channel"]}));
                touch("stringify")?;
                touch("send")?;
                calls.borrow_mut().push(Value::Null);
                Ok(())
            },
        ),
        "trade" => api::send_trade_subscribe(
            &s,
            input["force"].as_bool(),
            number(&input["attempt"]),
            |given| {
                assert_symbol(given);
                touch("desired-add")?;
                desired.set(true);
                Ok(())
            },
            |given| {
                assert_symbol(given);
                touch("sent-has")?;
                Ok(sent.get())
            },
            |given| {
                assert_symbol(given);
                touch("sent-add")?;
                sent.set(true);
                Ok(())
            },
            |m, c, a| rpc(m, c, Some(a)),
        ),
        "book-sub" | "book-unsub" => {
            let connected = || {
                touch("connected")?;
                Ok(input["connected"].as_bool().unwrap())
            };
            let get_channel = |given: Rc<Vec<u16>>| {
                assert!(Rc::ptr_eq(&given, &s), "{op}_native_helper_symbol_identity");
                touch("channel")?;
                Ok(units("order_book.")
                    .into_iter()
                    .chain(given.iter().copied())
                    .chain(units(".snapshot@15@100ms"))
                    .collect::<Vec<_>>())
            };
            if op == "book-sub" {
                api::subscribe_to_order_book(s.clone(), connected, get_channel, |m, c| {
                    rpc(m, c, None)
                })
            } else {
                api::unsubscribe_from_order_book(s.clone(), connected, get_channel, |m, c| {
                    rpc(m, c, None)
                })
            }
        }
        "count-sub" => api::subscribe_to_trades(&s, read_count, write_count, |given| {
            assert_symbol(given);
            touch("trade")?;
            calls.borrow_mut().push(json!({"symbol":given,"argc":1}));
            Ok(())
        }),
        "count-unsub" => api::unsubscribe_from_trades(
            &s,
            read_count,
            |given| {
                assert_symbol(given);
                touch("count-delete")?;
                count.set(None);
                Ok(())
            },
            write_count,
        ),
        "cleanup" => api::cleanup_ohlcv_subscription(
            &key,
            |given, value| {
                assert_key(given);
                assert!(!value, "cleanup_native_active_write");
                touch("active-set")?;
                active.set(value);
                Ok(())
            },
            |given| {
                assert_key(given);
                touch("version-get")?;
                Ok(version.get())
            },
            |given, value| {
                assert_key(given);
                touch("version-set")?;
                version.set(Some(value));
                Ok(())
            },
            |given| {
                assert_key(given);
                touch("timer-has")?;
                Ok(timeout.borrow().is_some())
            },
            |given| {
                assert_key(given);
                touch("timer-get")?;
                Ok(timeout.borrow().clone())
            },
            |given| {
                touch("timer-clear")?;
                assert!(
                    Rc::ptr_eq(given.as_ref().unwrap(), &timer),
                    "cleanup_native_timer_identity"
                );
                Ok(())
            },
            |given| {
                assert_key(given);
                touch("timer-delete")?;
                *timeout.borrow_mut() = None;
                Ok(())
            },
            |field, given| {
                assert_key(given);
                let index = fields
                    .iter()
                    .position(|name| *name == field)
                    .expect("cleanup_native_field");
                touch(&format!("{field}-delete"))?;
                retained.borrow_mut()[index] = false;
                deleted.borrow_mut().push(field);
                Ok(())
            },
        ),
        _ => panic!("unsupported operation"),
    };
    outcome(&result, &row["outcome"], &error, op);
    assert_eq!(bits(id.get()), row["id"], "{op}_native_counter");
    assert_eq!(
        count.get().map(bits),
        row["count"].as_str().map(str::to_owned),
        "{op}_native_count"
    );
    assert_eq!(
        version.get().map(bits),
        row["version"].as_str().map(str::to_owned),
        "{op}_native_version"
    );
    assert_eq!(
        row["active"].as_bool(),
        Some(active.get()),
        "{op}_native_active"
    );
    assert_eq!(
        row["timer"].as_bool(),
        Some(timeout.borrow().is_some()),
        "{op}_native_timer_state"
    );
    assert_eq!(
        row["desired"].as_bool(),
        Some(desired.get()),
        "{op}_native_desired"
    );
    assert_eq!(row["sent"].as_bool(), Some(sent.get()), "{op}_native_sent");
    for (i, field) in fields.iter().enumerate() {
        assert_eq!(
            row["retained"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some(field)),
            retained.borrow()[i],
            "cleanup_native_retained_field"
        );
    }
    assert_eq!(
        row["calls"].as_array().unwrap().len(),
        calls.borrow().len(),
        "{op}_native_call_count"
    );
    assert_eq!(
        row["requests"],
        json!(*requests.borrow()),
        "rpc_native_request_observations"
    );
    assert_eq!(
        row["pending"],
        json!(*pending.borrow()),
        "rpc_native_pending_observations"
    );
    assert_eq!(
        row["writes"],
        json!(*writes.borrow()),
        "counts_native_writes"
    );
    assert_eq!(
        row["deleted"],
        json!(*deleted.borrow()),
        "cleanup_native_delete_order"
    );
    if op != "rpc" {
        assert_eq!(
            row["calls"],
            json!(*calls.borrow()),
            "{op}_native_call_observations"
        );
    }
    assert_eq!(row["trace"], json!(*trace.borrow()), "{op}_native_trace");
}
fn broadcast(layout: &str, failure: &str, row: &Value) {
    assert_eq!(row["layout"], layout);
    assert_eq!(row["failure"], failure);
    let trace = RefCell::new(Vec::<String>::new());
    let error = Rc::new(Marker);
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if name == failure {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let markets = Rc::new(RefCell::new(if layout == "empty" {
        vec![]
    } else {
        vec![Rc::new(units("X")), Rc::new(units("Y"))]
    }));
    let books: RefCell<Vec<Option<(Vec<u16>, f64)>>> = RefCell::new(match layout {
        "empty" => vec![],
        "normal" => vec![
            Some((units("X"), 2.0)),
            Some((units("Y"), 0.0)),
            Some((vec![88, 0xd800], 1.0)),
        ],
        "grow" => vec![Some((units("X"), 1.0))],
        _ => vec![Some((units("X"), 0.0)), Some((units("Y"), 1.0))],
    });
    let visits = Cell::new(0);
    let symbols = Cell::new(0);
    let rpcs = Cell::new(0);
    let trades = Cell::new(0);
    let book_reads = Cell::new(0);
    let book_visits = Cell::new(0);
    let book_calls = Cell::new(0);
    let log_calls = Cell::new(0);
    let calls = RefCell::new(Vec::<Value>::new());
    let logs = RefCell::new(Vec::<Value>::new());
    let result = api::subscribe_to_channels(
        || {
            touch("markets")?;
            Ok(markets.clone())
        },
        |given: &Rc<RefCell<Vec<Rc<Vec<u16>>>>>, callback| {
            assert!(
                Rc::ptr_eq(given, &markets),
                "broadcast_native_markets_identity"
            );
            let pass = visits.get();
            visits.set(pass + 1);
            touch(&format!("visit{pass}"))?;
            let length = given.borrow().len();
            for index in 0..length {
                let value = given.borrow().get(index).cloned();
                if let Some(value) = value {
                    let count = symbols.get();
                    symbols.set(count + 1);
                    touch(&format!("market-symbol{count}"))?;
                    callback(&value)?;
                }
            }
            Ok(())
        },
        |given| {
            assert!(
                Rc::ptr_eq(given, &markets),
                "broadcast_native_length_identity"
            );
            Ok(given.borrow().len() as f64)
        },
        |m, channel| {
            let index = rpcs.get();
            rpcs.set(index + 1);
            touch(&format!("rpc{index}"))?;
            let expected = &row["calls"][calls.borrow().len()];
            assert_eq!(expected["kind"], "rpc", "broadcast_native_kind");
            assert_eq!(
                method(m),
                expected["method"].as_str().unwrap(),
                "broadcast_native_method"
            );
            assert_eq!(
                channel,
                decode(&expected["channel"]),
                "broadcast_native_channel"
            );
            calls
                .borrow_mut()
                .push(json!({"kind":"rpc","method":method(m),"channel":channel,"argc":2}));
            if index == 0 && layout == "grow" {
                markets.borrow_mut().push(Rc::new(units("Z")));
            }
            if index == 0 && layout == "replace" {
                markets.borrow_mut()[1] = Rc::new(vec![88, 0xd800]);
            }
            Ok(())
        },
        |s, force| {
            let index = trades.get();
            trades.set(index + 1);
            touch(&format!("trade{index}"))?;
            assert!(force, "broadcast_native_force");
            let expected = &row["calls"][calls.borrow().len()];
            assert_eq!(
                s,
                decode(&expected["symbol"]),
                "broadcast_native_trade_symbol"
            );
            calls
                .borrow_mut()
                .push(json!({"kind":"trade","symbol":s,"force":force,"argc":2}));
            if index == 0 && layout == "grow" {
                markets.borrow_mut().push(Rc::new(units("W")));
            }
            Ok(())
        },
        |text, severity| {
            let index = log_calls.get();
            log_calls.set(index + 1);
            touch(&format!("log{index}"))?;
            assert_eq!(
                units(&text),
                decode(&row["logs"][logs.borrow().len()][0]),
                "broadcast_native_log_text"
            );
            assert_eq!(severity, "info", "broadcast_native_log_severity");
            logs.borrow_mut().push(json!([units(&text), severity]));
            Ok(())
        },
        || {
            let index = book_reads.get();
            book_reads.set(index + 1);
            touch(&format!("book-size{index}"))?;
            Ok(books.borrow().iter().filter(|v| v.is_some()).count() as f64)
        },
        |callback| {
            touch("books-visit")?;
            let mut index = 0;
            loop {
                let next = books.borrow().get(index).cloned();
                let Some(next) = next else { break };
                if let Some((s, size)) = next {
                    let count = book_visits.get();
                    book_visits.set(count + 1);
                    touch(&format!("book-callback{count}"))?;
                    callback(&s, size)?;
                }
                index += 1;
            }
            Ok(())
        },
        |s| {
            let index = book_calls.get();
            book_calls.set(index + 1);
            touch(&format!("book-subscribe{index}"))?;
            assert_eq!(
                s,
                decode(&row["calls"][calls.borrow().len()]["symbol"]),
                "broadcast_native_book_symbol"
            );
            calls.borrow_mut().push(json!({"kind":"book","symbol":s}));
            if layout == "grow" && index == 0 {
                books.borrow_mut().push(Some((units("Z"), 1.0)));
            }
            if layout == "replace" && index == 0 {
                let mut value = books.borrow_mut();
                value[1] = None;
                value[0] = Some((units("X"), 1.0));
            }
            Ok(())
        },
    );
    outcome(&result, &row["outcome"], &error, "broadcast");
    let expected = row["markets"].as_array().unwrap();
    assert_eq!(
        markets.borrow().len(),
        expected.len(),
        "broadcast_native_market_count"
    );
    for (actual, wanted) in markets.borrow().iter().zip(expected) {
        assert_eq!(
            actual.as_slice(),
            decode(wanted),
            "broadcast_native_market_symbol"
        );
    }
    let actual: Vec<_> = books.borrow().iter().flatten().cloned().collect();
    let expected = row["books"].as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "broadcast_native_book_count");
    for ((s, size), wanted) in actual.iter().zip(expected) {
        assert_eq!(s, &decode(&wanted[0]), "broadcast_native_final_book_symbol");
        assert_eq!(
            size.to_bits(),
            wanted[1].as_f64().unwrap().to_bits(),
            "broadcast_native_final_book_size"
        );
    }
    assert_eq!(
        row["calls"],
        json!(*calls.borrow()),
        "broadcast_native_calls"
    );
    assert_eq!(row["logs"], json!(*logs.borrow()), "broadcast_native_logs");
    assert_eq!(
        row["trace"],
        json!(*trace.borrow()),
        "broadcast_native_trace"
    );
}
#[test]
fn actual_typescript_public_subscriptions() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_subscriptions_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "subscriptions_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1322, "subscriptions_source_case_count");
    let mut defs = Vec::new();
    for m in ["subscribe", "unsubscribe"] {
        for connected in [false, true] {
            for id in [
                0.0,
                -0.0,
                9_007_199_254_740_991.0,
                9_007_199_254_740_992.0,
                f64::NAN,
                f64::INFINITY,
            ] {
                for attempt in [None, Some(-1.0), Some(f64::NAN)] {
                    for failure in [
                        "none",
                        "connected",
                        "id-read",
                        "id-write",
                        "pending",
                        "stringify",
                        "send",
                    ] {
                        defs.push(("rpc",admission(json!({"method":m,"connected":connected,"id":bits(id),"attempt":attempt.map(bits),"failure":failure,"symbol":"surrogate"}))));
                    }
                }
            }
        }
    }
    for s in ["empty", "normal", "surrogate"] {
        for force in [None, Some(false), Some(true)] {
            for sent in [false, true] {
                for attempt in [None, Some(-1.0), Some(f64::NAN)] {
                    for failure in ["none", "desired-add", "sent-has", "sent-add", "rpc"] {
                        defs.push(("trade",admission(json!({"symbol":s,"force":force,"sent":sent,"attempt":attempt.map(bits),"failure":failure}))));
                    }
                }
            }
        }
    }
    for op in ["book-sub", "book-unsub"] {
        for s in ["empty", "normal", "surrogate"] {
            for connected in [false, true] {
                for failure in ["none", "connected", "channel", "rpc"] {
                    defs.push((
                        op,
                        admission(json!({"symbol":s,"connected":connected,"failure":failure})),
                    ));
                }
            }
        }
    }
    let values = [
        None,
        Some(0.0),
        Some(-0.0),
        Some(f64::NAN),
        Some(1.0),
        Some(2.0),
        Some(-1.0),
        Some(0.5),
        Some(f64::INFINITY),
        Some(9_007_199_254_740_992.0),
    ];
    for op in ["count-sub", "count-unsub"] {
        for s in ["empty", "normal", "surrogate"] {
            for count in values {
                for failure in [
                    "none",
                    "count-get",
                    "count-set",
                    if op == "count-sub" {
                        "trade"
                    } else {
                        "count-delete"
                    },
                ] {
                    defs.push((
                        op,
                        admission(json!({"symbol":s,"count":count.map(bits),"failure":failure})),
                    ));
                }
            }
        }
    }
    for has_timer in [false, true] {
        for version in values {
            for failure in [
                "none",
                "active-set",
                "version-get",
                "version-set",
                "timer-has",
                "timer-get",
                "timer-clear",
                "timer-delete",
                "ohlcvCallbacks-delete",
                "ohlcvIntervals-delete",
                "ohlcvLastTradeAt-delete",
                "ohlcvLastRecoveryAt-delete",
                "lastCandleCache-delete",
            ] {
                defs.push((
                    "cleanup",
                    admission(
                        json!({"hasTimer":has_timer,"version":version.map(bits),"failure":failure}),
                    ),
                ));
            }
        }
    }
    assert_eq!(defs.len(), 1322, "subscriptions_independent_case_count");
    let mut completed = 0;
    for (i, (op, input)) in defs.into_iter().enumerate() {
        simple(op, input, &cases[i]);
        completed += 1;
    }
    assert_eq!(completed, 1322, "subscriptions_completed_cases");
    let cases = corpus["broadcasts"].as_array().unwrap();
    assert_eq!(cases.len(), 56, "broadcast_source_count");
    completed = 0;
    for layout in ["empty", "normal", "grow", "replace"] {
        for failure in [
            "none",
            "markets",
            "market-symbol0",
            "rpc0",
            "rpc1",
            "trade0",
            "trade1",
            "log0",
            "book-size0",
            "book-callback0",
            "book-subscribe0",
            "book-size1",
            "log1",
            "visit1",
        ] {
            broadcast(layout, failure, &cases[completed]);
            completed += 1;
        }
    }
    assert_eq!(completed, 56, "broadcast_completed_cases");
}
