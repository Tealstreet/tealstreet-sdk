use safe_cex_venue_bitunix::exchange_lifecycle as ex;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug)]
enum Error {
    Host(String),
    Opaque(Rc<()>),
}
fn number(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
fn units(v: &str) -> Value {
    json!({"units":v.encode_utf16().collect::<Vec<_>>()})
}
fn symbol(v: &Vec<u16>) -> Value {
    json!({"units":v})
}
fn obs(v: &Value) -> Value {
    match v {
        Value::Number(n) => number(n.as_f64().unwrap()),
        Value::String(s) => units(s),
        Value::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), obs(v))).collect()),
        _ => v.clone(),
    }
}
fn settle<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => match future.poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("LIFECYCLE fixture future remained pending"),
        },
    }
}
#[test]
// clippy::possible_missing_else is a false positive on these minified harness
// lines: the flagged `}if` and `}{` are a guard-clause chain of early returns
// followed by a scoped block, not a dropped `else`. Inserting one would change
// control flow the source does not have.
#[allow(clippy::possible_missing_else)]
fn actual_typescript_exchange_lifecycle() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/exchange-lifecycle-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "LIFECYCLE actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 30, "LIFECYCLE source floor");
    let mut counts = [0; 4];
    for row in rows {
        let f = &row["fixture"];
        let events = RefCell::new(vec![]);
        let opaque_error = Rc::new(());
        let note = |name: &str, args: Vec<Value>| -> Result<(), Error> {
            let mut event = vec![json!(name)];
            event.extend(args);
            let event = json!(event);
            let i = events.borrow().len();
            assert_eq!(
                event, row["events"][i],
                "LIFECYCLE native effect {}:{name}:{i}",
                row["name"]
            );
            events.borrow_mut().push(event);
            if f["fail"] == name {
                Err(Error::Host(format!("{name} failed")))
            } else {
                Ok(())
            }
        };
        let primary = Rc::new("primary".to_owned());
        let secondary = Rc::new("secondary".to_owned());
        let mut old = vec![
            Some(Rc::new("old0".to_owned())),
            Some(Rc::new("old1".to_owned())),
        ];
        if f["hole"] == true {
            old[0] = None;
        }
        let old = Rc::new(RefCell::new(old));
        let shards = RefCell::new(old.clone());
        let dispose = |s: Rc<String>| {
            note("dispose", vec![units(&s)])?;
            if f["edit"] == true && *s == "old0" {
                let mut old = old.borrow_mut();
                old[1] = None;
                old.push(Some(Rc::new("extra".into())));
            }
            Ok(())
        };
        let get_shards = || {
            let captured = shards.borrow().clone();
            let len = captured.borrow().len();
            Ok((len, move |i| {
                Ok(captured.borrow().get(i).cloned().flatten())
            }))
        };
        let result = match row["name"].as_str().unwrap() {
            "dispose" => {
                counts[0] += 1;
                ex::dispose(
                    || note("base", vec![]),
                    || dispose(primary.clone()),
                    || dispose(secondary.clone()),
                    get_shards,
                    dispose,
                    || dispose(Rc::new("futures".into())),
                    || dispose(Rc::new("private".into())),
                )
                .map(|()| true)
            }
            "configurePublicWebsocketShards" => {
                counts[1] += 1;
                let count = f["count"].as_u64().unwrap_or(0) as usize;
                let buffers = RefCell::new(Vec::<Rc<RefCell<Vec<Vec<u16>>>>>::new());
                let sockets = RefCell::new(6 + usize::from(f["edit"] == true && f["hole"] != true));
                let result = ex::configure_public_websocket_shards(
                    get_shards,
                    dispose,
                    || {
                        note("replace", vec![])?;
                        *shards.borrow_mut() = Rc::new(RefCell::new(vec![]));
                        Ok(())
                    },
                    (
                        || Ok((0..count).collect::<Vec<_>>()),
                        |i| {
                            note("symbol", vec![number(i as f64)])?;
                            Ok(if i == 0 {
                                vec![0xd800]
                            } else {
                                format!("S{i}").encode_utf16().collect()
                            })
                        },
                    ),
                    (|| Ok(primary.clone()), || Ok(secondary.clone())),
                    |w, symbols| {
                        note(
                            "set",
                            vec![
                                units(w),
                                json!(symbols.borrow().iter().map(symbol).collect::<Vec<_>>()),
                            ],
                        )?;
                        buffers.borrow_mut().push(symbols.clone());
                        if f["editSymbols"] == true && **w == "primary" {
                            symbols.borrow_mut().pop();
                        }
                        Ok(())
                    },
                    |second, symbols| {
                        note(
                            if second {
                                "createSecondary"
                            } else {
                                "createPrimary"
                            },
                            vec![json!(
                                symbols.borrow().iter().map(symbol).collect::<Vec<_>>()
                            )],
                        )?;
                        buffers.borrow_mut().push(symbols.clone());
                        if f["editCreateSymbols"] == true && !second {
                            symbols.borrow_mut().push("extra".encode_utf16().collect());
                        }
                        let id = *sockets.borrow();
                        *sockets.borrow_mut() += 1;
                        Ok(Rc::new(format!("new{id}")))
                    },
                    |a, b| {
                        note("push", vec![json!([units(&a), units(&b)])])?;
                        shards.borrow().borrow_mut().extend([Some(a), Some(b)]);
                        Ok(())
                    },
                    || Ok(shards.borrow().borrow().iter().flatten().cloned().collect()),
                );
                if let Ok(values) = &result {
                    assert!(
                        Rc::ptr_eq(&values[0], &primary) && Rc::ptr_eq(&values[1], &secondary),
                        "LIFECYCLE returned primary identity"
                    );
                    let active = shards.borrow();
                    let active = active.borrow();
                    for (r, s) in values[2..].iter().zip(active.iter().flatten()) {
                        assert!(Rc::ptr_eq(r, s), "LIFECYCLE shard identity");
                    }
                    let buffers = buffers.borrow();
                    for pair in buffers.as_chunks::<2>().0 {
                        assert!(
                            Rc::ptr_eq(&pair[0], &pair[1]),
                            "LIFECYCLE shared symbol shard identity"
                        );
                    }
                }
                result.map(|_| true)
            }
            "listenOrderBook" => {
                counts[2] += 1;
                let symbol: Vec<u16> = f
                    .get("symbol")
                    .map(|v| serde_json::from_value(v.clone()).unwrap())
                    .unwrap_or(vec![0xd800, 65, 0xdc00]);
                let symbol = Rc::new(symbol);
                let callback = Rc::new(());
                let output = Rc::new(());
                ex::listen_order_book(symbol.clone(), callback.clone(), |s, c| {
                    assert!(
                        Rc::ptr_eq(&s, &symbol) && Rc::ptr_eq(&c, &callback),
                        "LIFECYCLE listener argument identity"
                    );
                    note("listen", vec![json!({"units":*s})])?;
                    if f["reject"] == true {
                        Err(Error::Opaque(opaque_error.clone()))
                    } else {
                        Ok(output.clone())
                    }
                })
                .map(|r| {
                    assert!(
                        Rc::ptr_eq(&r, &output),
                        "LIFECYCLE listener result identity"
                    );
                    true
                })
            }
            "fetchIncomeHistoryRange" => {
                counts[3] += 1;
                let a = f64::from_bits(
                    u64::from_str_radix(f["start"].as_str().unwrap_or("8000000000000000"), 16)
                        .unwrap(),
                );
                let b = f64::from_bits(
                    u64::from_str_radix(f["end"].as_str().unwrap_or("7ff8000000000000"), 16)
                        .unwrap(),
                );
                let income = f.get("income").cloned().map(Rc::new);
                let output = Rc::new(());
                settle(ex::fetch_income_history_range(a,b,income.clone(),|opts|{
     assert_eq!(opts.income_type.is_some(),income.is_some(),"LIFECYCLE undefined income carrier");if let(Some(a),Some(b))=(&opts.income_type,&income){assert!(Rc::ptr_eq(a,b),"LIFECYCLE income argument identity");}
     let result=note("fetch",vec![json!({"startTime":number(opts.start_time),"endTime":number(opts.end_time),"incomeType":opts.income_type.as_ref().map_or(json!({"undefined":true}),|v|obs(v))})]);
     let mut pending=false;let mut result=Some(result);let output=output.clone();let opaque_error=opaque_error.clone();
     poll_fn(move |_|{let r=result.take().unwrap();if r.is_err(){return Poll::Ready(r.map(|_|output.clone()));}if f["reject"]==true{return Poll::Ready(Err(Error::Opaque(opaque_error.clone())));}if f["deferred"]==true{if !pending{note("pending",vec![]).unwrap();pending=true;result=Some(Ok(()));return Poll::Pending;}note("settled",vec![]).unwrap();}Poll::Ready(Ok(output.clone()))})
    })).map(|r|{assert!(Rc::ptr_eq(&r,&output),"LIFECYCLE income result identity");true})
            }
            _ => panic!("Unknown lifecycle family"),
        };
        let error = match &result {
            Ok(_) => Value::Null,
            Err(Error::Host(message)) => json!({"message":message}),
            Err(Error::Opaque(e)) => {
                assert!(
                    Rc::ptr_eq(e, &opaque_error),
                    "LIFECYCLE native error identity"
                );
                json!({"opaque":true})
            }
        };
        assert_eq!(error, row["error"], "LIFECYCLE native error");
        assert_eq!(
            result.unwrap_or(false),
            row["identity"],
            "LIFECYCLE native result identity"
        );
        assert_eq!(
            events.borrow().len(),
            row["events"].as_array().unwrap().len(),
            "LIFECYCLE native event count"
        );
        assert_eq!(
            json!(
                shards
                    .borrow()
                    .borrow()
                    .iter()
                    .map(|s| s.as_ref().map(|s| s.as_str()))
                    .collect::<Vec<_>>()
            ),
            row["shards"],
            "LIFECYCLE native shard state"
        );
    }
    assert_eq!(counts, [5, 15, 4, 6], "LIFECYCLE per-family floor");
}
