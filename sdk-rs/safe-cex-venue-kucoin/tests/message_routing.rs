use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::OrderStatus;
use safe_cex_venue_kucoin::message_routing::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type Error = Rc<ExchangeError>;
type Trace = Vec<(String, Option<Vec<u16>>)>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn error() -> Error {
    let mut e = ExchangeError::new("routing failure");
    e.name = "TypeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::Js::str("ROUTE"));
    e.store_subscriber_ids = vec![7, 3];
    Rc::new(e)
}
fn verify(result: Result<(), Error>, fails: bool, e: &Error) -> bool {
    if fails {
        let a = result.expect_err("ROUTE native propagated error");
        assert_eq!(*a, **e, "ROUTE native original error fields");
        assert!(Rc::ptr_eq(&a, e), "ROUTE native error identity");
        true
    } else {
        result.expect("ROUTE native success");
        false
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Component {
    Text(Vec<u16>),
    Undefined,
    Number(i32),
}
fn component_text(v: &Component) -> Vec<u16> {
    match v {
        Component::Text(s) => s.clone(),
        Component::Undefined => units("undefined"),
        Component::Number(n) => units(&n.to_string()),
    }
}
struct Message {
    payload: i32,
    ty: Option<String>,
    topic: RefCell<Option<Vec<u16>>>,
    reads: Cell<usize>,
    mutate: bool,
    custom: bool,
}
fn get_topic(j: &Message) -> Option<Vec<u16>> {
    let v = j.topic.borrow().clone();
    j.reads.set(j.reads.get() + 1);
    if j.mutate && j.reads.get() == 1 {
        *j.topic.borrow_mut() = Some(units("/contractMarket/snapshot:BTC"));
    }
    v
}
fn starts(v: &[u16], p: &str) -> bool {
    v.starts_with(&units(p))
}
fn predicate(j: &Message, op: &str, fail: &str, e: &Error) -> Result<bool, Error> {
    if let Some(t) = op.strip_prefix("type:") {
        if fail == "type" {
            return Err(e.clone());
        }
        return Ok(j.ty.as_deref() == Some(t));
    }
    if let Some(t) = op.strip_prefix("equal:") {
        return Ok(get_topic(j).as_deref() == Some(&units(t)));
    }
    if let Some(p) = op.strip_prefix("optional:") {
        return Ok(get_topic(j).is_some_and(|v| {
            if j.custom {
                p == "/contractMarket/level2"
            } else {
                starts(&v, p)
            }
        }));
    }
    let present = get_topic(j).is_some_and(|v| !v.is_empty());
    if !present {
        return Ok(false);
    }
    Ok(get_topic(j).is_some_and(|v| {
        if j.custom {
            op == "/contractMarket/level2"
        } else {
            starts(&v, op)
        }
    }))
}
fn expected(kind: &str, scenario: usize) -> Trace {
    let mut t = vec![("track".into(), None)];
    if kind.ends_with("private") {
        t.push(("diagnostic".into(), None));
    }
    t.push(("parse".into(), None));
    let mut route = |stat: &str, target: &str| {
        t.push(("stats".into(), Some(units(stat))));
        t.push((target.into(), None));
    };
    match (kind, scenario) {
        (_, 1) => t.push((
            if kind.starts_with("spot") {
                "pong-interval"
            } else {
                "pong"
            }
            .into(),
            None,
        )),
        ("futures-private", 3) => t.push(("subscribe".into(), None)),
        ("futures-public", 4) => route("ticker", "tickerV2"),
        ("futures-public", 5 | 20) => route("snapshot", "snapshot"),
        ("futures-public", 6) => route("instrument", "instrument"),
        ("futures-private", 9) => route("order", "order"),
        ("futures-private", 10) => route("orderAlgo", "algo"),
        ("futures-private", 11..=14) => route("account", "refresh-snapshot"),
        ("spot-private", 15) => route("order", "order"),
        ("spot-private", 16) => route("balance", "refresh-balance"),
        ("spot-public", 17) => route("ticker", "ticker"),
        ("futures-public", 7 | 21) => {
            t.push(("internal:BTC".into(), None));
            let key = units(if scenario == 7 {
                "candle_INTERNAL_1min"
            } else {
                "candle_INTERNAL_undefined"
            });
            t.push(("stats".into(), Some(key.clone())));
            t.push(("for-of".into(), Some(key)));
            for id in [1, 2, 3] {
                t.push((format!("cb:{id}"), None));
            }
        }
        ("futures-public", 8) => {
            t.push(("internal:undefined".into(), None));
            let key = units("orderBook_undefined");
            t.push(("stats".into(), Some(key.clone())));
            t.push(("for-of".into(), Some(key)));
            for id in [1, 2, 3] {
                t.push((format!("cb:{id}"), None));
            }
        }
        ("futures-public", 24) => {
            t.push(("internal:7".into(), None));
            let key = units("orderBook_7");
            t.push(("stats".into(), Some(key.clone())));
            t.push(("for-of".into(), Some(key)));
            for id in [1, 2, 3] {
                t.push((format!("cb:{id}"), None));
            }
        }
        ("spot-public", 18) => {
            t.push(("candle-helper".into(), None));
            let key = vec![0xd800, 51];
            t.push(("stats".into(), Some(key.clone())));
            t.push(("for-each".into(), Some(key)));
            for id in [1, 2] {
                t.push((format!("cb:{id}"), None));
            }
        }
        ("spot-public", 19) => {
            let mut key = units("orderBook_BTC-");
            key.push(0xd800);
            t.push(("stats".into(), Some(key.clone())));
            t.push(("for-each".into(), Some(key)));
            for id in [1, 2] {
                t.push((format!("cb:{id}"), None));
            }
        }
        _ => {}
    }
    t
}
#[test]
fn routing_native_before_source() {
    let topics: [Option<&str>; 25] = [
        None,
        None,
        None,
        None,
        Some("/contractMarket/tickerV2:BTC"),
        Some("/contractMarket/snapshot:BTC"),
        Some("/contract/instrument:BTC"),
        Some("/contractMarket/limitCandle:BTC_1min"),
        Some("/contractMarket/level2"),
        Some("/contractMarket/tradeOrders:BTC"),
        Some("/contractMarket/advancedOrders:BTC"),
        Some("/contract/positionAll"),
        Some("/contractAccount/wallet"),
        Some("/contract/marginMode"),
        Some("/contract/crossLeverage"),
        Some("/spotMarket/tradeOrdersV2"),
        Some("/account/balance"),
        Some("/market/ticker:all"),
        Some("/market/candles:BTC_1min"),
        Some("/spotMarket/level2Depth50:BTC-"),
        Some("/contractMarket/tickerV2:BTC"),
        Some("/contractMarket/limitCandle:BTC"),
        None,
        None,
        Some("custom"),
    ];
    let mut input = json!({"routes":[],"algo":[]});
    let mut views = json!({"routes":[],"algo":[]});
    for kind in [
        "futures-public",
        "futures-private",
        "spot-public",
        "spot-private",
    ] {
        let selected = match kind {
            "futures-public" => 4,
            "futures-private" => 9,
            "spot-public" => 17,
            _ => 15,
        };
        let mut cases = (0..25).map(|s| (s, "", false, false)).collect::<Vec<_>>();
        cases.extend(
            ["track", "parse", "type", "stats", "handler"]
                .map(|fail| (selected, fail, false, false)),
        );
        cases.extend([(selected, "", true, false), (selected, "", false, true)]);
        for (scenario, fail, early_dispose, flip) in cases {
            let mut topic = topics[scenario].map(units);
            if scenario == 19 {
                topic.as_mut().unwrap().push(0xd800);
            }
            let message = Rc::new(Message {
                payload: 17,
                ty: match scenario {
                    1 => Some("pong".into()),
                    3 => Some("welcome".into()),
                    _ => None,
                },
                topic: RefCell::new(topic.clone()),
                reads: Cell::new(0),
                mutate: scenario == 20,
                custom: scenario == 24,
            });
            let disposed = Cell::new(early_dispose);
            let trace = RefCell::new(Trace::new());
            let e = error();
            let actual_error = Cell::new(false);
            let effect =
                |name: &str, j: Option<&Rc<Message>>, key: Option<&[u16]>| -> Result<(), Error> {
                    if let Some(j) = j {
                        assert!(
                            Rc::ptr_eq(j, &message),
                            "ROUTE native decoded message identity"
                        );
                        assert_eq!(j.payload, 17, "ROUTE native decoded payload");
                    }
                    trace
                        .borrow_mut()
                        .push((name.into(), key.map(<[u16]>::to_vec)));
                    if (fail == name) || (fail == "handler" && j.is_some()) {
                        actual_error.set(true);
                        return Err(e.clone());
                    }
                    if flip && name == "track" {
                        disposed.set(true);
                    }
                    if name == "for-of" || name == "for-each" {
                        let slots = RefCell::new(vec![1, 2]);
                        let length = if name == "for-each" { 2 } else { usize::MAX };
                        let mut at = 0;
                        while at < length && at < slots.borrow().len() {
                            let id = slots.borrow()[at];
                            trace.borrow_mut().push((format!("cb:{id}"), None));
                            if at == 0 {
                                slots.borrow_mut().push(3);
                            }
                            at += 1;
                        }
                    }
                    Ok(())
                };
            let parse = || -> Result<Option<Rc<Message>>, Error> {
                trace.borrow_mut().push(("parse".into(), None));
                if fail == "parse" {
                    actual_error.set(true);
                    return Err(e.clone());
                }
                Ok(if matches!(scenario, 2 | 22 | 23) {
                    None
                } else {
                    Some(message.clone())
                })
            };
            let pred = |j: &Rc<Message>, op: &str| {
                assert!(
                    Rc::ptr_eq(j, &message),
                    "ROUTE native predicate message identity"
                );
                let r = predicate(j, op, fail, &e);
                if r.is_err() {
                    actual_error.set(true);
                }
                r
            };
            let result = match kind {
                "futures-public" => futures_public_message(
                    &disposed,
                    effect,
                    parse,
                    pred,
                    |j, mode| {
                        if j.custom {
                            get_topic(j);
                            return Ok(Component::Number(7));
                        }
                        let t = get_topic(j).unwrap();
                        let part = t.split(|u| *u == 58).nth(1).map(<[u16]>::to_vec);
                        Ok(match part {
                            Some(s) => Component::Text(s),
                            None if mode == "candle" => Component::Text(vec![]),
                            None => Component::Undefined,
                        })
                    },
                    |part| {
                        let Component::Text(t) = part else {
                            panic!("ROUTE fixture split component type");
                        };
                        let mut parts = t.split(|u| *u == 95);
                        let symbol = Component::Text(parts.next().unwrap_or_default().to_vec());
                        let interval = parts
                            .next()
                            .map_or(Component::Undefined, |s| Component::Text(s.to_vec()));
                        Ok((symbol, interval))
                    },
                    |symbol| {
                        if scenario == 24 {
                            assert_eq!(
                                *symbol,
                                Component::Number(7),
                                "ROUTE native opaque split component type before mapping"
                            );
                        }
                        trace.borrow_mut().push((
                            format!(
                                "internal:{}",
                                String::from_utf16_lossy(&component_text(symbol))
                            ),
                            None,
                        ));
                        Ok(match symbol {
                            Component::Text(s) if !s.is_empty() => {
                                Some(Component::Text(units("INTERNAL")))
                            }
                            _ => None,
                        })
                    },
                    |v| Ok(component_text(v)),
                ),
                "futures-private" => futures_private_message(&disposed, effect, parse, pred),
                "spot-private" => spot_private_message(&disposed, effect, parse, pred),
                _ => spot_public_message(&disposed, effect, parse, pred, |j, op| {
                    if op == "candle-helper" {
                        let original = get_topic(j);
                        assert_eq!(
                            original,
                            Some(units("/market/candles:BTC_1min")),
                            "ROUTE native candle helper argument"
                        );
                        trace.borrow_mut().push((op.into(), None));
                        Ok(vec![0xd800, 51])
                    } else {
                        Ok(get_topic(j).unwrap())
                    }
                }),
            };
            let mut exp = if early_dispose {
                vec![]
            } else {
                expected(kind, scenario)
            };
            let failed = if early_dispose {
                false
            } else {
                match fail {
                    "track" => true,
                    "parse" => true,
                    "type" => !matches!(scenario, 2 | 22 | 23),
                    "stats" | "handler" => true,
                    _ => false,
                }
            };
            if failed {
                let stop = if fail == "type" {
                    exp.iter().position(|(n, _)| n == "parse").unwrap()
                } else if fail == "handler" {
                    exp.iter()
                        .position(|(n, _)| ["order", "tickerV2", "ticker"].contains(&n.as_str()))
                        .unwrap()
                } else {
                    exp.iter().position(|(n, _)| n == fail).unwrap()
                };
                exp.truncate(stop + 1);
            }
            assert_eq!(
                *trace.borrow(),
                exp,
                "ROUTE native selected effects/key/order"
            );
            assert_eq!(
                actual_error.get(),
                failed,
                "ROUTE native independent failure reachability"
            );
            let failed = verify(result, failed, &e);
            if scenario == 20 {
                assert_eq!(
                    message.reads.get(),
                    match kind {
                        "futures-public" => 4,
                        "futures-private" => 8,
                        "spot-public" => 3,
                        _ => 2,
                    },
                    "ROUTE native live topic read count"
                );
            }
            if kind == "futures-public" && scenario == 8 {
                assert!(
                    trace
                        .borrow()
                        .iter()
                        .any(|(n, _)| n == "internal:undefined"),
                    "ROUTE native missing symbol distinction"
                );
            }
            input["routes"].as_array_mut().unwrap().push(json!({"kind":kind,"scenario":scenario,"topic":topic,"type":message.ty,"fail":fail,"disposed":early_dispose,"flip":flip}));
            views["routes"].as_array_mut().unwrap().push(json!({"trace":trace.borrow().clone(),"error":failed,"topicReads":message.reads.get()}));
        }
    }
    for scenario in 0..14 {
        let e = error();
        let trace = RefCell::new(Trace::new());
        let raw = Rc::new(RefCell::new(match scenario {
            1 | 7 | 9 | 10 | 13 => Some("cancel"),
            2 => Some("triggered"),
            3 => Some("active"),
            4 | 8 => Some("open"),
            6 => Some(""),
            _ => None,
        }));
        let order = Rc::new(7);
        let message = Rc::new(11);
        let fail = match scenario {
            10 => "event",
            11 => "map",
            12 => "first",
            13 => "update",
            _ => "",
        };
        let call = |name: &str| {
            trace.borrow_mut().push((name.into(), None));
            if name == fail { Err(e.clone()) } else { Ok(()) }
        };
        let result = algo_order_message(
            &message,
            |_| {
                call("has")?;
                Ok(scenario != 0)
            },
            |_| {
                call("data")?;
                Ok(raw.clone())
            },
            |r| {
                assert!(Rc::ptr_eq(r, &raw), "ALGO native raw decoded identity");
                call("map")?;
                if scenario == 9 {
                    *raw.borrow_mut() = Some("active");
                }
                Ok(vec![order.clone()])
            },
            |orders| {
                call("first")?;
                Ok(if scenario == 5 || scenario == 10 {
                    None
                } else {
                    Some(orders[0].clone())
                })
            },
            |r| {
                assert!(Rc::ptr_eq(r, &raw), "ALGO native live event receiver");
                call("event")?;
                Ok(r.borrow()
                    .filter(|s| !s.is_empty())
                    .or(if scenario == 6 { Some("cancel") } else { None })
                    .map(units))
            },
            |p, status| {
                assert!(
                    Rc::ptr_eq(p, &order),
                    "ALGO native decoded parent order identity"
                );
                assert_eq!(**p, 7, "ALGO native parent order id");
                if let Some(s) = status {
                    assert_eq!(s, OrderStatus::Canceled, "ALGO native canceled enum");
                }
                trace
                    .borrow_mut()
                    .push((if status.is_some() { "cancel" } else { "add" }.into(), None));
                if fail == "update" {
                    Err(e.clone())
                } else {
                    Ok(())
                }
            },
        );
        let mut exp: Trace = if scenario == 0 {
            vec![("has".into(), None)]
        } else {
            ["has", "data", "map", "first", "event"]
                .map(|s| (s.into(), None))
                .to_vec()
        };
        if matches!(scenario, 1 | 6 | 7 | 13) {
            exp.push(("cancel".into(), None));
        }
        if matches!(scenario, 2 | 3 | 4 | 8 | 9) {
            exp.push(("add".into(), None));
        }
        if !fail.is_empty() && fail != "update" {
            let n = exp.iter().position(|(n, _)| n == fail).unwrap();
            exp.truncate(n + 1);
        }
        assert_eq!(
            *trace.borrow(),
            exp,
            "ALGO native map/slot/event/guard/update order"
        );
        let failed = verify(result, !fail.is_empty(), &e);
        input["algo"].as_array_mut().unwrap().push(json!(scenario));
        views["algo"]
            .as_array_mut()
            .unwrap()
            .push(json!({"trace":trace.into_inner(),"error":failed}));
    }
    assert_eq!(
        input["routes"].as_array().unwrap().len(),
        128,
        "ROUTE independent scenario floor"
    );
    assert_eq!(
        input["algo"].as_array().unwrap().len(),
        14,
        "ALGO independent scenario floor"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/message-routing.cjs"
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
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(views, source, "source/wire after all native assertions");
}
