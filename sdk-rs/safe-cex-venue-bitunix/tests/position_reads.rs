#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
use safe_cex_num::{JsCoercionError, JsJsonValue, numeric_ingress::js_deserialize_passthrough};
use safe_cex_types::{MarginType, Position, PositionMode};
use safe_cex_venue_bitunix::position_reads::{fetch_orders, fetch_positions};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn resolve<T>(f: impl Future<Output = T>) -> T {
    let mut f = pin!(f);
    for _ in 0..3 {
        if let Poll::Ready(x) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return x;
        }
    }
    panic!("POSITION scoped fixture stalled")
}
fn num(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn obs(v: JsJsonValue) -> Value {
    match v {
        JsJsonValue::Number(n) => num(n),
        JsJsonValue::Undefined => json!({"undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(b) => json!(b),
        JsJsonValue::String(s) => json!(s),
        JsJsonValue::Array(a) => json!(a.into_iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, obs(v))).collect()),
        _ => panic!(),
    }
}
fn position(p: Position) -> Value {
    let mut v = json!({"id":p.id,"symbol":p.symbol,"side":p.side,"entryPrice":num(p.entry_price),"notional":num(p.notional),"leverage":num(p.leverage),"unrealizedPnl":num(p.unrealized_pnl),"contracts":num(p.contracts),"liquidationPrice":num(p.liquidation_price)});
    if let Some(n) = p.realized_pnl.into_value() {
        v["realizedPnl"] = num(n);
    }
    if let Some(n) = p.margin_type.into_value() {
        v["marginType"] = json!(n);
    }
    if let Some(n) = p.position_mode.into_value() {
        v["positionMode"] = json!(n);
    }
    if let Some(n) = p.info.into_value() {
        v["info"] = obs(retained_model_values::prepared(&n));
    }
    assert!(
        p.margin_size.is_none() && p.extra.is_none() && p.last_update.is_none(),
        "POSITION native absent fields"
    );
    v
}
#[derive(Debug)]
struct Error(String);
impl From<JsCoercionError> for Error {
    fn from(e: JsCoercionError) -> Self {
        Self(e.message.into())
    }
}
#[test]
fn actual_typescript_position_reads() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/position-reads-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "POSITION actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 37, "POSITION independent source floor");
    let mut counts = [0; 2];
    for (i, row) in rows.iter().enumerate() {
        let f: Value = serde_json::from_str(row["fixture"].as_str().unwrap()).unwrap();
        let events = RefCell::new(Vec::<Value>::new());
        let note = |v: Value| {
            let j = events.borrow().len();
            assert_eq!(
                v, row["events"][j],
                "POSITION native effect {}:{j} case {i}",
                row["key"]
            );
            events.borrow_mut().push(v);
        };
        let actual = if row["key"] == "positions" {
            counts[0] += 1;
            let cached = Position {
                id: "cached".into(),
                symbol: "CACHED".into(),
                side: (safe_cex_types::PositionSide::Long).into(),
                entry_price: 0.,
                notional: 0.,
                leverage: 10.,
                unrealized_pnl: 0.,
                contracts: 0.,
                liquidation_price: 0.,
                margin_type: (None).into(),
                position_mode: (None).into(),
                realized_pnl: (None).into(),
                margin_size: (None).into(),
                info: (None).into(),
                extra: (None).into(),
                last_update: (None).into(),
                option: (None).into(),
            };
            let body = js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
                &serde_json::to_string(&f["body"]).unwrap(),
            ))
            .unwrap();
            let result = resolve(fetch_positions(
                |endpoint, config| {
                    note(obs(JsJsonValue::from(json!(["read", endpoint, config]))));
                    ready(if f["fail"] == true {
                        Err(Error("read failed".into()))
                    } else {
                        Ok(Some(body))
                    })
                },
                |symbol| {
                    let settings = &f["settings"][symbol];
                    note(obs(JsJsonValue::from(json!([
                        "settings", symbol, settings
                    ]))));
                    (
                        settings["marginType"].as_str().map(|s| {
                            if s == "isolated" {
                                MarginType::Isolated
                            } else {
                                MarginType::Cross
                            }
                        }),
                        settings["leverage"].as_f64(),
                    )
                },
                || {
                    note(json!(["mode"]));
                    if f["mode"] == "oneway" {
                        PositionMode::OneWay
                    } else {
                        PositionMode::Hedged
                    }
                },
                || {
                    note(json!(["markets"]));
                    f.get("markets")
                        .cloned()
                        .unwrap_or(json!([{"symbol":"BTCUSDT"},{"symbol":"ETHUSDT"}]))
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v["symbol"].as_str().unwrap().into())
                        .collect()
                },
                || vec![cached],
                |e: &Error| note(json!(["report", e.0])),
            ));
            json!(result.into_iter().map(position).collect::<Vec<_>>())
        } else {
            counts[1] += 1;
            let regular = Rc::new(json!({"id":"R"}));
            let tpsl = Rc::new(json!({"id":"T"}));
            let released = Cell::new(false);
            let polls = Cell::new(0);
            let (result, regular_error, tpsl_error) = resolve(fetch_orders(
                || {
                    note(json!(["regular"]));
                    poll_fn(|_| {
                        polls.set(polls.get() + 1);
                        if f["deferred"] == true && !released.get() {
                            Poll::Pending
                        } else {
                            Poll::Ready(if f["regularFail"] == true {
                                Err("regular")
                            } else {
                                Ok(vec![regular.clone()])
                            })
                        }
                    })
                },
                || {
                    note(json!(["tpsl"]));
                    poll_fn(|cx| {
                        released.set(true);
                        cx.waker().wake_by_ref();
                        Poll::Ready(if f["tpslFail"] == true {
                            Err("tpsl")
                        } else {
                            Ok(vec![tpsl.clone()])
                        })
                    })
                },
            ));
            assert_eq!(
                regular_error,
                (f["regularFail"] == true).then_some("regular"),
                "POSITION native regular rejection retained"
            );
            assert_eq!(
                tpsl_error,
                (f["tpslFail"] == true).then_some("tpsl"),
                "POSITION native TP/SL rejection retained"
            );
            if f["deferred"] == true {
                assert_eq!(
                    polls.get(),
                    2,
                    "POSITION native all-settled waits for both families"
                );
            }
            assert!(
                result
                    .iter()
                    .all(|r| Rc::ptr_eq(r, &regular) || Rc::ptr_eq(r, &tpsl)),
                "POSITION native order identity"
            );
            assert_eq!(row["identity"], true, "POSITION source order identity");
            json!(result.into_iter().map(|r| (*r).clone()).collect::<Vec<_>>())
        };
        assert_eq!(
            actual, row["result"],
            "POSITION decoded native {} case {i}",
            row["key"]
        );
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "POSITION independent effect count {i}"
        );
    }
    assert_eq!(counts, [32, 5], "POSITION independent family count");
}
