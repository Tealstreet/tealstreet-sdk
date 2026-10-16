use safe_cex_venue_bitget::{
    order_placement::*,
    order_preparation::*,
    rest_actions::*,
    runtime_orchestration::{self as body, *},
};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Callback = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
#[derive(Clone)]
enum V {
    Hole,
    Fn(Callback),
    U,
    Null,
    Bool(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
    Promise(Rc<Result<V, E>>),
    Task(Rc<RefCell<Option<RestActionFuture<'static, V, E>>>>),
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
        V::Promise(_) | V::Task(_) => "[object Promise]".into(),
        V::Fn(_) => "[function]".into(),
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
        (V::Promise(a), V::Promise(b)) => Rc::ptr_eq(a, b),
        (V::Task(a), V::Task(b)) => Rc::ptr_eq(a, b),
        (V::Fn(a), V::Fn(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("RUN native array required"),
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
            "RUN native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "RUN native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "RUN native independent array count {path}"
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
                "RUN native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "RUN native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "RUN native scalar {path}: {actual:?} != {expected:?}"
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
        V::Promise(_) | V::Task(_) => json!({"promise":true}),
        V::Fn(_) => json!({"fn":true}),
        V::O(_) => {
            json!({"o":keys(v).into_iter().map(|k|json!([k,wire(&key(v,&k))])).collect::<Vec<_>>()})
        }
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    for _ in 0..100 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut cx) {
            return r;
        }
    }
    panic!("RUN bounded fixture future did not settle")
}

#[derive(Clone, Debug)]
struct E {
    identity: Rc<()>,
    value: V,
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
fn key(v: &V, k: &[u16]) -> V {
    let V::O(v) = v else { return V::U };
    v.borrow()
        .iter()
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::U)
}
fn compare(a: &Json, b: &Json, path: &str) {
    if a == b {
        return;
    }
    match (a, b) {
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "RUN source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "RUN source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("RUN source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "RUN source value {path}"),
    }
}

impl std::fmt::Debug for V {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&text(self))
    }
}
fn method(path: &str, owner: V) -> V {
    o(vec![("path", s(path)), ("owner", owner)])
}
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        V::Null => 0.,
        V::U => f64::NAN,
        V::Bool(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        _ => text(v).parse().unwrap_or(f64::NAN),
    }
}
fn promise(r: Result<V, E>) -> V {
    V::Promise(Rc::new(r))
}
fn unwrap_promise(v: V) -> Result<V, E> {
    match v {
        V::Promise(p) => p.as_ref().clone(),
        v => Ok(v),
    }
}
fn units(v: &V) -> Vec<u16> {
    match v {
        V::S(s) => s.clone(),
        _ => text(v).encode_utf16().collect(),
    }
}
fn binding(member: &V, array: &V, name: &str) {
    native(&get(member, "path"), &s(name), "array method selector");
    let owner = get(member, "owner");
    if !matches!(owner, V::U) {
        alias(&owner, array, "captured array method owner");
    }
}

