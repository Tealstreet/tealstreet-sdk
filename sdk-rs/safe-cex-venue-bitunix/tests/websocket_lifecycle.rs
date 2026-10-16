use safe_cex_num::{JsJsonValue, js_json_stringify};
use safe_cex_venue_bitunix::{
    websocket_futures as futures, websocket_private as private, websocket_public as public,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Error = &'static str;
struct State {
    timer: Option<u64>,
    ping_at: f64,
    disposed: bool,
    queue: Rc<RefCell<Vec<Value>>>,
    loaded: Value,
    reads: usize,
}
fn resolve<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("WS mock future must be ready"),
    }
}
fn text(v: &str) -> JsJsonValue {
    JsJsonValue::String(v.into())
}
fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"type":"undefined"}),
        JsJsonValue::Null => json!({"type":"null"}),
        JsJsonValue::Bool(value) => json!({"type":"boolean","value":value}),
        JsJsonValue::Number(value) => {
            json!({"type":"number","bits":format!("{:016x}",if value.is_nan(){0x7ff8_0000_0000_0000}else{value.to_bits()})})
        }
        JsJsonValue::String(value) => {
            json!({"type":"string","units":value.encode_utf16().collect::<Vec<_>>()})
        }
        JsJsonValue::Array(values) => {
            json!({"type":"array","items":values.iter().map(obs).collect::<Vec<_>>()})
        }
        JsJsonValue::Object(values) => {
            json!({"type":"object","entries":values.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("unexpected mock BigInt"),
    }
}
fn value_obs(v: &Value) -> Value {
    obs(&JsJsonValue::from(v.clone()))
}
fn timer(v: Option<u64>) -> JsJsonValue {
    v.map_or(JsJsonValue::Undefined, |v| JsJsonValue::Number(v as f64))
}
fn flag(f: &Value, k: &str) -> bool {
    f[k].as_bool() == Some(true)
}
fn bits(f: &Value, k: &str, default: f64) -> f64 {
    f[k].as_str().map_or(default, |s| {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    })
}
fn record(events: &RefCell<Vec<Value>>, row: &Value, values: Vec<JsJsonValue>) {
    let native = json!(values.iter().map(obs).collect::<Vec<_>>());
    let index = events.borrow().len();
    assert_eq!(
        native, row["events"][index],
        "WS native effect {}:{index}",
        row["key"]
    );
    events.borrow_mut().push(native);
}
fn clear(events: &RefCell<Vec<Value>>, row: &Value, t: Option<u64>) -> Result<(), Error> {
    record(events, row, vec![text("clear"), timer(t)]);
    if flag(&row["fixture"], "clearFail") {
        Err("clear")
    } else {
        Ok(())
    }
}
fn set_timer(
    state: &RefCell<State>,
    events: &RefCell<Vec<Value>>,
    row: &Value,
    field: &str,
    t: Option<u64>,
) -> Result<(), Error> {
    record(events, row, vec![text("setTimer"), text(field), timer(t)]);
    if flag(&row["fixture"], "setFail") {
        return Err("set");
    };
    state.borrow_mut().timer = t;
    Ok(())
}
fn send(
    events: &RefCell<Vec<Value>>,
    row: &Value,
    frame: JsJsonValue,
    kind: &str,
) -> Result<(), Error> {
    record(events, row, vec![text("frame"), frame.clone()]);
    if flag(&row["fixture"], "stringifyFail") {
        return Err("stringify");
    }
    let wire = js_json_stringify(&frame).unwrap().unwrap();
    record(events, row, vec![text(kind), text(&wire)]);
    if flag(&row["fixture"], "sendFail") {
        Err("send")
    } else {
        Ok(())
    }
}
fn schedule(
    events: &RefCell<Vec<Value>>,
    row: &Value,
    handler: &str,
    delay: f64,
) -> Result<u64, Error> {
    record(
        events,
        row,
        vec![text("schedule"), text(handler), JsJsonValue::Number(delay)],
    );
    if flag(&row["fixture"], "scheduleFail") {
        Err("schedule")
    } else {
        Ok(500)
    }
}
#[test]
fn actual_typescript_websocket_lifecycle() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/websocket-lifecycle-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WS actual source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        public::BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION.to_bits(),
        300f64.to_bits(),
        "WS native subscription limit"
    );
    assert_eq!(
        obs(&JsJsonValue::Number(
            public::BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION
        )),
        corpus["max"],
        "WS native actual-source subscription limit"
    );
    assert_eq!(
        public::INTERVAL.len(),
        12,
        "WS independent native interval count"
    );
    assert_eq!(
        corpus["interval"]["entries"].as_array().unwrap().len(),
        12,
        "WS independent source interval count"
    );
    for (i, (key, value)) in public::INTERVAL.iter().enumerate() {
        assert_eq!(
            json!([key, obs(&text(value))]),
            corpus["interval"]["entries"][i],
            "WS native interval {i}"
        );
    }
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 82, "WS independent source lifecycle count");
    let expected_events = [
        5, 0, 5, 1, 4, 5, 3, 2, 5, 0, 5, 1, 4, 5, 3, 2, 6, 0, 4, 1, 5, 6, 2, 4, 4, 6, 6, 5, 3, 4,
        2, 4, 4, 6, 6, 5, 3, 4, 2, 4, 4, 6, 6, 5, 3, 4, 2, 4, 1, 4, 2, 4, 3, 4, 1, 4, 2, 4, 3, 4,
        1, 4, 2, 4, 3, 4, 4, 2, 3, 4, 1, 1, 4, 2, 2, 1, 3, 4, 5, 1, 3, 4,
    ];
    let mut counts = [0; 12];
    for (index, row) in rows.iter().enumerate() {
        let key = row["key"].as_str().unwrap();
        let fixture = &row["fixture"];
        assert_eq!(
            row["events"].as_array().unwrap().len(),
            expected_events[index],
            "WS independent source effects {index}"
        );
        let queue: Vec<Value> = fixture["queue"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![json!([{"symbol":"OLD","ch":"ticker"}])]);
        let old_queue = Rc::new(RefCell::new(queue));
        let state = RefCell::new(State {
            timer: fixture["timer"].as_u64(),
            ping_at: 99.0,
            disposed: flag(fixture, "disposed"),
            queue: old_queue.clone(),
            loaded: json!({"balance":false,"positions":false,"orders":false,"other":17}),
            reads: 0,
        });
        let events = RefCell::new(Vec::new());
        let result: Result<(), Error> = match key {
            "public.ping" | "futures.ping" | "private.ping" => {
                counts[if key == "public.ping" {
                    0
                } else if key == "futures.ping" {
                    1
                } else {
                    2
                }] += 1;
                let performance = || {
                    record(&events, row, vec![text("performance")]);
                    if flag(fixture, "performanceFail") {
                        Err("performance")
                    } else {
                        Ok(bits(fixture, "perfBits", 10.5))
                    }
                };
                let set = |v| {
                    record(&events, row, vec![text("pingAt"), JsJsonValue::Number(v)]);
                    if flag(fixture, "pingAtFail") {
                        Err("pingAt")
                    } else {
                        state.borrow_mut().ping_at = v;
                        Ok(())
                    }
                };
                let clock = || {
                    record(&events, row, vec![text("clock")]);
                    if flag(fixture, "clockFail") {
                        Err("clock")
                    } else {
                        Ok(bits(fixture, "timeBits", 1740000000000.0))
                    }
                };
                let sender = |frame| send(&events, row, frame, "sendLog");
                if key == "public.ping" {
                    public::ping(flag(fixture, "disposed"), performance, set, clock, sender)
                } else if key == "futures.ping" {
                    futures::ping(flag(fixture, "disposed"), performance, set, clock, sender)
                } else {
                    private::ping(
                        flag(fixture, "disposed"),
                        performance,
                        set,
                        sender,
                        || {
                            record(&events, row, vec![text("pong")]);
                            if flag(fixture, "pongFail") {
                                Err("pong")
                            } else {
                                Ok(())
                            }
                        },
                        || {
                            record(&events, row, vec![text("track")]);
                            if flag(fixture, "trackFail") {
                                Err("track")
                            } else {
                                Ok(())
                            }
                        },
                    )
                }
            }
            "private.throttledFetchBalance"
            | "private.throttledFetchOrders"
            | "private.throttledFetchPositions" => {
                counts[if key.ends_with("Balance") {
                    3
                } else if key.ends_with("Orders") {
                    4
                } else {
                    5
                }] += 1;
                let read = |field| {
                    let mut s = state.borrow_mut();
                    let t = if s.reads == 1 && fixture.get("secondTimer").is_some() {
                        fixture["secondTimer"].as_u64()
                    } else {
                        s.timer
                    };
                    s.reads += 1;
                    record(&events, row, vec![text("getTimer"), text(field), timer(t)]);
                    Ok(t)
                };
                let active = |t: &Option<u64>| t.is_some_and(|v| v != 0);
                let clear = |t| clear(&events, row, t);
                let handler = |name: &'static str| {
                    record(&events, row, vec![text("handler"), text(name)]);
                    if flag(fixture, "handlerFail") {
                        Err("handler")
                    } else {
                        Ok(name)
                    }
                };
                let schedule = |h, d| schedule(&events, row, h, d);
                let set = |field, t| set_timer(&state, &events, row, field, Some(t));
                if key.ends_with("Balance") {
                    private::throttled_fetch_balance(read, active, clear, handler, schedule, set)
                } else if key.ends_with("Orders") {
                    private::throttled_fetch_orders(read, active, clear, handler, schedule, set)
                } else {
                    private::throttled_fetch_positions(read, active, clear, handler, schedule, set)
                }
            }
            "private.fetchThrottledBalance"
            | "private.fetchThrottledOrders"
            | "private.fetchThrottledPositions" => {
                counts[if key.ends_with("Balance") {
                    6
                } else if key.ends_with("Orders") {
                    7
                } else {
                    8
                }] += 1;
                let field = if key.ends_with("Balance") {
                    "balance"
                } else if key.ends_with("Orders") {
                    "orders"
                } else {
                    "positions"
                };
                let balance = Rc::new(json!({"free":3}));
                let positions = Rc::new(json!([{"id":"P"}]));
                let orders = Rc::new(json!([{"id":"O"}]));
                let reset = |field| set_timer(&state, &events, row, field, None);
                let disposed = || Ok(state.borrow().disposed);
                let fetched = || {
                    record(&events, row, vec![text("fetch"), text(field)]);
                    if flag(fixture, "fetchFail") {
                        return Err("fetch");
                    };
                    if flag(fixture, "disposeAfterFetch") {
                        let mut s = state.borrow_mut();
                        s.disposed = true;
                        s.loaded["after"] = json!(42);
                    }
                    Ok(())
                };
                let loaded = || {
                    record(&events, row, vec![text("loaded")]);
                    if flag(fixture, "loadedFail") {
                        Err("loaded")
                    } else {
                        Ok(state.borrow().loaded.clone())
                    }
                };
                if field == "balance" {
                    resolve(private::fetch_throttled_balance(
                        reset,
                        disposed,
                        || ready(fetched().map(|()| (balance.clone(), positions.clone()))),
                        loaded,
                        |b, p, mut loaded: Value, flags| {
                            assert!(
                                Rc::ptr_eq(&b, &balance) && Rc::ptr_eq(&p, &positions),
                                "WS native balance/position payload identity"
                            );
                            for (k, v) in flags {
                                loaded[k] = json!(v);
                            }
                            record(
                                &events,
                                row,
                                vec![
                                    text("update"),
                                    JsJsonValue::Object(vec![
                                        ("balance".into(), JsJsonValue::from((*b).clone())),
                                        ("positions".into(), JsJsonValue::from((*p).clone())),
                                        ("loaded".into(), JsJsonValue::from(loaded)),
                                    ]),
                                ],
                            );
                            if flag(fixture, "updateFail") {
                                Err("update")
                            } else {
                                Ok(())
                            }
                        },
                    ))
                } else {
                    let payload = if field == "orders" { orders } else { positions };
                    let expected = payload.clone();
                    let fetch = || ready(fetched().map(|()| payload.clone()));
                    let update =
                        |key: &'static str,
                         values: Rc<Value>,
                         mut loaded: Value,
                         (flag_name, flag_value): (&str, bool)| {
                            assert!(
                                Rc::ptr_eq(&values, &expected),
                                "WS native fetched payload identity"
                            );
                            loaded[flag_name] = json!(flag_value);
                            record(
                                &events,
                                row,
                                vec![
                                    text("update"),
                                    JsJsonValue::Object(vec![
                                        (key.into(), JsJsonValue::from((*values).clone())),
                                        ("loaded".into(), JsJsonValue::from(loaded)),
                                    ]),
                                ],
                            );
                            if flag(fixture, "updateFail") {
                                Err("update")
                            } else {
                                Ok(())
                            }
                        };
                    if field == "orders" {
                        resolve(private::fetch_throttled_orders(
                            reset, disposed, fetch, loaded, update,
                        ))
                    } else {
                        resolve(private::fetch_throttled_positions(
                            reset, disposed, fetch, loaded, update,
                        ))
                    }
                }
            }
            "public.clearSubscriptionQueue" => {
                counts[9] += 1;
                public::clear_subscription_queue(
                    || {
                        let t = state.borrow().timer;
                        record(
                            &events,
                            row,
                            vec![text("getTimer"), text("subscriptionTimeoutId"), timer(t)],
                        );
                        Ok(t)
                    },
                    |t| clear(&events, row, t),
                    || set_timer(&state, &events, row, "subscriptionTimeoutId", None),
                    || {
                        record(
                            &events,
                            row,
                            vec![text("replaceQueue"), JsJsonValue::Array(vec![])],
                        );
                        if flag(fixture, "replaceFail") {
                            return Err("replace");
                        };
                        state.borrow_mut().queue = Rc::new(RefCell::new(Vec::new()));
                        Ok(())
                    },
                )
            }
            "public.drainSubscriptionQueue" => {
                counts[10] += 1;
                public::drain_subscription_queue(
                    || Ok(state.borrow().disposed),
                    || Ok(state.borrow().queue.borrow().is_empty()),
                    || {
                        let q = state.borrow().queue.clone();
                        let v = q.borrow_mut().remove(0);
                        Ok(Some(v))
                    },
                    |v: &Value| Ok(!v.as_array().unwrap().is_empty()),
                    (
                        |op, args| {
                            let frame = JsJsonValue::Object(vec![
                                ("op".into(), text(op)),
                                ("args".into(), JsJsonValue::from(args)),
                            ]);
                            record(&events, row, vec![text("frame"), frame.clone()]);
                            if flag(fixture, "stringifyFail") {
                                Err("stringify")
                            } else {
                                Ok(js_json_stringify(&frame).unwrap().unwrap())
                            }
                        },
                        |wire: String| {
                            record(&events, row, vec![text("send"), text(&wire)]);
                            if flag(fixture, "sendFail") {
                                Err("send")
                            } else {
                                Ok(())
                            }
                        },
                    ),
                    (
                        |h, d| schedule(&events, row, h, d),
                        |t| set_timer(&state, &events, row, "subscriptionTimeoutId", t),
                    ),
                )
            }
            "public.dispose" => {
                counts[11] += 1;
                public::dispose(
                    |topic, handler| {
                        record(&events, row, vec![text("off"), text(topic), text(handler)]);
                        if flag(fixture, "offFail") {
                            Err("off")
                        } else {
                            Ok(())
                        }
                    },
                    || {
                        record(&events, row, vec![text("getPingTimer")]);
                        clear(&events, row, fixture["timer"].as_u64())
                    },
                    || {
                        record(&events, row, vec![text("clearQueue")]);
                        if flag(fixture, "queueClearFail") {
                            Err("queue")
                        } else {
                            Ok(())
                        }
                    },
                    || {
                        record(&events, row, vec![text("baseDispose")]);
                        Ok(())
                    },
                )
            }
            other => panic!("unexpected lifecycle method {other}"),
        };
        match result {
            Ok(()) => assert!(row["error"].is_null(), "WS native resolution {index}"),
            Err(id) => assert_eq!(
                json!({"name":"Error","message":format!("{id} failed"),"id":id}),
                row["error"],
                "WS native rejection {index}"
            ),
        }
        let s = state.borrow();
        assert_eq!(
            obs(&timer(s.timer)),
            row["timer"],
            "WS native timer after method {index}"
        );
        assert_eq!(
            obs(&JsJsonValue::Number(s.ping_at)),
            row["pingAt"],
            "WS native ping timestamp {index}"
        );
        assert_eq!(
            value_obs(&json!(*s.queue.borrow())),
            row["queue"],
            "WS native queue contents {index}"
        );
        assert_eq!(
            value_obs(&json!(*old_queue.borrow())),
            row["oldQueue"],
            "WS native old queue alias {index}"
        );
        assert_eq!(
            Rc::ptr_eq(&s.queue, &old_queue),
            row["queueIdentity"].as_bool().unwrap(),
            "WS native queue replacement identity {index}"
        );
        assert_eq!(
            s.disposed,
            row["disposed"].as_bool().unwrap(),
            "WS native disposed-after-fetch state {index}"
        );
        assert_eq!(
            events.borrow().len(),
            expected_events[index],
            "WS independent native effect count {index}"
        );
    }
    assert_eq!(
        counts,
        [8, 8, 7, 8, 8, 8, 6, 6, 6, 5, 8, 4],
        "WS independent native method totals"
    );
}
