use safe_cex_venue_kucoin::{
    bootstrap_reads::{self as port, BootstrapAwait, BootstrapOps},
    public_streams::StreamOps,
};
use serde_json::{Value, json};
use std::future::Future;
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = Rc<Error>;
struct Error {
    name: &'static str,
    message: String,
    fresh: bool,
    code: Option<&'static str>,
    ids: Option<[u8; 2]>,
    response: V,
}
enum Native {
    U,
    B(bool),
    N(f64),
    S(String),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    Call(String),
    P(Result<V, E>),
    Err(E),
    Iter(RefCell<(Vec<V>, usize)>),
    Member(V, String),
    Mapper(Rc<dyn Fn(V) -> Result<V, E>>),
    AsyncMapper(Rc<dyn Fn(V) -> BootstrapAwait<V, E>>),
    Callback(Rc<dyn Fn() -> Result<V, E>>),
    Date(f64),
}
fn u() -> V {
    Rc::new(Native::U)
}
fn s(x: &str) -> V {
    Rc::new(Native::S(x.into()))
}
fn n(x: f64) -> V {
    Rc::new(Native::N(x))
}
fn b(x: bool) -> V {
    Rc::new(Native::B(x))
}
fn array(x: Vec<V>) -> V {
    Rc::new(Native::A(RefCell::new(x)))
}
fn object(x: Vec<(&str, V)>) -> V {
    Rc::new(Native::O(RefCell::new(
        x.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn get(v: &V, k: &str) -> V {
    match v.as_ref() {
        Native::O(x) => x
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(x) => {
            if k == "length" {
                n(x.borrow().len() as f64)
            } else {
                k.parse::<usize>()
                    .ok()
                    .and_then(|i| x.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        Native::Err(e) => match k {
            "message" => s(&e.message),
            "response" => e.response.clone(),
            _ => u(),
        },
        _ => u(),
    }
}
fn put(v: &V, k: &str, value: V) {
    let Native::O(x) = v.as_ref() else {
        panic!("object set")
    };
    let mut x = x.borrow_mut();
    if let Some((_, v)) = x.iter_mut().find(|(key, _)| key == k) {
        *v = value
    } else {
        x.push((k.into(), value))
    }
}
fn items(v: &V) -> Vec<V> {
    let Native::A(x) = v.as_ref() else {
        panic!("array")
    };
    x.borrow().clone()
}
fn text(v: &V) -> String {
    match v.as_ref() {
        Native::S(s) => s.clone(),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::U => "undefined".into(),
        Native::B(x) => x.to_string(),
        _ => panic!("primitive"),
    }
}
fn num(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::S(x) => safe_cex_num::number_from_string(x),
        Native::U => f64::NAN,
        _ => panic!("number"),
    }
}
fn truth(v: &V) -> bool {
    match v.as_ref() {
        Native::U => false,
        Native::B(x) => *x,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn null(v: &V) -> bool {
    matches!(v.as_ref(), Native::U)
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) => true,
        (Native::S(a), Native::S(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::B(a), Native::B(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::U => json!({"u":true}),
        Native::B(x) => json!(x),
        Native::S(x) => json!(x),
        Native::N(x) => {
            json!({"n":if x.is_nan(){"nan".into()}else{format!("{:016x}",x.to_bits())}})
        }
        Native::O(x) => {
            json!({"o":x.borrow().iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()})
        }
        Native::A(x) => json!(x.borrow().iter().map(pack).collect::<Vec<_>>()),
        Native::Call(x) => json!({"call":x}),
        Native::Err(e) => {
            if e.fresh {
                json!({"o":[]})
            } else {
                pack(&object(vec![
                    ("code", s(e.code.unwrap())),
                    (
                        "ids",
                        array(e.ids.unwrap().into_iter().map(|x| n(x as f64)).collect()),
                    ),
                    ("response", e.response.clone()),
                ]))
            }
        }
        _ => panic!("pack"),
    }
}
fn unused<T>() -> T {
    panic!("unused capability")
}
fn promise(v: Result<V, E>) -> V {
    Rc::new(Native::P(v))
}
fn await_ready(v: V) -> BootstrapAwait<V, E> {
    Box::pin(async move {
        if let Native::P(r) = v.as_ref() {
            r.clone()
        } else {
            Ok(v)
        }
    })
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready fixture"),
    }
}
struct State {
    method: &'static str,
    case: &'static str,
    err: E,
    receiver: V,
    store: V,
    markets: V,
    tickers: V,
    orders: V,
    balance: V,
    positions: V,
    raw: V,
    symbols: V,
    opts: V,
    metadata: V,
    sockets: V,
    connections: Vec<V>,
    events: RefCell<Vec<Vec<V>>>,
    captures: RefCell<Vec<String>>,
    timer: RefCell<Option<V>>,
    timer_return: RefCell<Option<V>>,
    disposed: Cell<bool>,
    dispose_reads: Cell<usize>,
    after_positions: Cell<bool>,
    requests: Cell<usize>,
    timer_started: Cell<bool>,
    closes: Cell<usize>,
    raw_predicates: RefCell<Vec<V>>,
}
fn event(t: &State, name: &str, args: Vec<V>) {
    let mut row = vec![s(name)];
    row.extend(args);
    t.events.borrow_mut().push(row);
}
fn fixture(method: &'static str, case: &'static str) -> Rc<State> {
    let err = Rc::new(Error {
        name: "TypeError",
        message: if case == "no-message" { "" } else { "DOWN" }.into(),
        fresh: false,
        code: Some("READ"),
        ids: Some([3, 7]),
        response: object(vec![(
            "data",
            object(vec![(
                "msg",
                s(if case == "no-message" { "" } else { "VENUE" }),
            )]),
        )]),
    });
    let markets = array(vec![
        object(vec![("id", s("M0"))]),
        object(vec![("id", s("M1"))]),
    ]);
    let tickers = array(vec![object(vec![("id", s("T0"))])]);
    let orders = array(vec![object(vec![("id", s("O0"))])]);
    let positions = array(vec![
        object(vec![
            ("id", s("REAL")),
            ("symbol", s("SYM")),
            ("contracts", n(2.)),
        ]),
        object(vec![
            ("id", s("OTHER")),
            ("symbol", s("OTHER")),
            ("contracts", n(3.)),
        ]),
    ]);
    let balance = object(vec![("total", n(123.))]);
    let store = object(vec![
        ("loaded", object(vec![("seed", b(true))])),
        ("markets", array(vec![])),
    ]);
    let receiver = object(vec![("snapshotWebsockets", array(vec![]))]);
    let valid = object(vec![
        ("symbol", s("API1")),
        ("currentQty", s(if case == "nan" { "oops" } else { "-4" })),
        ("positionSide", s(if case == "nan" { "" } else { "LONG" })),
        ("markPrice", s("10")),
        ("avgEntryPrice", s("1.5")),
        ("leverage", s("7x")),
        ("unrealisedPnl", s("-2.5")),
        ("liquidationPrice", s("0.8")),
        ("marginMode", s("CROSS")),
        ("positionMode", s("0")),
    ]);
    let raw = match method {
        "open" => {
            if case == "non-array" {
                object(vec![])
            } else {
                array(vec![object(vec![("currentQty", s("0"))]), valid])
            }
        }
        "markets" => {
            let valid = if case == "defaults" {
                object(vec![
                    ("symbol", s("API1")),
                    ("quoteCoin", s("USDT")),
                    ("rootSymbol", s("ETH")),
                    ("status", s("ONLINE")),
                    ("isQuanto", b(true)),
                ])
            } else {
                object(vec![
                    ("symbol", s("API1")),
                    ("settleCurrency", s("USDT")),
                    ("baseCurrency", s("XBT")),
                    ("status", s("Open")),
                    ("lotSize", s("2")),
                    ("tickSize", s("0.5")),
                    ("minOrderQty", s("3")),
                    ("maxOrderQty", s("99")),
                    ("maxLeverage", s("8")),
                    ("multiplier", s("0.1")),
                    ("isQuanto", b(true)),
                ])
            };
            array(vec![
                valid,
                object(vec![
                    ("symbol", s("API2")),
                    ("settleCurrency", s("USDT")),
                    ("baseCurrency", s("ETH")),
                    ("status", s("ONLINE")),
                    ("expireDate", s("FUTURE")),
                ]),
                object(vec![
                    ("symbol", s("API3")),
                    ("settleCurrency", s("USDC")),
                    ("baseCurrency", s("ETH")),
                    ("status", s("Open")),
                ]),
            ])
        }
        "tickers" => {
            let valid = object(vec![
                ("symbol", s("API1")),
                ("price", s("8.5")),
                ("bestBidPrice", s("8")),
                ("bestAskPrice", s("9")),
                ("markPrice", s("")),
                ("indexPrice", s("8.25")),
                ("volumeOf24h", s("")),
                ("volume24h", s("6")),
                ("size", s("2")),
                ("turnoverOf24h", s("77")),
                ("turnover24h", s("21")),
            ]);
            let unknown = object(vec![("symbol", s("NO"))]);
            if case == "object" {
                object(vec![("main", valid), ("missing", unknown)])
            } else if case == "non-object" {
                s("no")
            } else {
                array(vec![valid, unknown])
            }
        }
        "ohlcv" => array(vec![array(vec![
            s("1234500"),
            s("1"),
            s("3"),
            s("0.5"),
            s("2"),
            s("7.5"),
        ])]),
        _ => array(vec![]),
    };
    let symbols = if method == "positions" {
        match case {
            "no-symbols" => u(),
            "empty" => array(vec![]),
            "duplicate" => array(vec![s("SYM"), s("MISSING"), s("MISSING")]),
            _ => array(vec![s("SYM"), s("MISSING")]),
        }
    } else {
        array(if case == "missing" {
            vec![s("MISSING")]
        } else {
            vec![s("SYM"), s("OTHER")]
        })
    };
    let metadata = object(vec![
        ("symbol", s("MISSING")),
        ("side", s("short")),
        ("leverage", n(3.)),
        ("positionMode", s("hedged")),
        ("marginType", s("cross")),
        ("id", s("OVERRIDE")),
        ("contracts", n(99.)),
        ("tag", s("KEPT")),
    ]);
    let mut fields = vec![
        (
            "symbol",
            s(if case == "unknown" { "UNKNOWN" } else { "SYM" }),
        ),
        ("interval", s("1m")),
    ];
    if case == "explicit" {
        fields.push(("limit", n(2000.)));
        fields.push(("to", n(120_000_000.)));
    }
    if case == "from" || case == "zero-limit" {
        fields.push(("from", n(1_000_000.)));
    }
    if case == "zero-limit" {
        fields.push(("limit", n(0.)));
    }
    Rc::new(State {
        method,
        case,
        err,
        receiver,
        store,
        markets,
        tickers,
        orders,
        balance,
        positions,
        raw,
        symbols,
        opts: object(fields),
        metadata,
        sockets: array(vec![
            object(vec![("id", s("WS0"))]),
            object(vec![("id", s("WS1"))]),
        ]),
        connections: vec![
            object(vec![("id", s("CONN0"))]),
            object(vec![("id", s("CONN1"))]),
        ],
        events: RefCell::new(vec![]),
        captures: RefCell::new(vec![]),
        timer: RefCell::new(None),
        timer_return: RefCell::new(None),
        disposed: Cell::new(case == "disposed"),
        dispose_reads: Cell::new(0),
        after_positions: Cell::new(false),
        requests: Cell::new(0),
        timer_started: Cell::new(false),
        closes: Cell::new(0),
        raw_predicates: RefCell::new(vec![]),
    })
}
fn route(name: &str) -> &str {
    match name {
        "ENDPOINTS.MARKETS" => "/api/v1/contracts/active",
        "ENDPOINTS.TICKERS" => "/api/v1/allTickers",
        "ENDPOINTS.POSITIONS_V2" => "/api/v1/positions",
        "ENDPOINTS.KLINE" => "/api/v1/kline/query",
        "ENDPOINTS.GET_MARGIN_MODE" => "/api/v2/position/getMarginMode",
        "ENDPOINTS.GET_POSITION_MODE" => "/api/v2/position/getPositionMode",
        "ENDPOINTS.POSITION_LEVERAGE" => "/api/v2/getCrossUserLeverage",
        _ => panic!("route {name}"),
    }
}
fn invoke_member(t: &State, target: V, key: &str, args: Vec<V>) -> Result<V, E> {
    match key {
        "includes" => {
            let included = items(&target).iter().any(|v| equal(v, &args[0]));
            Ok(if t.case == "raw-predicate" {
                s(if included { "INCLUDED" } else { "" })
            } else {
                b(included)
            })
        }
        "push" => {
            let Native::A(rows) = target.as_ref() else {
                panic!("push")
            };
            rows.borrow_mut().push(args[0].clone());
            Ok(n(rows.borrow().len() as f64))
        }
        "subtract" => {
            let Native::Date(end) = target.as_ref() else {
                panic!("date")
            };
            let scale = match text(&args[1]).as_str() {
                "m" => 60_000.,
                "h" => 3_600_000.,
                _ => panic!("unit"),
            };
            Ok(Rc::new(Native::Date(end - num(&args[0]) * scale)))
        }
        "valueOf" => {
            let Native::Date(x) = target.as_ref() else {
                panic!("date")
            };
            Ok(n(*x))
        }
        "connectAndSubscribe" => {
            let id = text(&get(&target, "id"));
            let index = if id == "WS0" { 0 } else { 1 };
            assert!(
                Rc::ptr_eq(&target, &items(&t.sockets)[index]),
                "SNAPSHOT native original receiver"
            );
            event(t, "snap", vec![s(&id)]);
            Ok(t.connections[index].clone())
        }
        _ => panic!("member {key}"),
    }
}
fn ops(t: Rc<State>) -> Rc<BootstrapOps<V, E>> {
    let tc = t.clone();
    let ti = t.clone();
    let ts = t.clone();
    let close = t.clone();
    let raw = t.clone();
    let value = Rc::new(StreamOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(move |v, k, x| {
            assert!(Rc::ptr_eq(v, &ts.receiver));
            assert_eq!(k, "snapshotWebsockets");
            event(&ts, "snapshots", vec![x.clone()]);
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            tc.captures.borrow_mut().push(name.into());
            Ok(match name {
                "this" => tc.receiver.clone(),
                "this.isDisposed" => {
                    tc.dispose_reads.set(tc.dispose_reads.get() + 1);
                    if tc.case == "dispose-positions-once" && tc.after_positions.replace(false) {
                        b(true)
                    } else {
                        b(tc.disposed.get())
                    }
                }
                "this.isPublicDataOnly" => b(tc.case == "public" || tc.case == "external-public"),
                "this.store.loaded" => get(&tc.store, "loaded"),
                "this.store.markets" => get(&tc.store, "markets"),
                "this.apiMarginCoin" => s("USDT"),
                "this.store.marketsBySymbol" => object(vec![
                    ("SYM", object(vec![("id", s("API1"))])),
                    ("OTHER", object(vec![("id", s("API2"))])),
                ]),
                "this.store.marketsById" => object(vec![(
                    "API1",
                    object(vec![("id", s("API1")), ("symbol", s("SYM"))]),
                )]),
                "INTERVAL" => object(vec![("1m", s("1"))]),
                "MarginType.Cross" => s("cross"),
                "PositionMode.OneWay" => s("oneway"),
                "PositionSide.Long" => s("long"),
                "PositionSide.Short" => s("short"),
                _ if name.starts_with("ENDPOINTS.") => s(route(name)),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        invoke: Rc::new(move |callee, args| {
            let t = &ti;
            if let Native::Member(target, key) = callee.as_ref() {
                return invoke_member(t, target.clone(), key, args);
            }
            let Native::Call(name) = callee.as_ref() else {
                panic!("callee")
            };
            Ok(match name.as_str() {
                "this.initExternalPublicData" => {
                    event(t, "init", vec![]);
                    promise(if t.case == "reject" || t.case == "no-message" {
                        Err(t.err.clone())
                    } else {
                        Ok(b(t.case == "external" || t.case == "external-public"))
                    })
                }
                "this.fetchMarkets"
                | "this.fetchTickers"
                | "this.fetchOrders"
                | "this.fetchPositions"
                | "this.fetchBalance" => {
                    let short = name.strip_prefix("this.").unwrap();
                    event(t, short, args);
                    if name == "this.fetchPositions" {
                        t.after_positions.set(true)
                    }
                    if t.case == "dispose-markets" && name == "this.fetchMarkets"
                        || t.case == "dispose-orders" && name == "this.fetchOrders"
                        || t.case == "dispose-balance" && name == "this.fetchBalance"
                    {
                        t.disposed.set(true)
                    }
                    let v = match name.as_str() {
                        "this.fetchMarkets" => t.markets.clone(),
                        "this.fetchTickers" => t.tickers.clone(),
                        "this.fetchOrders" => t.orders.clone(),
                        "this.fetchPositions" => t.positions.clone(),
                        _ => t.balance.clone(),
                    };
                    promise(if t.method == "tick" && t.case == "reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(v)
                    })
                }
                "this.fetchOpenPositions" => {
                    event(t, "fetchOpen", vec![]);
                    promise(Ok(t.positions.clone()))
                }
                "this.fetchPositionMetadata" => {
                    event(t, "fetchMetadata", args.clone());
                    promise(Ok(array(
                        items(&args[0])
                            .into_iter()
                            .map(|_| t.metadata.clone())
                            .collect(),
                    )))
                }
                "this.store.update" => {
                    event(t, "update", args.clone());
                    let payload = &args[0];
                    for key in ["markets", "tickers", "orders", "loaded"] {
                        let v = get(payload, key);
                        if !matches!(v.as_ref(), Native::U) {
                            put(&t.store, key, v);
                        }
                    }
                    u()
                }
                "this.store.addOrUpdatePositionsFull" => {
                    assert!(Rc::ptr_eq(&args[0], &t.positions));
                    event(t, "addFull", args);
                    u()
                }
                "this.log" => {
                    event(t, "log", args);
                    u()
                }
                "this.emitAndLogError" => {
                    event(t, "emit", args);
                    u()
                }
                "this.publicWebsocket.connectAndSubscribe" => {
                    event(t, "public", vec![]);
                    u()
                }
                "this.privateWebsocket.connectAndSubscribe" => {
                    event(t, "private", vec![]);
                    u()
                }
                "createKucoinSnapshotWebsockets" => {
                    assert!(Rc::ptr_eq(&args[0], &t.receiver));
                    event(t, "createSnapshots", vec![args[1].clone()]);
                    t.sockets.clone()
                }
                "this.snapshotWebsockets.forEach" => {
                    let Native::Mapper(cb) = args[0].as_ref() else {
                        panic!("mapper")
                    };
                    let current = get(&t.receiver, "snapshotWebsockets");
                    assert!(Rc::ptr_eq(&current, &t.sockets));
                    for (i, ws) in items(&current).into_iter().enumerate() {
                        let returned = cb(ws)?;
                        assert!(
                            Rc::ptr_eq(&returned, &t.connections[i]),
                            "SNAPSHOT native original callback return"
                        );
                    }
                    u()
                }
                "this.tick" => {
                    if t.timer_started.get() {
                        event(t, "timerTick", args);
                        let result = promise(Ok(s("TIMER")));
                        *t.timer_return.borrow_mut() = Some(result.clone());
                        result
                    } else {
                        event(t, "tick", args);
                        promise(Ok(u()))
                    }
                }
                "this.getTickInterval" => n(250.),
                "loop" => {
                    assert!(matches!(args[0].as_ref(), Native::Callback(_)));
                    *t.timer.borrow_mut() = Some(args[0].clone());
                    event(t, "loop", vec![args[1].clone()]);
                    u()
                }
                "this.toInternalSymbol" => s(match text(&args[0]).as_str() {
                    "API1" => "SYM",
                    "API2" => "OTHER",
                    _ => "THIRD",
                }),
                "this.contractSizeFor" => n(0.1),
                "this.parsePositionMode" => s(if text(&args[0]) == "1" {
                    "hedged"
                } else {
                    "oneway"
                }),
                "this.parseMarginType" => s(if text(&args[0]) == "ISOLATED" {
                    "isolated"
                } else {
                    "cross"
                }),
                "this.parseRowPositionMode" => s(
                    if truth(&get(&args[0], "positionSide"))
                        && text(&get(&args[0], "positionSide")) != "BOTH"
                    {
                        "hedged"
                    } else {
                        "oneway"
                    },
                ),
                "createPositionId" => s(&if text(&args[2]) == "oneway" || !truth(&args[1]) {
                    text(&args[0])
                } else {
                    format!("{}:{}", text(&args[0]), text(&args[1]))
                }),
                "parseFloat" => n(safe_cex_num::parse_float(&text(&args[0]))),
                "parseInt" => n(safe_cex_num::parse_int(
                    &text(&args[0]),
                    args.get(1).map(|v| num(v) as i32),
                )),
                "Math.abs" => n(num(&args[0]).abs()),
                "Math.min" => n(num(&args[0]).min(num(&args[1]))),
                "Boolean" => b(truth(&args[0])),
                "multiply" => n(safe_cex_num::multiply(num(&args[0]), num(&args[1]))),
                "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
                "Object.values" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("values")
                    };
                    array(fields.borrow().iter().map(|(_, v)| v.clone()).collect())
                }
                "flatten" => array(
                    items(&args[0])
                        .into_iter()
                        .flat_map(|row| items(&row))
                        .collect(),
                ),
                "mapSeries" => {
                    let Native::AsyncMapper(cb) = args[1].as_ref() else {
                        panic!("async mapper")
                    };
                    let mut rows = vec![];
                    for symbol in items(&args[0]) {
                        match run(cb(symbol)) {
                            Ok(v) => rows.push(v),
                            Err(e) => return Ok(promise(Err(e))),
                        }
                    }
                    promise(Ok(array(rows)))
                }
                "Promise.all" => {
                    let mut rows = vec![];
                    for p in items(&args[0]) {
                        match run(await_ready(p)) {
                            Ok(v) => rows.push(v),
                            Err(e) => return Ok(promise(Err(e))),
                        }
                    }
                    promise(Ok(array(rows)))
                }
                "dayjs" => Rc::new(Native::Date(args.first().map(num).unwrap_or(5_000_000.))),
                "this.xhr.get" => {
                    t.requests.set(t.requests.get() + 1);
                    event(t, "get", args.clone());
                    if t.case == "reject" {
                        promise(Err(t.err.clone()))
                    } else {
                        let endpoint = text(&args[0]);
                        let is_margin = endpoint == route("ENDPOINTS.GET_MARGIN_MODE");
                        let is_mode = endpoint == route("ENDPOINTS.GET_POSITION_MODE");
                        let is_leverage = endpoint == route("ENDPOINTS.POSITION_LEVERAGE");
                        let bad = t.case == "bad"
                            || t.case == "bad-margin" && is_margin
                            || t.case == "bad-mode" && is_mode
                            || t.case == "bad-leverage" && is_leverage;
                        let body = if is_margin {
                            object(vec![(
                                "marginMode",
                                s(if t.case == "isolated" {
                                    "ISOLATED"
                                } else {
                                    "CROSS"
                                }),
                            )])
                        } else if is_mode {
                            object(vec![(
                                "positionMode",
                                s(if t.case == "hedged" { "1" } else { "0" }),
                            )])
                        } else if is_leverage {
                            object(vec![("leverage", s("7"))])
                        } else {
                            t.raw.clone()
                        };
                        promise(Ok(object(vec![(
                            "data",
                            object(vec![
                                ("code", s(if bad { "400000" } else { "200000" })),
                                ("msg", s(if bad { "DENIED" } else { "" })),
                                ("data", body),
                            ]),
                        )])))
                    }
                }
                _ => panic!("invoke {name}"),
            })
        }),
        intrinsic: Rc::new(|name, args| {
            Ok(match name {
                "get-index" => get(&args[0], &text(&args[1])),
                "define-field" => {
                    put(&args[0], &text(&args[1]), args[2].clone());
                    u()
                }
                "spread-into" => {
                    let Native::O(fields) = args[1].as_ref() else {
                        panic!("spread")
                    };
                    let fields = fields.borrow().clone();
                    for (k, v) in fields {
                        put(&args[0], &k, v);
                    }
                    u()
                }
                "spread-arrays" => {
                    array(items(&args[0]).into_iter().chain(items(&args[1])).collect())
                }
                "loaded-log" => s(&format!(
                    "Loaded {} KuCoin Futures {}",
                    text(&args[0]),
                    text(&args[1])
                )),
                "mod-four-zero" => b(num(&args[0]) % 4. == 0.),
                "add-one" => {
                    if matches!(args[0].as_ref(), Native::S(_)) {
                        s(&format!("{}1", text(&args[0])))
                    } else {
                        n(num(&args[0]) + 1.)
                    }
                }
                "greater-zero" => b(num(&args[0]) > 0.),
                "typeof-object" => b(matches!(args[0].as_ref(), Native::A(_) | Native::O(_))),
                "margin-mode-error" => s(&format!(
                    "Failed to fetch margin mode for {}",
                    text(&args[0])
                )),
                "multiply-numbers" => n(num(&args[0]) * num(&args[1])),
                "divide-thousand" => n(num(&args[0]) / 1000.),
                _ => panic!("intrinsic {name}"),
            })
        }),
        truthy: Rc::new(truth),
        nullish: Rc::new(null),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        callback: Rc::new(|_, _, _| unused()),
        map: Rc::new(|v, cb| {
            Ok(array(
                items(&v)
                    .into_iter()
                    .map(|v| cb(v))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }),
        filter: Rc::new(|v, cb| {
            let mut rows = vec![];
            for v in items(&v) {
                if cb(v.clone())? {
                    rows.push(v)
                }
            }
            Ok(array(rows))
        }),
        for_of: Rc::new(|_, _| unused()),
        await_value: Rc::new(await_ready),
        spawn: Rc::new(|_| unused()),
        error_value: Rc::new(|e| Rc::new(Native::Err(e.clone()))),
    });
    Rc::new(BootstrapOps {
        value,
        raw_filter: Rc::new(move |v, cb| {
            let mut result = vec![];
            for row in items(&v) {
                let returned = cb(row.clone())?;
                raw.raw_predicates.borrow_mut().push(returned.clone());
                if truth(&returned) {
                    result.push(row)
                }
            }
            Ok(array(result))
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        boolean: Rc::new(b),
        mapper: Rc::new(|cb| Rc::new(Native::Mapper(cb))),
        async_mapper: Rc::new(|cb| Rc::new(Native::AsyncMapper(cb))),
        callback: Rc::new(|cb| Rc::new(Native::Callback(cb))),
        response_pair: Rc::new(|v| {
            let rows = items(&v);
            Ok((get(&rows[0], "data"), get(&rows[1], "data")))
        }),
        interval_parts: Rc::new(|v| {
            assert_eq!(text(&v), "1m");
            Ok((s("1"), s("m")))
        }),
        iterator: Rc::new(|v| Ok(Rc::new(Native::Iter(RefCell::new((items(&v), 0)))))),
        next: Rc::new(|v| {
            let Native::Iter(rows) = v.as_ref() else {
                panic!("iter")
            };
            let mut rows = rows.borrow_mut();
            let v = rows.0.get(rows.1).cloned();
            rows.1 += 1;
            Ok(v)
        }),
        close_throw: Rc::new(move |_, e| {
            close.closes.set(close.closes.get() + 1);
            e
        }),
        some: Rc::new(|v, cb| {
            for row in items(&v) {
                if cb(row)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
    })
}
const FAMILIES: &[(&str, &[&str])] = &[
    (
        "start",
        &[
            "normal",
            "external",
            "public",
            "external-public",
            "dispose-markets",
            "dispose-orders",
            "reject",
            "no-message",
        ],
    ),
    (
        "tick",
        &[
            "default",
            "skip-balance",
            "public",
            "disposed",
            "dispose-positions-once",
            "dispose-balance",
            "dispose-orders",
            "reject",
        ],
    ),
    (
        "meta",
        &[
            "cross",
            "isolated",
            "hedged",
            "missing",
            "disposed",
            "bad-margin",
            "bad-mode",
            "bad-leverage",
            "reject",
        ],
    ),
    (
        "positions",
        &[
            "no-symbols",
            "filtered",
            "empty",
            "duplicate",
            "raw-predicate",
        ],
    ),
    ("open", &["normal", "bad", "non-array", "nan", "reject"]),
    ("markets", &["normal", "bad", "reject", "defaults"]),
    (
        "tickers",
        &["array", "object", "non-object", "bad", "reject"],
    ),
    (
        "ohlcv",
        &[
            "default",
            "explicit",
            "from",
            "unknown",
            "reject",
            "zero-limit",
        ],
    ),
];
fn execute(t: &Rc<State>, h: Rc<BootstrapOps<V, E>>) -> (Result<V, E>, V) {
    let result = match t.method {
        "start" => run(port::start(h)),
        "tick" => run(port::tick(
            h,
            if t.case == "skip-balance" { n(1.) } else { u() },
        )),
        "meta" => run(port::fetch_position_metadata(h, t.symbols.clone())),
        "positions" => run(port::fetch_positions(h, t.symbols.clone())),
        "open" => run(port::fetch_open_positions(&h)),
        "markets" => run(port::fetch_markets(h)),
        "tickers" => run(port::fetch_tickers(&h)),
        "ohlcv" => run(port::fetch_ohlcv(h, t.opts.clone())),
        _ => panic!("method"),
    };
    let timer = if let Some(callback) = t.timer.borrow().clone() {
        let Native::Callback(cb) = callback.as_ref() else {
            panic!("callback")
        };
        t.timer_started.set(true);
        let Ok(returned) = cb() else {
            panic!("timer invocation")
        };
        assert!(
            Rc::ptr_eq(&returned, t.timer_return.borrow().as_ref().unwrap()),
            "TIMER native original return handle"
        );
        let Ok(value) = run(await_ready(returned)) else {
            panic!("timer completion")
        };
        value
    } else {
        u()
    };
    (result, timer)
}
fn names(t: &State) -> Vec<String> {
    t.events.borrow().iter().map(|r| text(&r[0])).collect()
}
fn expected_names(m: &str, c: &str) -> Vec<&'static str> {
    match m {
        "start" => {
            let mut names = vec!["init"];
            if c == "reject" || c == "no-message" {
                names.push("emit");
                return names;
            }
            let external = c == "external" || c == "external-public";
            if !external {
                names.push("fetchMarkets");
                if c == "dispose-markets" {
                    return names;
                }
                names.extend([
                    "update",
                    "log",
                    "fetchTickers",
                    "update",
                    "log",
                    "public",
                    "createSnapshots",
                    "snapshots",
                    "snap",
                    "snap",
                ]);
            }
            if c == "public" || c == "external-public" {
                names.push("update");
                return names;
            }
            names.extend(["private", "tick", "fetchOrders"]);
            if c == "dispose-orders" {
                return names;
            }
            names.extend(["update", "log", "log"]);
            names
        }
        "tick" => match c {
            "public" | "disposed" => vec![],
            "dispose-positions-once" => vec!["fetchPositions"],
            "dispose-balance" => vec!["fetchPositions", "addFull", "fetchBalance"],
            "dispose-orders" => vec!["fetchPositions", "addFull", "fetchBalance", "fetchOrders"],
            "reject" => vec!["fetchPositions", "emit", "loop", "timerTick"],
            "skip-balance" => vec![
                "fetchPositions",
                "addFull",
                "fetchOrders",
                "update",
                "loop",
                "timerTick",
            ],
            _ => vec![
                "fetchPositions",
                "addFull",
                "fetchBalance",
                "fetchOrders",
                "update",
                "loop",
                "timerTick",
            ],
        },
        "meta" => match c {
            "missing" | "disposed" => vec![],
            "reject" => vec!["get", "get", "emit"],
            "bad-margin" | "bad-mode" => vec!["get", "get", "emit", "get", "get", "emit"],
            "isolated" => vec!["get", "get", "get", "get"],
            _ => vec!["get", "get", "get", "get", "get", "get"],
        },
        "positions" => {
            if c == "no-symbols" {
                vec!["fetchOpen"]
            } else {
                vec!["fetchOpen", "fetchMetadata"]
            }
        }
        "open" | "markets" | "tickers" => {
            if c == "bad" {
                vec!["get", "emit"]
            } else {
                vec!["get"]
            }
        }
        "ohlcv" => vec!["get"],
        _ => unreachable!(),
    }
}
fn assert_number(v: &V, expected: f64, label: &str) {
    let Native::N(actual) = v.as_ref() else {
        panic!("{label}: expected decoded native number")
    };
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}")
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}");
    }
}
fn assert_text(v: &V, expected: &str, label: &str) {
    let Native::S(actual) = v.as_ref() else {
        panic!("{label}: expected decoded native string")
    };
    assert_eq!(actual, expected, "{label}");
}
fn assert_boolean(v: &V, expected: bool, label: &str) {
    assert!(
        matches!(v.as_ref(),Native::B(actual) if *actual==expected),
        "{label}"
    );
}
fn assert_fields(v: &V, expected: &[&str], label: &str) {
    let Native::O(fields) = v.as_ref() else {
        panic!("{label}: object")
    };
    assert_eq!(
        fields
            .borrow()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        expected,
        "{label}"
    );
}
fn assert_native(t: &State, result: &Result<V, E>, timer: &V) {
    let (m, c) = (t.method, t.case);
    assert_eq!(names(t), expected_names(m, c), "SEQUENCE native {m}/{c}");
    let events = t.events.borrow();
    let emits = events
        .iter()
        .filter(|r| text(&r[0]) == "emit")
        .collect::<Vec<_>>();
    let requests = events
        .iter()
        .filter(|r| text(&r[0]) == "get")
        .collect::<Vec<_>>();
    for emit in &emits {
        assert_eq!(
            emit.len(),
            if m == "meta" && c == "reject" { 3 } else { 2 },
            "EMIT native argument count"
        );
        if emit.len() == 3 {
            let Native::Err(e) = emit[1].as_ref() else {
                panic!("EMIT native error")
            };
            assert!(Rc::ptr_eq(e, &t.err), "EMIT native original error identity");
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "DOWN");
            assert_eq!(e.code, Some("READ"));
            assert_eq!(e.ids, Some([3, 7]));
            assert!(!e.fresh);
        }
        assert_text(
            emit.last().unwrap(),
            if c == "reject" {
                "VENUE"
            } else if c == "no-message" {
                "Failed to start KuCoin exchange"
            } else {
                "DENIED"
            },
            "EMIT native diagnostic value",
        );
    }
    if c == "reject" && matches!(m, "open" | "markets" | "tickers" | "ohlcv") {
        let Err(e) = result else {
            panic!("ERROR native expected original rejection")
        };
        assert!(
            Rc::ptr_eq(e, &t.err),
            "ERROR native original error identity"
        );
        assert_eq!(e.name, "TypeError");
        assert_eq!(e.message, "DOWN");
        assert_eq!(e.code, Some("READ"));
        assert_eq!(e.ids, Some([3, 7]));
        assert!(!e.fresh);
        assert_eq!(requests.len(), 1, "ERROR native independent request count");
        assert_eq!(emits.len(), 0, "ERROR native no catch emission");
        return;
    }
    let Ok(value) = result else {
        panic!("RESULT native success {m}/{c}")
    };
    match m {
        "start" => {
            assert!(
                matches!(value.as_ref(), Native::U),
                "START native undefined"
            );
            let updates = events
                .iter()
                .filter(|r| text(&r[0]) == "update")
                .collect::<Vec<_>>();
            let expected = match c {
                "reject" | "no-message" | "dispose-markets" => 0,
                "external-public" | "external" => 1,
                "dispose-orders" => 2,
                _ => 3,
            };
            assert_eq!(
                updates.len(),
                expected,
                "START native independent update count"
            );
            for update in &updates {
                let flags = get(&update[1], "loaded");
                assert_boolean(
                    &get(&flags, "seed"),
                    true,
                    "START native inherited loaded flag",
                );
            }
            let external = c == "external" || c == "external-public";
            if !external && !matches!(c, "dispose-markets" | "reject" | "no-message") {
                assert!(
                    Rc::ptr_eq(&get(&updates[0][1], "markets"), &t.markets),
                    "START native original markets reference"
                );
                assert_boolean(
                    &get(&get(&updates[0][1], "loaded"), "markets"),
                    true,
                    "START native loaded markets",
                );
                assert!(
                    Rc::ptr_eq(&get(&updates[1][1], "tickers"), &t.tickers),
                    "START native original tickers reference"
                );
                assert_boolean(
                    &get(&get(&updates[1][1], "loaded"), "tickers"),
                    true,
                    "START native loaded tickers",
                );
                let created = events
                    .iter()
                    .find(|r| text(&r[0]) == "createSnapshots")
                    .unwrap();
                assert_number(&created[1], 2., "START native live market count");
                let assigned = get(&t.receiver, "snapshotWebsockets");
                assert!(
                    Rc::ptr_eq(&assigned, &t.sockets),
                    "START native assigned snapshot identity"
                );
                assert_eq!(
                    items(&assigned).len(),
                    2,
                    "START native independent snapshot count"
                );
            }
            if c == "public" || c == "external-public" {
                let flags = get(&updates.last().unwrap()[1], "loaded");
                for key in ["balance", "positions", "orders"] {
                    assert_boolean(&get(&flags, key), true, "START native public account flags");
                }
            } else if !matches!(
                c,
                "dispose-markets" | "dispose-orders" | "reject" | "no-message"
            ) {
                let payload = &updates.last().unwrap()[1];
                assert!(
                    Rc::ptr_eq(&get(payload, "orders"), &t.orders),
                    "START native original orders reference"
                );
                assert_boolean(
                    &get(&get(payload, "loaded"), "orders"),
                    true,
                    "START native loaded orders",
                );
                let logs = events
                    .iter()
                    .filter(|r| text(&r[0]) == "log")
                    .collect::<Vec<_>>();
                assert_text(
                    &logs[logs.len() - 2][1],
                    "Loaded 1 KuCoin Futures orders",
                    "START native order log",
                );
                assert_text(
                    &logs[logs.len() - 1][1],
                    "Ready to trade on KuCoin Futures",
                    "START native ready log",
                );
            }
        }
        "tick" => {
            assert!(matches!(value.as_ref(), Native::U), "TICK native undefined");
            let loops = events
                .iter()
                .filter(|r| text(&r[0]) == "loop")
                .collect::<Vec<_>>();
            let scheduled = matches!(c, "default" | "skip-balance" | "reject");
            assert_eq!(
                loops.len(),
                usize::from(scheduled),
                "TICK native independent scheduled count"
            );
            if scheduled {
                assert_number(&loops[0][1], 250., "TICK native interval");
                assert_text(timer, "TIMER", "TIMER native decoded completion");
                let event = events.last().unwrap();
                assert_number(
                    &event[1],
                    if c == "skip-balance" { 2. } else { 1. },
                    "TIMER native next counter",
                );
            } else {
                assert!(
                    matches!(timer.as_ref(), Native::U),
                    "TIMER native absent result"
                );
            }
            if c == "dispose-positions-once" {
                assert_eq!(
                    t.dispose_reads.get(),
                    2,
                    "TICK native no disposal reread after return"
                );
            }
            if c == "default" || c == "skip-balance" {
                let payload = &events.iter().find(|r| text(&r[0]) == "update").unwrap()[1];
                assert!(
                    Rc::ptr_eq(&get(payload, "orders"), &t.orders),
                    "TICK native original orders"
                );
                let flags = get(payload, "loaded");
                assert_boolean(&get(&flags, "seed"), true, "TICK native inherited flag");
                assert_boolean(
                    &get(&flags, "positions"),
                    true,
                    "TICK native loaded positions",
                );
                assert_boolean(&get(&flags, "orders"), true, "TICK native loaded orders");
                if c == "default" {
                    assert!(
                        Rc::ptr_eq(&get(payload, "balance"), &t.balance),
                        "TICK native original balance"
                    );
                    assert_boolean(&get(&flags, "balance"), true, "TICK native loaded balance");
                    assert_fields(
                        payload,
                        &["balance", "orders", "loaded"],
                        "TICK native update keys",
                    );
                } else {
                    assert!(
                        matches!(get(payload, "balance").as_ref(), Native::U),
                        "TICK native absent skipped balance"
                    );
                    assert!(
                        matches!(get(&flags, "balance").as_ref(), Native::U),
                        "TICK native absent skipped balance flag"
                    );
                    assert_fields(payload, &["orders", "loaded"], "TICK native skipped keys");
                }
            }
        }
        "meta" => {
            let count = match c {
                "missing" | "disposed" => 0,
                "reject" => 2,
                "isolated" | "bad-margin" | "bad-mode" => 4,
                _ => 6,
            };
            assert_eq!(
                requests.len(),
                count,
                "META native independent request count"
            );
            if count > 0 {
                assert_text(
                    &requests[0][1],
                    "/api/v2/position/getMarginMode",
                    "META native margin route",
                );
                assert_text(
                    &get(&get(&requests[0][2], "params"), "symbol"),
                    "API1",
                    "META native margin symbol",
                );
                assert_text(
                    &requests[1][1],
                    "/api/v2/position/getPositionMode",
                    "META native mode route",
                );
                assert_eq!(
                    requests[1].len(),
                    2,
                    "META native account-wide mode request"
                );
            }
            for (i, request) in requests.iter().enumerate() {
                let three = matches!(c, "cross" | "hedged" | "bad-leverage");
                let slot = i % if three { 3 } else { 2 };
                let endpoint = if slot == 0 {
                    "/api/v2/position/getMarginMode"
                } else if slot == 1 {
                    "/api/v2/position/getPositionMode"
                } else {
                    "/api/v2/getCrossUserLeverage"
                };
                assert_text(
                    &request[1],
                    endpoint,
                    "META native complete sequential routes",
                );
                if slot == 1 {
                    assert_eq!(request.len(), 2, "META native mode request argument count");
                } else {
                    assert_eq!(
                        request.len(),
                        3,
                        "META native scoped request argument count"
                    );
                    let params = get(&request[2], "params");
                    assert_fields(&params, &["symbol"], "META native symbol-only query");
                    assert_text(
                        &get(&params, "symbol"),
                        if i < if three { 3 } else { 2 } {
                            "API1"
                        } else {
                            "API2"
                        },
                        "META native complete sequential query symbols",
                    );
                }
            }
            let rows = items(value);
            let expected = if matches!(
                c,
                "missing" | "disposed" | "bad-margin" | "bad-mode" | "reject"
            ) {
                0
            } else if c == "hedged" {
                4
            } else {
                2
            };
            assert_eq!(rows.len(), expected, "META native independent output count");
            for (i, row) in rows.iter().enumerate() {
                assert_fields(
                    row,
                    &["symbol", "side", "leverage", "positionMode", "marginType"],
                    "META native fields",
                );
                assert_text(
                    &get(row, "symbol"),
                    if i < if c == "hedged" { 2 } else { 1 } {
                        "SYM"
                    } else {
                        "OTHER"
                    },
                    "META native sequential symbol",
                );
                assert_text(
                    &get(row, "side"),
                    if c == "hedged" && i % 2 == 1 {
                        "short"
                    } else {
                        "long"
                    },
                    "META native side",
                );
                assert_text(
                    &get(row, "positionMode"),
                    if c == "hedged" { "hedged" } else { "oneway" },
                    "META native mode",
                );
                assert_text(
                    &get(row, "marginType"),
                    if c == "isolated" { "isolated" } else { "cross" },
                    "META native margin type",
                );
                assert_number(
                    &get(row, "leverage"),
                    if c == "isolated" || c == "bad-leverage" {
                        1.
                    } else {
                        7.
                    },
                    "META native leverage",
                );
            }
        }
        "positions" => {
            if c == "raw-predicate" {
                let predicates = t.raw_predicates.borrow();
                assert_eq!(
                    predicates.len(),
                    2,
                    "POSITIONS native independent raw predicate count"
                );
                assert_text(
                    &predicates[0],
                    "INCLUDED",
                    "POSITIONS native raw includes predicate",
                );
                assert_text(
                    &predicates[1],
                    "",
                    "POSITIONS native raw includes rejection",
                );
            }
            if c == "no-symbols" {
                assert!(
                    Rc::ptr_eq(value, &t.positions),
                    "POSITIONS native original unfiltered array"
                );
            } else {
                let rows = items(value);
                let count = match c {
                    "empty" => 0,
                    "duplicate" => 3,
                    _ => 2,
                };
                assert_eq!(
                    rows.len(),
                    count,
                    "POSITIONS native independent result count"
                );
                let missing = &events[1][1];
                let expected = match c {
                    "empty" => 0,
                    "duplicate" => 2,
                    _ => 1,
                };
                assert_eq!(
                    items(missing).len(),
                    expected,
                    "POSITIONS native independent missing count"
                );
                for symbol in items(missing) {
                    assert_text(&symbol, "MISSING", "POSITIONS native missing symbol");
                }
                if count > 0 {
                    assert!(
                        Rc::ptr_eq(&rows[0], &items(&t.positions)[0]),
                        "POSITIONS native original open row"
                    );
                    for row in &rows[1..] {
                        assert_text(
                            &get(row, "id"),
                            "OVERRIDE",
                            "POSITIONS native spread overrides generated id",
                        );
                        assert_text(
                            &get(row, "symbol"),
                            "MISSING",
                            "POSITIONS native fake symbol",
                        );
                        assert_text(&get(row, "side"), "short", "POSITIONS native fake side");
                        assert_number(
                            &get(row, "leverage"),
                            3.,
                            "POSITIONS native metadata leverage",
                        );
                        for key in [
                            "entryPrice",
                            "notional",
                            "unrealizedPnl",
                            "contracts",
                            "liquidationPrice",
                        ] {
                            assert_number(&get(row, key), 0., "POSITIONS native zero fields");
                        }
                        let info = get(row, "info");
                        assert!(
                            !Rc::ptr_eq(&info, &t.metadata),
                            "POSITIONS native info copy identity"
                        );
                        assert_text(&get(&info, "id"), "OVERRIDE", "POSITIONS native info id");
                        assert_number(
                            &get(&info, "contracts"),
                            99.,
                            "POSITIONS native info retains metadata quantity",
                        );
                        assert_text(&get(&info, "tag"), "KEPT", "POSITIONS native info field");
                    }
                    assert_number(
                        &get(&t.metadata, "contracts"),
                        99.,
                        "POSITIONS native original metadata unchanged",
                    );
                }
            }
        }
        "open" => {
            assert_eq!(requests.len(), 1, "OPEN native independent request count");
            assert_text(&requests[0][1], "/api/v1/positions", "OPEN native v2 route");
            let rows = items(value);
            let expected = if c == "bad" || c == "non-array" { 0 } else { 1 };
            assert_eq!(
                rows.len(),
                expected,
                "OPEN native zero-size filtering count"
            );
            if expected == 1 {
                let row = &rows[0];
                assert_fields(
                    row,
                    &[
                        "id",
                        "symbol",
                        "side",
                        "entryPrice",
                        "notional",
                        "leverage",
                        "unrealizedPnl",
                        "contracts",
                        "liquidationPrice",
                        "positionMode",
                        "marginType",
                        "extra",
                        "info",
                    ],
                    "OPEN native field order",
                );
                assert_text(
                    &get(row, "id"),
                    if c == "nan" { "SYM" } else { "SYM:long" },
                    "OPEN native id",
                );
                assert_text(
                    &get(row, "side"),
                    if c == "nan" { "short" } else { "long" },
                    "OPEN native explicit side precedence",
                );
                assert_text(&get(row, "symbol"), "SYM", "OPEN native internal symbol");
                assert_text(
                    &get(row, "positionMode"),
                    if c == "nan" { "oneway" } else { "hedged" },
                    "OPEN native row position mode",
                );
                assert_text(&get(row, "marginType"), "cross", "OPEN native margin");
                for (key, expected) in [
                    ("entryPrice", 1.5),
                    ("leverage", 7.),
                    ("unrealizedPnl", -2.5),
                    ("liquidationPrice", 0.8),
                ] {
                    assert_number(&get(row, key), expected, "OPEN native numeric mapped field");
                }
                assert_number(
                    &get(row, "contracts"),
                    if c == "nan" { f64::NAN } else { 0.4 },
                    "OPEN native base contracts",
                );
                assert_number(
                    &get(&get(row, "extra"), "contractsNative"),
                    if c == "nan" { f64::NAN } else { 4. },
                    "OPEN native absolute native contracts",
                );
                assert_number(
                    &get(row, "notional"),
                    if c == "nan" { f64::NAN } else { 4. },
                    "OPEN native converted notional",
                );
                assert!(
                    Rc::ptr_eq(&get(row, "info"), &items(&t.raw)[1]),
                    "OPEN native original info identity"
                );
            }
        }
        "markets" => {
            assert_eq!(
                requests.len(),
                1,
                "MARKETS native independent request count"
            );
            assert_text(
                &requests[0][1],
                "/api/v1/contracts/active",
                "MARKETS native route",
            );
            let rows = items(value);
            assert_eq!(
                rows.len(),
                if c == "bad" { 0 } else { 1 },
                "MARKETS native quote/expiry filtered count"
            );
            if c != "bad" {
                let row = &rows[0];
                assert_fields(
                    row,
                    &[
                        "id",
                        "symbol",
                        "base",
                        "quote",
                        "active",
                        "precision",
                        "limits",
                        "isExpiring",
                        "isInverse",
                        "contractSize",
                        "info",
                    ],
                    "MARKETS native fields",
                );
                assert_text(&get(row, "id"), "API1", "MARKETS native id");
                assert_text(&get(row, "symbol"), "SYM", "MARKETS native symbol");
                assert_text(
                    &get(row, "base"),
                    if c == "defaults" { "ETH" } else { "BTC" },
                    "MARKETS native base alias",
                );
                assert_text(&get(row, "quote"), "USDT", "MARKETS native quote fallback");
                assert_boolean(&get(row, "active"), true, "MARKETS native active");
                assert_boolean(&get(row, "isExpiring"), false, "MARKETS native expiry");
                assert_boolean(&get(row, "isInverse"), false, "MARKETS native inverse");
                let precision = get(row, "precision");
                assert_number(
                    &get(&precision, "amount"),
                    if c == "defaults" { 1. } else { 2. },
                    "MARKETS native amount precision",
                );
                assert_number(
                    &get(&precision, "price"),
                    if c == "defaults" { 0.01 } else { 0.5 },
                    "MARKETS native price precision",
                );
                let limits = get(row, "limits");
                let amount = get(&limits, "amount");
                let leverage = get(&limits, "leverage");
                assert_number(
                    &get(&amount, "min"),
                    if c == "defaults" { 1. } else { 3. },
                    "MARKETS native amount min",
                );
                assert_number(
                    &get(&amount, "max"),
                    if c == "defaults" { f64::INFINITY } else { 99. },
                    "MARKETS native amount max",
                );
                assert_number(&get(&leverage, "min"), 1., "MARKETS native leverage min");
                assert_number(
                    &get(&leverage, "max"),
                    if c == "defaults" { 20. } else { 8. },
                    "MARKETS native leverage max",
                );
                assert_number(
                    &get(row, "contractSize"),
                    if c == "defaults" { 1. } else { 0.1 },
                    "MARKETS native multiplier",
                );
                let info = get(row, "info");
                assert_boolean(
                    &get(&info, "isQuanto"),
                    false,
                    "MARKETS native corrected quanto flag",
                );
                assert_boolean(
                    &get(&items(&t.raw)[0], "isQuanto"),
                    true,
                    "MARKETS native original info unchanged",
                );
                assert!(
                    !Rc::ptr_eq(&info, &items(&t.raw)[0]),
                    "MARKETS native info copy"
                );
            }
        }
        "tickers" => {
            assert_eq!(
                requests.len(),
                1,
                "TICKERS native independent request count"
            );
            assert_text(
                &requests[0][1],
                "/api/v1/allTickers",
                "TICKERS native route",
            );
            let rows = items(value);
            assert_eq!(
                rows.len(),
                if c == "bad" || c == "non-object" {
                    0
                } else {
                    1
                },
                "TICKERS native independent filtered count"
            );
            if !rows.is_empty() {
                let row = &rows[0];
                assert_fields(
                    row,
                    &[
                        "id",
                        "symbol",
                        "bid",
                        "ask",
                        "last",
                        "mark",
                        "index",
                        "percentage",
                        "fundingRate",
                        "fundingTime",
                        "openInterest",
                        "volume",
                        "quoteVolume",
                    ],
                    "TICKERS native fields",
                );
                assert_text(&get(row, "id"), "API1", "TICKERS native market id");
                assert_text(&get(row, "symbol"), "SYM", "TICKERS native internal symbol");
                for (key, expected) in [
                    ("bid", 8.),
                    ("ask", 9.),
                    ("last", 8.5),
                    ("mark", 8.5),
                    ("index", 8.25),
                    ("volume", 6.),
                    ("quoteVolume", 77.),
                    ("percentage", 0.),
                    ("fundingRate", 0.),
                    ("fundingTime", 0.),
                    ("openInterest", 0.),
                ] {
                    assert_number(&get(row, key), expected, "TICKERS native numeric field");
                }
            }
        }
        "ohlcv" => {
            assert_eq!(requests.len(), 1, "OHLCV native independent request count");
            assert_text(&requests[0][1], "/api/v1/kline/query", "OHLCV native route");
            let params = get(&requests[0][2], "params");
            assert_fields(
                &params,
                &["symbol", "granularity", "from", "to"],
                "OHLCV native query fields",
            );
            assert_text(
                &get(&params, "symbol"),
                if c == "unknown" { "UNKNOWN" } else { "API1" },
                "OHLCV native symbol fallback",
            );
            assert_text(
                &get(&params, "granularity"),
                "1",
                "OHLCV native interval mapping",
            );
            let end = if c == "explicit" {
                120_000_000.
            } else {
                5_000_000.
            };
            let start = if c == "from" || c == "zero-limit" {
                1_000_000.
            } else {
                end - if c == "explicit" {
                    60_000_000.
                } else {
                    30_000_000.
                }
            };
            assert_number(&get(&params, "from"), start, "OHLCV native from bound");
            assert_number(&get(&params, "to"), end, "OHLCV native to bound");
            let rows = items(value);
            assert_eq!(rows.len(), 1, "OHLCV native independent candle count");
            assert_fields(
                &rows[0],
                &["timestamp", "open", "high", "low", "close", "volume"],
                "OHLCV native candle fields",
            );
            for (key, expected) in [
                ("timestamp", 1234.5),
                ("open", 1.),
                ("high", 3.),
                ("low", 0.5),
                ("close", 2.),
                ("volume", 7.5),
            ] {
                assert_number(
                    &get(&rows[0], key),
                    expected,
                    "OHLCV native decoded candle field",
                );
            }
        }
        _ => unreachable!(),
    }
}
#[test]
fn bootstrap_reads_native_before_source_oracle() {
    open_position_iterator_witnesses();
    let mut records = vec![];
    let mut cases = vec![];
    for &(method, family) in FAMILIES {
        for &case in family {
            let t = fixture(method, case);
            let (result, timer) = execute(&t, ops(t.clone()));
            assert_native(&t, &result, &timer);
            records.push((t, result, timer));
            cases.push(json!({"method":method,"case":case}));
        }
    }
    assert_eq!(
        records.len(),
        50,
        "independent fixture floor; stopped ohlcv/zero-limit"
    );
    let actual=records.iter().map(|(t,result,timer)|json!({"method":t.method,"case":t.case,"result":match result{Ok(v)=>json!({"ok":pack(v)}),Err(e)=>json!({"error":{"name":e.name,"message":e.message,"fresh":e.fresh,"code":e.code,"ids":e.ids}})},"timer":pack(timer),"events":t.events.borrow().iter().map(|r|r.iter().map(pack).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/bootstrap-reads-oracle.mjs"
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
        .write_all(
            serde_json::to_string(&json!({"cases":cases}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 50, "independent source fixture floor");
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!(
            a, &e,
            "wire assertion after all native checks: {}/{}",
            a["method"], a["case"]
        );
    }
}

fn open_position_iterator_witnesses() {
    let t = fixture("open", "normal");
    let mut h = ops(t.clone());
    let original = h.member.clone();
    let error = t.err.clone();
    Rc::get_mut(&mut h).unwrap().member = Rc::new(move |target, key| {
        if key == "push" {
            Err(error.clone())
        } else {
            original(target, key)
        }
    });
    let Err(e) = run(port::fetch_open_positions(&h)) else {
        panic!("OPEN native expected member rejection")
    };
    assert!(Rc::ptr_eq(&e, &t.err), "OPEN native member error identity");
    assert_eq!(e.name, "TypeError");
    assert_eq!(e.message, "DOWN");
    assert_eq!(e.code, Some("READ"));
    assert_eq!(e.ids, Some([3, 7]));
    assert!(!e.fresh);
    assert_eq!(
        t.closes.get(),
        1,
        "OPEN native push member failure closes once"
    );
    assert_eq!(
        t.requests.get(),
        1,
        "OPEN native member failure request count"
    );
    let t = fixture("open", "normal");
    let mut h = ops(t.clone());
    let error = t.err.clone();
    Rc::get_mut(&mut h).unwrap().next = Rc::new(move |_| Err(error.clone()));
    let Err(e) = run(port::fetch_open_positions(&h)) else {
        panic!("OPEN native expected next rejection")
    };
    assert!(Rc::ptr_eq(&e, &t.err), "OPEN native next error identity");
    assert_eq!(e.name, "TypeError");
    assert_eq!(e.message, "DOWN");
    assert_eq!(e.code, Some("READ"));
    assert_eq!(e.ids, Some([3, 7]));
    assert!(!e.fresh);
    assert_eq!(t.closes.get(), 0, "OPEN native next failure does not close");
    assert_eq!(
        t.requests.get(),
        1,
        "OPEN native next failure request count"
    );
}
