use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_okx::message_routing::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type Error = Rc<ExchangeError>;
type Trace = Vec<(String, Value)>;
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn e() -> Error {
    let mut x = ExchangeError::new("routing failure");
    x.name = "TypeError".into();
    x.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "ROUTE"
    )));
    x.store_subscriber_ids = vec![7, 3];
    Rc::new(x)
}
#[derive(Clone, Debug, PartialEq)]
enum Arg {
    Text(Vec<u16>),
    Number(i32),
}
fn truthy(a: &Arg) -> bool {
    match a {
        Arg::Text(s) => !s.is_empty(),
        Arg::Number(n) => *n != 0,
    }
}
fn text(a: &Arg) -> Vec<u16> {
    match a {
        Arg::Text(s) => s.clone(),
        Arg::Number(n) => u(&n.to_string()),
    }
}
fn view(a: &Arg) -> Value {
    match a {
        Arg::Text(s) => json!({"string":s}),
        Arg::Number(n) => json!({"number":n}),
    }
}
struct Decoded {
    payload: i32,
    root_truthy: bool,
    root_null: bool,
    event: Option<&'static str>,
    code: Option<i32>,
    msg: bool,
    data: bool,
    channel: Arg,
    id: Arg,
    kind: Arg,
}
fn decoded(s: usize) -> Decoded {
    let (event, code) = match s {
        3 | 4 => (Some("subscribe"), None),
        5 | 6 => (Some("login"), Some(0)),
        7 => (Some("login"), Some(1)),
        8 => (Some("login"), None),
        9 | 10 | 17 => (Some("error"), None),
        _ => (None, if s == 24 { Some(0) } else { None }),
    };
    let channel = match s {
        11 | 12 | 16 | 17 | 18 => Arg::Text(u("orders")),
        13 => Arg::Text(u("positions")),
        14 => Arg::Text(u("account")),
        19 => Arg::Text(u("candle1m")),
        20 => Arg::Number(0),
        21 | 22 => Arg::Text(u("__proto__")),
        23 => Arg::Text(u("custom-order")),
        25 => Arg::Number(5),
        _ => Arg::Text(vec![]),
    };
    let id = if s == 19 {
        Arg::Text(vec![0xd800, 88])
    } else if matches!(s, 18 | 20 | 22 | 25) {
        Arg::Text(u("X"))
    } else {
        Arg::Text(vec![])
    };
    let kind = Arg::Text(if matches!(s, 18..=22 | 25) {
        u("SWAP")
    } else {
        vec![]
    });
    Decoded {
        payload: 17,
        root_truthy: !matches!(s, 2 | 15 | 24),
        root_null: matches!(s, 2 | 15),
        event,
        code,
        msg: matches!(s, 9 | 17),
        data: matches!(s, 11 | 13 | 16..=25),
        channel,
        id,
        kind,
    }
}
fn raw(s: usize) -> String {
    match s{
 0|1=>"pong".into(),2=>"invalid".into(),3=>r#"{"event":"subscribe","payload":17}"#.into(),4=>r#"{"event": "subscribe","payload":17}"#.into(),
 5=>r#"{"event":"login","code":0,"payload":17}"#.into(),6=>r#"{"event":"login","code":"0","payload":17}"#.into(),7=>r#"{"event":"login","code":1,"payload":17}"#.into(),8=>r#"{"event":"login","payload":17}"#.into(),
 9=>r#"{"event":"error","msg":"bad","payload":17}"#.into(),10=>r#"{"event":"error","msg":false,"payload":17}"#.into(),11=>r#"{"arg":{"channel":"orders"},"data":[],"payload":17}"#.into(),12=>r#"{"arg":{"channel":"orders"},"data":0,"payload":17}"#.into(),13=>r#"{"arg":{"channel":"positions"},"data":{},"payload":17}"#.into(),14=>r#"{"arg":{"channel":"account"},"payload":17}"#.into(),15=>r#"{"event":"login""#.into(),16=>r#"{"note":"login","arg":{"channel":"orders"},"data":[],"payload":17}"#.into(),17=>r#"{"event":"error","arg":{"channel":"orders"},"msg":"bad","data":[],"payload":17}"#.into(),
 18=>r#"{"arg":{"channel":"orders","instId":"X","instType":"SWAP"},"data":[],"payload":17}"#.into(),19=>r#"{"arg":{"channel":"candle1m","instId":"\ud800X","instType":"SWAP"},"data":[],"payload":17}"#.into(),20=>r#"{"arg":{"channel":0,"instId":"X","instType":"SWAP"},"data":[],"payload":17}"#.into(),21=>r#"{"arg":{"channel":"__proto__","instId":"","instType":"SWAP"},"payload":17}"#.into(),22=>r#"{"arg":{"channel":"__proto__","instId":"X","instType":"SWAP"},"payload":17}"#.into(),23=>r#"{"arg":{"channel":"custom-order"},"data":[],"payload":17}"#.into(),24=>"false".into(),_=>r#"{"arg":{"channel":"orders","instId":"X","instType":"SWAP"},"data":[],"payload":17}"#.into()}
}
fn expected(kind: &str, s: usize) -> Trace {
    let mut t: Trace = ["track", "fallback", "diagnostic"]
        .map(|x| (x.into(), Value::Null))
        .to_vec();
    if matches!(s, 0 | 1) {
        t.push(("pong".into(), Value::Null));
        return t;
    }
    if kind == "private" {
        if matches!(s, 2 | 3 | 4 | 19..=23) {
            return t;
        }
        t.push(("parse".into(), Value::Null));
        if matches!(s, 5..=8 | 16 | 24) {
            t.push(("code".into(), Value::Null));
        }
        if matches!(s, 5 | 6 | 24) {
            t.push(("subscribe".into(), Value::Null));
        }
        if matches!(s, 9 | 17) {
            t.push(("emit-error".into(), Value::Null));
        }
        if matches!(s, 11 | 16 | 18 | 25) {
            if s != 16 {
                t.push(("order".into(), Value::Null));
            }
        }
        if s == 13 {
            t.push(("position".into(), Value::Null));
        }
        if s == 14 {
            t.push(("account".into(), Value::Null));
        }
        return t;
    }
    t.push(("parse".into(), Value::Null));
    if matches!(s, 2 | 3 | 4 | 15 | 24) {
        return t;
    }
    if matches!(s, 5..=8) {
        t.push(("code".into(), Value::Null));
        if matches!(s, 5 | 6) {
            t.push(("subscribe".into(), Value::Null));
        }
        return t;
    }
    if matches!(s, 9 | 10 | 20) {
        return t;
    }
    t.push(("membership".into(), Value::Null));
    if matches!(s, 11 | 12 | 16 | 17 | 18 | 23) {
        t.push(("stats".into(), json!(u("orders"))));
        t.push(("order".into(), Value::Null));
        return t;
    }
    t.push(("timeframe".into(), Value::Null));
    if s == 19 {
        let id = vec![0xd800, 88];
        t.push((
            "dispatch".into(),
            json!([u("candle.1m."), id.clone()].concat()),
        ));
        t.push((
            "dispatch".into(),
            json!([u("candle1m.SWAP."), id.clone()].concat()),
        ));
        t.push(("dispatch".into(), json!([u("candle1m."), id].concat())));
    }
    if s == 22 {
        t.push(("format".into(), Value::Null));
        for k in [
            "candle.[object Object].X",
            "__proto__.SWAP.X",
            "__proto__.X",
        ] {
            t.push(("dispatch".into(), json!(u(k))));
        }
    }
    if s == 25 {
        for k in ["5.SWAP.X", "5.X"] {
            t.push(("dispatch".into(), json!(u(k))));
        }
    }
    let d = decoded(s);
    t.push(("dispatch-value".into(), view(&d.channel)));
    t
}
#[test]
fn routing_native_before_source() {
    let mut input = json!({"routes":[],"dispatch":[]});
    let mut output = json!({"routes":[],"dispatch":[]});
    for kind in ["private", "business"] {
        let mut cases = (0..26).map(|s| (s, "", false, false)).collect::<Vec<_>>();
        cases.extend([
            (5, "parse", false, false),
            (5, "code", false, false),
            (5, "subscribe", false, false),
            (9, "msg", false, false),
            (9, "emit-error", false, false),
            (11, "order", false, false),
            (13, "position", false, false),
            (11, "track", false, false),
            (18, "", true, false),
            (18, "", false, true),
        ]);
        for (s, fail, disposed_at_entry, flip) in cases {
            let j = Rc::new(decoded(s));
            let cause = e();
            let disposed = Cell::new(disposed_at_entry);
            let trace = RefCell::new(Trace::new());
            let arg_reads = Cell::new(0);
            let failure_reached = Cell::new(false);
            let raw = raw(s);
            let record = |name: &str, key: Value| -> Result<(), Error> {
                trace.borrow_mut().push((name.into(), key));
                if name == fail {
                    failure_reached.set(true);
                    Err(cause.clone())
                } else {
                    Ok(())
                }
            };
            let check = |p: &Rc<Decoded>| {
                assert!(Rc::ptr_eq(p, &j), "MESSAGE native decoded root identity");
                assert_eq!(p.payload, 17, "MESSAGE native decoded payload");
            };
            let effect =
                |name: &str, p: Option<&Rc<Decoded>>, key: Option<&[u16]>, value: Option<&Arg>| {
                    if let Some(p) = p {
                        check(p);
                    }
                    if let Some(v) = value {
                        assert_eq!(v, &j.channel, "BUSINESS native original uncoerced channel");
                        if s == 25 {
                            assert_eq!(
                                v,
                                &Arg::Number(5),
                                "BUSINESS native changed second-read type"
                            );
                        }
                    }
                    if flip && name == "track" {
                        disposed.set(true);
                    }
                    record(
                        name,
                        key.map_or_else(|| value.map_or(Value::Null, view), |k| json!(k)),
                    )
                };
            let parse = || {
                record("parse", Value::Null)?;
                Ok(j.clone())
            };
            let private_pred = |p: &Rc<Decoded>, op: &str| -> Result<bool, Error> {
                check(p);
                match op {
                    "optional-code-zero" => {
                        if !p.root_null {
                            record("code", Value::Null)?;
                        }
                        Ok(p.code == Some(0))
                    }
                    "message-truthy" => {
                        if fail == "msg" {
                            failure_reached.set(true);
                            return Err(cause.clone());
                        }
                        Ok(p.root_truthy && p.msg)
                    }
                    "data-truthy" => Ok(p.root_truthy && p.data),
                    _ => Ok(p.root_truthy),
                }
            };
            let result = if kind == "private" {
                private_message(
                    &disposed,
                    matches!(s, 0 | 1),
                    |n, p| effect(n, p, None, None),
                    |needle| {
                        Ok(if s == 24 {
                            needle == "login"
                        } else {
                            raw.contains(needle)
                        })
                    },
                    parse,
                    private_pred,
                )
            } else {
                business_message(
                    &disposed,
                    matches!(s, 0 | 1),
                    effect,
                    || {
                        let p = parse()?;
                        Ok(if p.root_truthy { Some(p) } else { None })
                    },
                    |p, op| {
                        check(p);
                        match op {
                            "event:subscribe" => Ok(p.event == Some("subscribe")),
                            "event:login" => Ok(p.event == Some("login")),
                            _ => {
                                record("code", Value::Null)?;
                                Ok(p.code == Some(0))
                            }
                        }
                    },
                    |p, field| {
                        check(p);
                        arg_reads.set(arg_reads.get() + 1);
                        let v = match field {
                            "channel" => &p.channel,
                            "instId" => &p.id,
                            _ => &p.kind,
                        };
                        let first_string = if field == "channel" && s == 25 {
                            true
                        } else {
                            matches!(v, Arg::Text(_))
                        };
                        let present = match field {
                            "channel" => matches!(s,11..=14|16..=23|25),
                            "instId" => matches!(s, 18..=22 | 25),
                            _ => matches!(s, 18..=22 | 25),
                        };
                        if first_string && present {
                            arg_reads.set(arg_reads.get() + 1);
                            Ok(v.clone())
                        } else {
                            Ok(Arg::Text(vec![]))
                        }
                    },
                    truthy,
                    |v| {
                        record("membership", Value::Null)?;
                        Ok(
                            matches!(v,Arg::Text(k)if ["orders","orders-algo","algo-advance"].iter().any(|s|*k==u(s))||s==23&&*k==u("custom-order")),
                        )
                    },
                    |v| {
                        record("timeframe", Value::Null)?;
                        let k = text(v);
                        Ok(if k == u("candle1m") {
                            Some(false)
                        } else if k == u("__proto__") {
                            Some(true)
                        } else {
                            None
                        })
                    },
                    |v| Ok(text(v)),
                    |object| {
                        if object {
                            record("format", Value::Null)?;
                            if s == 21 {
                                return Err(cause.clone());
                            }
                            Ok(u("[object Object]"))
                        } else {
                            Ok(u("1m"))
                        }
                    },
                )
            };
            let mut exp = if disposed_at_entry {
                vec![]
            } else {
                expected(kind, s)
            };
            if !disposed_at_entry && !fail.is_empty() {
                let stop = exp.iter().position(|(n, _)| n == fail);
                if let Some(stop) = stop {
                    exp.truncate(stop + 1);
                } else if fail == "msg" && kind == "private" {
                    exp.truncate(4);
                }
            }
            assert_eq!(
                *trace.borrow(),
                exp,
                "MESSAGE native precedence/key/effect order {kind} scenario={s} fail={fail}"
            );
            let should_fail = !disposed_at_entry
                && match fail {
                    "parse" | "code" | "subscribe" | "order" | "track" => true,
                    "msg" | "emit-error" | "position" => kind == "private",
                    _ => false,
                };
            assert_eq!(
                failure_reached.get(),
                should_fail,
                "MESSAGE native independent failure reachability"
            );
            let swallowed = kind == "private"
                && matches!(s, 5 | 9)
                && !disposed_at_entry
                && ["parse", "code", "subscribe", "msg", "emit-error"].contains(&fail);
            if swallowed {
                assert!(
                    result.is_ok(),
                    "PRIVATE native swallowed login/error failure"
                );
                assert!(
                    failure_reached.get(),
                    "PRIVATE native catch witnessed a real failure"
                );
            } else if should_fail {
                let err = result.unwrap_err();
                assert_eq!(*err, *cause, "MESSAGE native error fields");
                assert!(Rc::ptr_eq(&err, &cause), "MESSAGE native error identity");
            } else {
                result.expect("MESSAGE native successful route");
            }
            if kind == "business" && !disposed_at_entry && s == 20 {
                assert_eq!(arg_reads.get(), 5, "BUSINESS native typeof/read floor");
            }
            if kind == "business" && !disposed_at_entry && s == 25 {
                assert_eq!(arg_reads.get(), 6, "BUSINESS native second reads stay live");
            }
            input["routes"].as_array_mut().unwrap().push(json!({"kind":kind,"scenario":s,"raw":raw,"fail":fail,"disposed":disposed_at_entry,"flip":flip}));
            output["routes"].as_array_mut().unwrap().push(json!({"trace":trace.borrow().clone(),"error":should_fail&&!swallowed,"argReads":arg_reads.get()}));
        }
    }
    for scenario in 0..12 {
        let message = Rc::new(17);
        let callbacks = Rc::new(RefCell::new(vec![1, 2]));
        let replacement = Rc::new(RefCell::new(vec![9]));
        let table = RefCell::new(callbacks.clone());
        let cause = e();
        let trace = RefCell::new(Trace::new());
        let key = if scenario == 11 {
            Arg::Number(5)
        } else {
            Arg::Text(vec![0xd800, 75])
        };
        let fail = match scenario {
            3 => "lookup",
            4 => "length",
            5 => "stats",
            6 => "for-of",
            7 => "cb:1",
            _ => "",
        };
        let call = |name: &str| {
            trace.borrow_mut().push((name.into(), Value::Null));
            if name == fail {
                Err(cause.clone())
            } else {
                Ok(())
            }
        };
        let result: Result<(), Error> = dispatch_message_handler(
            &key,
            &message,
            |h| {
                assert_eq!(h, &key, "DISPATCH native original handler value");
                call("lookup")?;
                Ok(if scenario == 0 {
                    None
                } else {
                    Some(table.borrow().clone())
                })
            },
            |c| {
                assert!(
                    Rc::ptr_eq(c, &callbacks),
                    "DISPATCH native captured length receiver"
                );
                call("length")?;
                Ok(scenario != 1)
            },
            |h| {
                assert_eq!(h, &key, "DISPATCH native stats handler value");
                call("stats")?;
                if scenario == 8 {
                    *table.borrow_mut() = replacement.clone();
                }
                Ok(())
            },
            |c, j| {
                assert!(
                    Rc::ptr_eq(&c, &callbacks),
                    "DISPATCH native retained original callback container"
                );
                assert!(
                    Rc::ptr_eq(j, &message),
                    "DISPATCH native decoded message identity"
                );
                assert_eq!(**j, 17, "DISPATCH native decoded payload");
                call("for-of")?;
                let mut at = 0;
                while at < c.borrow().len() {
                    let id = c.borrow()[at];
                    call(&format!("cb:{id}"))?;
                    if scenario == 9 && at == 0 {
                        c.borrow_mut().push(3);
                    }
                    if scenario == 10 && at == 0 {
                        c.borrow_mut().truncate(1);
                    }
                    at += 1;
                }
                Ok(())
            },
        );
        let mut exp: Trace = ["lookup", "length", "stats", "for-of", "cb:1", "cb:2"]
            .map(|s| (s.into(), Value::Null))
            .to_vec();
        if scenario == 0 {
            exp.truncate(1);
        }
        if scenario == 1 {
            exp.truncate(2);
        }
        if scenario == 9 {
            exp.push(("cb:3".into(), Value::Null));
        }
        if scenario == 10 {
            exp.truncate(5);
        }
        if !fail.is_empty() {
            let stop = exp.iter().position(|(n, _)| n == fail).unwrap();
            exp.truncate(stop + 1);
        }
        assert_eq!(
            *trace.borrow(),
            exp,
            "DISPATCH native capture/guard/stats/live fanout order"
        );
        if !fail.is_empty() {
            let err = result.unwrap_err();
            assert_eq!(*err, *cause, "DISPATCH native error fields");
            assert!(Rc::ptr_eq(&err, &cause), "DISPATCH native error identity");
        } else {
            result.unwrap();
        }
        input["dispatch"]
            .as_array_mut()
            .unwrap()
            .push(json!(scenario));
        output["dispatch"]
            .as_array_mut()
            .unwrap()
            .push(json!({"trace":trace.into_inner(),"error":!fail.is_empty()}));
    }
    assert_eq!(
        input["routes"].as_array().unwrap().len(),
        72,
        "MESSAGE independent floor"
    );
    assert_eq!(
        input["dispatch"].as_array().unwrap().len(),
        12,
        "DISPATCH independent floor"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/message-routing-oracle.mjs"
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
    assert_eq!(output, source, "source/wire after all native assertions");
}
