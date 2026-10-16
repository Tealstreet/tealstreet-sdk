// Field tables written in the loop shape their siblings use, so adding a second
// field is one row rather than a restructure. One row today is not a reason to
// unroll the table.
#![allow(clippy::single_element_loop)]
use safe_cex_venue_bitget::account_positions::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
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
struct E {
    token: &'static str,
    message: V,
}
struct H {
    op: &'static str,
    scene: &'static str,
    response: V,
    open: V,
    meta: V,
    stored: V,
    trace: RefCell<Vec<String>>,
    emissions: RefCell<Vec<(V, Option<V>)>>,
    params: RefCell<Vec<V>>,
    extend: RefCell<Option<V>>,
    testnet_reads: Cell<usize>,
    data_reads: Cell<usize>,
    list_reads: Cell<usize>,
    alternate: V,
}
const OPS: [&str; 8] = [
    "balance",
    "uta-balance",
    "metadata",
    "uta-metadata",
    "positions",
    "open",
    "uta-open",
    "inverse-balance",
];
const SCENES: [&str; 8] = [
    "normal", "empty", "error", "reject", "route", "promote", "plain", "fast",
];
fn input(op: &str, scene: &str) -> V {
    if scene == "error" || scene == "promote" {
        return o(vec![
            ("code", s(if scene == "promote" { "40085" } else { "35" })),
            ("msg", s("bad")),
            ("data", V::Null),
        ]);
    }
    if scene == "empty" {
        return o(vec![
            ("code", s("00000")),
            (
                "data",
                if op == "balance" || op == "inverse-balance" {
                    a(vec![])
                } else {
                    V::Null
                },
            ),
        ]);
    }
    let data = match op {
        "balance" => a(vec![
            o(vec![("marginCoin", s("BTC"))]),
            o(vec![
                ("marginCoin", s("USDT")),
                ("unionTotalMargin", s("100")),
                ("accountEquity", s("91")),
                ("unionAvailable", s("60")),
                ("available", s("55")),
                ("unrealizedPL", s("3")),
                ("crossedUnrealizedPL", s("1")),
                ("isolatedUnrealizedPL", s("2")),
            ]),
        ]),
        "uta-balance" => o(vec![
            ("accountEquity", s("bad")),
            ("usdtEquity", s("")),
            ("effEquity", s("")),
            ("unrealisedPnl", s("4")),
            ("usdtUnrealisedPnl", s("99")),
            (
                "assets",
                a(vec![o(vec![
                    ("coin", s("USDT")),
                    ("usdValue", s("200")),
                    ("equity", s("190")),
                    ("available", s("80")),
                ])]),
            ),
        ]),
        "metadata" => o(vec![
            (
                "posMode",
                s(if scene == "plain" {
                    "one_way_mode"
                } else {
                    "hedge_mode"
                }),
            ),
            (
                "marginMode",
                s(if scene == "plain" {
                    "crossed"
                } else {
                    "isolated"
                }),
            ),
            ("isolatedLongLever", s("7x")),
            ("isolatedShortLever", s("9")),
            ("crossedMarginLeverage", s("11")),
        ]),
        "uta-metadata" => o(vec![
            (
                "holdMode",
                s(if scene == "plain" {
                    "one_way_mode"
                } else {
                    "hedge_mode"
                }),
            ),
            (
                "symbolConfigList",
                a(vec![
                    o(vec![
                        ("symbol", s("BTCID")),
                        ("category", s("wrong")),
                        ("longLeverage", s("99")),
                    ]),
                    o(vec![
                        ("symbol", s("BTCID")),
                        ("category", s("LINEAR")),
                        ("marginMode", s("isolated")),
                        ("longLeverage", s("3")),
                        ("shortLeverage", s("4")),
                        ("leverage", s("8")),
                    ]),
                ]),
            ),
        ]),
        "open" => a(vec![o(vec![("raw", V::N(1.))]), o(vec![("raw", V::N(2.))])]),
        "uta-open" => o(vec![(
            "list",
            a(vec![o(vec![("raw", V::N(1.))]), o(vec![("raw", V::N(2.))])]),
        )]),
        "inverse-balance" => a(vec![
            o(vec![
                ("marginCoin", s("BTC")),
                ("unionTotalMargin", s("2")),
                ("unionAvailable", s("1")),
                ("unrealizedPL", s("0.1")),
                ("usdtEquity", s("0")),
            ]),
            o(vec![
                ("marginCoin", s("ETH")),
                ("accountEquity", s("4")),
                ("available", s("2")),
                ("unrealizedPL", s("0.25")),
                ("usdtEquity", s("10000")),
            ]),
        ]),
        _ => V::Null,
    };
    o(vec![("code", s("00000")), ("data", data)])
}
fn position(id: &str, symbol: &str, side: &str, mode: &str) -> V {
    o(vec![
        ("id", s(id)),
        ("symbol", s(symbol)),
        ("side", s(side)),
        ("positionMode", s(mode)),
    ])
}
fn meta(symbol: &str, side: &str, leverage: f64, mode: &str, margin: &str) -> V {
    o(vec![
        ("symbol", s(symbol)),
        ("side", s(side)),
        ("leverage", V::N(leverage)),
        ("positionMode", s(mode)),
        ("marginType", s(margin)),
    ])
}
impl H {
    fn new(op: &'static str, scene: &'static str) -> Self {
        let p = position("B", "BTC", "long", "hedged");
        let open = a(vec![p.clone(), position("S", "SOL", "long", "oneway"), p]);
        let meta = a(vec![
            meta("BTC", "long", 3., "hedged", "cross"),
            meta("BTC", "short", 4., "hedged", "isolated"),
            meta("ETH", "long", 5., "oneway", "cross"),
            meta("BTC", "short", 6., "hedged", "cross"),
        ]);
        Self {
            op,
            scene,
            response: input(op, scene),
            open,
            meta,
            stored: o(vec![("cached", V::Bool(true))]),
            trace: RefCell::new(vec![]),
            emissions: RefCell::new(vec![]),
            params: RefCell::new(vec![]),
            extend: RefCell::new(None),
            testnet_reads: Cell::new(0),
            data_reads: Cell::new(0),
            list_reads: Cell::new(0),
            alternate: if op == "balance" {
                a(vec![o(vec![
                    ("marginCoin", s("USDT")),
                    ("accountEquity", s("250")),
                    ("available", s("100")),
                    ("unrealizedPL", s("10")),
                ])])
            } else {
                o(vec![("list", a(vec![o(vec![("raw", V::N(9.))])]))])
            },
        }
    }
    fn record(&self, s: impl Into<String>) {
        self.trace.borrow_mut().push(s.into())
    }
    fn err(&self) -> E {
        E {
            token: "original",
            message: s("rejected"),
        }
    }
}
impl AccountPositionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, name: &str) -> Result<V, E> {
        Ok(match name {
            "xhr" => s("regular"),
            "unlimitedXHR" => s("fast"),
            "usesUtaRouting" => V::Bool(self.scene == "route"),
            "apiProductType" => s("USDT-FUTURES"),
            "apiMarginCoin" => s("USDT"),
            "apiUtaCategory" => s("linear"),
            "isDisposed" => V::Bool(false),
            "options.testnet" => {
                self.record("testnet");
                let read = self.testnet_reads.get();
                self.testnet_reads.set(read + 1);
                if self.scene == "second-testnet-failure" && read == 1 {
                    return Err(self.err());
                }
                V::Bool(self.scene == "toggle-testnet" && read == 1)
            }
            "store.balance" => self.stored.clone(),
            "store.marketsBySymbol" => o(vec![("BTC", o(vec![("id", s("BTCID"))]))]),
            "store.tickers" => a(vec![o(vec![
                ("symbol", s("BTCUSD")),
                ("mark", V::N(30000.)),
            ])]),
            _ => panic!("unknown receiver {name}"),
        })
    }
    fn property(&self, v: &V, name: &str) -> Result<V, E> {
        if name == "data"
            && equal(v, &self.response)
            && matches!(
                self.scene,
                "classic-data-toggle" | "classic-data-failure" | "uta-data-toggle"
            )
        {
            self.record("response-data");
            let read = self.data_reads.get();
            self.data_reads.set(read + 1);
            if read == 1 {
                if self.scene == "classic-data-failure" {
                    return Err(self.err());
                }
                return Ok(self.alternate.clone());
            }
        }
        if name == "list"
            && equal(v, &get(&self.response, "data"))
            && matches!(self.scene, "uta-list-toggle" | "uta-list-failure")
        {
            self.record("response-list");
            let read = self.list_reads.get();
            self.list_reads.set(read + 1);
            if read == 1 {
                if self.scene == "uta-list-failure" {
                    return Err(self.err());
                }
                return Ok(get(&self.alternate, "list"));
            }
        }
        if matches!(v, V::U | V::Null) {
            Err(E {
                token: "TypeError",
                message: s(&format!(
                    "Cannot read properties of {} (reading '{name}')",
                    text(v)
                )),
            })
        } else {
            Ok(get(v, name))
        }
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
    fn call(&self, name: &str, args: Vec<V>) -> Result<AccountFuture<'_, V, E>, E> {
        self.record(format!("call:{name}"));
        if self.scene == "reject" && name == "fetchOpenPositions" {
            return Ok(Box::pin(async { Err(self.err()) }));
        }
        let v = match name {
            "ensureAccountTypeDetected" => V::U,
            "fetchUtaBalance" | "fetchUtaOpenPositions" | "fetchUtaPositionMetadata" => {
                o(vec![("delegated", s(name))])
            }
            "fetchOpenPositions" => {
                if self.scene == "empty" {
                    a(vec![])
                } else {
                    self.open.clone()
                }
            }
            "fetchPositionMetadata" => {
                self.params.borrow_mut().push(args[0].clone());
                self.meta.clone()
            }
            _ => panic!("call {name}"),
        };
        Ok(Box::pin(async move { Ok(v) }))
    }
    fn read(
        &self,
        client: V,
        endpoint: &'static str,
        params: Vec<(&'static str, V)>,
    ) -> Result<AccountFuture<'_, V, E>, E> {
        self.record(format!("read:{}:{endpoint}", text(&client)));
        self.params.borrow_mut().push(o(params));
        Ok(Box::pin(async {
            if let Some(V::A(array)) = self.extend.borrow_mut().take() {
                array.borrow_mut().push(s("ETH"));
            }
            if self.scene == "reject" {
                Err(self.err())
            } else {
                Ok(self.response.clone())
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
        self.record("promote");
        Ok(self.scene == "promote")
    }
    fn account_upnl(&self, f: f64) -> Result<f64, E> {
        self.record("upnl");
        Ok(f + 2.)
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
fn execute(h: &H) -> Result<V, E> {
    let syms = a(vec![s("BTC"), s("ETH")]);
    run(async {
        match h.op {
            "balance" => body::fetch_balance(h, h.scene == "fast").await,
            "uta-balance" => body::fetch_uta_balance(h, s("regular")).await,
            "metadata" => body::fetch_position_metadata(h, syms).await,
            "uta-metadata" => body::fetch_uta_position_metadata(h, syms).await,
            "positions" => {
                body::fetch_positions(
                    h,
                    if h.scene == "route" {
                        None
                    } else {
                        Some(a(vec![s("BTC"), s("ETH"), s("BTC")]))
                    },
                )
                .await
            }
            "open" => body::fetch_open_positions(h).await,
            "uta-open" => body::fetch_uta_open_positions(h).await,
            "inverse-balance" => body::fetch_inverse_balance(h).await,
            _ => panic!(),
        }
    })
}
fn fake(m: V, id: &str) -> V {
    let mut f = vec![("id", s(id))];
    let V::O(fields) = m else { panic!() };
    for (k, v) in fields.borrow().iter() {
        let k = String::from_utf16(k).unwrap(); // only known fixture keys
        match k.as_str() {
            "symbol" => f.push(("symbol", v.clone())),
            "side" => f.push(("side", v.clone())),
            "leverage" => f.push(("leverage", v.clone())),
            "positionMode" => f.push(("positionMode", v.clone())),
            "marginType" => f.push(("marginType", v.clone())),
            _ => panic!(),
        }
    }
    for key in [
        "entryPrice",
        "notional",
        "unrealizedPnl",
        "realizedPnl",
        "contracts",
        "liquidationPrice",
    ] {
        f.push((key, V::N(0.)))
    }
    o(f)
}
fn expected(h: &H) -> Result<V, (&'static str, &'static str)> {
    let (op, sc) = (h.op, h.scene);
    if sc == "reject" {
        return if matches!(op, "metadata" | "uta-metadata") {
            Ok(a(vec![]))
        } else {
            Err(("original", "rejected"))
        };
    }
    if sc == "route" && matches!(op, "balance" | "metadata" | "open") {
        return Ok(o(vec![(
            "delegated",
            s(match op {
                "balance" => "fetchUtaBalance",
                "metadata" => "fetchUtaPositionMetadata",
                _ => "fetchUtaOpenPositions",
            }),
        )]));
    }
    if sc == "promote" && matches!(op, "balance" | "open") {
        return Ok(o(vec![(
            "delegated",
            s(if op == "balance" {
                "fetchUtaBalance"
            } else {
                "fetchUtaOpenPositions"
            }),
        )]));
    }
    if matches!(sc, "error" | "promote") && op != "positions" {
        return if matches!(op, "balance" | "uta-balance" | "inverse-balance") {
            Err((
                "Error",
                if op == "inverse-balance" && sc == "promote" {
                    safe_cex_venue_bitget::account_predicates::BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE
                } else {
                    "bad"
                },
            ))
        } else {
            Ok(a(vec![]))
        };
    }
    if sc == "empty" && op != "positions" {
        return if matches!(op, "balance" | "uta-balance" | "inverse-balance") {
            Ok(h.stored.clone())
        } else if op == "metadata" {
            Ok(a(vec![]))
        } else if op == "open" {
            Err((
                "TypeError",
                "Cannot read properties of null (reading 'map')",
            ))
        } else {
            Ok(a(vec![]))
        };
    }
    Ok(match op {
        "balance" => o(vec![
            ("used", V::N(40.)),
            ("free", V::N(60.)),
            ("total", V::N(95.)),
            ("upnl", V::N(5.)),
            ("info", values(&get(&h.response, "data"))[1].clone()),
        ]),
        "uta-balance" => o(vec![
            ("used", V::N(120.)),
            ("free", V::N(80.)),
            ("total", V::N(194.)),
            ("upnl", V::N(6.)),
            ("info", get(&h.response, "data")),
        ]),
        "metadata" => {
            if sc == "plain" {
                a(vec![
                    meta("BTC", "long", 11., "oneway", "cross"),
                    meta("ETH", "long", 11., "oneway", "cross"),
                ])
            } else {
                a(vec![
                    meta("BTC", "long", 7., "hedged", "isolated"),
                    meta("BTC", "short", 9., "hedged", "isolated"),
                    meta("ETH", "long", 7., "hedged", "isolated"),
                    meta("ETH", "short", 9., "hedged", "isolated"),
                ])
            }
        }
        "uta-metadata" => {
            if sc == "plain" {
                a(vec![
                    meta("BTC", "long", 3., "oneway", "isolated"),
                    meta("ETH", "long", 1., "oneway", "cross"),
                ])
            } else {
                a(vec![
                    meta("BTC", "long", 3., "hedged", "isolated"),
                    meta("BTC", "short", 4., "hedged", "isolated"),
                    meta("ETH", "long", 1., "hedged", "cross"),
                    meta("ETH", "short", 1., "hedged", "cross"),
                ])
            }
        }
        "positions" => {
            if sc == "route" {
                h.open.clone()
            } else if sc == "empty" {
                a(vec![
                    fake(meta("BTC", "long", 3., "hedged", "cross"), "BTC:long"),
                    fake(meta("BTC", "short", 4., "hedged", "isolated"), "BTC:short"),
                    fake(meta("ETH", "long", 5., "oneway", "cross"), "ETH"),
                ])
            } else {
                a(vec![
                    values(&h.open)[0].clone(),
                    fake(meta("BTC", "short", 4., "hedged", "isolated"), "BTC:short"),
                    fake(meta("ETH", "long", 5., "oneway", "cross"), "ETH"),
                ])
            }
        }
        "open" | "uta-open" => a(vec![
            o(vec![("raw", V::N(1.)), ("uta", V::Bool(op == "uta-open"))]),
            o(vec![("raw", V::N(2.)), ("uta", V::Bool(op == "uta-open"))]),
        ]),
        "inverse-balance" => o(vec![
            ("total", V::N(66375.)),
            ("upnl", V::N(3625.)),
            ("used", V::N(35000.)),
            ("free", V::N(35000.)),
            (
                "coins",
                o(vec![
                    (
                        "BTC",
                        o(vec![
                            ("base", s("BTC")),
                            ("amount", V::N(2.)),
                            ("notional", V::N(60000.)),
                        ]),
                    ),
                    (
                        "ETH",
                        o(vec![
                            ("base", s("ETH")),
                            ("amount", V::N(4.)),
                            ("notional", V::N(10000.)),
                        ]),
                    ),
                ]),
            ),
            ("info", get(&h.response, "data")),
        ]),
        _ => panic!(),
    })
}
fn expected_trace(op: &str, sc: &str) -> Vec<String> {
    let mut t = Vec::<String>::new();
    let mut add = |v: &str| t.push(v.into());
    match op {
        "balance" => {
            add("call:ensureAccountTypeDetected");
            if sc == "route" {
                add("call:fetchUtaBalance")
            } else {
                add(if sc == "fast" {
                    "read:fast:BALANCE"
                } else {
                    "read:regular:BALANCE"
                });
                match sc {
                    "error" => add("promote"),
                    "promote" => {
                        add("promote");
                        add("call:fetchUtaBalance")
                    }
                    "empty" | "reject" => {}
                    _ => add("upnl"),
                }
            }
        }
        "uta-balance" => {
            add("read:regular:UTA_ACCOUNT_ASSETS");
            if !matches!(sc, "empty" | "error" | "promote" | "reject") {
                add("upnl")
            }
        }
        "metadata" => {
            add("call:ensureAccountTypeDetected");
            if sc == "route" {
                add("call:fetchUtaPositionMetadata")
            } else {
                add("read:regular:FETCH_SINGLE_ACCOUNT");
                if matches!(sc, "empty" | "reject") {
                    add("emit")
                } else {
                    if matches!(sc, "error" | "promote") {
                        add("emit")
                    }
                    add("read:regular:FETCH_SINGLE_ACCOUNT");
                    if matches!(sc, "error" | "promote") {
                        add("emit")
                    }
                }
            }
        }
        "uta-metadata" => {
            add("read:regular:ACCOUNT_SETTINGS_V3");
            if matches!(sc, "error" | "promote" | "reject") {
                add("emit")
            }
        }
        "positions" => {
            add("call:fetchOpenPositions");
            if !matches!(sc, "route" | "reject") {
                add("call:fetchPositionMetadata")
            }
        }
        "open" => {
            add("call:ensureAccountTypeDetected");
            if sc == "route" {
                add("call:fetchUtaOpenPositions")
            } else {
                add("read:regular:POSITIONS_V2");
                match sc {
                    "error" => {
                        add("promote");
                        add("emit")
                    }
                    "promote" => {
                        add("promote");
                        add("call:fetchUtaOpenPositions")
                    }
                    "empty" | "reject" => {}
                    _ => add("map:false"),
                }
            }
        }
        "uta-open" => {
            add("read:regular:UTA_POSITIONS");
            if matches!(sc, "error" | "promote") {
                add("emit")
            } else if sc != "reject" {
                add("map:true")
            }
        }
        "inverse-balance" => add("read:regular:BALANCE"),
        _ => panic!(),
    };
    t
}
#[test]
fn account_positions_native_then_source() {
    let mut cases = vec![];
    let mut wires = vec![];
    for op in OPS {
        for scene in SCENES {
            let h = H::new(op, scene);
            let result = execute(&h);
            let expectation = expected(&h);
            match (&result, expectation) {
                (Ok(v), Ok(e)) => {
                    native(v, &e, &format!("{op}/{scene}"));
                    if matches!(op, "balance" | "uta-balance" | "inverse-balance")
                        && scene == "empty"
                    {
                        assert!(
                            equal(v, &h.stored),
                            "ACCOUNT native cached balance identity"
                        )
                    }
                    if op == "positions" && scene == "route" {
                        assert!(
                            equal(v, &h.open),
                            "ACCOUNT native no-symbol open array identity"
                        )
                    }
                    if op == "balance" && matches!(scene, "normal" | "plain" | "fast") {
                        assert!(
                            equal(&get(v, "info"), &values(&get(&h.response, "data"))[1]),
                            "ACCOUNT native classic info identity"
                        )
                    }
                    if matches!(op, "uta-balance" | "inverse-balance")
                        && !matches!(scene, "empty" | "error" | "reject" | "promote")
                    {
                        assert!(
                            equal(&get(v, "info"), &get(&h.response, "data")),
                            "ACCOUNT native response info identity"
                        )
                    }
                    if op == "positions" && !matches!(scene, "route" | "empty") {
                        assert!(
                            equal(&values(v)[0], &values(&h.open)[0]),
                            "ACCOUNT native original open position identity"
                        )
                    }
                }
                (Err(e), Err((token, msg))) => {
                    assert_eq!(e.token, token, "ACCOUNT native original error type");
                    assert_eq!(text(&e.message), msg, "ACCOUNT native error message")
                }
                other => panic!("ACCOUNT native error disposition {op}/{scene}: {other:?}"),
            }
            if op == "positions" && scene != "route" && scene != "reject" {
                let p = h.params.borrow();
                native(
                    &p[0],
                    &a(vec![s("BTC"), s("ETH"), s("BTC")]),
                    "metadata request independent symbols",
                );
            }
            if op == "balance" && matches!(scene, "normal" | "fast") {
                assert!(
                    h.trace.borrow().contains(&format!(
                        "read:{}:BALANCE",
                        if scene == "fast" { "fast" } else { "regular" }
                    )),
                    "ACCOUNT native client selection"
                );
            }
            let expected_trace = expected_trace(op, scene);
            assert_eq!(
                *h.trace.borrow(),
                expected_trace,
                "ACCOUNT native operation order {op}/{scene}"
            );
            let emit_count = match (op, scene) {
                ("metadata", "error" | "promote") => 2,
                ("metadata", "empty" | "reject")
                | ("uta-metadata", "error" | "promote" | "reject")
                | ("open", "error")
                | ("uta-open", "error" | "promote") => 1,
                _ => 0,
            };
            assert_eq!(
                h.emissions.borrow().len(),
                emit_count,
                "ACCOUNT native independent emission count {op}/{scene}"
            );
            if op != "positions" {
                let expected_params = match op {
                    "balance" | "inverse-balance" => o(vec![("productType", s("USDT-FUTURES"))]),
                    "metadata" => o(vec![
                        ("symbol", s("BTC")),
                        ("productType", s("USDT-FUTURES")),
                        ("marginCoin", s("USDT")),
                    ]),
                    "open" => o(vec![
                        ("productType", s("USDT-FUTURES")),
                        ("marginCoin", s("USDT")),
                    ]),
                    "uta-open" => o(vec![("category", s("linear"))]),
                    _ => o(vec![]),
                };
                let params = h.params.borrow();
                if scene == "route" && matches!(op, "balance" | "metadata" | "open") {
                    assert_eq!(params.len(), 0, "ACCOUNT native delegated no classic read")
                } else {
                    let count = if op == "metadata" && !matches!(scene, "empty" | "reject") {
                        2
                    } else {
                        1
                    };
                    assert_eq!(params.len(), count, "ACCOUNT native independent read count");
                    native(&params[0], &expected_params, "first native request params");
                    if count == 2 {
                        native(
                            &params[1],
                            &o(vec![
                                ("symbol", s("ETH")),
                                ("productType", s("USDT-FUTURES")),
                                ("marginCoin", s("USDT")),
                            ]),
                            "second native request params",
                        );
                    }
                }
            }
            let wire_result = match result {
                Ok(v) => json!({"ok":wire(&v)}),
                Err(e) => json!({"error":wire(&e.message),"token":e.token}),
            };
            wires.push(json!({"result":wire_result,"trace":*h.trace.borrow(),"params":h.params.borrow().iter().map(wire).collect::<Vec<_>>(),"emissions":h.emissions.borrow().iter().map(|(e,d)|json!([wire(e),d.as_ref().map(wire)])).collect::<Vec<_>>() }));
            cases.push(json!({"op":op,"scene":scene,"response":wire(&h.response),"open":wire(&h.open),"meta":wire(&h.meta),"stored":wire(&h.stored)}));
        }
    }
    assert_eq!(cases.len(), 64, "ACCOUNT independent fixture floor");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts/account-positions-capture.mjs");
    let mut child = Command::new("node")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let write_result = child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes());
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "ACCOUNT actual source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    write_result.unwrap();
    let oracle: Vec<Json> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(oracle.len(), 64, "ACCOUNT independent source output floor");
    for (i, (actual, source)) in wires.iter().zip(oracle.iter()).enumerate() {
        assert_eq!(
            actual, source,
            "ACCOUNT actual source case {i}: {}/{}",
            cases[i]["op"], cases[i]["scene"]
        );
    }
}
#[test]
fn native_nonfinite_and_utf16() {
    let h = H::new("uta-metadata", "normal");
    let surrogate = V::S(vec![0xd800, 65]);
    let symbol = a(vec![surrogate.clone()]);
    let v = run(body::fetch_uta_position_metadata(&h, symbol)).unwrap();
    let expected = a(vec![
        meta("unused", "long", 1., "hedged", "cross"),
        meta("unused", "short", 1., "hedged", "cross"),
    ]);
    for x in values(&expected) {
        h.set_key(&x, &s("symbol"), surrogate.clone()).unwrap();
    }
    native(&v, &expected, "unpaired symbol");
    let mut h = H::new("inverse-balance", "normal");
    h.response = o(vec![
        ("code", s("0")),
        (
            "data",
            a(vec![o(vec![
                ("marginCoin", s("BTC")),
                ("accountEquity", s("bad")),
                ("available", s("1")),
                ("unrealizedPL", s("0")),
                ("usdtEquity", s("0")),
            ])]),
        ),
    ]);
    let v = execute(&h).unwrap();
    assert!(
        num(&get(&v, "total")).is_nan(),
        "ACCOUNT native inverse NaN total"
    );
    assert_eq!(
        num(&get(&v, "free")),
        1.,
        "ACCOUNT native inverse finite free despite NaN equity"
    );
}
#[test]
fn native_live_serial_metadata_and_holes() {
    let h = H::new("metadata", "normal");
    let symbols = a(vec![s("BTC")]);
    *h.extend.borrow_mut() = Some(symbols.clone());
    let actual = run(body::fetch_position_metadata(&h, symbols)).unwrap();
    native(
        &actual,
        &a(vec![
            meta("BTC", "long", 7., "hedged", "isolated"),
            meta("BTC", "short", 9., "hedged", "isolated"),
            meta("ETH", "long", 7., "hedged", "isolated"),
            meta("ETH", "short", 9., "hedged", "isolated"),
        ]),
        "live mapSeries append",
    );
    assert_eq!(
        h.params.borrow().len(),
        2,
        "ACCOUNT native appended symbol read count"
    );
    let h = H::new("metadata", "normal");
    let actual = run(body::fetch_position_metadata(
        &h,
        a(vec![V::Hole, s("BTC"), V::Hole]),
    ))
    .unwrap();
    native(
        &actual,
        &a(vec![
            V::U,
            meta("BTC", "long", 7., "hedged", "isolated"),
            meta("BTC", "short", 9., "hedged", "isolated"),
        ]),
        "flatten intermediate hole and trailing hole",
    );
}
#[test]
fn native_environment_mismatch_and_leverage_warning() {
    for (test, current, suggested) in [(false, "Live/Production", "Demo/Testnet")] {
        let mut h = H::new("open", "normal");
        h.response = o(vec![
            ("code", s("35")),
            ("msg", s("environment is incorrect")),
        ]);
        let result = execute(&h).unwrap();
        native(&result, &a(vec![]), "environment mismatch empty");
        assert!(!test);
        let emissions = h.emissions.borrow();
        assert_eq!(emissions.len(), 1, "ACCOUNT native mismatch emit count");
        native(
            &emissions[0].0,
            &s(&format!(
                "API key environment mismatch: Your API key is for {suggested}, but configured for {current}. Check Testnet setting."
            )),
            "environment mismatch message",
        );
    }
    let h = H::new("metadata", "normal");
    let data = get(&h.response, "data");
    h.set_key(&data, &s("posMode"), s("one_way_mode")).unwrap();
    let result = run(body::fetch_position_metadata(&h, a(vec![s("BTC")]))).unwrap();
    native(
        &result,
        &a(vec![meta("BTC", "long", 7., "oneway", "isolated")]),
        "mismatched one-way isolated leverage",
    );
    assert_eq!(
        h.emissions.borrow().len(),
        1,
        "ACCOUNT native warning count"
    );
    native(
        &h.emissions.borrow()[0].0,
        &s("Mismatched one-way isolated leverages for BTC. Please set them to be equal."),
        "warning message",
    );
}
#[test]
fn native_diagnostic_getter_order_then_source() {
    let mut cases = vec![];
    let mut native_outputs = vec![];
    for scene in ["toggle-testnet", "second-testnet-failure"] {
        let mut h = H::new("open", scene);
        h.response = o(vec![
            ("code", s("35")),
            ("msg", s("environment is incorrect")),
        ]);
        let result = execute(&h);
        assert_eq!(
            h.testnet_reads.get(),
            2,
            "ACCOUNT native two testnet getter reads"
        );
        assert_eq!(
            *h.trace.borrow(),
            if scene == "toggle-testnet" {
                vec![
                    "call:ensureAccountTypeDetected",
                    "read:regular:POSITIONS_V2",
                    "promote",
                    "testnet",
                    "testnet",
                    "emit",
                ]
            } else {
                vec![
                    "call:ensureAccountTypeDetected",
                    "read:regular:POSITIONS_V2",
                    "promote",
                    "testnet",
                    "testnet",
                ]
            },
            "ACCOUNT native testnet getter order"
        );
        if scene == "toggle-testnet" {
            native(
                result.as_ref().unwrap(),
                &a(vec![]),
                "toggle diagnostic result",
            );
            assert_eq!(
                h.emissions.borrow().len(),
                1,
                "ACCOUNT native toggle emission count"
            );
            native(
                &h.emissions.borrow()[0].0,
                &s(
                    "API key environment mismatch: Your API key is for Live/Production, but configured for Live/Production. Check Testnet setting.",
                ),
                "toggle diagnostic text",
            );
        } else {
            let e = result.as_ref().unwrap_err();
            assert_eq!(
                e.token, "original",
                "ACCOUNT native second getter original error"
            );
            native(&e.message, &s("rejected"), "second getter error payload");
            assert_eq!(
                h.emissions.borrow().len(),
                0,
                "ACCOUNT native failed second getter no emission"
            );
        }
        let wire_result = match result {
            Ok(v) => json!({"ok":wire(&v)}),
            Err(e) => json!({"error":wire(&e.message),"token":e.token}),
        };
        native_outputs.push(json!({"result":wire_result,"trace":*h.trace.borrow(),"params":h.params.borrow().iter().map(wire).collect::<Vec<_>>(),"emissions":h.emissions.borrow().iter().map(|(e,d)|json!([wire(e),d.as_ref().map(wire)])).collect::<Vec<_>>()}));
        cases.push(json!({"op":"open","scene":scene,"response":wire(&h.response),"open":wire(&h.open),"meta":wire(&h.meta),"stored":wire(&h.stored)}));
    }
    for (op, scene) in [
        ("balance", "classic-data-toggle"),
        ("balance", "classic-data-failure"),
        ("uta-open", "uta-data-toggle"),
        ("uta-open", "uta-list-toggle"),
        ("uta-open", "uta-list-failure"),
    ] {
        let h = H::new(op, scene);
        let result = execute(&h);
        if scene.contains("failure") {
            let e = result.as_ref().unwrap_err();
            assert_eq!(
                e.token, "original",
                "ACCOUNT native second data getter error identity"
            );
            native(
                &e.message,
                &s("rejected"),
                "second data getter error payload",
            );
        } else if op == "balance" {
            native(
                result.as_ref().unwrap(),
                &o(vec![
                    ("used", V::N(150.)),
                    ("free", V::N(100.)),
                    ("total", V::N(238.)),
                    ("upnl", V::N(12.)),
                    ("info", values(&h.alternate)[0].clone()),
                ]),
                "second classic data getter value",
            );
            assert!(
                equal(
                    &get(result.as_ref().unwrap(), "info"),
                    &values(&h.alternate)[0]
                ),
                "ACCOUNT native second data info identity"
            );
        } else {
            native(
                result.as_ref().unwrap(),
                &a(vec![o(vec![("raw", V::N(9.)), ("uta", V::Bool(true))])]),
                "second UTA data/list getter value",
            );
        }
        assert_eq!(
            h.emissions.borrow().len(),
            0,
            "ACCOUNT native data getter error not swallowed"
        );
        let trace = match scene {
            "classic-data-toggle" => vec![
                "call:ensureAccountTypeDetected",
                "read:regular:BALANCE",
                "response-data",
                "response-data",
                "upnl",
            ],
            "classic-data-failure" => vec![
                "call:ensureAccountTypeDetected",
                "read:regular:BALANCE",
                "response-data",
                "response-data",
            ],
            "uta-data-toggle" => vec![
                "read:regular:UTA_POSITIONS",
                "response-data",
                "response-data",
                "map:true",
            ],
            "uta-list-toggle" => vec![
                "read:regular:UTA_POSITIONS",
                "response-list",
                "response-list",
                "map:true",
            ],
            "uta-list-failure" => vec![
                "read:regular:UTA_POSITIONS",
                "response-list",
                "response-list",
            ],
            _ => panic!(),
        };
        assert_eq!(
            *h.trace.borrow(),
            trace,
            "ACCOUNT native guarded second data getter order"
        );
        let wire_result = match result {
            Ok(v) => json!({"ok":wire(&v)}),
            Err(e) => json!({"error":wire(&e.message),"token":e.token}),
        };
        native_outputs.push(json!({"result":wire_result,"trace":*h.trace.borrow(),"params":h.params.borrow().iter().map(wire).collect::<Vec<_>>(),"emissions":[]}));
        cases.push(json!({"op":op,"scene":scene,"response":wire(&h.response),"open":wire(&h.open),"meta":wire(&h.meta),"stored":wire(&h.stored),"alternate":wire(&h.alternate)}));
    }
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/account-positions-capture.mjs"),
        )
        .arg("--diagnostic")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let write = child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "ACCOUNT source diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    write.unwrap();
    let source: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        7,
        "ACCOUNT independent diagnostic source floor"
    );
    for (i, (native, source)) in native_outputs.iter().zip(source.iter()).enumerate() {
        assert_eq!(native, source, "ACCOUNT source diagnostic case {i}");
    }
}
