use safe_cex_venue_bitget::public_data::{self as body, *};
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
        _ => panic!("PUBLIC native array required"),
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
            "PUBLIC native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "PUBLIC native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "PUBLIC native independent array count {path}"
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
                "PUBLIC native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "PUBLIC native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "PUBLIC native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Hole => json!({"hole":true}),
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::Bool(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(t) => json!({"s":t}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(_) => {
            json!({"o":keys(v).into_iter().map(|k|json!([k,wire(&key(v,&k))])).collect::<Vec<_>>()})
        }
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("settled fixture futures"),
    }
}
#[derive(Clone, Debug)]
struct E {
    identity: Rc<()>,
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
struct Iter {
    value: V,
    index: usize,
}
struct H {
    op: &'static str,
    scene: &'static str,
    events: RefCell<Vec<Event>>,
    payloads: RefCell<Vec<V>>,
    original: Rc<()>,
    rows: V,
    opts: V,
    markets: V,
    by_id: V,
    topics: V,
    handlers: V,
    ws: RefCell<V>,
    attempts: Cell<f64>,
    fallback: Cell<bool>,
    last: Cell<f64>,
    count: Cell<f64>,
    probe: Cell<usize>,
}
const REST: [&str; 4] = ["markets", "inverse", "tickers", "candles"];
const SOCKET: [&str; 4] = ["connect", "open", "changed", "subscribe"];
const REST_SCENES: [&str; 6] = ["normal", "empty", "reject", "edge", "filtered", "guarded"];
const WS_SCENES: [&str; 6] = [
    "normal",
    "empty",
    "disposed",
    "fallback",
    "no-fallback",
    "emit-failure",
];
fn market_raw(id: &str, quote: &str, edge: bool) -> V {
    o(vec![
        ("symbol", s(id)),
        ("baseCoin", s("BTC")),
        ("quoteCoin", s(quote)),
        (
            "symbolStatus",
            s(if edge { "limit_open" } else { "normal" }),
        ),
        ("pricePlace", s(if edge { "bad" } else { "2" })),
        ("priceEndStep", s("5")),
        ("sizeMultiplier", s(if edge { "-0" } else { "0.001" })),
        ("maxOrderQty", s("0")),
        ("maxMarketOrderQty", s(if edge { "0" } else { "6" })),
        ("maxTradeNum", s(if edge { "0" } else { "7" })),
        ("minTradeNum", s(if edge { "-0" } else { "0" })),
        ("minLever", s("")),
        ("maxLever", s("0")),
    ])
}
fn ticker_raw(id: &str, edge: bool) -> V {
    o(vec![
        ("symbol", s(id)),
        ("lastPr", s(if edge { "-0" } else { "10" })),
        ("bidPr", s("9")),
        ("askPr", s("11")),
        ("indexPrice", s("12")),
        ("changeUtc24h", s(if edge { "bad" } else { "0.05" })),
        ("fundingRate", s("0.001")),
        ("baseVolume", s("20")),
        ("quoteVolume", s("200")),
    ])
}
fn symbols(n: usize) -> V {
    a((0..n)
        .map(|i| o(vec![("symbol", s(&format!("M{i}")))]))
        .collect())
}
fn make(op: &'static str, scene: &'static str) -> H {
    let edge = scene == "edge";
    let rows = match op {
        "markets" | "inverse" => a(vec![
            market_raw("m0", if scene == "filtered" { "EUR" } else { "USDT" }, edge),
            market_raw("m1", if scene == "filtered" { "EUR" } else { "USD" }, edge),
            market_raw("x", "EUR", edge),
            V::Hole,
        ]),
        "tickers" => a(vec![
            ticker_raw("m0", edge),
            ticker_raw("missing", edge),
            ticker_raw("m1", edge),
        ]),
        "candles" => a(vec![a([
            if scene == "filtered" { "-0" } else { "1000000" },
            "1",
            "3",
            "0.5",
            "2",
            "999",
            "7",
        ]
        .into_iter()
        .map(s)
        .collect())]),
        _ => a(vec![]),
    };
    let opts = if edge {
        o(vec![
            ("symbol", s("BTC")),
            ("interval", s("1d")),
            ("limit", V::N(2000.)),
            ("to", V::N(1e10)),
            ("from", V::N(1000.)),
        ])
    } else {
        o(vec![
            ("symbol", s("BTC")),
            (
                "interval",
                s(if scene == "filtered" { "n/a" } else { "1m" }),
            ),
            ("limit", V::N(if scene == "empty" { 0. } else { 2. })),
            (
                "to",
                V::N(if matches!(scene, "empty" | "filtered") {
                    0.
                } else {
                    1e6
                }),
            ),
            (
                "from",
                V::N(if matches!(scene, "empty" | "filtered") {
                    0.
                } else if scene == "guarded" {
                    1000.
                } else {
                    900000.
                }),
            ),
        ])
    };
    let topics = o(vec![
        (
            "ticker",
            if scene == "fallback" {
                V::U
            } else {
                o(vec![("old", V::N(1.))])
            },
        ),
        ("candle1m", o(vec![("BTC", V::N(2.))])),
    ]);
    H {
        op,
        scene,
        events: RefCell::new(vec![]),
        payloads: RefCell::new(vec![]),
        original: Rc::new(()),
        rows: if scene == "empty" {
            if op == "candles" { V::Null } else { a(vec![]) }
        } else {
            rows
        },
        opts,
        markets: symbols(if scene == "empty" { 0 } else { 7 }),
        by_id: if scene == "filtered" {
            o(vec![])
        } else {
            o(vec![
                ("m0", o(vec![("id", s("id0")), ("symbol", s("S0"))])),
                ("m1", o(vec![("id", s("id1")), ("symbol", s("S1"))])),
            ])
        },
        topics,
        handlers: o(["onOpen", "onMessage", "onClose", "onMarketsChanged"]
            .into_iter()
            .map(|n| (n, o(vec![("id", s(n))])))
            .collect()),
        ws: RefCell::new(if scene == "no-fallback" {
            V::Null
        } else {
            s("socket")
        }),
        attempts: Cell::new(
            if matches!(scene, "fallback" | "no-fallback" | "emit-failure") {
                1.
            } else {
                0.
            },
        ),
        fallback: Cell::new(false),
        last: Cell::new(0.),
        count: Cell::new(0.),
        probe: Cell::new(0),
    }
}
fn put(v: &V, key: Vec<u16>, field: V) {
    let V::O(v) = v else { panic!("native object") };
    let mut v = v.borrow_mut();
    if let Some((_, f)) = v.iter_mut().find(|(k, _)| *k == key) {
        *f = field
    } else {
        v.push((key, field))
    }
}
fn keys(v: &V) -> Vec<Vec<u16>> {
    let V::O(v) = v else {
        panic!("native keys object")
    };
    let mut keys = v
        .borrow()
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    keys.sort_by_key(|k| {
        let t = String::from_utf16_lossy(k);
        match t.parse::<u32>() {
            Ok(n) if n != u32::MAX && n.to_string() == t => (0, n),
            _ => (1, 0),
        }
    });
    keys
}
fn stringify(v: &V) -> String {
    match v {
        V::Hole | V::U | V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        V::S(_) => serde_json::to_string(&text(v)).unwrap(),
        V::A(v) => format!(
            "[{}]",
            v.borrow()
                .iter()
                .map(stringify)
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::O(_) => format!(
            "{{{}}}",
            keys(v)
                .into_iter()
                .filter_map(|k| {
                    let field = key(v, &k);
                    if matches!(field, V::U) {
                        None
                    } else {
                        Some(format!(
                            "{}:{}",
                            serde_json::to_string(&String::from_utf16_lossy(&k)).unwrap(),
                            stringify(&field)
                        ))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}
fn key(v: &V, k: &[u16]) -> V {
    let V::O(v) = v else { return V::U };
    v.borrow()
        .iter()
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::U)
}
impl H {
    fn record(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        });
    }
    fn err(&self) -> E {
        E {
            identity: self.original.clone(),
        }
    }
    fn array_len(&self, v: &V) -> usize {
        let V::A(v) = v else {
            panic!("native array op")
        };
        let n = v.borrow().len();
        n
    }
    fn get_index(&self, v: &V, i: usize) -> V {
        let V::A(v) = v else { panic!("native index") };
        v.borrow().get(i).cloned().unwrap_or(V::U)
    }
}
impl PublicDataHost for H {
    type Value = V;
    type Error = E;
    type Iterator = Iter;
    fn receiver(&self, n: &str) -> Result<V, E> {
        self.record(&format!("get:{n}"), vec![]);
        Ok(match n {
            "apiProductType" | "parent.apiProductType" => s("usdt-futures"),
            "apiMarginCoin" => {
                let i = self.probe.get();
                self.probe.set(i + 1);
                s(if self.scene == "guarded" && i == 1 {
                    "USD"
                } else {
                    "USDT"
                })
            }
            "store.marketsById" => {
                let i = self.probe.get();
                self.probe.set(i + 1);
                if self.scene == "guarded" && i == 1 {
                    return Err(self.err());
                }
                self.by_id.clone()
            }
            "isDisposed" => V::Bool(self.scene == "disposed"),
            "lastMessageCount" => V::N(self.last.get()),
            "messageCount" => V::N(self.count.get()),
            "connectionAttempts" => V::N(self.attempts.get()),
            "MAX_ATTEMPTS" => V::N(2.),
            "useFallbackUrl" => V::Bool(self.fallback.get()),
            "topics" => self.topics.clone(),
            "parent.store.markets" => self.markets.clone(),
            "parent.name" => s("bitget"),
            "parent.options" => o(vec![]),
            "ws" => self.ws.borrow().clone(),
            "onOpen" | "onMessage" | "onClose" | "onMarketsChanged" => get(&self.handlers, n),
            _ => panic!("receiver {n}"),
        })
    }
    fn set_receiver(&self, n: &str, v: V) -> Result<(), E> {
        self.record(&format!("set:{n}"), vec![v.clone()]);
        match n {
            "ws" => *self.ws.borrow_mut() = v,
            "connectionAttempts" => self.attempts.set(self.numeric(&v)?),
            "useFallbackUrl" => self.fallback.set(truth(&v)),
            "lastMessageCount" => self.last.set(self.numeric(&v)?),
            "messageCount" => self.count.set(self.numeric(&v)?),
            _ => panic!("set {n}"),
        };
        Ok(())
    }
    fn capture(&self, n: &str) -> Result<V, E> {
        if n == "ws.addEventListener" {
            self.record("get:ws", vec![]);
        }
        self.record(&format!("capture:{n}"), vec![]);
        Ok(s(n))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        let (name, owner) = if matches!(f, V::O(_)) {
            (text(&get(&f, "method")), get(&f, "owner"))
        } else {
            (text(&f), V::U)
        };
        self.record(&format!("call:{name}"), args.clone());
        if self.scene == "emit-failure"
            && matches!(name.as_str(), "parent.emitter.emit" | "subscribe" | "send")
        {
            return Err(self.err());
        }
        Ok(match name.as_str() {
            "createWebsocket" => s("new-socket"),
            "dayjs.subtract" => {
                let ms = self.numeric(&get(&owner, "ms"))?;
                let n = self.numeric(&args[0])?;
                let mul = match text(&args[1]).as_str() {
                    "m" => 60000.,
                    "h" => 3600000.,
                    "d" => 86400000.,
                    _ => 1.,
                };
                o(vec![("ms", V::N(ms - n * mul))])
            }
            _ => V::U,
        })
    }
    fn capture_member(&self, v: &V, n: &str) -> Result<V, E> {
        assert_eq!(n, "subtract");
        self.record("capture:dayjs.subtract", vec![]);
        Ok(o(vec![
            ("method", s("dayjs.subtract")),
            ("owner", v.clone()),
        ]))
    }
    fn property(&self, v: &V, n: &str) -> Result<V, E> {
        if equal(v, &self.opts) && n == "to" && self.scene == "guarded" {
            self.record("probe:to", vec![]);
            let at = self.probe.get();
            self.probe.set(at + 1);
            return Ok(V::N(if at == 1 { 0. } else { 1e6 }));
        }
        if n == "length" && matches!(v, V::A(_)) {
            return Ok(V::N(self.array_len(v) as f64));
        }
        Ok(get(v, n))
    }
    fn key_get(&self, v: &V, k: &V) -> Result<V, E> {
        let V::S(k) = k else {
            panic!("native fixture key")
        };
        Ok(key(v, k))
    }
    fn assign(&self, v: &V, k: V, f: V) -> Result<(), E> {
        let V::S(k) = k else { panic!("key") };
        put(v, k, f);
        Ok(())
    }
    fn define(&self, v: &V, k: V, f: V) -> Result<(), E> {
        self.assign(v, k, f)
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn numeric(&self, v: &V) -> Result<f64, E> {
        Ok(match v {
            V::N(n) => *n,
            _ => panic!("native fixture numeric"),
        })
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn string(&self, u: &[u16]) -> V {
        V::S(u.to_vec())
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        let V::O(v) = v else { panic!("spread") };
        Ok(V::O(Rc::new(RefCell::new(v.borrow().clone()))))
    }
    fn index(&self, v: &V, i: usize) -> Result<V, E> {
        Ok(self.get_index(v, i))
    }
    fn iterator(&self, v: &V) -> Result<Iter, E> {
        Ok(Iter {
            value: v.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        if i.index >= self.array_len(&i.value) {
            return Ok(None);
        }
        let v = self.get_index(&i.value, i.index);
        i.index += 1;
        Ok(Some(if matches!(v, V::Hole) { V::U } else { v }))
    }
    fn filter<F>(&self, v: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        let mut out = vec![];
        for i in 0..self.array_len(&v) {
            let item = self.get_index(&v, i);
            if !matches!(item, V::Hole) && f(item.clone(), i)? {
                out.push(item)
            }
        }
        Ok(a(out))
    }
    fn map<F>(&self, v: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        let mut out = vec![];
        for i in 0..self.array_len(&v) {
            let item = self.get_index(&v, i);
            out.push(if matches!(item, V::Hole) {
                V::Hole
            } else {
                f(item, i)?
            })
        }
        Ok(a(out))
    }
    fn flat_map<F>(&self, v: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        let mut out = vec![];
        for i in 0..self.array_len(&v) {
            let item = self.get_index(&v, i);
            if !matches!(item, V::Hole) {
                for item in values(&f(item, i)?) {
                    if !matches!(item, V::Hole) {
                        out.push(item)
                    }
                }
            }
        }
        Ok(a(out))
    }
    fn reduce<F>(&self, v: V, mut acc: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, V, usize) -> Result<V, E>,
    {
        for i in 0..self.array_len(&v) {
            let item = self.get_index(&v, i);
            if !matches!(item, V::Hole) {
                acc = f(acc, item, i)?
            }
        }
        Ok(acc)
    }
    fn operation(&self, n: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match n {
            "parseFloat" => V::N(safe_cex_num::parse_float(&text(&args[0]))),
            "parseInt" => V::N(safe_cex_num::parse_int(&text(&args[0]), Some(10))),
            "power" => V::N(self.numeric(&args[0])?.powf(self.numeric(&args[1])?)),
            "Math.min" => V::N(safe_cex_num::math_min(
                self.numeric(&args[0])?,
                self.numeric(&args[1])?,
            )),
            "increment" => V::N(self.numeric(&args[0])? + 1.),
            "greaterEqual" => V::Bool(self.numeric(&args[0])? >= self.numeric(&args[1])?),
            "INTERVAL" => match text(&args[0]).as_str() {
                "1m" => s("1m"),
                "1d" => s("1Dutc"),
                _ => V::U,
            },
            "splitDigits" => match text(&args[0]).as_str() {
                "1m" => a(vec![s(""), s("1"), s("m")]),
                "1d" => a(vec![s(""), s("1"), s("d")]),
                _ => a(vec![args[0].clone()]),
            },
            "timeframeToSeconds" => match text(&args[0]).as_str() {
                "1m" => V::N(60.),
                "1d" => V::N(86400.),
                _ => V::U,
            },
            "dayjs" => o(vec![("ms", args.first().cloned().unwrap_or(V::N(1e6)))]),
            "dayjs.valueOf" => {
                self.record("call:dayjs.valueOf", vec![]);
                get(&args[0], "ms")
            }
            "resolveTransportRouting" => {
                self.record("routing", args);
                o(vec![(
                    "wsPublic",
                    o(vec![(
                        "fallback",
                        if self.scene == "no-fallback" {
                            V::U
                        } else {
                            s("wss://fallback")
                        },
                    )]),
                )])
            }
            "BASE_WS_URL" => s("wss://ws.bitget.com/v2/ws/public"),
            "ENABLE_RECONNECTING_WEBSOCKET" => V::Bool(self.scene != "no-fallback"),
            "Object.keys" => a(keys(&args[0]).into_iter().map(V::S).collect()),
            "Object.entries" => a(keys(&args[0])
                .into_iter()
                .map(|k| a(vec![V::S(k.clone()), key(&args[0], &k)]))
                .collect()),
            "toUpperCase" => s(&text(&args[0]).to_uppercase()),
            "JSON.stringify" => {
                self.payloads.borrow_mut().push(args[0].clone());
                s(&stringify(&args[0]))
            }
            _ => panic!("operation {n}"),
        })
    }
    fn read(&self, f: V, endpoint: &'static str, params: V) -> Result<PublicRead<'_, V, E>, E> {
        assert_eq!(text(&f), "xhr.get", "PUBLIC native captured xhr callable");
        self.record(&format!("read:{endpoint}"), vec![params]);
        let rows = self.rows.clone();
        Ok(Box::pin(async move {
            if self.scene == "reject" {
                Err(self.err())
            } else {
                Ok(o(vec![("data", rows)]))
            }
        }))
    }
}
fn execute(h: &H) -> Result<V, E> {
    match h.op {
        "markets" => run(body::fetch_markets(h)),
        "inverse" => run(body::fetch_inverse_markets(h)),
        "tickers" => run(body::fetch_tickers(h)),
        "candles" => run(body::fetch_ohlcv(h, h.opts.clone())),
        "connect" => body::connect_and_subscribe(h).map(|()| V::U),
        "open" => body::on_open(h).map(|()| V::U),
        "changed" => body::on_markets_changed(h, h.markets.clone()).map(|()| V::U),
        "subscribe" => body::subscribe(h).map(|()| V::U),
        _ => panic!(),
    }
}
fn error_expected(h: &H) -> bool {
    h.scene == "reject"
        || h.scene == "guarded" && h.op == "tickers"
        || h.scene == "emit-failure" && h.op != "connect"
}
fn expected_market(raw: V, id: &str, edge: bool, inverse: bool) -> V {
    let mut fields = vec![
        ("id", s(id)),
        ("symbol", s(id)),
        ("base", s("BTC")),
        ("quote", s(if id == "m0" { "USDT" } else { "USD" })),
        ("active", V::Bool(true)),
        (
            "precision",
            o(vec![
                ("amount", V::N(if edge { -0. } else { 0.001 })),
                ("price", V::N(if edge { f64::NAN } else { 0.05 })),
            ]),
        ),
        (
            "limits",
            o(vec![
                (
                    "amount",
                    o(vec![
                        ("min", V::N(0.)),
                        (
                            "max",
                            V::N(if edge {
                                f64::INFINITY
                            } else if inverse {
                                7.
                            } else {
                                6.
                            }),
                        ),
                    ]),
                ),
                ("leverage", o(vec![("min", V::N(1.)), ("max", V::N(20.))])),
            ]),
        ),
    ];
    if inverse {
        fields.push(("isInverse", V::Bool(true)))
    }
    fields.push(("info", raw));
    o(fields)
}
fn expected_ticker(id: usize, edge: bool) -> V {
    o(vec![
        ("id", s(&format!("id{id}"))),
        ("symbol", s(&format!("S{id}"))),
        ("bid", V::N(9.)),
        ("ask", V::N(11.)),
        ("last", V::N(if edge { -0. } else { 10. })),
        ("mark", V::N(if edge { -0. } else { 10. })),
        ("index", V::N(12.)),
        ("percentage", V::N(if edge { f64::NAN } else { 5. })),
        ("fundingRate", V::N(0.001)),
        ("fundingTime", V::N(0.)),
        ("openInterest", V::N(0.)),
        ("volume", V::N(20.)),
        ("quoteVolume", V::N(200.)),
    ])
}
fn names(h: &H) -> Vec<String> {
    h.events.borrow().iter().map(|e| e.name.clone()).collect()
}
fn check_rest(h: &H, r: &Result<V, E>) {
    assert_eq!(
        r.is_err(),
        error_expected(h),
        "PUBLIC native REST disposition {}/{}",
        h.op,
        h.scene
    );
    if let Err(e) = r {
        assert!(
            Rc::ptr_eq(&e.identity, &h.original),
            "PUBLIC native REST original error identity"
        );
    } else {
        let expected = match h.op {
            "markets" | "inverse" => {
                let indices = if matches!(h.scene, "empty" | "filtered") {
                    vec![]
                } else if h.op == "inverse" {
                    vec![1]
                } else if h.scene == "guarded" {
                    vec![0, 1]
                } else {
                    vec![0]
                };
                let mut out = vec![];
                for i in indices {
                    let raw = h.get_index(&h.rows, i);
                    out.push(expected_market(
                        raw,
                        &format!("m{i}"),
                        h.scene == "edge",
                        h.op == "inverse",
                    ))
                }
                a(out)
            }
            "tickers" => {
                if matches!(h.scene, "empty" | "filtered") {
                    a(vec![])
                } else {
                    a(vec![
                        expected_ticker(0, h.scene == "edge"),
                        expected_ticker(1, h.scene == "edge"),
                    ])
                }
            }
            "candles" => {
                if h.scene == "empty" {
                    a(vec![])
                } else {
                    a(vec![o(vec![
                        (
                            "timestamp",
                            V::N(if h.scene == "filtered" { -0. } else { 1000. }),
                        ),
                        ("open", V::N(1.)),
                        ("high", V::N(3.)),
                        ("low", V::N(0.5)),
                        ("close", V::N(2.)),
                        ("volume", V::N(7.)),
                    ])])
                }
            }
            _ => panic!(),
        };
        native(
            r.as_ref().unwrap(),
            &expected,
            &format!("REST result {}/{}", h.op, h.scene),
        );
        if matches!(h.op, "markets" | "inverse") {
            for out in values(r.as_ref().unwrap()) {
                assert!(
                    values(&h.rows)
                        .iter()
                        .any(|raw| equal(raw, &get(&out, "info"))),
                    "PUBLIC native market info alias"
                )
            }
        }
    }
    let events = h.events.borrow();
    let requests: Vec<_> = events
        .iter()
        .filter(|e| e.name.starts_with("read:"))
        .collect();
    assert_eq!(requests.len(), 1, "PUBLIC independent REST request count");
    let params = if h.op == "candles" {
        let mut f = vec![
            ("symbol", s("BTC")),
            (
                "granularity",
                if h.scene == "filtered" {
                    V::U
                } else {
                    s(if h.scene == "edge" { "1Dutc" } else { "1m" })
                },
            ),
            (
                "endTime",
                V::N(if h.scene == "edge" {
                    7689601000.
                } else if h.scene == "guarded" {
                    0.
                } else {
                    1e6
                }),
            ),
        ];
        if !matches!(h.scene, "empty" | "filtered" | "guarded") {
            f.push((
                "startTime",
                V::N(if h.scene == "edge" { 1000. } else { 900000. }),
            ))
        }
        f.push(("productType", s("usdt-futures")));
        f.push((
            "limit",
            V::N(if h.scene == "edge" {
                1000.
            } else if h.scene == "empty" {
                500.
            } else {
                2.
            }),
        ));
        o(f)
    } else {
        o(vec![("productType", s("usdt-futures"))])
    };
    native(
        &requests[0].args[0],
        &params,
        &format!("REST params {}/{}", h.op, h.scene),
    );
    let mut expected = vec![];
    if h.op == "candles" {
        if h.scene == "guarded" {
            expected.extend(["probe:to", "probe:to"])
        }
        if matches!(h.scene, "empty" | "filtered") {
            expected.extend(["capture:dayjs.subtract", "call:dayjs.subtract"])
        }
        expected.extend(["call:dayjs.valueOf", "call:dayjs.valueOf"])
    }
    expected.extend(["capture:xhr.get", "get:apiProductType"]);
    expected.push(match h.op {
        "markets" | "inverse" => "read:MARKETS",
        "tickers" => "read:TICKERS",
        _ => "read:KLINE",
    });
    if h.scene != "reject" {
        if h.op == "markets" && h.scene != "empty" {
            expected.extend(["get:apiMarginCoin"; 3])
        }
        if h.op == "tickers" && h.scene != "empty" {
            expected.extend(vec![
                "get:store.marketsById";
                if h.scene == "guarded" { 2 } else { 3 }
            ])
        }
    }
    assert_eq!(
        names(h),
        expected,
        "PUBLIC native REST operation order {}/{}",
        h.op,
        h.scene
    );
}
fn arg(id: &str, channel: &str) -> V {
    o(vec![
        ("instType", s("USDT-FUTURES")),
        ("channel", s(channel)),
        ("instId", s(id)),
    ])
}
fn check_socket(h: &H, r: &Result<V, E>) {
    assert_eq!(
        r.is_err(),
        error_expected(h),
        "PUBLIC native socket disposition {}/{}",
        h.op,
        h.scene
    );
    if let Err(e) = r {
        assert!(
            Rc::ptr_eq(&e.identity, &h.original),
            "PUBLIC native socket original error identity"
        )
    } else {
        native(r.as_ref().unwrap(), &V::U, "socket return")
    }
    let events = h.events.borrow();
    let sends: Vec<_> = events.iter().filter(|e| e.name == "call:send").collect();
    let expected_args = match h.op {
        "changed" => {
            if matches!(h.scene, "empty" | "disposed" | "no-fallback") {
                vec![]
            } else if h.scene == "fallback" {
                vec![
                    arg("M0", "ticker"),
                    arg("M3", "ticker"),
                    arg("M6", "ticker"),
                ]
            } else {
                vec![arg("M2", "ticker"), arg("M5", "ticker")]
            }
        }
        "subscribe" => {
            let mut a = vec![];
            if h.scene != "fallback" {
                a.push(arg("old", "ticker"))
            }
            a.push(arg("BTC", "candle1m"));
            a
        }
        _ => vec![],
    };
    let expected_sends =
        usize::from(h.op == "subscribe" || h.op == "changed" && !expected_args.is_empty());
    assert_eq!(
        sends.len(),
        expected_sends,
        "PUBLIC native independent send count {}/{}",
        h.op,
        h.scene
    );
    assert_eq!(
        h.payloads.borrow().len(),
        expected_sends,
        "PUBLIC native independent payload count"
    );
    if expected_sends > 0 {
        let expected = o(vec![
            ("op", s("subscribe")),
            ("args", a(expected_args.clone())),
        ]);
        native(
            &h.payloads.borrow()[0],
            &expected,
            "native subscription payload",
        );
        assert!(
            matches!(&sends[0].args[0], V::S(_)),
            "PUBLIC native send argument type"
        );
    }
    if h.op == "connect" && h.scene != "disposed" {
        assert_eq!(
            h.attempts.get(),
            if matches!(h.scene, "fallback" | "no-fallback" | "emit-failure") {
                2.
            } else {
                1.
            },
            "PUBLIC native attempt count"
        );
        assert_eq!(
            h.fallback.get(),
            matches!(h.scene, "fallback" | "no-fallback" | "emit-failure"),
            "PUBLIC native fallback flag"
        );
        native(
            &get(&h.topics, "ticker"),
            &if h.scene == "empty" {
                o(vec![])
            } else {
                o(vec![("M0", V::N(1.)), ("M3", V::N(1.)), ("M6", V::N(1.))])
            },
            "native sampled ticker topics",
        );
        native(
            &get(&h.topics, "candle1m"),
            &o(vec![("BTC", V::N(2.))]),
            "native preserved candle topics",
        );
        native(&h.ws.borrow(), &s("new-socket"), "native assigned ws");
        assert_eq!(
            h.last.get().to_bits(),
            0f64.to_bits(),
            "PUBLIC native last count"
        );
        assert_eq!(
            h.count.get().to_bits(),
            0f64.to_bits(),
            "PUBLIC native reset count"
        );
        let creator = events
            .iter()
            .find(|e| e.name == "call:createWebsocket")
            .unwrap();
        native(
            &creator.args[0],
            &s(if matches!(h.scene, "fallback" | "emit-failure") {
                "wss://fallback"
            } else {
                "wss://ws.bitget.com/v2/ws/public"
            }),
            "native connection URL",
        );
        let listeners: Vec<_> = events
            .iter()
            .filter(|e| e.name == "call:ws.addEventListener")
            .collect();
        assert_eq!(
            listeners.len(),
            3,
            "PUBLIC native independent listener count"
        );
        for (e, (event, handler)) in listeners.iter().zip([
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ]) {
            native(
                &a(e.args.clone()),
                &a(vec![s(event), o(vec![("id", s(handler))])]),
                "native listener identity",
            );
            assert!(
                equal(&e.args[1], &get(&h.handlers, handler)),
                "PUBLIC native captured listener alias"
            )
        }
    }
    if h.op == "changed" && !expected_args.is_empty() {
        let mut fields = if h.scene == "fallback" {
            vec![]
        } else {
            vec![("old", V::N(1.))]
        };
        let owned: Vec<_> = expected_args
            .iter()
            .map(|v| text(&get(v, "instId")))
            .collect();
        for k in &owned {
            fields.push((k.as_str(), V::N(1.)))
        }
        native(&get(&h.topics, "ticker"), &o(fields), "native new topics");
    }
    if h.op == "open" && !matches!(h.scene, "disposed" | "emit-failure") {
        let changes: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    e.name.as_str(),
                    "call:parent.emitter.off" | "call:parent.emitter.on"
                )
            })
            .collect();
        assert_eq!(
            changes.len(),
            2,
            "PUBLIC native independent market listener calls"
        );
        for e in changes {
            native(
                &e.args[0],
                &s("markets:changed"),
                "native markets listener event",
            );
            native(
                &e.args[1],
                &o(vec![("id", s("onMarketsChanged"))]),
                "native markets listener selection",
            );
            assert!(
                equal(&e.args[1], &get(&h.handlers, "onMarketsChanged")),
                "PUBLIC native market listener alias"
            );
        }
    }
    let mut expected: Vec<String> = vec![];
    let mut push = |n: &str| expected.push(n.into());
    match h.op {
        "connect" => {
            push("get:isDisposed");
            if h.scene != "disposed" {
                for n in [
                    "get:lastMessageCount",
                    "get:messageCount",
                    "get:connectionAttempts",
                    "set:connectionAttempts",
                    "get:connectionAttempts",
                    "get:MAX_ATTEMPTS",
                ] {
                    push(n)
                }
                if matches!(h.scene, "fallback" | "no-fallback" | "emit-failure") {
                    push("set:useFallbackUrl")
                }
                for n in [
                    "get:messageCount",
                    "set:lastMessageCount",
                    "set:messageCount",
                    "get:topics",
                    "get:parent.store.markets",
                    "get:parent.name",
                    "get:parent.options",
                    "routing",
                    "get:useFallbackUrl",
                ] {
                    push(n)
                }
                if matches!(h.scene, "fallback" | "emit-failure") {
                    push("capture:parent.emitter.emit");
                    push("call:parent.emitter.emit")
                }
                for n in ["capture:createWebsocket", "call:createWebsocket", "set:ws"] {
                    push(n)
                }
                for handler in ["onOpen", "onMessage", "onClose"] {
                    push("get:ws");
                    push("capture:ws.addEventListener");
                    push(&format!("get:{handler}"));
                    push("call:ws.addEventListener")
                }
            }
        }
        "open" => {
            push("get:isDisposed");
            if h.scene != "disposed" {
                push("capture:subscribe");
                push("call:subscribe");
                if h.scene != "emit-failure" {
                    push("capture:ping");
                    push("call:ping");
                    for method in ["off", "on"] {
                        push(&format!("capture:parent.emitter.{method}"));
                        push("get:onMarketsChanged");
                        push(&format!("call:parent.emitter.{method}"))
                    }
                    if h.scene != "no-fallback" {
                        push("capture:emitWsReconnected");
                        push("call:emitWsReconnected")
                    }
                }
            }
        }
        "changed" => {
            if h.scene != "empty" {
                push("get:ws");
                if h.scene != "no-fallback" {
                    push("get:isDisposed");
                    if h.scene != "disposed" {
                        push("get:topics");
                        for _ in &expected_args {
                            push("get:parent.apiProductType")
                        }
                        for (i, _) in expected_args.iter().enumerate() {
                            push("get:topics");
                            if h.scene == "fallback" && i == 0 {
                                push("get:topics")
                            }
                            push("get:topics")
                        }
                        if !expected_args.is_empty() {
                            push("capture:send");
                            push("call:send")
                        }
                    }
                }
            }
        }
        "subscribe" => {
            push("get:topics");
            for _ in &expected_args {
                push("get:parent.apiProductType")
            }
            push("capture:send");
            push("call:send")
        }
        _ => panic!(),
    };
    assert_eq!(
        names(h),
        expected,
        "PUBLIC native socket operation order {}/{}",
        h.op,
        h.scene
    );
}
fn input(h: &H) -> Json {
    json!({"op":h.op,"scene":h.scene,"rows":wire(&h.rows),"opts":wire(&h.opts),"markets":wire(&h.markets),"byId":wire(&h.by_id),"topics":wire(&h.topics),"handlers":wire(&h.handlers),"ws":wire(&h.ws.borrow()),"attempts":h.attempts.get(),"fallback":h.fallback.get(),"last":h.last.get(),"count":h.count.get()})
}
fn output(h: &H, r: &Result<V, E>) -> Json {
    json!({"result":match r {Ok(v)=>json!({"ok":wire(v)}),Err(e)=>json!({"original":Rc::ptr_eq(&e.identity,&h.original)})},"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"topics":wire(&h.topics),"ws":wire(&h.ws.borrow()),"attempts":wire(&V::N(h.attempts.get())),"fallback":h.fallback.get(),"last":wire(&V::N(h.last.get())),"count":wire(&V::N(h.count.get()))})
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut c = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/public-data-capture.mjs"),
        )
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "PUBLIC source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn native_rest_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in REST {
        for scene in REST_SCENES {
            let h = make(op, scene);
            cases.push(input(&h));
            let result = execute(&h);
            check_rest(&h, &result);
            outputs.push(output(&h, &result));
        }
    }
    let src = source(&cases, "rest");
    assert_eq!(src.len(), 24, "PUBLIC independent REST floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        compare(a, b, &format!("REST case {i}"))
    }
}
#[test]
fn native_socket_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in SOCKET {
        for scene in WS_SCENES {
            let h = make(op, scene);
            if op == "subscribe" && scene == "fallback" {
                put(&h.topics, "ticker".encode_utf16().collect(), o(vec![]));
            }
            cases.push(input(&h));
            let result = execute(&h);
            check_socket(&h, &result);
            outputs.push(output(&h, &result));
        }
    }
    let src = source(&cases, "socket");
    assert_eq!(src.len(), 24, "PUBLIC independent socket floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        compare(a, b, &format!("socket case {i}"))
    }
}

fn compare(a: &Json, b: &Json, path: &str) {
    if a == b {
        return;
    }
    match (a, b) {
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "PUBLIC source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "PUBLIC source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("PUBLIC source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "PUBLIC source value {path}"),
    }
}
#[test]
fn native_activity_sampling_and_keys_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for i in 0..4 {
        let h = make(
            match i {
                0 | 1 => "connect",
                2 => "changed",
                _ => "subscribe",
            },
            "normal",
        );
        if i == 0 {
            h.count.set(3.)
        }
        if i == 1 {
            h.last.set(2.)
        }
        let old_ticker = get(&h.topics, "ticker");
        if i == 3 {
            put(
                &h.topics,
                "ticker".encode_utf16().collect(),
                o(vec![("10", V::N(1.)), ("2", V::N(1.)), ("z", V::N(1.))]),
            );
        }
        let h = if i == 2 {
            H {
                markets: symbols(1),
                ..h
            }
        } else {
            h
        };
        cases.push(input(&h));
        let result = execute(&h);
        native(result.as_ref().unwrap(), &V::U, "special bootstrap return");
        if i < 2 {
            assert_eq!(
                h.attempts.get().to_bits(),
                0f64.to_bits(),
                "PUBLIC native activity avoids attempt increment"
            );
            assert!(!h.fallback.get(), "PUBLIC native activity avoids fallback");
            assert_eq!(
                h.last.get(),
                if i == 0 { 3. } else { 0. },
                "PUBLIC native activity snapshots current count"
            );
            assert_eq!(
                h.count.get().to_bits(),
                0f64.to_bits(),
                "PUBLIC native activity resets count"
            );
            let mut expected = vec!["get:isDisposed", "get:lastMessageCount"];
            if i == 0 {
                expected.push("get:messageCount")
            }
            expected.extend([
                "get:messageCount",
                "set:lastMessageCount",
                "set:messageCount",
                "get:topics",
                "get:parent.store.markets",
                "get:parent.name",
                "get:parent.options",
                "routing",
                "get:useFallbackUrl",
                "capture:createWebsocket",
                "call:createWebsocket",
                "set:ws",
            ]);
            let mut expected: Vec<String> = expected.into_iter().map(String::from).collect();
            for handler in ["onOpen", "onMessage", "onClose"] {
                expected.extend([
                    "get:ws".into(),
                    "capture:ws.addEventListener".into(),
                    format!("get:{handler}"),
                    "call:ws.addEventListener".into(),
                ]);
            }
            assert_eq!(
                names(&h),
                expected,
                "PUBLIC native activity operation order"
            );
            native(
                &get(&h.topics, "ticker"),
                &o(vec![("M0", V::N(1.)), ("M3", V::N(1.)), ("M6", V::N(1.))]),
                "native activity sampling",
            );
        } else if i == 2 {
            assert_eq!(
                names(&h),
                ["get:ws", "get:isDisposed", "get:topics"],
                "PUBLIC native no selected market skips send"
            );
            assert!(
                equal(&old_ticker, &get(&h.topics, "ticker")),
                "PUBLIC native no selection retains ticker object alias"
            );
            native(
                &get(&h.topics, "ticker"),
                &o(vec![("old", V::N(1.))]),
                "native no selection retains keys",
            );
        } else {
            let events = h.events.borrow();
            let send: Vec<_> = events.iter().filter(|e| e.name == "call:send").collect();
            assert_eq!(send.len(), 1, "PUBLIC independent integer-key send count");
            let expected = o(vec![
                ("op", s("subscribe")),
                (
                    "args",
                    a(vec![
                        arg("2", "ticker"),
                        arg("10", "ticker"),
                        arg("z", "ticker"),
                        arg("BTC", "candle1m"),
                    ]),
                ),
            ]);
            assert_eq!(
                h.payloads.borrow().len(),
                1,
                "PUBLIC native independent integer-key payload count"
            );
            native(
                &h.payloads.borrow()[0],
                &expected,
                "native integer-key enumeration order",
            );
            assert!(
                matches!(&send[0].args[0], V::S(_)),
                "PUBLIC native integer-key send type"
            );
            assert_eq!(
                names(&h),
                [
                    "get:topics",
                    "get:parent.apiProductType",
                    "get:parent.apiProductType",
                    "get:parent.apiProductType",
                    "get:parent.apiProductType",
                    "capture:send",
                    "call:send"
                ],
                "PUBLIC native integer-key operation order"
            );
        }
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "special");
    assert_eq!(src.len(), 4, "PUBLIC independent activity/key floor");
    for (i, (a, b)) in outputs.iter().zip(&src).enumerate() {
        compare(a, b, &format!("activity/key case {i}"))
    }
}
