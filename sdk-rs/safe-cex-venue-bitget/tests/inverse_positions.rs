use safe_cex_venue_bitget::{account_positions::*, inverse_positions::*};
// Both modules export fetch_open_positions and the fixture implements both host
// traits, so the two globs leave the name ambiguous (E0659). This suite is the
// INVERSE one - file name, test name and every assertion message say so.
use safe_cex_venue_bitget::inverse_positions::fetch_open_positions;
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
// The native-value harness below is a verbatim copy of account_positions.rs's,
// where every variant and helper is exercised. Keeping the two textually
// identical is the point of the copy, so this suite allows the parts it does
// not reach rather than pruning its half of a shared fixture - V::Hole/V::Null
// are the missing/undefined/null trichotomy the wire decoders turn on.
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum V {
    Hole,
    U,
    Null,
    Bool(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
}
fn s(x: &str) -> V {
    V::S(x.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(fields: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::Hole | V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => n.to_string(),
        V::S(s) => String::from_utf16_lossy(s),
        V::A(_) => "".into(),
        V::O(_) => "[object Object]".into(),
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == &k.encode_utf16().collect::<Vec<_>>())
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
#[allow(dead_code)]
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        _ => panic!("ACCOUNT native numeric field required: {v:?}"),
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Hole | V::U | V::Null => false,
        V::Bool(b) => *b,
        V::N(n) => *n != 0. && !n.is_nan(),
        V::S(t) => !t.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("ACCOUNT native array required"),
    }
}
fn native(actual: &V, expected: &V, path: &str) {
    match (actual, expected) {
        (V::N(x), V::N(y)) => assert!(
            if y.is_nan() {
                x.is_nan()
            } else {
                x.to_bits() == y.to_bits()
            },
            "ACCOUNT native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "ACCOUNT native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "ACCOUNT native independent array count {path}"
            );
            for (i, (x, y)) in x.iter().zip(y.iter()).enumerate() {
                native(x, y, &format!("{path}.{i}"))
            }
        }
        (V::O(x), V::O(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "ACCOUNT native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "ACCOUNT native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "ACCOUNT native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Hole => panic!("internal sparse hole"),
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::Bool(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(t) => json!({"s":t}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k,wire(v)])).collect::<Vec<_>>()})
        }
    }
}
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct E {
    token: &'static str,
    message: V,
}
struct H {
    scene: &'static str,
    response: V,
    markets: V,
    trace: RefCell<Vec<String>>,
    emissions: RefCell<Vec<(V, Option<V>)>>,
    params: RefCell<Vec<V>>,
    logs: RefCell<Vec<V>>,
}
impl H {
    fn err(&self) -> E {
        E {
            token: "original",
            message: s("rejected"),
        }
    }
    fn record(&self, v: impl Into<String>) {
        self.trace.borrow_mut().push(v.into())
    }
}
impl AccountPositionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, name: &str) -> Result<V, E> {
        Ok(match name {
            "xhr" => s("regular"),
            "apiProductType" => s("coin-futures"),
            "store.markets" => self.markets.clone(),
            "POSITION_SIDE" => o(vec![("long", s("long")), ("short", s("short"))]),
            "store.marketsBySymbol" => o(vec![(
                "BTCUSD",
                o(vec![("isInverse", V::Bool(self.scene != "linear"))]),
            )]),
            "store.tickersBySymbol" => o(vec![(
                "BTCUSD",
                o(vec![
                    (
                        "mark",
                        // Per-scene fixture: "last" and "entry" each specify
                        // their own mark independently, and they coincide at 0
                        // only today - the sibling "last" field below is the
                        // same shape with arms that differ.
                        #[allow(clippy::if_same_then_else)]
                        V::N(if self.scene == "last" {
                            0.
                        } else if self.scene == "entry" {
                            0.
                        } else {
                            50000.
                        }),
                    ),
                    (
                        "last",
                        V::N(if self.scene == "entry" { 0. } else { 40000. }),
                    ),
                ]),
            )]),
            _ => panic!("inverse receiver {name}"),
        })
    }
    fn property(&self, v: &V, name: &str) -> Result<V, E> {
        Ok(get(v, name))
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn same_value_zero(&self, a: &V, b: &V) -> bool {
        equal(a, b) || matches!((a,b),(V::N(a),V::N(b)) if a.is_nan()&&b.is_nan())
    }
    fn text(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::S(t) => t.clone(),
            _ => text(v).encode_utf16().collect(),
        })
    }
    fn upper(&self, v: &V) -> Result<V, E> {
        Ok(s(&text(v).to_uppercase()))
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn string(&self, v: &[u16]) -> V {
        V::S(v.to_vec())
    }
    fn number(&self, v: f64) -> V {
        V::N(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::A(_))
    }
    fn length(&self, v: &V) -> Result<usize, E> {
        match v {
            V::A(v) => Ok(v.borrow().len()),
            _ => Err(E {
                token: "TypeError",
                message: s("not an array"),
            }),
        }
    }
    fn has_index(&self, v: &V, i: usize) -> Result<bool, E> {
        Ok(match v {
            V::A(v) => v.borrow().get(i).is_some_and(|v| !matches!(v, V::Hole)),
            _ => false,
        })
    }
    fn index(&self, v: &V, i: usize) -> Result<V, E> {
        match v {
            V::A(v) => Ok(v
                .borrow()
                .get(i)
                .filter(|v| !matches!(v, V::Hole))
                .cloned()
                .unwrap_or(V::U)),
            _ => Err(self.err()),
        }
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn keyed_property(&self, v: &V, k: &V) -> Result<V, E> {
        self.property(v, &text(k))
    }
    fn spread_metadata(&self, target: V, m: V) -> Result<V, E> {
        if let V::O(fields) = m {
            for (k, v) in fields.borrow().iter() {
                self.set_key(&target, &V::S(k.clone()), v.clone())?
            }
        }
        Ok(target)
    }
    fn numeric(&self, v: &V) -> Result<f64, E> {
        Ok(match v {
            V::N(n) => *n,
            _ => text(v).parse().unwrap_or(f64::NAN),
        })
    }
    fn define_key(&self, target: &V, k: &V, v: V) -> Result<(), E> {
        self.set_key(target, k, v)
    }
    fn set_key(&self, target: &V, k: &V, v: V) -> Result<(), E> {
        let V::O(target) = target else { panic!() };
        let mut fields = target.borrow_mut();
        let k = self.text(k)?;
        if let Some((_, old)) = fields.iter_mut().find(|(key, _)| *key == k) {
            *old = v
        } else {
            fields.push((k, v))
        }
        Ok(())
    }
    fn await_value(&self, v: V) -> AccountFuture<'_, V, E> {
        Box::pin(async move { Ok(v) })
    }
    fn call(&self, _: &str, _: Vec<V>) -> Result<AccountFuture<'_, V, E>, E> {
        panic!("inverse sync collaborators must not await")
    }
    fn read(
        &self,
        client: V,
        endpoint: &'static str,
        params: Vec<(&'static str, V)>,
    ) -> Result<AccountFuture<'_, V, E>, E> {
        assert!(equal(&client, &s("regular")));
        assert_eq!(endpoint, "POSITIONS_V2");
        let params = o(params);
        let coin = get(&params, "marginCoin");
        self.params.borrow_mut().push(params);
        let is_btc = equal(&coin, &s("BTC"));
        Ok(Box::pin(async move {
            if is_btc && self.scene == "reject" {
                Err(self.err())
            } else if is_btc && self.scene == "error" {
                Ok(o(vec![("code", s("400")), ("msg", s("bad"))]))
            } else {
                Ok(o(vec![
                    ("code", s("00000")),
                    (
                        "data",
                        if is_btc {
                            self.response.clone()
                        } else {
                            a(vec![])
                        },
                    ),
                ]))
            }
        }))
    }
    fn finite(&self, v: &V, f: f64) -> Result<f64, E> {
        use safe_cex_venue_bitget::numeric::{FiniteNumberInput, to_finite_number};
        Ok(match v {
            V::N(n) => to_finite_number(FiniteNumberInput::Number(*n), f),
            V::S(t) => to_finite_number(FiniteNumberInput::String(&String::from_utf16_lossy(t)), f),
            _ => f,
        })
    }
    fn parse_float(&self, v: &V) -> Result<f64, E> {
        Ok(safe_cex_num::parse_float(&text(v)))
    }
    fn parse_int(&self, v: &V, r: Option<i32>) -> Result<f64, E> {
        Ok(safe_cex_num::parse_int(&text(v), r))
    }
    fn success(&self, c: &V) -> Result<bool, E> {
        Ok(matches!(text(c).as_str(), "0" | "00000"))
    }
    fn promote(&self, _: &V) -> Result<bool, E> {
        panic!("unused")
    }
    fn account_upnl(&self, _: f64) -> Result<f64, E> {
        panic!("unused")
    }
    fn error_message(&self, r: &V, f: &str) -> Result<V, E> {
        Ok(if truth(&get(r, "msg")) {
            get(r, "msg")
        } else {
            s(f)
        })
    }
    fn environment_mismatch(&self, v: &V) -> Result<bool, E> {
        Ok(text(v).to_lowercase().contains("environment is incorrect"))
    }
    fn emit(&self, e: V, d: Option<V>) -> Result<(), E> {
        self.record("emit");
        self.emissions.borrow_mut().push((e, d));
        Ok(())
    }
    fn make_error(&self, message: V) -> E {
        E {
            token: "Error",
            message,
        }
    }
    fn caught_value(&self, e: &E) -> V {
        o(vec![("message", e.message.clone())])
    }
    fn caught_detail(&self, e: &E) -> Result<V, E> {
        Ok(e.message.clone())
    }
    fn map_positions(&self, v: V, uta: bool) -> Result<V, E> {
        if matches!(v, V::Null | V::U) {
            return Err(E {
                token: "TypeError",
                message: s(&format!(
                    "Cannot read properties of {} (reading 'map')",
                    text(&v)
                )),
            });
        }
        self.record(format!("map:{uta}"));
        Ok(a(values(&v)
            .into_iter()
            .map(|v| o(vec![("raw", get(&v, "raw")), ("uta", V::Bool(uta))]))
            .collect()))
    }
    fn position_id(&self, symbol: &V, side: &V, mode: &V) -> Result<V, E> {
        Ok(if text(mode) == "oneway" || !truth(side) {
            symbol.clone()
        } else {
            s(&format!("{}:{}", text(symbol), text(side)))
        })
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture futures are settled"),
    }
}
impl InversePositionsHost for H {
    fn capture_replace(&self, receiver: V) -> Result<V, E> {
        if matches!(receiver, V::S(_)) {
            Ok(receiver)
        } else {
            Err(self.err())
        }
    }
    fn invoke_replace(&self, method: V, needle: V, replacement: V) -> Result<V, E> {
        let V::S(mut value) = method else { panic!() };
        let V::S(needle) = needle else { panic!() };
        let V::S(replacement) = replacement else {
            panic!()
        };
        if let Some(at) = value.windows(needle.len()).position(|v| v == needle) {
            value.splice(at..at + needle.len(), replacement);
        }
        Ok(V::S(value))
    }
    fn sync_call(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        match name {
            "log" => {
                if self.scene == "log-error" {
                    return Err(self.err());
                }
                self.logs.borrow_mut().push(args[0].clone());
                Ok(V::U)
            }
            "resolveLiquidationPrice" => {
                assert!(equal(&args[0], &s("BTCUSD:long")));
                Ok(V::N(21000.))
            }
            "virtualClock.getCurrentTimeMs" => Ok(V::N(1234.)),
            _ => panic!("inverse native call {name}"),
        }
    }
}
fn raw() -> V {
    o(vec![
        ("total", s("1000")),
        ("openPriceAvg", s("20000")),
        ("symbol", s("BTCUSD_COIN-FUTURES")),
        ("posMode", s("hedge_mode")),
        ("marginMode", s("isolated")),
        ("holdSide", s("long")),
        ("unrealizedPL", s("0.125")),
        ("markPrice", s("3")),
        ("marginSize", s("2")),
        ("leverage", s("7")),
        ("achievedProfits", s("0.5")),
        ("totalFee", s("0.1")),
        ("deductedFee", s("0.2")),
        ("liquidationPrice", s("0")),
    ])
}
#[test]
fn inverse_positions_native_then_source() {
    let scenes = [
        "normal",
        "last",
        "entry",
        "linear",
        "error",
        "reject",
        "log-error",
        "empty",
    ];
    let mut cases = vec![];
    let mut captured = vec![];
    for scene in scenes {
        let row = raw();
        let response = a(if scene == "empty" {
            vec![]
        } else {
            vec![row.clone()]
        });
        let h = H {
            scene,
            response: response.clone(),
            markets: a(vec![
                o(vec![("base", s("BTC"))]),
                o(vec![("base", s("ETH"))]),
                o(vec![("base", s("BTC"))]),
            ]),
            trace: RefCell::default(),
            emissions: RefCell::default(),
            params: RefCell::default(),
            logs: RefCell::default(),
        };
        let result = run(fetch_open_positions(&h)).unwrap();
        let rows = values(&result);
        let success = !matches!(scene, "empty" | "error" | "reject" | "log-error");
        assert_eq!(
            rows.len(),
            usize::from(success),
            "INVERSE native independent position count"
        );
        assert_eq!(
            h.params.borrow().len(),
            2,
            "INVERSE native independent unique coin request count"
        );
        for (i, coin) in ["BTC", "ETH"].iter().enumerate() {
            native(
                &h.params.borrow()[i],
                &o(vec![
                    ("productType", s("coin-futures")),
                    ("marginCoin", s(coin)),
                ]),
                "INVERSE native params",
            );
        }
        if success {
            let p = &rows[0];
            assert!(
                equal(&get(p, "info"), &row),
                "INVERSE native raw info identity"
            );
            let contracts = match scene {
                "last" => 0.025,
                "entry" => 0.05,
                "linear" => 1000.,
                _ => 0.02,
            };
            let expected = o(vec![
                ("id", s("BTCUSD:long")),
                ("symbol", s("BTCUSD")),
                ("side", s("long")),
                ("entryPrice", V::N(20000.)),
                (
                    "notional",
                    V::N(if scene == "linear" { 3000. } else { 1000. }),
                ),
                ("leverage", V::N(7.)),
                ("unrealizedPnl", V::N(0.125)),
                ("contracts", V::N(contracts)),
                ("realizedPnl", V::N(0.5 + 0.1 - 0.2)),
                ("liquidationPrice", V::N(21000.)),
                ("marginSize", V::N(2.)),
                ("positionMode", s("hedged")),
                ("marginType", s("isolated")),
                (
                    "extra",
                    o(vec![
                        ("contractsNative", V::N(1000.)),
                        ("unrealizedPnlNative", V::N(0.125)),
                    ]),
                ),
                ("info", row.clone()),
                ("lastUpdate", V::N(1234.)),
            ]);
            native(p, &expected, "INVERSE native decoded position");
        }
        assert_eq!(
            h.emissions.borrow().len(),
            usize::from(matches!(scene, "error" | "reject" | "log-error")),
            "INVERSE native independent error count"
        );
        cases.push(json!({"scene":scene,"row":wire(&row)}));
        captured.push(json!({"result":wire(&result),"params":h.params.borrow().iter().map(wire).collect::<Vec<_>>(),"emissions":h.emissions.borrow().iter().map(|(e,_)|wire(e)).collect::<Vec<_>>(),"logs":h.logs.borrow().iter().map(wire).collect::<Vec<_>>() }));
    }
    assert_eq!(cases.len(), 8, "INVERSE independent input floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-positions-capture.mjs"
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
        .write_all(json!(cases).to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "INVERSE source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json!(captured),
        expected,
        "INVERSE source positions after native assertions"
    );
}
