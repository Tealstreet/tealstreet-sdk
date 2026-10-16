use safe_cex_venue_paradex::{ws_private_lifecycle as private, ws_public_lifecycle as public};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
struct Marker(&'static str);
type Host = Rc<Marker>;
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn unit_result(result: &Result<(), Host>, source: &Value, error: &Host, name: &str) {
    match result {
        Ok(()) => {
            assert_eq!(source["ok"].as_bool(), Some(true), "{name}_native_success");
            assert_eq!(
                source["undefined"].as_bool(),
                Some(true),
                "{name}_source_void"
            );
        }
        Err(actual) => {
            assert_eq!(source["ok"].as_bool(), Some(false), "{name}_native_error");
            assert!(Rc::ptr_eq(actual, error), "{name}_native_error_identity");
            assert_eq!(source["same"].as_bool(), Some(true));
        }
    }
}
fn slot(slot: &RefCell<Option<Host>>, expected: &Value, leases: &[Host], name: &str) {
    match slot.borrow().as_ref() {
        Some(value) => {
            assert_eq!(expected.as_str(), Some(value.0), "{name}_native_slot");
            assert!(
                leases.iter().any(|lease| Rc::ptr_eq(lease, value)),
                "{name}_native_slot_identity"
            );
        }
        None => assert!(expected.is_null(), "{name}_native_empty_slot"),
    }
}
fn native_case(venue: &str, op: &str, input: Value, source: &Value) {
    assert_eq!(source["venue"], venue, "ws_live_venue");
    assert_eq!(source["op"], op, "ws_live_operation");
    assert_eq!(source["input"], input, "ws_live_input");
    let prefix = format!("{venue}_{op}");
    let error = Rc::new(Marker("error"));
    let trace = RefCell::new(Vec::<String>::new());
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if input["failure"] == name {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let leases: Vec<Host> = [
        "ping", "orders", "new", "r0", "r1", "r2", "p0", "p1", "p2", "socket", "fetch",
    ]
    .into_iter()
    .map(|name| Rc::new(Marker(name)))
    .collect();
    let mask = input["mask"].as_u64().unwrap_or(0);
    let layout = input["layout"].as_u64().unwrap_or(0);
    let ping = RefCell::new(if mask & 1 != 0 {
        Some(leases[0].clone())
    } else {
        None
    });
    let orders = RefCell::new(if mask & 2 != 0 {
        Some(leases[1].clone())
    } else {
        None
    });
    let retry = RefCell::new(if layout == 2 {
        vec![leases[3].clone(), leases[4].clone()]
    } else if layout == 1 {
        vec![leases[3].clone()]
    } else {
        vec![]
    });
    let poll = RefCell::new(if layout == 2 {
        vec![leases[6].clone(), leases[7].clone()]
    } else if layout == 1 {
        vec![leases[6].clone()]
    } else {
        vec![]
    });
    let socket = RefCell::new(if op == "open" && input["hasSocket"].as_bool().unwrap() {
        Some(leases[9].clone())
    } else {
        None
    });
    let ping_reads = Cell::new(0);
    let order_reads = Cell::new(0);
    let connected_reads = Cell::new(0);
    let disposed = Cell::new(input["disposed"].as_bool().unwrap_or(false));
    let base = Cell::new(false);
    let repeated = Cell::new(false);
    let sent_cleared = Cell::new(false);
    let pending_cleared = Cell::new(false);
    let sends = RefCell::new(Vec::<Value>::new());
    let listeners = RefCell::new(Vec::new());
    let callback: RefCell<Option<Box<dyn FnOnce() -> Result<(), Host> + '_>>> = RefCell::new(None);
    let read_disposed = || {
        touch("disposed")?;
        Ok(disposed.get())
    };
    let read_connected = || {
        let count = connected_reads.get();
        connected_reads.set(count + 1);
        touch(&format!("connected{count}"))?;
        Ok(if count == 0 {
            input["connected"].as_bool().unwrap()
        } else {
            input["lateConnected"].as_bool().unwrap()
        })
    };
    let read_ping = || {
        let count = ping_reads.get();
        ping_reads.set(count + 1);
        touch(&format!("ping{count}"))?;
        Ok(ping.borrow().clone())
    };
    let read_orders = || {
        let count = order_reads.get();
        order_reads.set(count + 1);
        touch(&format!("orders{count}"))?;
        Ok(orders.borrow().clone())
    };
    let clear = |timer: Option<Host>| {
        let timer = timer.expect("ws_native_present_timer");
        assert!(
            leases.iter().any(|lease| Rc::ptr_eq(lease, &timer)),
            "ws_native_clear_identity"
        );
        touch(&format!("clear-{}", timer.0))?;
        if layout == 2 && timer.0 == "r0" {
            retry.borrow_mut().push(leases[5].clone());
        }
        if layout == 2 && timer.0 == "p0" {
            poll.borrow_mut().push(leases[8].clone());
        }
        Ok(())
    };
    let write_ping_none = |value: Option<Host>| {
        assert!(value.is_none(), "{prefix}_native_null_timer");
        touch("write-ping")?;
        *ping.borrow_mut() = value;
        Ok(())
    };
    let write_orders_none = |value: Option<Host>| {
        assert!(value.is_none(), "{prefix}_native_null_timer");
        touch("write-orders")?;
        *orders.borrow_mut() = value;
        Ok(())
    };
    let base_dispose = || {
        touch("base")?;
        base.set(true);
        Ok(())
    };
    let id_input = input["id"]
        .as_str()
        .map(|text| f64::from_bits(u64::from_str_radix(text, 16).unwrap()));
    let id = Cell::new(id_input.unwrap_or(0.0));
    let reads = Cell::new(0);
    let writes = Cell::new(0);
    let id_reads = RefCell::new(Vec::new());
    let id_writes = RefCell::new(Vec::new());
    let result = match op {
        "connect" => {
            let url = Rc::new(vec![117_u16, 114, 108, 45, 0xd800]);
            let read_url = || {
                touch("url")?;
                Ok(url.clone())
            };
            let create = |given: Rc<Vec<u16>>| {
                assert!(Rc::ptr_eq(&given, &url), "{prefix}_native_url_identity");
                touch("create")?;
                Ok(leases[9].clone())
            };
            let write = |given: Host| {
                assert!(
                    Rc::ptr_eq(&given, &leases[9]),
                    "{prefix}_native_socket_identity"
                );
                touch("write-socket")?;
                *socket.borrow_mut() = Some(given);
                Ok(())
            };
            let add = |event: &'static str| {
                touch("ws-read")?;
                assert!(
                    Rc::ptr_eq(socket.borrow().as_ref().unwrap(), &leases[9]),
                    "{prefix}_native_listener_socket"
                );
                assert_eq!(
                    event,
                    ["open", "message", "close"][listeners.borrow().len()],
                    "{prefix}_native_event"
                );
                touch(&format!("add-{event}"))?;
                listeners.borrow_mut().push(json!([event, true, true]));
                Ok(())
            };
            if venue == "public" {
                public::connect_and_subscribe(read_disposed, read_url, create, write, add)
            } else {
                private::connect_and_subscribe(read_disposed, read_url, create, write, add)
            }
        }
        "ping" => {
            let send = |message: &'static str| {
                touch("send")?;
                let value: Value = serde_json::from_str(message).unwrap();
                assert_eq!(value["type"], "ping", "{prefix}_native_message_type");
                sends.borrow_mut().push(value);
                Ok(())
            };
            let repeat = || {
                touch("repeat")?;
                repeated.set(true);
                Ok(())
            };
            let schedule = |given, delay| {
                assert_eq!(delay, 30_000, "{prefix}_native_delay");
                touch("schedule")?;
                *callback.borrow_mut() = Some(given);
                Ok(leases[2].clone())
            };
            let write = |timer: Host| {
                assert!(
                    Rc::ptr_eq(&timer, &leases[2]),
                    "{prefix}_native_timer_identity"
                );
                touch("write-ping")?;
                *ping.borrow_mut() = Some(timer);
                Ok(())
            };
            if venue == "public" {
                public::ping(read_connected, read_disposed, send, repeat, schedule, write)
            } else {
                private::ping(read_connected, read_disposed, send, repeat, schedule, write)
            }
        }
        "dispose" => {
            if venue == "public" {
                let visit =
                    |timers: &RefCell<Vec<Host>>,
                     callback: &mut dyn FnMut(Host) -> Result<(), Host>| {
                        let mut index = 0;
                        loop {
                            let next = timers.borrow().get(index).cloned();
                            let Some(timer) = next else { break };
                            callback(timer)?;
                            index += 1;
                        }
                        Ok(())
                    };
                public::dispose(
                    read_ping,
                    clear,
                    write_ping_none,
                    |cb| visit(&retry, cb),
                    || {
                        touch("retry-clear")?;
                        retry.borrow_mut().clear();
                        Ok(())
                    },
                    |cb| visit(&poll, cb),
                    || {
                        touch("poll-clear")?;
                        poll.borrow_mut().clear();
                        Ok(())
                    },
                    base_dispose,
                )
            } else {
                private::dispose(
                    read_ping,
                    read_orders,
                    clear,
                    write_ping_none,
                    write_orders_none,
                    base_dispose,
                )
            }
        }
        "throttle" => private::throttled_fetch_orders(
            read_orders,
            clear,
            || {
                touch("fetch-get")?;
                Ok(leases[10].clone())
            },
            |fetch: Host, delay| {
                assert!(
                    Rc::ptr_eq(&fetch, &leases[10]),
                    "throttle_native_callback_identity"
                );
                assert_eq!(delay, 1_000, "throttle_native_delay");
                touch("schedule")?;
                Ok(leases[2].clone())
            },
            |timer: Host| {
                assert!(
                    Rc::ptr_eq(&timer, &leases[2]),
                    "throttle_native_timer_identity"
                );
                touch("write-orders")?;
                *orders.borrow_mut() = Some(timer);
                Ok(())
            },
        ),
        "open" => public::on_open(
            read_disposed,
            |text, severity| {
                assert_eq!(
                    text,
                    if severity == "info" {
                        "Paradex public WebSocket connected"
                    } else {
                        "WebSocket disconnected in onOpen, closing connection"
                    },
                    "onopen_native_log_text"
                );
                assert!(
                    ["info", "warning"].contains(&severity),
                    "onopen_native_log_severity"
                );
                touch(if severity == "info" {
                    "log-info"
                } else {
                    "log-warning"
                })
            },
            || touch("ping-call"),
            || {
                touch("clear-sent")?;
                sent_cleared.set(true);
                Ok(())
            },
            || {
                touch("clear-pending")?;
                pending_cleared.set(true);
                Ok(())
            },
            read_connected,
            || {
                touch("ws-read")?;
                Ok(socket.borrow().clone())
            },
            |given: Host| {
                assert!(
                    Rc::ptr_eq(&given, &leases[9]),
                    "onopen_native_close_identity"
                );
                touch("close")
            },
            || touch("subscribe"),
        ),
        "channels" => private::subscribe_to_private_channels(
            || {
                let index = reads.get();
                reads.set(index + 1);
                touch(&format!("id-read{index}"))?;
                let old = id.get();
                id_reads.borrow_mut().push(bits(old));
                Ok(old)
            },
            |next| {
                let index = writes.get();
                writes.set(index + 1);
                touch(&format!("id-write{index}"))?;
                id_writes.borrow_mut().push(bits(next));
                id.set(next);
                Ok(())
            },
            |message| {
                let index = sends.borrow().len();
                touch(&format!("send{index}"))?;
                assert_eq!(message.jsonrpc, "2.0", "channels_native_jsonrpc");
                assert_eq!(message.method, "subscribe", "channels_native_method");
                assert_eq!(
                    message.channel,
                    ["orders.ALL", "positions", "balance_events"][index],
                    "channels_native_channel"
                );
                assert_eq!(
                    bits(message.id),
                    source["idReads"][index].as_str().unwrap(),
                    "channels_native_id"
                );
                let expected = &source["sends"][index];
                assert_eq!(expected["jsonrpc"], message.jsonrpc);
                assert_eq!(expected["method"], message.method);
                assert_eq!(expected["params"]["channel"], message.channel);
                sends.borrow_mut().push(json!({"jsonrpc":message.jsonrpc,"method":message.method,"params":{"channel":message.channel},"id":message.id}));
                Ok(())
            },
        ),
        _ => panic!("unknown operation"),
    };
    unit_result(&result, &source["before"]["outcome"], &error, &prefix);
    slot(&ping, &source["before"]["timer"], &leases, &prefix);
    slot(&orders, &source["before"]["orders"], &leases, &prefix);
    let before_trace = trace.borrow().clone();
    if op == "ping" && result.is_ok() && callback.borrow().is_some() {
        disposed.set(input["lateDisposed"].as_bool().unwrap());
        let deferred = callback.borrow_mut().take().unwrap()();
        unit_result(&deferred, &source["deferred"], &error, &prefix);
    } else {
        assert!(source["deferred"].is_null());
    }
    slot(&ping, &source["ping"], &leases, &prefix);
    slot(&orders, &source["orders"], &leases, &prefix);
    assert_eq!(
        source["base"].as_bool(),
        Some(base.get()),
        "{prefix}_native_base"
    );
    assert_eq!(
        source["socket"].as_bool(),
        Some(socket.borrow().is_some()),
        "{prefix}_native_socket"
    );
    assert_eq!(
        source["retry"].as_u64(),
        Some(retry.borrow().len() as u64),
        "{prefix}_native_retry_count"
    );
    assert_eq!(
        source["poll"].as_u64(),
        Some(poll.borrow().len() as u64),
        "{prefix}_native_poll_count"
    );
    assert_eq!(
        source["repeat"].as_bool(),
        Some(repeated.get()),
        "{prefix}_native_repeat"
    );
    assert_eq!(
        source["sentCleared"].as_bool(),
        Some(sent_cleared.get()),
        "onopen_native_sent_state"
    );
    assert_eq!(
        source["pendingCleared"].as_bool(),
        Some(pending_cleared.get()),
        "onopen_native_pending_state"
    );
    if id_input.is_some() {
        assert_eq!(source["id"], bits(id.get()), "channels_native_counter");
    }
    assert_eq!(
        source["idReads"],
        json!(*id_reads.borrow()),
        "channels_native_id_reads"
    );
    assert_eq!(
        source["idWrites"],
        json!(*id_writes.borrow()),
        "channels_native_id_writes"
    );
    assert_eq!(
        source["before"]["trace"],
        json!(before_trace),
        "{prefix}_native_before_trace"
    );
    assert_eq!(
        source["trace"],
        json!(*trace.borrow()),
        "{prefix}_native_trace"
    );
    assert_eq!(
        source["listeners"],
        json!(*listeners.borrow()),
        "{prefix}_native_listeners"
    );
    if op != "channels" {
        assert_eq!(
            source["sends"],
            json!(*sends.borrow()),
            "{prefix}_native_sends"
        );
    } else {
        assert_eq!(
            source["sends"].as_array().unwrap().len(),
            sends.borrow().len(),
            "channels_native_send_count"
        );
    }
}
#[test]
fn actual_typescript_ws_lifecycle() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_lifecycle_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ws_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 434, "ws_source_count");
    let mut defs = Vec::new();
    for venue in ["public", "private"] {
        for disposed in [false, true] {
            for failure in [
                "none",
                "disposed",
                "url",
                "create",
                "write-socket",
                "add-open",
                "add-message",
                "add-close",
            ] {
                defs.push((
                    venue,
                    "connect",
                    json!({"disposed":disposed,"failure":failure,"id":null}),
                ));
            }
        }
        for connected in [false, true] {
            for late_disposed in [false, true] {
                for late_connected in [false, true] {
                    for failure in [
                        "none",
                        "connected0",
                        "send",
                        "schedule",
                        "write-ping",
                        "disposed",
                        "connected1",
                        "repeat",
                    ] {
                        defs.push((venue, "ping", json!({"connected":connected,"lateDisposed":late_disposed,"lateConnected":late_connected,"failure":failure,"id":null})));
                    }
                }
            }
        }
    }
    for mask in 0..2 {
        for layout in 0..3 {
            for failure in [
                "none",
                "ping0",
                "ping1",
                "clear-ping",
                "write-ping",
                "clear-r0",
                "clear-r1",
                "retry-clear",
                "clear-p0",
                "clear-p1",
                "poll-clear",
                "base",
            ] {
                defs.push((
                    "public",
                    "dispose",
                    json!({"mask":mask,"layout":layout,"failure":failure,"id":null}),
                ));
            }
        }
    }
    for mask in 0..4 {
        for failure in [
            "none",
            "ping0",
            "ping1",
            "clear-ping",
            "write-ping",
            "orders0",
            "orders1",
            "clear-orders",
            "write-orders",
            "base",
        ] {
            defs.push((
                "private",
                "dispose",
                json!({"mask":mask,"failure":failure,"id":null}),
            ));
        }
    }
    for mask in [0, 2] {
        for failure in [
            "none",
            "orders0",
            "orders1",
            "clear-orders",
            "fetch-get",
            "schedule",
            "write-orders",
        ] {
            defs.push((
                "private",
                "throttle",
                json!({"mask":mask,"failure":failure,"id":null}),
            ));
        }
    }
    for disposed in [false, true] {
        for connected in [false, true] {
            for has_socket in [false, true] {
                for failure in [
                    "none",
                    "disposed",
                    "log-info",
                    "ping-call",
                    "clear-sent",
                    "clear-pending",
                    "connected0",
                    "log-warning",
                    "ws-read",
                    "close",
                    "subscribe",
                ] {
                    defs.push(("public", "open", json!({"disposed":disposed,"connected":connected,"hasSocket":has_socket,"failure":failure,"id":null})));
                }
            }
        }
    }
    for id in [
        0.0,
        -0.0,
        9_007_199_254_740_991.0,
        9_007_199_254_740_992.0,
        f64::NAN,
        f64::INFINITY,
    ] {
        for failure in [
            "none",
            "id-read0",
            "id-write0",
            "send0",
            "id-read1",
            "id-write1",
            "send1",
            "id-read2",
            "id-write2",
            "send2",
        ] {
            defs.push((
                "private",
                "channels",
                json!({"id":bits(id),"failure":failure}),
            ));
        }
    }
    assert_eq!(defs.len(), 434, "ws_independent_count");
    let mut completed = 0;
    for (i, (venue, op, input)) in defs.into_iter().enumerate() {
        native_case(venue, op, input, &cases[i]);
        completed += 1;
    }
    assert_eq!(completed, 434, "ws_completed_cases");
    let cases = source["interests"].as_array().unwrap();
    assert_eq!(cases.len(), 400, "interest_source_count");
    completed = 0;
    let mut channels: Vec<Vec<u16>> = [
        "markets_summary",
        "trades.X",
        "trades.",
        "trades.X.extra",
        "bbo.X",
        "bbo.",
        "order_book.X",
        "order_book.",
        "unknown",
    ]
    .into_iter()
    .map(units)
    .collect();
    let mut surrogate = units("trades.");
    surrogate.push(0xd800);
    channels.push(surrogate);
    for channel in channels {
        for mask in 0..8 {
            for failure in ["none", "market", "desired", "unused", "book"] {
                let row = &cases[completed];
                assert_eq!(row["channel"], json!(channel), "interest_live_channel");
                assert_eq!(row["mask"], mask);
                assert_eq!(row["failure"], failure);
                let trace = RefCell::new(Vec::<String>::new());
                let error = Rc::new(Marker("error"));
                let touch = |name: &str| {
                    trace.borrow_mut().push(name.to_owned());
                    if name == failure {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                };
                let result = public::has_active_subscription_interest(
                    &channel,
                    || {
                        touch("market")?;
                        Ok(if mask & 2 != 0 { 2.0 } else { 0.0 })
                    },
                    |symbol| {
                        touch("desired")?;
                        Ok(mask & 1 != 0 && (symbol == [88] || symbol == [0xd800]))
                    },
                    |symbol| {
                        touch("market")?;
                        Ok(mask & 2 != 0 && (symbol == [88] || symbol == [0xd800]))
                    },
                    |symbol| {
                        touch("book")?;
                        Ok(if mask & 4 != 0 && symbol == [88] {
                            Some(2.0)
                        } else {
                            None
                        })
                    },
                );
                match result {
                    Ok(value) => {
                        assert_eq!(
                            row["outcome"]["ok"].as_bool(),
                            Some(true),
                            "interest_native_success"
                        );
                        assert_eq!(
                            row["outcome"]["value"].as_bool(),
                            Some(value),
                            "interest_native_value"
                        );
                    }
                    Err(actual) => {
                        assert_eq!(
                            row["outcome"]["ok"].as_bool(),
                            Some(false),
                            "interest_native_error"
                        );
                        assert!(
                            Rc::ptr_eq(&actual, &error),
                            "interest_native_error_identity"
                        );
                        assert_eq!(row["outcome"]["same"].as_bool(), Some(true));
                    }
                }
                assert_eq!(
                    row["trace"],
                    json!(*trace.borrow()),
                    "interest_native_trace"
                );
                completed += 1;
            }
        }
    }
    assert_eq!(completed, 400, "interest_completed_cases");
}