fn alias(a: &V, b: &V, why: &str) {
    assert!(equal(a, b), "RUN native original reference {why}")
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
#[derive(Clone)]
struct H {
    op: &'static str,
    scene: &'static str,
    state: V,
    events: Rc<RefCell<Vec<Event>>>,
    callbacks: Rc<RefCell<Vec<V>>>,
    packets: Rc<RefCell<Vec<V>>>,
    balance: V,
    markets: V,
    tickers: V,
    orders: V,
    positions: V,
    normal: V,
    records: V,
    metadata: V,
    original: E,
    callback: V,
    old_handler: V,
    flight: V,
}
fn fixture(op: &'static str, scene: &'static str) -> H {
    let events = Rc::new(RefCell::new(vec![]));
    let callbacks = Rc::new(RefCell::new(vec![]));
    let e = events.clone();
    let callback = V::Fn(Rc::new(move |args| {
        e.borrow_mut().push(Event {
            name: "callback".into(),
            args,
        });
        Ok(V::U)
    }));
    let old_handler = V::Fn(Rc::new(|_| Ok(V::U)));
    let balance = o(vec![("total", V::N(17.))]);
    let markets = a(vec![s("BTC"), s("ETH")]);
    let tickers = a(vec![s("BTC")]);
    let orders = a(vec![o(vec![("id", s("fresh")), ("type", s("limit"))])]);
    let normal = a(vec![
        V::Hole,
        o(vec![("id", s("normal")), ("type", s("limit"))]),
        o(vec![("id", s("virtual")), ("type", s("stop_market"))]),
    ]);
    let positions = if scene == "position" {
        a(vec![o(vec![
            ("positionMode", s("hedged")),
            ("marginType", s("cross")),
        ])])
    } else {
        a(vec![])
    };
    let records = a(vec![
        o(vec![("id", s("a")), ("incomeType", s("funding"))]),
        o(vec![("id", s("b")), ("incomeType", s("trade"))]),
        o(vec![("id", s("c")), ("incomeType", s("funding"))]),
    ]);
    let metadata = a(vec![o(vec![
        ("positionMode", s("oneway")),
        (
            "marginType",
            if scene == "partial" {
                V::U
            } else {
                s("isolated")
            },
        ),
    ])]);
    let loaded = o(vec![
        ("markets", V::Bool(false)),
        ("tickers", V::Bool(false)),
        ("balance", V::Bool(false)),
        ("positions", V::Bool(false)),
        ("orders", V::Bool(false)),
    ]);
    let cache = o(vec![("KEEP", s("keep"))]);
    if scene == "cache" || scene == "expired" {
        put(
            &cache,
            units(&s("BTCUSD")),
            o(vec![
                ("at", V::N(if scene == "expired" { -4000. } else { 900. })),
                (
                    "value",
                    o(vec![
                        ("positionMode", s("hedged")),
                        ("marginType", s("cross")),
                    ]),
                ),
            ]),
        );
    }
    let flight = promise(Ok(if op == "resolve" {
        o(vec![
            ("positionMode", s("hedged")),
            ("marginType", s("cross")),
        ])
    } else {
        V::U
    }));
    let inflight = o(vec![("KEEP", s("flight-keep"))]);
    if scene == "existing" {
        put(&inflight, units(&s("BTCUSD")), flight.clone());
    }
    let handlers = o(vec![]);
    let topics = o(vec![]);
    let topic = if op == "book" {
        "orderBook_BTCUSD"
    } else {
        "candle_BTCUSD_1m"
    };
    let channel = if op == "book" { "books" } else { "candle1m" };
    if scene == "shared" {
        put(&handlers, units(&s(topic)), a(vec![old_handler.clone()]));
        put(&topics, units(&s(channel)), o(vec![("BTCUSD", V::N(1.))]));
    }
    let state = o(vec![
        ("loaded", loaded),
        ("suppression", V::N(7.)),
        ("disposed", V::Bool(scene == "disposed")),
        ("connected", V::Bool(!["delayed", "early"].contains(&scene))),
        ("detected", V::Bool(scene == "detected")),
        ("uta", V::Bool(false)),
        (
            "detection",
            if scene == "existing" {
                flight.clone()
            } else {
                V::Null
            },
        ),
        ("cache", cache),
        ("inflight", inflight),
        ("handlers", handlers),
        ("topics", topics),
        (
            "storeOrders",
            a(vec![
                o(vec![("id", s("keep")), ("type", s("limit"))]),
                o(vec![
                    ("id", s("keep__stop_loss")),
                    ("type", s("stop_market")),
                ]),
            ]),
        ),
    ]);
    let msg = if scene.starts_with("environment") {
        s("environment is incorrect")
    } else if scene == "empty-message" {
        s("")
    } else {
        s("original")
    };
    let response = if scene == "classic-error" {
        o(vec![(
            "data",
            o(vec![("code", s("40084")), ("msg", s("classic"))]),
        )])
    } else if scene == "auth-error" {
        o(vec![(
            "data",
            o(vec![("code", s("401")), ("msg", s("auth"))]),
        )])
    } else if scene == "environment-error" {
        o(vec![(
            "data",
            o(vec![("msg", s("environment is incorrect"))]),
        )])
    } else {
        V::U
    };
    H {
        op,
        scene,
        state,
        events,
        callbacks,
        packets: Rc::new(RefCell::new(vec![])),
        balance,
        markets,
        tickers,
        orders,
        positions,
        normal,
        records,
        metadata,
        original: E {
            identity: Rc::new(()),
            value: o(vec![("message", msg), ("response", response)]),
        },
        callback,
        old_handler,
        flight,
    }
}
impl H {
    fn record(&self, n: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: n.into(),
            args,
        });
    }
    fn calls(&self, n: &str) -> Vec<Vec<V>> {
        self.events
            .borrow()
            .iter()
            .filter(|e| e.name == format!("call:{n}"))
            .map(|e| e.args.clone())
            .collect()
    }
    fn disposed_after(&self, n: &str) {
        if self.scene
            == format!(
                "disposed-{}",
                match n {
                    "fetchMarkets" => "markets",
                    "fetchTickers" => "tickers",
                    "ensureAccountTypeDetected" => "detection",
                    "tick" => "tick",
                    "fetchOrders" => "orders",
                    "fetchPositions" => "positions",
                    "fetchBalance" => "balance",
                    _ => "never",
                }
            )
        {
            put(&self.state, units(&s("disposed")), V::Bool(true));
        }
    }
}
struct YieldOnce {
    result: Option<Result<V, E>>,
    yielded: bool,
}
impl Future for YieldOnce {
    type Output = Result<V, E>;
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.result.take().unwrap())
        }
    }
}
impl RestActionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, n: &str) -> Result<V, E> {
        Ok(match n {
            "isDisposed" => get(&self.state, "disposed"),
            "isConnected" => get(&self.state, "connected"),
            "isPublicDataOnly" => V::Bool(["public", "external-public"].contains(&self.scene)),
            "options.testnet" => V::Bool(self.scene == "environment-demo"),
            "_suppressBitgetValidationErrorEmission" => get(&self.state, "suppression"),
            "_accountTypeDetected" => get(&self.state, "detected"),
            "isUta" => get(&self.state, "uta"),
            "_accountTypeDetectionInflight" => get(&self.state, "detection"),
            "_positioningCache" => get(&self.state, "cache"),
            "_positioningInflight" => get(&self.state, "inflight"),
            "store.loaded" => get(&self.state, "loaded"),
            "store.orders" => get(&self.state, "storeOrders"),
            "messageHandlers" => get(&self.state, "handlers"),
            "topics" => get(&self.state, "topics"),
            "parent.apiProductType" => s("umcbl"),
            "INTERVAL" => o(vec![("1m", s("1m"))]),
            "BitgetExchange.POSITIONING_CACHE_TTL_MS" => V::N(5000.),
            "BILL_MAX_WINDOW_MS" => V::N(2592000000.),
            "BILL_PAGE_LIMIT" => V::N(100.),
            "LogSeverity.Warning" => s("warning"),
            "OrderType.StopLoss" => s("stop_market"),
            "OrderType.TakeProfit" => s("take_profit_market"),
            "ENDPOINTS.ACCOUNT_SETTINGS_V3" => s("/api/v3/account/settings"),
            _ => panic!("RUN receiver {n}"),
        })
    }
    fn set_receiver(&self, n: &str, v: V) -> Result<(), E> {
        let key = match n {
            "_suppressBitgetValidationErrorEmission" => "suppression",
            "_accountTypeDetected" => "detected",
            "isUta" => "uta",
            "_accountTypeDetectionInflight" => "detection",
            _ => panic!("RUN receiver write"),
        };
        self.record(&format!("set:{n}"), vec![v.clone()]);
        if self.scene == "finally-error"
            && n == "_accountTypeDetectionInflight"
            && matches!(v, V::Null)
        {
            return Err(self.original.clone());
        }
        put(&self.state, units(&s(key)), v);
        Ok(())
    }
    fn capture(&self, n: &str) -> Result<V, E> {
        if ![
            "getBitgetApiErrorMessage",
            "isBitgetEnvironmentMismatch",
            "isBitgetSuccessCode",
            "Error",
            "Math.min",
            "Date.now",
            "loop",
            "paginateIncomeRange",
            "Array.isArray",
            "parseInt",
            "parseFloat",
            "JSON.stringify",
            "Object.keys",
            "setTimeout",
            "clearTimeout",
            "updateOrderBook",
            "calcOrderBookTotal",
        ]
        .contains(&n)
        {
            self.record(&format!("capture:{n}"), vec![]);
        }
        Ok(method(n, V::U))
    }
    fn capture_member(&self, v: &V, n: &str) -> Result<V, E> {
        Ok(method(n, v.clone()))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        if let V::Fn(cb) = f {
            return cb(args);
        }
        let name = text(&get(&f, "path"));
        let owner = get(&f, "owner");
        if ![
            "getBitgetApiErrorMessage",
            "isBitgetEnvironmentMismatch",
            "isBitgetSuccessCode",
            "Error",
            "Math.min",
            "Array.isArray",
            "parseInt",
            "parseFloat",
            "JSON.stringify",
            "Object.keys",
            "push",
            "endsWith",
            "toUpperCase",
            "finally",
            "updateOrderBook",
            "calcOrderBookTotal",
        ]
        .contains(&name.as_str())
        {
            self.record(&format!("call:{name}"), args.clone());
        }
        self.disposed_after(&name);
        Ok(match name.as_str() {
            "getBitgetApiErrorMessage" => {
                let msg = get(&args[0], "msg");
                if truth(&msg) { msg } else { args[1].clone() }
            }
            "isBitgetEnvironmentMismatch" => V::Bool(
                text(&args[0])
                    .to_lowercase()
                    .contains("environment is incorrect"),
            ),
            "isBitgetSuccessCode" => V::Bool(!truth(&args[0]) || num(&args[0]) == 0.),
            "Math.min" => V::N(num(&args[0]).min(num(&args[1]))),
            "Date.now" => V::N(1000.),
            "Array.isArray" => V::Bool(matches!(args[0], V::A(_))),
            "parseInt" | "parseFloat" => V::N(num(&args[0])),
            "endsWith" => V::Bool(text(&owner).ends_with(&text(&args[0]))),
            "toUpperCase" => s(&text(&owner).to_uppercase()),
            "Object.keys" => a(keys(&args[0]).into_iter().map(V::S).collect()),
            "JSON.stringify" => {
                self.packets.borrow_mut().push(args[0].clone());
                s(&json_string(&args[0]))
            }
            "push" => {
                let V::A(a) = owner else {
                    panic!("RUN push owner")
                };
                a.borrow_mut().extend(args);
                V::N(a.borrow().len() as f64)
            }
            "finally" => {
                let host = self.clone();
                let cb = args[0].clone();
                self.launch(Box::pin(async move {
                    let result = host.await_value(owner).await;
                    host.invoke(cb, vec![])?;
                    result
                }))?
            }
            "initExternalPublicData" => V::Bool(self.scene.starts_with("external")),
            "ensureAccountTypeDetected" => {
                if self.scene == "detect-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            "fetchBalance" => {
                if [
                    "balance-error",
                    "environment-live",
                    "environment-demo",
                    "empty-message",
                ]
                .contains(&self.scene)
                {
                    return Err(self.original.clone());
                }
                self.balance.clone()
            }
            "fetchMarkets" => {
                if self.scene == "fetch-error" {
                    return Err(self.original.clone());
                }
                self.markets.clone()
            }
            "fetchTickers" => self.tickers.clone(),
            "fetchOrders" => self.orders.clone(),
            "fetchPositions" => {
                if self.scene == "fetch-error" || self.scene == "emit-error" {
                    return Err(self.original.clone());
                }
                self.positions.clone()
            }
            "fetchNormalOrders" => self.normal.clone(),
            "store.addOrUpdatePositionsFull" => V::U,
            "store.update" => {
                let v = &args[0];
                if !matches!(get(v, "loaded"), V::U) {
                    put(&self.state, units(&s("loaded")), get(v, "loaded"));
                }
                V::U
            }
            "log" => {
                if self.scene == "log-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            "emitAndLogError" => {
                if self.scene == "emit-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            "tick" => V::U,
            "getTickInterval" => V::N(250.),
            "loop" | "setTimeout" => {
                self.callbacks.borrow_mut().push(args[0].clone());
                V::N(1.)
            }
            "clearTimeout" => V::U,
            "xhr.get" => {
                if [
                    "classic-error",
                    "auth-error",
                    "network",
                    "environment-error",
                    "log-error",
                ]
                .contains(&self.scene)
                {
                    return Err(self.original.clone());
                }
                o(vec![(
                    "data",
                    o(vec![
                        (
                            "code",
                            s(if self.scene == "classic" {
                                "40084"
                            } else if self.scene == "api-error" {
                                "40001"
                            } else {
                                "00000"
                            }),
                        ),
                        ("msg", s("probe failure")),
                    ]),
                )])
            }
            "_detectAccountType" => promise(if self.scene == "probe-error" {
                Err(self.original.clone())
            } else {
                Ok(V::U)
            }),
            "getOrFetchPositionsForSymbol" => {
                if self.scene == "positions-error" {
                    return Err(self.original.clone());
                }
                self.positions.clone()
            }
            "fetchPositionMetadata" => {
                if self.scene == "reject" {
                    return Err(self.original.clone());
                }
                self.metadata.clone()
            }
            "paginateIncomeRange" => {
                let request = o(vec![
                    ("startTime", V::N(0.)),
                    ("endTime", V::N(1000.)),
                    ("limit", V::N(100.)),
                ]);
                self.invoke(args[0].clone(), vec![request])?;
                o(vec![
                    ("records", self.records.clone()),
                    (
                        "truncated",
                        V::Bool(self.scene == "truncated" || self.scene == "log-error"),
                    ),
                    ("exhaustedChunks", V::N(2.)),
                ])
            }
            "_fetchBillPage" => {
                if self.scene == "page-error" {
                    return Err(self.original.clone());
                }
                o(vec![("rows", a(vec![]))])
            }
            "send" => {
                if self.scene == "send-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            "cleanupMessageHandlerStats" => V::U,
            "updateOrderBook" => {
                let arr = get(&args[0], &text(&args[1]));
                let V::A(a) = arr else {
                    panic!("RUN native levels")
                };
                let mut levels = a.borrow_mut();
                let price = num(&get(&args[2], "price"));
                let amount = num(&get(&args[2], "amount"));
                if let Some(i) = levels.iter().position(|v| num(&get(v, "price")) == price) {
                    if amount == 0. {
                        levels.remove(i);
                    } else {
                        put(&levels[i], units(&s("amount")), V::N(amount));
                    }
                } else if amount != 0. {
                    levels.push(args[2].clone());
                    levels.sort_by(|a, b| {
                        let cmp = num(&get(a, "price"))
                            .partial_cmp(&num(&get(b, "price")))
                            .unwrap();
                        if text(&args[1]) == "bids" {
                            cmp.reverse()
                        } else {
                            cmp
                        }
                    });
                }
                V::U
            }
            "calcOrderBookTotal" => {
                for side in ["bids", "asks"] {
                    let mut sum = 0.;
                    for v in values(&get(&args[0], side)) {
                        sum += num(&get(&v, "amount"));
                        put(&v, units(&s("total")), V::N(sum));
                    }
                }
                V::U
            }
            _ if name.ends_with("connectAndSubscribe") => V::U,
            _ => panic!("RUN call {name}"),
        })
    }
    fn start(&self, f: V, args: Vec<V>) -> Result<RestActionFuture<'_, V, E>, E> {
        Ok(Box::pin(YieldOnce {
            result: Some(unwrap_promise(self.invoke(f, args)?)),
            yielded: false,
        }))
    }
    fn property(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::Null | V::U) {
            return Err(E {
                identity: Rc::new(()),
                value: o(vec![(
                    "message",
                    s(&format!(
                        "Cannot read properties of {} (reading '{k}')",
                        text(v)
                    )),
                )]),
            });
        }
        if let V::A(arr) = v {
            if k == "length" {
                return Ok(V::N(arr.borrow().len() as f64));
            }
            if k == "map" || k == "forEach" {
                return Ok(method(k, V::U));
            }
        }
        Ok(get(v, k))
    }
    fn key_get(&self, v: &V, k: &V) -> Result<V, E> {
        Ok(match v {
            V::A(v) => v.borrow().get(num(k) as usize).cloned().unwrap_or(V::U),
            _ => get(v, &text(k)),
        })
    }
    fn has(&self, v: &V, k: &str) -> Result<bool, E> {
        Ok(keys(v).contains(&units(&s(k))))
    }
    fn assign(&self, v: &V, k: &str, value: V) -> Result<(), E> {
        put(v, units(&s(k)), value);
        Ok(())
    }
    fn define(&self, v: &V, k: &str, value: V) -> Result<(), E> {
        put(v, units(&s(k)), value);
        Ok(())
    }
    fn delete(&self, v: &V, k: &V) -> Result<(), E> {
        let V::O(o) = v else {
            panic!("RUN delete object")
        };
        o.borrow_mut().retain(|(key, _)| *key != units(k));
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Null | V::U)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn string(&self, s: &[u16]) -> V {
        V::S(s.to_vec())
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn null(&self) -> V {
        V::Null
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, f: Vec<(&str, V)>) -> V {
        o(f)
    }
    fn filter<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "filter");
        let mut out = vec![];
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v.clone())? {
                out.push(v)
            }
        }
        Ok(a(out))
    }
    fn find<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "find");
        for v in values(&array) {
            if f(v.clone())? {
                return Ok(v);
            }
        }
        Ok(V::U)
    }
    fn some<F>(&self, m: V, array: V, mut f: F) -> Result<bool, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "some");
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn operation(&self, n: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match n {
            "add" => V::N(num(&args[0]) + num(&args[1])),
            "subtract" => V::N(num(&args[0]) - num(&args[1])),
            "divide" => V::N(num(&args[0]) / num(&args[1])),
            "remainder" => V::N(num(&args[0]) % num(&args[1])),
            "less" => V::Bool(num(&args[0]) < num(&args[1])),
            "greater" => V::Bool(num(&args[0]) > num(&args[1])),
            "lessEqual" => V::Bool(num(&args[0]) <= num(&args[1])),
            "increment" => V::N(num(&args[0]) + 1.),
            "template" => s(&text(&args[0])),
            "typeofObject" => V::Bool(matches!(args[0], V::O(_) | V::A(_))),
            "objectSpread" => {
                let out = o(vec![]);
                for k in keys(&args[0]) {
                    put(&out, k.clone(), key(&args[0], &k));
                }
                out
            }
            "arrayAppendOwn" => {
                let V::A(v) = &args[0] else {
                    panic!("RUN array")
                };
                v.borrow_mut().push(args[1].clone());
                V::U
            }
            "environmentMessage" => s(&format!(
                "API key environment mismatch: Your API key appears to be for {} trading, but Tealstreet is configured for {}. Please check your Testnet setting.",
                text(&args[0]),
                text(&args[1])
            )),
            "marketsLoadedMessage" => s(&format!("Loaded {} Bitget markets", text(&args[0]))),
            "probeFallbackMessage" => s(&format!(
                "Bitget account type probe failed; assuming Classic: {}",
                text(&args[0])
            )),
            "newDateISO" => s(if num(&args[0]) == 0. {
                "1970-01-01T00:00:00.000Z"
            } else {
                "1970-01-01T00:00:01.000Z"
            }),
            "incomeWarning" => s(&format!(
                "Income history for {}..{} is INCOMPLETE — {} chunk(s) could not be walked to completion. Any total derived from it is a lower bound.",
                text(&args[0]),
                text(&args[1]),
                text(&args[2])
            )),
            "bookTopic" => s(&format!("orderBook_{}", text(&args[0]))),
            "candleTopic" => s(&format!("candle_{}_{}", text(&args[0]), text(&args[1]))),
            "candleChannel" => s(&format!("candle{}", text(&args[0]))),
            _ => panic!("RUN op {n}"),
        })
    }
    fn new_error(&self, f: V, msg: V) -> Result<E, E> {
        native(&get(&f, "path"), &s("Error"), "error constructor selector");
        Ok(E {
            identity: Rc::new(()),
            value: o(vec![("message", msg)]),
        })
    }
    fn error_value(&self, e: &E) -> V {
        e.value.clone()
    }
}
struct Iter {
    array: V,
    index: usize,
}
impl OrderPlacementHost for H {
    type Iterator = Iter;
    fn spread_into(&self, array: &V, elements: V) -> Result<(), E> {
        let mut i = self.iterator(&elements)?;
        let V::A(out) = array else {
            panic!("native array literal")
        };
        while let Some(v) = self.next(&mut i)? {
            out.borrow_mut().push(v)
        }
        Ok(())
    }
    fn iterator(&self, value: &V) -> Result<Iter, E> {
        if !matches!(value, V::A(_)) {
            return Err(E {
                identity: Rc::new(()),
                value: o(vec![("message", s("(intermediate value) is not iterable"))]),
            });
        }
        Ok(Iter {
            array: value.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        let V::A(v) = &i.array else {
            panic!("native iterator")
        };
        let v = v.borrow();
        let out = v.get(i.index).cloned();
        i.index += 1;
        Ok(out.map(|v| if matches!(v, V::Hole) { V::U } else { v }))
    }
    fn pair(&self, v: V) -> Result<(V, V), E> {
        let values = values(&v);
        Ok((
            values.first().cloned().unwrap_or(V::U),
            values.get(1).cloned().unwrap_or(V::U),
        ))
    }
    fn map<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<V, E>,
    {
        binding(&m, &array, "map");
        let mut out = vec![];
        for v in values(&array) {
            out.push(if matches!(v, V::Hole) { V::Hole } else { f(v)? })
        }
        Ok(a(out))
    }
    fn reduce<F>(&self, m: V, array: V, mut initial: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, V) -> Result<V, E>,
    {
        binding(&m, &array, "reduce");
        for v in values(&array) {
            if !matches!(v, V::Hole) {
                initial = f(initial, v)?
            }
        }
        Ok(initial)
    }
    fn for_each<F>(&self, m: V, array: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(V) -> Result<(), E>,
    {
        binding(&m, &array, "forEach");
        for v in values(&array) {
            if !matches!(v, V::Hole) {
                f(v)?
            }
        }
        Ok(())
    }
    fn assign_key(&self, v: &V, k: &V, value: V) -> Result<(), E> {
        put(v, units(k), value);
        Ok(())
    }
}
impl OrderPreparationHost for H {
    fn times<F>(&self, _m: V, _count: V, _f: F) -> Result<V, E>
    where
        F: FnMut() -> Result<V, E>,
    {
        panic!("RUN times not used")
    }
    fn launch(&self, mut body: RestActionFuture<'static, V, E>) -> Result<V, E> {
        let w = Waker::noop().clone();
        let mut cx = Context::from_waker(&w);
        Ok(match body.as_mut().poll(&mut cx) {
            Poll::Ready(r) => promise(r),
            Poll::Pending => V::Task(Rc::new(RefCell::new(Some(body)))),
        })
    }
    fn await_value(&self, v: V) -> RestActionFuture<'_, V, E> {
        Box::pin(async move {
            match v {
                V::Task(p) => {
                    let f = p.borrow_mut().take().unwrap();
                    f.await
                }
                v => {
                    YieldOnce {
                        result: Some(unwrap_promise(v)),
                        yielded: false,
                    }
                    .await
                }
            }
        })
    }
}
impl RuntimeOrchestrationHost for H {
    fn first(&self, v: V) -> Result<V, E> {
        Ok(values(&v).first().cloned().unwrap_or(V::U))
    }
    fn function<F>(&self, f: F) -> V
    where
        F: Fn(Vec<V>) -> Result<V, E> + 'static,
    {
        V::Fn(Rc::new(f))
    }
}
fn plain(v: &V) -> Json {
    match v {
        V::S(s) => json!(String::from_utf16_lossy(s)),
        V::N(n) => json!(n),
        V::Bool(b) => json!(b),
        V::Null => Json::Null,
        V::A(a) => json!(a.borrow().iter().map(plain).collect::<Vec<_>>()),
        V::O(_) => {
            let mut o = serde_json::Map::new();
            for k in keys(v) {
                o.insert(String::from_utf16_lossy(&k), plain(&key(v, &k)));
            }
            Json::Object(o)
        }
        _ => panic!("RUN JSON fixture primitive"),
    }
}
fn count(h: &H, n: &str, k: usize) {
    assert_eq!(
        h.calls(n).len(),
        k,
        "RUN native independent call count {}:{}:{n}",
        h.op,
        h.scene
    )
}
fn ok(r: &Result<V, E>) -> &V {
    r.as_ref().expect("RUN expected native success")
}
fn err(h: &H, r: &Result<V, E>, original: bool, msg: &str) {
    let e = r.as_ref().expect_err("RUN expected native failure");
    assert_eq!(
        Rc::ptr_eq(&e.identity, &h.original.identity),
        original,
        "RUN native original error identity"
    );
    native(&get(&e.value, "message"), &s(msg), "error message");
}
fn execute(h: &H) -> Result<V, E> {
    run(async {
        match h.op {
            "validate" => body::validate_account(h).await,
            "start" => body::start(h).await,
            "tick" => {
                body::tick(
                    h,
                    match h.scene {
                        "one" => V::N(1.),
                        "two" => V::N(2.),
                        "four" => V::N(4.),
                        _ => V::U,
                    },
                )
                .await
            }
            "detect" => body::detect_account_type(h).await,
            "ensure" => body::ensure_account_type_detected(h).await,
            "resolve" => body::resolve_symbol_positioning(h, s("BTCUSD")).await,
            "income" => {
                body::fetch_income_history_range(
                    h,
                    V::N(0.),
                    V::N(1000.),
                    if h.scene == "filter" {
                        s("funding")
                    } else if h.scene == "none" {
                        s("absent")
                    } else if h.scene == "empty-type" {
                        s("")
                    } else {
                        V::U
                    },
                )
                .await
            }
            "candle" => body::listen_ohlcv(
                h,
                o(vec![("symbol", s("BTCUSD")), ("interval", s("1m"))]),
                h.callback.clone(),
            ),
            "book" => body::listen_order_book(h, s("BTCUSD"), h.callback.clone()),
            _ => panic!("RUN op"),
        }
    })
}
fn json_string(v: &V) -> String {
    match v {
        V::O(_) => format!(
            "{{{}}}",
            keys(v)
                .iter()
                .map(|k| format!(
                    "{}:{}",
                    serde_json::to_string(&String::from_utf16_lossy(k)).unwrap(),
                    json_string(&key(v, k))
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::A(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(json_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        v => plain(v).to_string(),
    }
}
fn check(h: &H, r: &Result<V, E>) {
    match h.op {
        "validate" => {
            let expected = match h.scene {
                "ok" => o(vec![("error", s("")), ("balance", h.balance.clone())]),
                "environment-live" | "environment-demo" => o(vec![(
                    "error",
                    s(&format!(
                        "API key environment mismatch: Your API key appears to be for {} trading, but Tealstreet is configured for {}. Please check your Testnet setting.",
                        if h.scene == "environment-demo" {
                            "Live/Production"
                        } else {
                            "Demo/Testnet"
                        },
                        if h.scene == "environment-demo" {
                            "Demo/Testnet"
                        } else {
                            "Live/Production"
                        }
                    )),
                )]),
                "empty-message" => o(vec![("error", s("Invalid API key or secret"))]),
                _ => o(vec![("error", s("original"))]),
            };
            native(ok(r), &expected, "validation native result");
            native(
                &get(&h.state, "suppression"),
                &V::N(7.),
                "validation suppression restored exactly",
            );
            if h.scene == "ok" {
                alias(
                    &get(ok(r), "balance"),
                    &h.balance,
                    "validation fetched balance alias",
                );
            }
            count(h, "fetchBalance", usize::from(h.scene != "detect-error"));
        }
        "start" => {
            if h.scene == "fetch-error" {
                err(h, r, true, "original");
                count(h, "store.update", 0);
                return;
            }
            native(ok(r), &V::U, "startup return");
            let external = h.scene.starts_with("external");
            let public = h.scene == "public" || h.scene == "external-public";
            let updates = if h.scene == "disposed-markets" {
                0
            } else if h.scene == "disposed-tickers" {
                1
            } else if ["disposed-detection", "disposed-tick", "disposed-orders"].contains(&h.scene)
            {
                2
            } else if external {
                1
            } else {
                3
            };
            count(h, "store.update", updates);
            count(h, "fetchMarkets", usize::from(!external));
            count(
                h,
                "fetchTickers",
                usize::from(!external && h.scene != "disposed-markets"),
            );
            if updates > 0 && !external {
                alias(
                    &get(&h.calls("store.update")[0][0], "markets"),
                    &h.markets,
                    "startup market array alias",
                );
            }
            let flags = get(&h.state, "loaded");
            native(
                &get(&flags, "markets"),
                &V::Bool(!external && h.scene != "disposed-markets"),
                "startup market loaded flag",
            );
            native(
                &get(&flags, "tickers"),
                &V::Bool(!external && !matches!(h.scene, "disposed-markets" | "disposed-tickers")),
                "startup ticker loaded flag",
            );
            native(
                &get(&flags, "orders"),
                &V::Bool(public || !h.scene.starts_with("disposed")),
                "startup orders loaded flag",
            );
            count(
                h,
                "privateWebsocket.connectAndSubscribe",
                usize::from(
                    !public
                        && !matches!(
                            h.scene,
                            "disposed-markets" | "disposed-tickers" | "disposed-detection"
                        ),
                ),
            );
        }
        "tick" => {
            if h.scene == "emit-error" {
                err(h, r, true, "original");
                count(h, "loop", 0);
                return;
            }
            native(ok(r), &V::U, "tick return");
            let stopped = h.scene == "public" || h.scene.starts_with("disposed");
            count(h, "loop", usize::from(!stopped));
            if h.scene == "fetch-error" {
                count(h, "emitAndLogError", 1);
                count(h, "store.update", 0);
            } else if !stopped {
                count(h, "store.update", 1);
                let update = h.calls("store.update")[0][0].clone();
                if h.scene != "one" {
                    alias(&get(&update, "balance"), &h.balance, "tick balance alias");
                }
                if h.scene == "zero" || h.scene == "four" {
                    let rows = values(&get(&update, "orders"));
                    assert_eq!(
                        rows.len(),
                        2,
                        "RUN native independent reconciled order count"
                    );
                    alias(
                        &rows[0],
                        &values(&get(&h.state, "storeOrders"))[0],
                        "retained nonvirtual row",
                    );
                    alias(&rows[1], &values(&h.normal)[2], "fetched virtual row");
                }
                native(
                    &get(&get(&update, "loaded"), "positions"),
                    &V::Bool(true),
                    "tick position loaded flag",
                );
            }
            if !stopped {
                let cb = h.callbacks.borrow()[0].clone();
                h.invoke(cb, vec![]).unwrap();
                native(
                    &h.calls("tick").last().unwrap()[0],
                    &V::N(if h.scene == "one" {
                        2.
                    } else if h.scene == "two" {
                        3.
                    } else if h.scene == "four" {
                        5.
                    } else {
                        1.
                    }),
                    "live tick callback increment",
                );
            }
        }
        "detect" => {
            native(
                &h.calls("xhr.get")[0][0],
                &s("/api/v3/account/settings"),
                "native probe endpoint",
            );
            let fail =
                ["api-error", "auth-error", "environment-error", "log-error"].contains(&h.scene);
            if fail {
                err(
                    h,
                    r,
                    h.scene != "api-error",
                    if h.scene == "api-error" {
                        "probe failure"
                    } else if h.scene == "environment-error" {
                        "environment is incorrect"
                    } else {
                        "original"
                    },
                );
            } else {
                native(ok(r), &V::U, "probe return");
            }
            native(
                &get(&h.state, "detected"),
                &V::Bool(!fail),
                "probe finally detected flag",
            );
            native(
                &get(&h.state, "uta"),
                &V::Bool(h.scene == "uta"),
                "probe native UTA flag",
            );
            count(
                h,
                "log",
                usize::from(h.scene == "network" || h.scene == "log-error"),
            );
        }
        "ensure" => {
            if h.scene == "probe-error" || h.scene == "finally-error" {
                err(h, r, true, "original");
            } else {
                native(ok(r), &V::U, "ensure return");
            }
            count(
                h,
                "_detectAccountType",
                usize::from(!["public", "detected", "existing"].contains(&h.scene)),
            );
            if h.scene == "existing" {
                alias(
                    &get(&h.state, "detection"),
                    &h.flight,
                    "existing inflight retained",
                );
            } else if h.scene != "finally-error" {
                native(
                    &get(&h.state, "detection"),
                    &V::Null,
                    "detection finally clears inflight",
                );
            }
        }
        "resolve" => {
            if h.scene == "reject" || h.scene == "positions-error" {
                err(h, r, true, "original");
            } else {
                native(
                    ok(r),
                    &o(vec![
                        (
                            "positionMode",
                            s(if ["position", "cache", "existing"].contains(&h.scene) {
                                "hedged"
                            } else {
                                "oneway"
                            }),
                        ),
                        (
                            "marginType",
                            if h.scene == "partial" {
                                V::U
                            } else {
                                s(if ["position", "cache", "existing"].contains(&h.scene) {
                                    "cross"
                                } else {
                                    "isolated"
                                })
                            },
                        ),
                    ]),
                    "native symbol positioning",
                );
                if h.scene == "cache" {
                    alias(
                        ok(r),
                        &get(&get(&get(&h.state, "cache"), "BTCUSD"), "value"),
                        "cached positioning alias",
                    );
                }
                if h.scene == "metadata" || h.scene == "expired" {
                    let cache = get(&get(&h.state, "cache"), "BTCUSD");
                    native(&get(&cache, "at"), &V::N(1000.), "positioning cache clock");
                    alias(&get(&cache, "value"), ok(r), "cache result alias");
                }
            }
            native(
                &get(&get(&h.state, "inflight"), "KEEP"),
                &s("flight-keep"),
                "unrelated inflight survives",
            );
            if h.scene == "existing" {
                alias(
                    &get(&get(&h.state, "inflight"), "BTCUSD"),
                    &h.flight,
                    "existing symbol promise retained",
                );
            } else {
                native(
                    &get(&get(&h.state, "inflight"), "BTCUSD"),
                    &V::U,
                    "own inflight finally deleted",
                );
            }
            count(
                h,
                "fetchPositionMetadata",
                usize::from(["metadata", "partial", "expired", "reject"].contains(&h.scene)),
            );
        }
        "income" => {
            if h.scene == "page-error" || h.scene == "log-error" {
                err(h, r, true, "original");
                return;
            }
            let expected = if h.scene == "filter" {
                a(vec![
                    values(&h.records)[0].clone(),
                    values(&h.records)[2].clone(),
                ])
            } else if h.scene == "none" {
                a(vec![])
            } else {
                h.records.clone()
            };
            native(ok(r), &expected, "income independent selected rows");
            if h.scene == "filter" {
                let v = values(ok(r));
                alias(
                    &v[0],
                    &values(&h.records)[0],
                    "filtered original income row",
                );
                alias(&v[1], &values(&h.records)[2], "second filtered income row");
            } else if h.scene != "none" {
                alias(ok(r), &h.records, "unfiltered income records alias");
            }
            native(
                &h.calls("_fetchBillPage")[0][0],
                &o(vec![
                    ("startTime", V::N(0.)),
                    ("endTime", V::N(1000.)),
                    ("limit", V::N(100.)),
                ]),
                "income native page callback arguments",
            );
            count(h, "log", usize::from(h.scene == "truncated"));
            if h.scene == "truncated" {
                native(
                    &h.calls("log")[0][0],
                    &s(
                        "Income history for 1970-01-01T00:00:00.000Z..1970-01-01T00:00:01.000Z is INCOMPLETE — 2 chunk(s) could not be walked to completion. Any total derived from it is a lower bound.",
                    ),
                    "native truncation warning",
                );
                native(
                    &h.calls("log")[0][1],
                    &s("warning"),
                    "native warning severity",
                );
            }
        }
        "candle" | "book" => check_listen(h, r),
        _ => panic!("RUN check"),
    }
}
fn frame(book: bool, update: bool) -> V {
    if book {
        o(vec![
            ("action", s(if update { "update" } else { "snapshot" })),
            (
                "data",
                a(vec![o(vec![
                    (
                        "bids",
                        a(vec![a(vec![s("100"), s(if update { "0" } else { "2" })])]),
                    ),
                    (
                        "asks",
                        a(vec![a(vec![s("101"), s(if update { "3" } else { "4" })])]),
                    ),
                ])]),
            ),
        ])
    } else {
        o(vec![(
            "data",
            a(vec![a(vec![
                s("1700000000000"),
                s("1"),
                s("3"),
                s("0.5"),
                s("2"),
                s("999"),
                s("6"),
            ])]),
        )])
    }
}
fn check_listen(h: &H, r: &Result<V, E>) {
    let book = h.op == "book";
    let topic = if book {
        "orderBook_BTCUSD"
    } else {
        "candle_BTCUSD_1m"
    };
    let channel = if book { "books" } else { "candle1m" };
    if h.scene == "send-error" {
        native_packet(h, "subscribe", 0);
        assert_eq!(
            h.packets.borrow().len(),
            1,
            "RUN native independent failed-send packet count"
        );
        err(h, r, true, "original");
        native(
            &get(&get(&get(&h.state, "topics"), channel), "BTCUSD"),
            &V::N(1.),
            "subscribed count installed before send failure",
        );
        return;
    }
    let cleanup = ok(r).clone();
    assert!(matches!(cleanup, V::Fn(_)), "RUN native cleanup callable");
    if h.scene == "delayed" {
        count(h, "setTimeout", 1);
        native(
            &h.calls("setTimeout")[0][1],
            &V::N(100.),
            "native polling delay",
        );
        put(&h.state, units(&s("connected")), V::Bool(true));
        h.invoke(h.callbacks.borrow()[0].clone(), vec![]).unwrap();
    }
    if !["early", "disposed"].contains(&h.scene) {
        native_packet(h, "subscribe", 0);
        assert!(
            matches!(h.calls("send")[0][0], V::S(_)),
            "RUN native send argument string datatype"
        );
        let handlers = values(&get(&get(&h.state, "handlers"), topic));
        assert_eq!(
            handlers.len(),
            if h.scene == "shared" { 2 } else { 1 },
            "RUN native independent handler count"
        );
        let parser = handlers.last().unwrap().clone();
        native(
            &get(&get(&get(&h.state, "topics"), channel), "BTCUSD"),
            &V::N(if h.scene == "shared" { 2. } else { 1. }),
            "native subscribed topic count",
        );
        let data = if h.scene == "invalid" {
            o(vec![("data", a(vec![V::N(3.)]))])
        } else {
            frame(book, false)
        };
        h.invoke(parser.clone(), vec![data]).unwrap();
        if h.scene == "invalid" {
            assert_eq!(
                h.events
                    .borrow()
                    .iter()
                    .filter(|e| e.name == "callback")
                    .count(),
                0,
                "RUN native invalid frame no callback"
            );
        } else {
            let values0 = h
                .events
                .borrow()
                .iter()
                .filter(|e| e.name == "callback")
                .map(|e| e.args[0].clone())
                .collect::<Vec<_>>();
            assert_eq!(values0.len(), 1, "RUN native independent callback count");
            if book {
                native(
                    &values0[0],
                    &o(vec![
                        (
                            "bids",
                            a(vec![o(vec![
                                ("price", V::N(100.)),
                                ("amount", V::N(2.)),
                                ("total", V::N(2.)),
                            ])]),
                        ),
                        (
                            "asks",
                            a(vec![o(vec![
                                ("price", V::N(101.)),
                                ("amount", V::N(4.)),
                                ("total", V::N(4.)),
                            ])]),
                        ),
                    ]),
                    "native snapshot levels",
                );
                if h.scene == "update" {
                    h.invoke(parser, vec![frame(true, true)]).unwrap();
                    let v = h
                        .events
                        .borrow()
                        .iter()
                        .filter(|e| e.name == "callback")
                        .map(|e| e.args[0].clone())
                        .collect::<Vec<_>>();
                    assert_eq!(v.len(), 2, "RUN native update callback count");
                    alias(&v[0], &v[1], "same orderbook reference");
                    native(
                        &v[1],
                        &o(vec![
                            ("bids", a(vec![])),
                            (
                                "asks",
                                a(vec![o(vec![
                                    ("price", V::N(101.)),
                                    ("amount", V::N(3.)),
                                    ("total", V::N(3.)),
                                ])]),
                            ),
                        ]),
                        "native update removes bid changes ask",
                    );
                }
            } else {
                native(
                    &values0[0],
                    &o(vec![
                        ("timestamp", V::N(1700000000.)),
                        ("open", V::N(1.)),
                        ("high", V::N(3.)),
                        ("low", V::N(0.5)),
                        ("close", V::N(2.)),
                        ("volume", V::N(6.)),
                    ]),
                    "native candle columns",
                );
            }
        }
    }
    h.invoke(cleanup.clone(), vec![]).unwrap();
    h.invoke(cleanup, vec![]).unwrap();
    let retained = values(&get(&get(&h.state, "handlers"), topic));
    assert_eq!(
        retained.len(),
        usize::from(h.scene == "shared"),
        "RUN native teardown retains empty handler key"
    );
    if h.scene == "shared" {
        alias(
            &retained[0],
            &h.old_handler,
            "other subscriber handler retained",
        );
        native(
            &get(&get(&get(&h.state, "topics"), channel), "BTCUSD"),
            &V::N(1.),
            "other subscriber count retained",
        );
    } else {
        native(
            &get(&get(&h.state, "topics"), channel),
            &V::U,
            "last subscriber channel deleted",
        );
    }
    count(
        h,
        "send",
        if ["early", "disposed"].contains(&h.scene) {
            0
        } else if h.scene == "shared" {
            1
        } else {
            2
        },
    );
    count(
        h,
        "cleanupMessageHandlerStats",
        usize::from(!["early", "disposed", "shared"].contains(&h.scene)),
    );
    if !["early", "disposed", "shared"].contains(&h.scene) {
        native_packet(h, "unsubscribe", 1);
        assert_eq!(
            h.packets.borrow().len(),
            2,
            "RUN native independent packet object count"
        );
        let ev = h.events.borrow();
        let send = ev.iter().rposition(|e| e.name == "call:send").unwrap();
        let stats = ev
            .iter()
            .position(|e| e.name == "call:cleanupMessageHandlerStats")
            .unwrap();
        assert!(
            if book { stats < send } else { send < stats },
            "RUN native unsubscribe cleanup/send order"
        );
    }
    assert_eq!(
        h.packets.borrow().len(),
        if ["early", "disposed"].contains(&h.scene) {
            0
        } else if h.scene == "shared" {
            1
        } else {
            2
        },
        "RUN native independent scene packet count"
    );
    if h.scene == "early" {
        count(h, "clearTimeout", 1);
        put(&h.state, units(&s("connected")), V::Bool(true));
        h.invoke(h.callbacks.borrow()[0].clone(), vec![]).unwrap();
        count(h, "send", 0);
    }
}
fn result_wire(h: &H, r: &Result<V, E>) -> Json {
    match r {
        Ok(v) => json!({"ok":wire(v)}),
        Err(e) => {
            json!({"original":Rc::ptr_eq(&e.identity,&h.original.identity),"message":wire(&get(&e.value,"message"))})
        }
    }
}
fn source(cases: Json, group: &str) -> Json {
    let mut c = Command::new("node")
        .arg(format!(
            "{}/scripts/runtime-orchestration-capture.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(group)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(cases.to_string().as_bytes())
        .unwrap();
    let r = c.wait_with_output().unwrap();
    assert!(
        r.status.success(),
        "RUN source error {}",
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
fn suite(op: &'static str, scenes: &[&'static str], floor: usize) {
    assert_eq!(scenes.len(), floor, "RUN independent native input floor");
    let mut all = vec![];
    for &scene in scenes {
        let h = fixture(op, scene);
        let r = execute(&h);
        check(&h, &r);
        all.push((h, r));
    }
    assert_eq!(all.len(), floor, "RUN independent native result floor");
    let inputs = all
        .iter()
        .map(|(h, _)| json!({"op":op,"scene":h.scene}))
        .collect::<Vec<_>>();
    let actual=all.iter().map(|(h,r)|json!({"result":result_wire(h,r),"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"state":wire(&h.state)})).collect::<Vec<_>>();
    compare(
        &json!(actual),
        &source(json!(inputs), op),
        "nine source runtime bodies",
    );
}
#[test]
fn native_validate_then_source() {
    suite(
        "validate",
        &[
            "ok",
            "balance-error",
            "detect-error",
            "environment-live",
            "environment-demo",
            "empty-message",
        ],
        6,
    )
}
#[test]
fn native_start_then_source() {
    suite(
        "start",
        &[
            "normal",
            "external",
            "public",
            "external-public",
            "disposed-markets",
            "disposed-tickers",
            "disposed-detection",
            "disposed-tick",
            "disposed-orders",
            "fetch-error",
        ],
        10,
    )
}
#[test]
fn native_tick_then_source() {
    suite(
        "tick",
        &[
            "zero",
            "one",
            "two",
            "four",
            "public",
            "disposed",
            "disposed-positions",
            "disposed-balance",
            "fetch-error",
            "emit-error",
        ],
        10,
    )
}
#[test]
fn native_detect_then_source() {
    suite(
        "detect",
        &[
            "uta",
            "classic",
            "classic-error",
            "api-error",
            "auth-error",
            "network",
            "environment-error",
            "log-error",
        ],
        8,
    )
}
#[test]
fn native_ensure_then_source() {
    suite(
        "ensure",
        &[
            "public",
            "detected",
            "existing",
            "probe",
            "probe-error",
            "finally-error",
        ],
        6,
    )
}
#[test]
fn native_resolve_then_source() {
    suite(
        "resolve",
        &[
            "position",
            "cache",
            "expired",
            "existing",
            "metadata",
            "partial",
            "reject",
            "positions-error",
        ],
        8,
    )
}
#[test]
fn native_income_then_source() {
    suite(
        "income",
        &[
            "normal",
            "filter",
            "none",
            "truncated",
            "page-error",
            "empty-type",
            "log-error",
        ],
        7,
    )
}
#[test]
fn native_candle_then_source() {
    suite(
        "candle",
        &[
            "connected",
            "delayed",
            "early",
            "disposed",
            "shared",
            "invalid",
            "send-error",
        ],
        7,
    )
}
#[test]
fn native_book_then_source() {
    suite(
        "book",
        &[
            "connected",
            "delayed",
            "early",
            "disposed",
            "shared",
            "invalid",
            "send-error",
            "update",
        ],
        8,
    )
}

fn native_packet(h: &H, kind: &str, index: usize) {
    let channel = if h.op == "book" { "books" } else { "candle1m" };
    let packets = h.packets.borrow();
    assert!(
        packets.len() > index,
        "RUN native required pre-stringify packet object"
    );
    let packet = &packets[index];
    native(
        packet,
        &o(vec![
            ("op", s(kind)),
            (
                "args",
                a(vec![o(vec![
                    ("instType", s("UMCBL")),
                    ("channel", s(channel)),
                    ("instId", s("BTCUSD")),
                ])]),
            ),
        ]),
        "native packet fields before stringify/source comparison",
    );
}
