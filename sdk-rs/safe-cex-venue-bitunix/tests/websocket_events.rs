use safe_cex_num::{JsJsonValue, js_json_stringify, js_own_keys};
use safe_cex_venue_bitunix::{websocket_futures as futures, websocket_public as public};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<Value>;
fn number(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
fn undefined() -> Value {
    json!({"undefined":true})
}
fn text(v: &str) -> Value {
    json!({"units":v.encode_utf16().collect::<Vec<_>>()})
}
fn bits(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn string(v: &Value) -> Option<Vec<u16>> {
    v.get("units")
        .map(|v| serde_json::from_value(v.clone()).unwrap())
}
fn equal(v: &Value, s: &str) -> bool {
    string(v).is_some_and(|v| v == s.encode_utf16().collect::<Vec<_>>())
}
fn truthy(v: &Value) -> bool {
    if v == &undefined() || v.is_null() || v == false {
        false
    } else if v.get("bits").is_some() {
        let n = bits(v);
        n != 0.0 && !n.is_nan()
    } else if let Some(s) = string(v) {
        !s.is_empty()
    } else {
        true
    }
}
fn prop(v: &Value, key: &str) -> Value {
    v["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e[0] == key))
        .map_or_else(undefined, |e| e[1].clone())
}
fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => undefined(),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => number(*v),
        JsJsonValue::String(v) => text(v),
        JsJsonValue::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("Unexpected test BigInt"),
    }
}
fn value_obs(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            json!({"entries":js_own_keys(o.keys().map(String::as_str)).into_iter().map(|k|json!([k,value_obs(&o[k])])).collect::<Vec<_>>()})
        }
        _ => obs(&JsJsonValue::from(v.clone())),
    }
}
fn error(name: &str) -> Error {
    Rc::new(json!({"error":["Error",format!("{name} failed")]}))
}
fn settle<T>(future: impl Future<Output = T>) -> T {
    let mut f = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => match f.poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("EVENT fixture remained pending"),
        },
    }
}
#[test]
fn actual_typescript_websocket_events() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/websocket-events-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "EVENT actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 124, "EVENT source floor");
    let mut counts = [0; 7];
    for row in rows {
        let key = row["key"].as_str().unwrap();
        let f = &row["fixture"];
        let events = RefCell::new(Vec::<Value>::new());
        let note = |name: &str, args: Vec<Value>| -> Result<(), Error> {
            let mut e = vec![json!(name)];
            e.extend(args);
            let e = json!(e);
            let i = events.borrow().len();
            assert_eq!(e, row["events"][i], "EVENT native effect {key}:{name}:{i}");
            events.borrow_mut().push(e);
            if f["fail"] == name {
                Err(error(name))
            } else {
                Ok(())
            }
        };
        let disposed = RefCell::new(f["disposed"] == true);
        let secondary = RefCell::new(f["secondary"] == true);
        let count = RefCell::new(
            f["countBits"]
                .as_str()
                .map_or(0., |s| f64::from_bits(u64::from_str_radix(s, 16).unwrap())),
        );
        let timer = RefCell::new(f["timerBits"].as_str().map_or_else(undefined, |s| {
            number(f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
        }));
        let timer_reads = RefCell::new(0usize);
        let is_disposed = || {
            let d = *disposed.borrow();
            note("disposed", vec![json!(d)])?;
            Ok(d)
        };
        let read_timer = || {
            let i = *timer_reads.borrow();
            *timer_reads.borrow_mut() += 1;
            let v = f["timerReads"]
                .get(i)
                .map_or_else(|| timer.borrow().clone(), value_obs);
            note("timer", vec![v.clone()])?;
            Ok(Some(v))
        };
        let ping = || {
            note("ping", vec![])?;
            if f["disposeAtPing"] == true {
                *disposed.borrow_mut() = true;
            }
            Ok(())
        };
        let schedule = |handler: &str, delay: f64| {
            note("schedule", vec![text(handler), number(delay)])?;
            Ok(number(99.))
        };
        let set = |v: Value| {
            note("setTimer", vec![v.clone()])?;
            *timer.borrow_mut() = v;
            Ok(())
        };
        let result: Result<(), Error> = if key.ends_with("onOpen") {
            let open = if key.starts_with("public") {
                counts[0] += 1;
                public::on_open(
                    is_disposed,
                    || {
                        let s = *secondary.borrow();
                        note("secondary", vec![json!(s)])?;
                        Ok(s)
                    },
                    |channel| {
                        note("markets", vec![text(channel)])?;
                        if f["flipSecondary"] == true && channel == "ticker" {
                            *secondary.borrow_mut() = true;
                        }
                        Ok(())
                    },
                    || note("reset", vec![]),
                    || note("subscribe", vec![]),
                    (
                        |e, h| note("off", vec![text(e), text(h)]),
                        |e, h| note("on", vec![text(e), text(h)]),
                    ),
                    (
                        read_timer,
                        |t| t.as_ref().is_some_and(truthy),
                        |t| note("clear", vec![t.unwrap_or_else(undefined)]),
                    ),
                    (ping, schedule, set),
                )
            } else {
                counts[1] += 1;
                futures::on_open(
                    is_disposed,
                    || note("reset", vec![]),
                    |frame| {
                        note("frame", vec![obs(&frame)])?;
                        let wire = js_json_stringify(&frame).unwrap().unwrap();
                        note("send", vec![text(&wire)])
                    },
                    || {
                        note("topics", vec![])?;
                        Ok(row["topicEntries"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|e| (e[0].clone(), bits(&e[1])))
                            .collect())
                    },
                    |topic| note("send", vec![topic]),
                    (
                        read_timer,
                        |t| t.as_ref().is_some_and(truthy),
                        |t| note("clear", vec![t.unwrap_or_else(undefined)]),
                    ),
                    (ping, schedule, set),
                )
            };
            open.and_then(|()| {
                if f["repeat"] == true {
                    if key.starts_with("public") {
                        public::ping_loop(is_disposed, ping, schedule, set)?;
                    } else {
                        futures::ping_loop(is_disposed, ping, schedule, set)?;
                    }
                    if f["disposeBeforeRepeat"] == true {
                        *disposed.borrow_mut() = true;
                        if key.starts_with("public") {
                            public::ping_loop(is_disposed, ping, schedule, set)?;
                        } else {
                            futures::ping_loop(is_disposed, ping, schedule, set)?;
                        }
                    }
                }
                Ok(())
            })
        } else if key.ends_with("onMessage") {
            let input = Rc::new(row["input"].clone());
            let parsed = Rc::new(row["parsed"].clone());
            let list = Rc::new(RefCell::new(vec![Some("h0"), Some("h1")]));
            let payloads = RefCell::new(vec![]);
            let logged = RefCell::new(None::<Error>);
            let failure = || -> Error {
                Rc::new(
                    row["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| e[0] == "log")
                        .map_or_else(
                            || row["error"].clone(),
                            |e| e.as_array().unwrap().last().unwrap().clone(),
                        ),
                )
            };
            let parse = |data: Rc<Value>| {
                note("parse", vec![(*data).clone()])?;
                if key.starts_with("public") && !row["parseError"].is_null() {
                    Err(Rc::new(row["parseError"].clone()))
                } else {
                    Ok(parsed.clone())
                }
            };
            let call = |h: Option<&str>, argument: Value| {
                if let Some(h) = h {
                    note("call", vec![text(h), argument])?;
                    if f["throwHandler"] == true {
                        return Err(failure());
                    }
                    if h == "h0" && f["edit"] == "append" {
                        list.borrow_mut().push(Some("h2"));
                    }
                    if h == "h0" && f["edit"] == "delete" {
                        list.borrow_mut()[1] = None;
                    }
                    Ok(())
                } else {
                    Err(failure())
                }
            };
            let c = || {
                let v = *count.borrow();
                note("count", vec![number(v)])?;
                Ok(v)
            };
            let set_count = |v| {
                note("setCount", vec![number(v)])?;
                *count.borrow_mut() = v;
                Ok(())
            };
            let result = if key.starts_with("public") {
                counts[2] += 1;
                public::on_message(
                    input.clone(),
                    is_disposed,
                    || note("track", vec![]),
                    (c, set_count),
                    (parse, |p| {
                        assert!(Rc::ptr_eq(p, &parsed), "EVENT public parser identity");
                        if p.is_null() {
                            Err(failure())
                        } else {
                            Ok(prop(p, "ch"))
                        }
                    }),
                    (truthy, equal, |t, prefix| {
                        if let Some(units) = string(t) {
                            Ok(units.starts_with(&prefix.encode_utf16().collect::<Vec<_>>()))
                        } else {
                            Err(failure())
                        }
                    }),
                    |t| note("stats", vec![t.clone()]),
                    |topic, p| {
                        assert!(Rc::ptr_eq(p, &parsed), "EVENT public stream identity");
                        note("stream", vec![text(topic), (**p).clone()])
                    },
                    (
                        |p, t, keys| {
                            assert_eq!(keys, ["symbol", "ch"], "EVENT native key fields");
                            note(
                                "key",
                                vec![json!({"entries":[[keys[0],prop(p,"symbol")],[keys[1],t]]})],
                            )?;
                            Ok(row["lookupKey"].clone())
                        },
                        |k| {
                            note("handlers", vec![k])?;
                            if f["noHandlers"] == true {
                                Err(failure())
                            } else {
                                Ok(list.clone())
                            }
                        },
                        |l, i| Ok(l.borrow().get(i).copied()),
                        |h, args| {
                            assert_eq!(args.len(), 1, "EVENT public array length");
                            assert!(
                                Rc::ptr_eq(
                                    &args[0],
                                    payloads
                                        .borrow()
                                        .first()
                                        .map_or(&args[0], |a: &Rc<Vec<Rc<Rc<Value>>>>| &a[0])
                                ),
                                "EVENT public shared parsed identity"
                            );
                            assert!(
                                Rc::ptr_eq(args[0].as_ref(), &parsed),
                                "EVENT public parsed argument identity"
                            );
                            assert!(
                                !payloads.borrow().iter().any(|a| Rc::ptr_eq(a, &args)),
                                "EVENT distinct public handler arrays"
                            );
                            let arg = json!([(**args[0]).clone()]);
                            payloads.borrow_mut().push(args);
                            call(h, arg)
                        },
                    ),
                    |e| {
                        *logged.borrow_mut() = Some(e.clone());
                        note("log", vec![(*e).clone()])
                    },
                )
            } else {
                counts[3] += 1;
                let read = |data: &Rc<Value>| {
                    assert!(Rc::ptr_eq(data, &input), "EVENT blob input identity");
                    let prefix =
                        note("readBlob", vec![]).and_then(|()| note("blobPending", vec![]));
                    let mut prefix = Some(prefix);
                    let mut pending = false;
                    let disposed = &disposed;
                    poll_fn(move |_| {
                        let r = prefix.take().unwrap();
                        if let Err(e) = r {
                            return Poll::Ready(Err(e));
                        }
                        if !pending {
                            pending = true;
                            prefix = Some(Ok(()));
                            return Poll::Pending;
                        }
                        if f["disposeAfterBlob"] == true {
                            *disposed.borrow_mut() = true;
                        }
                        if let Err(e) = note("blobReady", vec![]) {
                            return Poll::Ready(Err(e));
                        }
                        if !row["readError"].is_null() {
                            Poll::Ready(Err(Rc::new(row["readError"].clone())))
                        } else {
                            Poll::Ready(Ok(Rc::new(row["decoded"].clone())))
                        }
                    })
                };
                settle(futures::on_message(
                    input.clone(),
                    is_disposed,
                    || note("track", vec![]),
                    (c, set_count),
                    |d| d.get("blob") == Some(&Value::Bool(true)),
                    read,
                    |message, e| {
                        *logged.borrow_mut() = Some(e.clone());
                        note("log", vec![text(message), (*e).clone()])
                    },
                    parse,
                    (
                        |p| {
                            assert!(Rc::ptr_eq(p, &parsed), "EVENT futures parser identity");
                            let channel = prop(p, "channel");
                            note(
                                "handlers",
                                vec![string(&channel).map_or_else(
                                    || {
                                        text(if channel == undefined() {
                                            "undefined"
                                        } else if channel.get("bits").is_some() {
                                            "7"
                                        } else {
                                            "unknown"
                                        })
                                    },
                                    |u| json!({"units":u}),
                                )],
                            )?;
                            Ok(f["noHandlers"] != true)
                        },
                        |p| {
                            let channel = prop(p, "channel");
                            note(
                                "handlers",
                                vec![string(&channel).map_or_else(
                                    || {
                                        text(if channel == undefined() {
                                            "undefined"
                                        } else if channel.get("bits").is_some() {
                                            "7"
                                        } else {
                                            "unknown"
                                        })
                                    },
                                    |u| json!({"units":u}),
                                )],
                            )?;
                            let len = list.borrow().len();
                            Ok((list.clone(), len))
                        },
                        |l, i| Ok(l.borrow().get(i).copied().flatten()),
                        |h, p| {
                            assert!(Rc::ptr_eq(p, &parsed), "EVENT futures argument identity");
                            call(Some(h), (**p).clone())
                        },
                    ),
                ))
            };
            if let Some(e) = logged.borrow().as_ref() {
                assert_eq!(**e, *failure(), "EVENT logged native exception");
            }
            result
        } else {
            let ticker = Rc::new(());
            let data = Rc::new(());
            let leaf_index = RefCell::new(0usize);
            let depth = key.ends_with("handleDepthBookStreamEvent");
            let raw=f.get("data").cloned().unwrap_or_else(||if depth{json!({"b":[["12.5","2"]],"a":[["14","2"]]})}else{json!({"la":"12.5","o":"10","b":"2.5","q":"25","mp":"12","ip":"11.5","fr":"-0.01","ft":"123","a":[["14","2"]]})});
            let get_ticker = || {
                note("symbol", vec![])?;
                note("ticker", vec![])?;
                Ok((f["noTicker"] != true).then(|| ticker.clone()))
            };
            let get_data = || {
                note("data", vec![])?;
                Ok(data.clone())
            };
            let read = |d: &Rc<()>, field: &str| {
                assert!(Rc::ptr_eq(d, &data), "EVENT stream data identity");
                if !depth {
                    note("field", vec![text(field)])?;
                }
                if raw.is_object()
                    && (depth
                        || raw.get(field).is_some()
                        || f["fieldUnits"]["key"] == field
                        || f["symbolValue"] == true && field == "la")
                {
                    note("property", vec![text(field)])?;
                }
                let raw_value = if f["fieldUnits"]["key"] == field {
                    json!({"units":f["fieldUnits"]["units"]})
                } else if f["symbolValue"] == true && field == "la" {
                    json!({"symbol":"input"})
                } else if depth {
                    raw.get(field)
                        .and_then(|v| v.get(0))
                        .and_then(|v| v.get(0))
                        .map_or_else(undefined, value_obs)
                } else {
                    raw.get(field).map_or_else(undefined, value_obs)
                };
                note("float", vec![raw_value.clone()])?;
                let i = *leaf_index.borrow();
                *leaf_index.borrow_mut() += 1;
                let leaf = &row["leaves"][i];
                assert_eq!(raw_value, leaf["value"], "EVENT native coercion input");
                if let Some(e) = leaf.get("error") {
                    Err(Rc::new(e.clone()))
                } else {
                    Ok(serde_json::from_value::<Vec<u16>>(leaf["units"].clone()).unwrap())
                }
            };
            let update4 = |t, patch: [(&str, f64); 4]| {
                assert!(Rc::ptr_eq(&t, &ticker), "EVENT ticker identity");
                note(
                    "update",
                    vec![
                        json!({"entries":patch.iter().map(|(k,v)|json!([k,number(*v)])).collect::<Vec<_>>()}),
                    ],
                )
            };
            if depth {
                counts[6] += 1;
                public::handle_depth_book_stream_event(get_ticker, get_data, read, |t, patch| {
                    assert!(Rc::ptr_eq(&t, &ticker), "EVENT depth ticker identity");
                    note(
                        "update",
                        vec![
                            json!({"entries":patch.iter().map(|(k,v)|json!([k,number(*v)])).collect::<Vec<_>>()}),
                        ],
                    )
                })
            } else if key.ends_with("handleTickerStreamEvent") {
                counts[4] += 1;
                public::handle_ticker_stream_event(get_ticker, get_data, read, update4)
            } else {
                counts[5] += 1;
                public::handle_price_stream_event(get_ticker, get_data, read, update4)
            }
        };
        assert_eq!(
            result.as_ref().err().map(|e| e.as_ref()),
            if row["error"].is_null() {
                None
            } else {
                Some(&row["error"])
            },
            "EVENT native result {key}"
        );
        assert_eq!(
            events.borrow().len(),
            row["events"].as_array().unwrap().len(),
            "EVENT native effect count {key}"
        );
        assert_eq!(
            number(*count.borrow()),
            row["count"],
            "EVENT native counter {key}"
        );
        assert_eq!(*timer.borrow(), row["timer"], "EVENT native timer {key}");
    }
    assert_eq!(counts, [13, 13, 23, 27, 16, 16, 16], "EVENT family floors");
}
