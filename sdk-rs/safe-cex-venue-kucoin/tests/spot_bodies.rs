use safe_cex_venue_kucoin::{
    bootstrap_reads::{BootstrapAwait, BootstrapOps},
    public_streams::StreamOps,
    spot_bodies::{self as port, SpotBodyOps},
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
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Error")
            .field("message", &self.message)
            .finish()
    }
}
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
    AsyncArrow(Rc<dyn Fn(Vec<V>) -> BootstrapAwait<V, E>>),
    AsyncMapper(Rc<dyn Fn(V) -> BootstrapAwait<V, E>>),
    Callback(Rc<dyn Fn() -> Result<V, E>>),
    Date(f64),
    Comparator(Rc<dyn Fn(V, V) -> Result<V, E>>),
    Assignment(V, String),
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
    store: V,
    markets: V,
    tickers: V,
    orders: V,
    positions: V,
    balance: V,
    raw: V,
    opts: V,
    symbols: V,
    events: RefCell<Vec<Vec<V>>>,
    requests: RefCell<Vec<Vec<V>>>,
    timer: RefCell<Option<V>>,
    timer_return: RefCell<Option<V>>,
    disposed: Cell<bool>,
    timer_started: Cell<bool>,
    closes: Cell<usize>,
    prototypes: RefCell<Vec<(V, V)>>,
    mapped_orders: RefCell<Vec<V>>,
}
fn event(t: &State, name: &str, args: Vec<V>) {
    let mut row = vec![s(name)];
    row.extend(args);
    t.events.borrow_mut().push(row);
}
fn fixture(method: &'static str, case: &'static str) -> Rc<State> {
    let err = Rc::new(Error {
        name: "TypeError",
        message: "DOWN".into(),
        fresh: false,
        code: Some("READ"),
        ids: Some([3, 7]),
        response: object(vec![("data", object(vec![("msg", s("VENUE"))]))]),
    });
    let markets = array(vec![
        object(vec![("id", s("M0"))]),
        object(vec![("id", s("M1"))]),
    ]);
    let tickers = array(vec![object(vec![("id", s("T0"))])]);
    let orders = array(vec![object(vec![("id", s("OLD"))])]);
    let positions = array(vec![object(vec![("id", s("POS"))])]);
    let balance = object(vec![("total", n(123.))]);
    let market = object(vec![
        ("id", s("API")),
        (
            "precision",
            object(vec![("amount", n(0.1)), ("price", n(0.5))]),
        ),
    ]);
    let store = object(vec![
        ("loaded", object(vec![("seed", b(true))])),
        ("markets", markets.clone()),
        ("tickers", tickers.clone()),
        ("orders", orders.clone()),
        ("balance", balance.clone()),
        (
            "marketsBySymbol",
            object(if case == "missing" {
                vec![]
            } else {
                vec![("SYM", market)]
            }),
        ),
    ]);
    let raw = match method {
        "balance" => array(if case == "empty" {
            vec![]
        } else {
            vec![
                object(vec![
                    (
                        "currency",
                        s(if case == "prototype" {
                            "__proto__"
                        } else {
                            "ETH"
                        }),
                    ),
                    ("available", s("2")),
                    ("holds", s("0.5")),
                    ("balance", s("2.5")),
                ]),
                object(vec![
                    ("currency", s("USDT")),
                    ("available", s("3")),
                    ("holds", s("1")),
                    ("balance", s("4")),
                ]),
            ]
        }),
        "fills" => array(vec![if case == "defaults" {
            object(vec![
                ("symbol", s("SYM")),
                ("orderId", s("F")),
                ("feeCurrency", s("USDT")),
            ])
        } else {
            object(vec![
                ("createdAt", s("1234x")),
                ("symbol", s("SYM")),
                ("orderId", s("F")),
                ("price", s("12.5")),
                ("size", s("2")),
                ("side", s("sell")),
                ("fee", s("0.25")),
                ("feeCurrency", s("USDT")),
            ])
        }]),
        "ohlcv" => array(vec![
            array(vec![s("300"), s("3"), s("4"), s("5"), s("2"), s("6")]),
            array(vec![s("100"), s("1"), s("2"), s("3"), s("0.5"), s("7.5")]),
        ]),
        _ => array(vec![]),
    };
    let mut fields = vec![(
        "symbol",
        s(if case == "missing" && method == "fills" {
            ""
        } else {
            "SYM"
        }),
    )];
    match method {
        "place" => {
            fields.extend(vec![
                (
                    "type",
                    s(if case == "unsupported" {
                        "stop_loss"
                    } else if case == "market" {
                        "market"
                    } else {
                        "limit"
                    }),
                ),
                ("side", s("sell")),
                ("amount", n(2.37)),
                ("price", n(12.7)),
                (
                    "timeInForce",
                    s(match case {
                        "fok" => "FillOrKill",
                        "post" => "PostOnly",
                        _ => "ImmediateOrCancel",
                    }),
                ),
            ]);
            if case == "attached" {
                fields.push(("stopLoss", n(1.)));
            }
        }
        "update" => {
            fields = vec![
                (
                    "order",
                    object(vec![("symbol", s("SYM")), ("id", s("ORIGINAL"))]),
                ),
                (
                    "update",
                    object(if case == "undefined" {
                        vec![("price", u())]
                    } else {
                        vec![("price", n(12.7)), ("amount", n(2.37))]
                    }),
                ),
            ];
        }
        "fills" => {
            if case != "defaults" {
                fields.extend(vec![
                    ("orderId", s("ORD")),
                    ("limit", n(200.)),
                    ("from", n(1000.)),
                    ("to", n(2000.)),
                ]);
            }
        }
        "ohlcv" => {
            fields.push(("interval", s("1m")));
            if case == "limit" {
                fields.extend(vec![("limit", n(2000.)), ("to", n(120_000_000.))]);
            }
            if case == "from" || case == "zero-limit" {
                fields.push(("from", n(1_000_000.)));
            }
            if case == "zero-limit" {
                fields.push(("limit", n(0.)));
            }
        }
        _ => (),
    }
    Rc::new(State {
        method,
        case,
        err,
        store,
        markets,
        tickers,
        orders,
        positions,
        balance,
        raw,
        opts: object(fields),
        symbols: if case == "discovery" || case == "empty" {
            u()
        } else {
            array(vec![s("SYM")])
        },
        events: RefCell::new(vec![]),
        requests: RefCell::new(vec![]),
        timer: RefCell::new(None),
        timer_return: RefCell::new(None),
        disposed: Cell::new(case == "disposed"),
        timer_started: Cell::new(false),
        closes: Cell::new(0),
        prototypes: RefCell::new(vec![]),
        mapped_orders: RefCell::new(vec![]),
    })
}
fn endpoint(key: &str) -> &'static str {
    match key {
        "ACCOUNTS" => "/api/v1/accounts",
        "OPEN_ORDERS" => "/api/v1/hf/orders/active/page",
        "PLACE_ORDER" => "/api/v1/hf/orders",
        "MODIFY_ORDER" => "/api/v1/hf/orders/alter",
        "FILLS" => "/api/v1/hf/fills",
        "KLINE" => "/api/v1/market/candles",
        _ => panic!("endpoint {key}"),
    }
}
fn invoke_member(t: &State, target: V, key: &str, args: Vec<V>) -> Result<V, E> {
    Ok(match key {
        "map" => {
            let mut out = vec![];
            for row in items(&target) {
                out.push(match args[0].as_ref() {
                    Native::Mapper(cb) => cb(row)?,
                    Native::AsyncMapper(cb) => promise(run(cb(row))),
                    _ => panic!("mapper"),
                });
            }
            array(out)
        }
        "flat" => array(items(&target).iter().flat_map(items).collect()),
        "push" => {
            let Native::A(rows) = target.as_ref() else {
                panic!("push")
            };
            rows.borrow_mut().extend(args);
            n(rows.borrow().len() as f64)
        }
        "sort" => {
            let Native::Comparator(cb) = args[0].as_ref() else {
                panic!("comparator")
            };
            let mut rows = items(&target);
            for i in 1..rows.len() {
                let mut j = i;
                while j > 0 && num(&cb(rows[j - 1].clone(), rows[j].clone())?) > 0. {
                    rows.swap(j - 1, j);
                    j -= 1;
                }
            }
            let Native::A(dst) = target.as_ref() else {
                panic!("sort")
            };
            *dst.borrow_mut() = rows;
            target.clone()
        }
        "valueOf" => {
            let Native::Date(x) = target.as_ref() else {
                panic!("date")
            };
            n(*x)
        }
        "subtract" => {
            let Native::Date(x) = target.as_ref() else {
                panic!("date")
            };
            assert_eq!(text(&args[1]), "m");
            Rc::new(Native::Date(*x - num(&args[0]) * 60_000.))
        }
        _ => panic!("member {} in {}", key, t.method),
    })
}
fn response(t: &State, endpoint: &str, args: &[V]) -> V {
    let params = if args.len() > 1 {
        get(&args[1], "params")
    } else {
        u()
    };
    let page = num(&get(&params, "pageNum"));
    let bad = t.case == "bad" || t.case == "bad-later" && page == 2.;
    let body = match t.method {
        "orders" => {
            let count = if t.case == "non-array" {
                0
            } else if t.case == "missing-total" && page == 1. {
                50
            } else {
                1
            };
            let rows = array(
                (0..count)
                    .map(|i| {
                        object(vec![(
                            "id",
                            s(&format!("P{}-{}", safe_cex_num::js_number_string(page), i)),
                        )])
                    })
                    .collect(),
            );
            let mut fields = vec![("items", if t.case == "non-array" { s("NO") } else { rows })];
            if t.case != "missing-total" {
                fields.push((
                    "totalPage",
                    n(if t.case == "max-pages" {
                        101.
                    } else if t.case == "paged" || t.case == "bad-later" {
                        2.
                    } else {
                        1.
                    }),
                ));
            }
            object(fields)
        }
        "place" => object(if t.case == "empty-id" {
            vec![]
        } else {
            vec![("orderId", s("PLACED"))]
        }),
        "update" => object(if t.case == "fallback" {
            vec![]
        } else {
            vec![("newOrderId", s("REPLACED"))]
        }),
        "fills" => object(vec![("items", t.raw.clone())]),
        _ => t.raw.clone(),
    };
    assert_eq!(
        endpoint,
        match t.method {
            "balance" => self::endpoint("ACCOUNTS"),
            "orders" => self::endpoint("OPEN_ORDERS"),
            "place" => self::endpoint("PLACE_ORDER"),
            "update" => self::endpoint("MODIFY_ORDER"),
            "fills" => self::endpoint("FILLS"),
            "ohlcv" => self::endpoint("KLINE"),
            _ => panic!("response"),
        }
    );
    object(vec![(
        "data",
        object(vec![
            ("code", s(if bad { "400000" } else { "200000" })),
            ("msg", s(if bad { "DENIED" } else { "" })),
            ("data", body),
        ]),
    )])
}
fn ops(t: Rc<State>) -> Rc<SpotBodyOps<V, E>> {
    let read = t.clone();
    let invoke = t.clone();
    let close = t.clone();
    let assignment = t.clone();
    let value = Rc::new(StreamOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |path| {
            Ok(match path {
                "this.store.loaded" => get(&read.store, "loaded"),
                "this.store.balance" => get(&read.store, "balance"),
                "this.store.orders" => get(&read.store, "orders"),
                "this.store.marketsBySymbol" => get(&read.store, "marketsBySymbol"),
                "this.isDisposed" => b(read.disposed.get()),
                "this.isPublicDataOnly" => {
                    b(read.case == "public" || read.case == "external-public")
                }
                "KUCOIN_SPOT_INTERVAL" => object(vec![("1m", s("1min"))]),
                path if path.starts_with("KUCOIN_SPOT_ENDPOINTS.") => {
                    s(endpoint(path.split('.').nth(1).unwrap()))
                }
                "OrderType.Limit" => s("limit"),
                "OrderType.Market" => s("market"),
                "OrderSide.Sell" => s("sell"),
                "OrderSide.Buy" => s("buy"),
                path if path.starts_with("OrderTimeInForce.") => s(path.split('.').nth(1).unwrap()),
                _ => Rc::new(Native::Call(path.into())),
            })
        }),
        invoke: Rc::new(move |callee, args| {
            if let Native::AsyncArrow(body) = callee.as_ref() {
                return Ok(promise(run(body(args))));
            }
            if let Native::Member(target, key) = callee.as_ref() {
                return invoke_member(&invoke, target.clone(), key, args);
            }
            let Native::Call(name) = callee.as_ref() else {
                panic!("callee")
            };
            let t = &invoke;
            Ok(match name.as_str() {
                "parseFloat" => n(safe_cex_num::parse_float(&text(&args[0]))),
                "parseInt" => {
                    assert_eq!(num(&args[1]), 10.);
                    n(safe_cex_num::parse_int(&text(&args[0]), Some(10)))
                }
                "Number.isFinite" => b(matches!(args[0].as_ref(),Native::N(x) if x.is_finite())),
                "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
                "Math.min" => n(num(&args[0]).min(num(&args[1]))),
                "Math.floor" => n(num(&args[0]).floor()),
                "Date.now" => n(5_000_000.),
                "dayjs" => Rc::new(Native::Date(if args.is_empty() {
                    5_000_000.
                } else {
                    num(&args[0])
                })),
                "adjust" => n(safe_cex_num::adjust(num(&args[0]), num(&args[1]))),
                "omitUndefined" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("omit")
                    };
                    let result = object(vec![]);
                    for (k, v) in fields.borrow().iter() {
                        if !matches!(v.as_ref(), Native::U) {
                            put(&result, k, v.clone());
                        }
                    }
                    result
                }
                "uuid" => s("UUID"),
                "Promise.all" => {
                    let mut rows = vec![];
                    let mut first = None;
                    for value in items(&args[0]) {
                        match run(await_ready(value)) {
                            Ok(value) => rows.push(value),
                            Err(e) => {
                                if first.is_none() {
                                    first = Some(e)
                                }
                            }
                        }
                    }
                    promise(match first {
                        Some(e) => Err(e),
                        None => Ok(array(rows)),
                    })
                }
                "this.initExternalPublicData" => {
                    event(t, "external", vec![]);
                    if t.case == "reject" {
                        promise(Err(t.err.clone()))
                    } else {
                        promise(Ok(b(t.case == "external" || t.case == "external-public")))
                    }
                }
                "this.fetchMarkets" => {
                    event(t, "fetchMarkets", vec![]);
                    if t.case == "dispose-markets" {
                        t.disposed.set(true);
                    }
                    promise(Ok(t.markets.clone()))
                }
                "this.fetchTickers" => {
                    event(t, "fetchTickers", vec![]);
                    promise(Ok(t.tickers.clone()))
                }
                "this.fetchPositions" => {
                    event(t, "fetchPositions", vec![]);
                    if t.case == "dispose-positions" {
                        t.disposed.set(true);
                    }
                    promise(Ok(t.positions.clone()))
                }
                "this.fetchOrders" => {
                    event(t, "fetchOrders", vec![]);
                    promise(if t.case == "reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(t.orders.clone())
                    })
                }
                "this.fetchBalance" => {
                    event(t, "fetchBalance", vec![]);
                    if t.case == "dispose-all" {
                        t.disposed.set(true);
                    }
                    promise(Ok(t.balance.clone()))
                }
                "this.tick" => {
                    if t.timer_started.get() {
                        event(t, "timerTick", vec![]);
                        let out = promise(Ok(s("TIMER")));
                        *t.timer_return.borrow_mut() = Some(out.clone());
                        out
                    } else {
                        event(t, "tick", vec![]);
                        if t.case == "dispose-tick" {
                            t.disposed.set(true);
                        }
                        promise(Ok(u()))
                    }
                }
                "this.getTickInterval" => n(77.),
                "loop" => {
                    *t.timer.borrow_mut() = Some(args[0].clone());
                    event(t, "loop", vec![args[1].clone()]);
                    u()
                }
                "this.publicWebsocket.connectAndSubscribe" => {
                    event(t, "publicConnect", vec![]);
                    u()
                }
                "this.privateWebsocket.connectAndSubscribe" => {
                    event(t, "privateConnect", vec![]);
                    u()
                }
                "this.store.update" => {
                    event(t, "update", args.clone());
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("update")
                    };
                    for (k, v) in fields.borrow().iter() {
                        put(&t.store, k, v.clone());
                    }
                    u()
                }
                "this.store.addOrUpdatePositionsFull" => {
                    event(t, "positions", args);
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
                "this.getUsdPrice" => {
                    if t.case == "close" {
                        return Err(t.err.clone());
                    }
                    if text(&args[0]) == "ETH" || text(&args[0]) == "__proto__" {
                        n(10.)
                    } else {
                        n(1.)
                    }
                }
                "this.fetchActiveOrderSymbols" => {
                    event(t, "activeSymbols", vec![]);
                    promise(Ok(array(if t.case == "empty" {
                        vec![]
                    } else {
                        vec![s("SYM")]
                    })))
                }
                "this.mapOrder" => {
                    t.mapped_orders.borrow_mut().push(args[0].clone());
                    args[0].clone()
                }
                "this.resolveCallerClientOrderId" => {
                    event(t, "resolveClientId", args);
                    if t.case == "market" { u() } else { s("CLIENT") }
                }
                "this.xhr.get" | "this.unlimitedXHR.post" => {
                    event(
                        t,
                        if name == "this.xhr.get" {
                            "get"
                        } else {
                            "post"
                        },
                        args.clone(),
                    );
                    t.requests.borrow_mut().push(args.clone());
                    if t.case == "reject" {
                        promise(Err(t.err.clone()))
                    } else {
                        promise(Ok(response(t, &text(&args[0]), &args)))
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
                    for (k, v) in fields.borrow().iter() {
                        put(&args[0], k, v.clone());
                    }
                    u()
                }
                "template-string" => s(&text(&args[0])),
                "template-loaded-spot-markets" => {
                    s(&format!("Loaded {} KuCoin Spot markets", text(&args[0])))
                }
                "template-missing-market" => s(&format!("Market {} not found", text(&args[0]))),
                "template-unsupported-spot-order" => s(&format!(
                    "KuCoin Spot does not support {} orders",
                    text(&args[0])
                )),
                "template-update-spot-order" => s(&format!(
                    "Failed to update KuCoin Spot order {}",
                    text(&args[0])
                )),
                "add" => n(num(&args[0]) + num(&args[1])),
                "multiply" => n(num(&args[0]) * num(&args[1])),
                "divide" => n(num(&args[0]) / num(&args[1])),
                "subtract" => n(num(&args[0]) - num(&args[1])),
                "lte" => b(num(&args[0]) <= num(&args[1])),
                "lt" => b(num(&args[0]) < num(&args[1])),
                "gte" => b(num(&args[0]) >= num(&args[1])),
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
        map: Rc::new(|_, _| unused()),
        filter: Rc::new(|_, _| unused()),
        for_of: Rc::new(|_, _| unused()),
        await_value: Rc::new(await_ready),
        spawn: Rc::new(|_| unused()),
        error_value: Rc::new(|e| Rc::new(Native::Err(e.clone()))),
    });
    let base = Rc::new(BootstrapOps {
        value,
        raw_filter: Rc::new(|_, _| unused()),
        member: Rc::new(|v, key| Ok(Rc::new(Native::Member(v, key.into())))),
        boolean: Rc::new(b),
        mapper: Rc::new(|cb| Rc::new(Native::Mapper(cb))),
        async_mapper: Rc::new(|cb| Rc::new(Native::AsyncMapper(cb))),
        callback: Rc::new(|cb| Rc::new(Native::Callback(cb))),
        response_pair: Rc::new(|_| unused()),
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
        some: Rc::new(|_, _| unused()),
    });
    Rc::new(SpotBodyOps {
        base,
        async_arrow: Rc::new(|name, length, body| {
            assert_eq!(
                name, "fetchSymbolOrders",
                "KUCOIN-BODY named inner async arrow"
            );
            assert_eq!(length, 1, "KUCOIN-BODY inner async arity");
            Rc::new(Native::AsyncArrow(body))
        }),
        triple: Rc::new(|v| {
            let rows = items(&v);
            Ok((rows[0].clone(), rows[1].clone(), rows[2].clone()))
        }),
        has: Rc::new(|key, v| {
            let Native::O(fields) = v.as_ref() else {
                panic!("has")
            };
            Ok(fields.borrow().iter().any(|(k, _)| k == key))
        }),
        new_error: Rc::new(|ctor, msg| {
            assert!(matches!(ctor.as_ref(),Native::Call(name)if name=="Error"));
            Ok(Rc::new(Error {
                name: "Error",
                message: text(&msg),
                fresh: true,
                code: None,
                ids: None,
                response: u(),
            }))
        }),
        comparator: Rc::new(|cb| Rc::new(Native::Comparator(cb))),
        spread_arguments: Rc::new(|v| Ok(items(&v))),
        assignment_reference: Rc::new(|target, key| {
            Ok(Rc::new(Native::Assignment(target, text(&key))))
        }),
        assign: Rc::new(move |reference, value| {
            let Native::Assignment(target, key) = reference.as_ref() else {
                panic!("assignment")
            };
            if key == "__proto__" {
                assignment
                    .prototypes
                    .borrow_mut()
                    .push((target.clone(), value));
            } else {
                put(target, key, value);
            }
            Ok(())
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
            "dispose-positions",
            "dispose-tick",
            "reject",
        ],
    ),
    (
        "tick",
        &["normal", "public", "disposed", "dispose-all", "reject"],
    ),
    (
        "balance",
        &["normal", "bad", "reject", "empty", "close", "prototype"],
    ),
    (
        "orders",
        &[
            "explicit",
            "discovery",
            "empty",
            "paged",
            "missing-total",
            "bad-later",
            "reject",
            "max-pages",
            "non-array",
        ],
    ),
    (
        "place",
        &[
            "limit",
            "market",
            "fok",
            "post",
            "unsupported",
            "attached",
            "bad",
            "reject",
            "missing",
            "empty-id",
        ],
    ),
    (
        "update",
        &[
            "normal",
            "fallback",
            "undefined",
            "missing",
            "bad",
            "reject",
        ],
    ),
    ("fills", &["normal", "missing", "bad", "reject", "defaults"]),
    (
        "ohlcv",
        &["normal", "from", "limit", "zero-limit", "bad", "reject"],
    ),
];
fn execute(t: &Rc<State>, h: Rc<SpotBodyOps<V, E>>) -> (Result<V, E>, V) {
    let result = match t.method {
        "start" => run(port::start(h)),
        "tick" => run(port::tick(h)),
        "balance" => run(port::fetch_balance(h)),
        "orders" => run(port::fetch_orders(h, t.symbols.clone())),
        "place" => run(port::place_order(h, t.opts.clone())),
        "update" => run(port::update_order(h, t.opts.clone())),
        "fills" => run(port::fetch_historical_fills(h, t.opts.clone())),
        "ohlcv" => run(port::fetch_ohlcv(h, t.opts.clone())),
        _ => panic!("method"),
    };
    let timer = t.timer.borrow().clone();
    let timer = if let Some(cb) = timer {
        let Native::Callback(cb) = cb.as_ref() else {
            panic!("timer")
        };
        t.timer_started.set(true);
        let returned = cb().expect("timer callback");
        assert!(
            Rc::ptr_eq(&returned, t.timer_return.borrow().as_ref().unwrap()),
            "TICK native original callback return"
        );
        run(await_ready(returned)).expect("timer result")
    } else {
        u()
    };
    (result, timer)
}
fn number(v: &V, expected: f64, label: &str) {
    let Native::N(actual) = v.as_ref() else {
        panic!("{label}: native number")
    };
    assert_eq!(actual.to_bits(), expected.to_bits(), "{label}");
}
fn string(v: &V, expected: &str, label: &str) {
    let Native::S(actual) = v.as_ref() else {
        panic!("{label}: native string")
    };
    assert_eq!(actual, expected, "{label}");
}
fn boolean(v: &V, expected: bool, label: &str) {
    let Native::B(actual) = v.as_ref() else {
        panic!("{label}: native boolean")
    };
    assert_eq!(*actual, expected, "{label}");
}
fn named(t: &State, name: &str) -> Vec<Vec<V>> {
    t.events
        .borrow()
        .iter()
        .filter(|row| text(&row[0]) == name)
        .cloned()
        .collect()
}
fn assert_native(t: &State, result: &Result<V, E>, timer: &V) {
    let rejected = matches!(
        (t.method, t.case),
        ("start", "reject")
            | (
                "place",
                "unsupported" | "attached" | "bad" | "reject" | "missing"
            )
            | ("update", "missing" | "bad" | "reject")
            | ("ohlcv", "reject")
    );
    assert_eq!(
        result.is_err(),
        rejected,
        "SPOT native independent rejection: {}/{}",
        t.method,
        t.case
    );
    let expected_emits = match (t.method, t.case) {
        ("start", _) | ("ohlcv", "reject") | ("place" | "update", "missing") => 0,
        (_, "bad" | "bad-later" | "reject" | "close" | "unsupported" | "attached") => 1,
        _ => 0,
    };
    assert_eq!(
        named(t, "emit").len(),
        expected_emits,
        "DIAGNOSTIC native independent emission count"
    );
    for row in named(t, "emit") {
        string(
            &row[1],
            match t.case {
                "bad" | "bad-later" => "DENIED",
                "unsupported" => "KuCoin Spot does not support stop_loss orders",
                "attached" => "KuCoin Spot attached TP/SL orders are not supported",
                _ => "VENUE",
            },
            "DIAGNOSTIC native literal message",
        );
    }
    if t.method == "place" {
        let calls = named(t, "resolveClientId");
        assert_eq!(calls.len(), 1, "PLACE native caller resolution count");
        assert!(
            Rc::ptr_eq(&calls[0][1], &t.opts),
            "PLACE native original caller options"
        );
    }
    if let Err(e) = result {
        let original = t.case == "reject";
        if original {
            assert!(Rc::ptr_eq(e, &t.err), "ERROR native original identity");
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "DOWN");
            assert_eq!(e.code, Some("READ"));
            assert_eq!(e.ids, Some([3, 7]));
            assert!(Rc::ptr_eq(&e.response, &t.err.response));
        } else {
            assert!(e.fresh, "ERROR native constructed error");
            assert_eq!(e.code, None, "ERROR native constructed code absent");
            assert_eq!(e.ids, None, "ERROR native constructed ids absent");
            assert!(
                matches!(e.response.as_ref(), Native::U),
                "ERROR native constructed response absent"
            );
            assert_eq!(e.name, "Error");
            assert_eq!(
                e.message,
                match t.case {
                    "missing" => "Market SYM not found",
                    "unsupported" => "KuCoin Spot does not support stop_loss orders",
                    "attached" => "KuCoin Spot attached TP/SL orders are not supported",
                    "bad" => "DENIED",
                    _ => panic!("unexpected error"),
                }
            );
        }
        assert_eq!(
            named(t, "emit").len(),
            if t.method == "start" || t.method == "ohlcv" || t.case == "missing" {
                0
            } else {
                1
            },
            "ERROR native emission count"
        );
        if t.case == "missing" || t.case == "unsupported" || t.case == "attached" {
            assert_eq!(
                t.requests.borrow().len(),
                0,
                "WRITE native pre-request rejection"
            );
        }
        return;
    }
    let out = result.as_ref().unwrap();
    match t.method {
        "start" => {
            assert!(
                matches!(out.as_ref(), Native::U),
                "START native undefined result"
            );
            let external = t.case == "external" || t.case == "external-public";
            let early = t.case == "dispose-markets";
            assert_eq!(
                named(t, "update").len(),
                if early {
                    0
                } else {
                    usize::from(!external) * 2
                        + usize::from(t.case == "public" || t.case == "external-public")
                },
                "START native independent update count"
            );
            let flags = get(&t.store, "loaded");
            boolean(&get(&flags, "seed"), true, "START native seed flag");
            if !external && !early {
                boolean(&get(&flags, "markets"), true, "START native markets loaded");
                boolean(&get(&flags, "tickers"), true, "START native tickers loaded");
                assert!(Rc::ptr_eq(&get(&t.store, "markets"), &t.markets));
                assert!(Rc::ptr_eq(&get(&t.store, "tickers"), &t.tickers));
            }
            assert_eq!(
                named(t, "publicConnect").len(),
                usize::from(!external && !early)
            );
            assert_eq!(
                named(t, "positions").len(),
                usize::from(!early && t.case != "dispose-positions")
            );
            if !early && t.case != "dispose-positions" {
                assert!(
                    Rc::ptr_eq(&named(t, "positions")[0][1], &t.positions),
                    "START native original positions"
                );
            }
            let public = t.case == "public" || t.case == "external-public";
            assert_eq!(
                named(t, "privateConnect").len(),
                usize::from(!early && t.case != "dispose-positions" && !public)
            );
            if public {
                assert_eq!(items(&get(&t.store, "orders")).len(), 0);
                for key in ["used", "free", "total", "upnl"] {
                    number(
                        &get(&get(&t.store, "balance"), key),
                        0.,
                        "START native public zero balance",
                    );
                }
                for key in ["balance", "positions", "orders"] {
                    boolean(&get(&flags, key), true, "START native public loaded");
                }
            }
        }
        "tick" => {
            assert!(matches!(out.as_ref(), Native::U));
            let skip = t.case == "public" || t.case == "disposed";
            let stopped = t.case == "dispose-all";
            for key in ["fetchOrders", "fetchPositions", "fetchBalance"] {
                assert_eq!(
                    named(t, key).len(),
                    usize::from(!skip),
                    "TICK native all requests initiate"
                );
            }
            assert_eq!(
                named(t, "update").len(),
                usize::from(!skip && !stopped && t.case != "reject"),
                "TICK native independent update count"
            );
            assert_eq!(
                named(t, "positions").len(),
                usize::from(!skip && !stopped && t.case != "reject")
            );
            assert_eq!(
                named(t, "loop").len(),
                usize::from(!skip && !stopped),
                "TICK native loop floor"
            );
            if !skip && !stopped {
                string(timer, "TIMER", "TICK native callback value");
                number(&named(t, "loop")[0][1], 77., "TICK native interval");
            }
            if !skip && !stopped && t.case != "reject" {
                assert!(Rc::ptr_eq(&get(&t.store, "orders"), &t.orders));
                assert!(Rc::ptr_eq(&get(&t.store, "balance"), &t.balance));
                assert!(Rc::ptr_eq(&named(t, "positions")[0][1], &t.positions));
                for key in ["balance", "positions", "orders"] {
                    boolean(
                        &get(&get(&t.store, "loaded"), key),
                        true,
                        "TICK native loaded",
                    );
                }
            }
        }
        "balance" => {
            assert_eq!(t.requests.borrow().len(), 1);
            string(
                &get(&get(&t.requests.borrow()[0][1], "params"), "type"),
                "trade",
                "BALANCE native query",
            );
            if matches!(t.case, "bad" | "reject" | "close") {
                assert!(
                    Rc::ptr_eq(out, &t.balance),
                    "BALANCE native original fallback"
                );
                assert_eq!(named(t, "emit").len(), 1);
                string(
                    &named(t, "emit")[0][1],
                    if t.case == "bad" { "DENIED" } else { "VENUE" },
                    "BALANCE native emitted message",
                );
                assert_eq!(
                    t.closes.get(),
                    usize::from(t.case == "close"),
                    "BALANCE native iterator close"
                );
            } else {
                for (key, value) in [("free", 23.), ("used", 6.), ("total", 29.), ("upnl", 0.)] {
                    number(
                        &get(out, key),
                        if t.case == "empty" { 0. } else { value },
                        "BALANCE native aggregate",
                    );
                }
                assert!(Rc::ptr_eq(&get(out, "info"), &t.raw));
                if t.case == "prototype" {
                    let prototypes = t.prototypes.borrow();
                    assert_eq!(
                        prototypes.len(),
                        2,
                        "BALANCE native prototype assignment count"
                    );
                    for (index, key) in ["currencies", "coins"].into_iter().enumerate() {
                        let target = get(out, key);
                        assert!(
                            Rc::ptr_eq(&prototypes[index].0, &target),
                            "BALANCE native assigned prototype target"
                        );
                        let Native::O(fields) = target.as_ref() else {
                            panic!("native object")
                        };
                        assert_eq!(
                            fields.borrow().len(),
                            1,
                            "BALANCE native prototype excluded from own fields"
                        );
                        assert_eq!(fields.borrow()[0].0, "USDT");
                    }
                    assert!(
                        Rc::ptr_eq(&get(&prototypes[0].1, "info"), &items(&t.raw)[0]),
                        "BALANCE native prototype original account"
                    );
                    number(
                        &get(&prototypes[0].1, "free"),
                        2.,
                        "BALANCE native prototype free",
                    );
                    number(
                        &get(&prototypes[1].1, "notional"),
                        25.,
                        "BALANCE native prototype notional",
                    );
                } else if t.case != "empty" {
                    let eth = get(&get(out, "currencies"), "ETH");
                    number(&get(&eth, "free"), 2., "BALANCE native coin free");
                    number(&get(&eth, "used"), 0.5, "BALANCE native coin holds");
                    number(&get(&eth, "total"), 2.5, "BALANCE native coin balance");
                    assert!(Rc::ptr_eq(&get(&eth, "info"), &items(&t.raw)[0]));
                    number(
                        &get(&get(&get(out, "coins"), "ETH"), "notional"),
                        25.,
                        "BALANCE native coin notional",
                    );
                }
            }
        }
        "orders" => {
            if t.case == "reject" {
                assert!(
                    Rc::ptr_eq(out, &t.orders),
                    "ORDERS native original fallback"
                );
            } else {
                let rows = items(out);
                let count = match t.case {
                    "empty" | "non-array" => 0,
                    "paged" => 2,
                    "missing-total" => 50,
                    "max-pages" => 100,
                    _ => 1,
                };
                assert_eq!(rows.len(), count, "ORDERS native independent output count");
                assert_eq!(
                    t.mapped_orders.borrow().len(),
                    count,
                    "ORDERS native independent mapper count"
                );
                for (index, row) in rows.iter().enumerate() {
                    assert!(
                        Rc::ptr_eq(row, &t.mapped_orders.borrow()[index]),
                        "ORDERS native original mapped row"
                    );
                }
                if count > 0 {
                    string(
                        &get(&rows[0], "id"),
                        "P1-0",
                        "ORDERS native first mapped order",
                    );
                }
                if t.case == "paged" {
                    string(&get(&rows[1], "id"), "P2-0", "ORDERS native second page");
                }
                if t.case == "max-pages" {
                    string(&get(&rows[99], "id"), "P100-0", "ORDERS native page cap");
                }
            }
            let requests = t.requests.borrow();
            let expected = match t.case {
                "empty" => 0,
                "paged" | "bad-later" => 2,
                "max-pages" => 100,
                _ => 1,
            };
            assert_eq!(
                requests.len(),
                expected,
                "ORDERS native independent request count"
            );
            for (i, row) in requests.iter().enumerate() {
                string(
                    &row[0],
                    "/api/v1/hf/orders/active/page",
                    "ORDERS native endpoint",
                );
                let query = get(&row[1], "params");
                string(&get(&query, "symbol"), "SYM", "ORDERS native symbol");
                number(
                    &get(&query, "pageNum"),
                    (i + 1) as f64,
                    "ORDERS native page sequence",
                );
                number(&get(&query, "pageSize"), 50., "ORDERS native page size");
            }
            assert_eq!(
                named(t, "activeSymbols").len(),
                usize::from(t.case == "discovery" || t.case == "empty")
            );
        }
        "place" => {
            let rows = items(out);
            assert_eq!(
                rows.len(),
                usize::from(t.case != "empty-id"),
                "PLACE native result count"
            );
            if !rows.is_empty() {
                string(&rows[0], "PLACED", "PLACE native returned id");
            }
            assert_eq!(t.requests.borrow().len(), 1);
            let payload = t.requests.borrow()[0][1].clone();
            string(&get(&payload, "symbol"), "API", "PLACE native API symbol");
            string(&get(&payload, "size"), "2.4", "PLACE native size");
            string(&get(&payload, "side"), "sell", "PLACE native side");
            string(
                &get(&payload, "clientOid"),
                if t.case == "market" { "UUID" } else { "CLIENT" },
                "PLACE native caller ID",
            );
            string(
                &get(&payload, "type"),
                if t.case == "market" {
                    "market"
                } else {
                    "limit"
                },
                "PLACE native order type",
            );
            if t.case == "market" {
                assert!(matches!(get(&payload, "price").as_ref(), Native::U));
                assert!(matches!(get(&payload, "timeInForce").as_ref(), Native::U));
            } else {
                string(
                    &get(&payload, "price"),
                    "12.5",
                    "PLACE native adjusted price",
                );
                string(
                    &get(&payload, "timeInForce"),
                    match t.case {
                        "fok" => "FOK",
                        "post" => "GTC",
                        _ => "IOC",
                    },
                    "PLACE native time in force",
                );
            }
            if t.case == "post" {
                boolean(&get(&payload, "postOnly"), true, "PLACE native post only");
            }
        }
        "update" => {
            let rows = items(out);
            assert_eq!(rows.len(), 1, "UPDATE native id count");
            string(
                &rows[0],
                if t.case == "fallback" {
                    "ORIGINAL"
                } else {
                    "REPLACED"
                },
                "UPDATE native replacement fallback",
            );
            let payload = t.requests.borrow()[0][1].clone();
            string(
                &get(&payload, "orderId"),
                "ORIGINAL",
                "UPDATE native original id",
            );
            string(&get(&payload, "symbol"), "API", "UPDATE native API symbol");
            if t.case == "undefined" {
                assert!(matches!(get(&payload, "newPrice").as_ref(), Native::U));
                assert!(matches!(get(&payload, "newSize").as_ref(), Native::U));
            } else {
                string(&get(&payload, "newPrice"), "12.5", "UPDATE native price");
                string(&get(&payload, "newSize"), "2.4", "UPDATE native size");
            }
        }
        "fills" => {
            let rows = items(out);
            let empty = matches!(t.case, "missing" | "bad" | "reject");
            assert_eq!(
                rows.len(),
                usize::from(!empty),
                "FILLS native independent count"
            );
            assert_eq!(t.requests.borrow().len(), usize::from(t.case != "missing"));
            if !empty {
                let row = &rows[0];
                number(
                    &get(row, "timestamp"),
                    if t.case == "defaults" {
                        5_000_000.
                    } else {
                        1234.
                    },
                    "FILLS native timestamp",
                );
                for (key, value) in [("price", 12.5), ("qty", 2.), ("commission", 0.25)] {
                    number(
                        &get(row, key),
                        if t.case == "defaults" { 0. } else { value },
                        "FILLS native decoded number",
                    );
                }
                string(
                    &get(row, "side"),
                    if t.case == "defaults" { "buy" } else { "sell" },
                    "FILLS native side",
                );
                string(&get(row, "orderId"), "F", "FILLS native order id");
                string(
                    &get(row, "commissionAsset"),
                    "USDT",
                    "FILLS native commission asset",
                );
            }
            if t.case != "missing" {
                let params = get(&t.requests.borrow()[0][1], "params");
                number(&get(&params, "limit"), 100., "FILLS native request clamp");
            }
        }
        "ohlcv" => {
            let rows = items(out);
            assert_eq!(
                rows.len(),
                if t.case == "bad" { 0 } else { 2 },
                "OHLCV native independent count"
            );
            if t.case != "bad" {
                for (i, (timestamp, open, close, high, low, volume)) in
                    [(100., 1., 2., 3., 0.5, 7.5), (300., 3., 4., 5., 2., 6.)]
                        .into_iter()
                        .enumerate()
                {
                    for (key, value) in [
                        ("timestamp", timestamp),
                        ("open", open),
                        ("close", close),
                        ("high", high),
                        ("low", low),
                        ("volume", volume),
                    ] {
                        number(
                            &get(&rows[i], key),
                            value,
                            "OHLCV native ordered decoded candle",
                        );
                    }
                }
            }
            let params = get(&t.requests.borrow()[0][1], "params");
            number(
                &get(&params, "startAt"),
                match t.case {
                    "from" | "zero-limit" => 1000.,
                    "limit" => 30000.,
                    _ => -25000.,
                },
                "OHLCV native start seconds",
            );
            number(
                &get(&params, "endAt"),
                if t.case == "limit" { 120000. } else { 5000. },
                "OHLCV native end seconds",
            );
            string(&get(&params, "type"), "1min", "OHLCV native interval");
        }
        _ => panic!("assert method"),
    }
}
#[test]
fn spot_bodies_native_before_source_oracle() {
    let mut cases = vec![];
    let mut runs = vec![];
    for (method, family) in FAMILIES {
        for case in *family {
            let t = fixture(method, case);
            let (result, timer) = execute(&t, ops(t.clone()));
            assert_native(&t, &result, &timer);
            cases.push(json!({"method":method,"case":case}));
            runs.push((t, result, timer));
        }
    }
    assert_eq!(cases.len(), 55, "SPOT independent fixture floor");
    let native=runs.into_iter().map(|(t,result,timer)|{
        let result=match result {Ok(v)=>json!({"ok":pack(&v)}),Err(e)=>json!({"error":{"name":e.name,"message":e.message,"fresh":e.fresh,"code":e.code,"ids":e.ids}})};
        json!({"method":t.method,"case":t.case,"result":result,"timer":pack(&timer),"closes":t.closes.get(),"events":t.events.borrow().iter().map(|row|row.iter().map(pack).collect::<Vec<_>>()).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    let oracle =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spot-bodies-oracle.mjs");
    let mut child = Command::new("node")
        .arg(oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("node oracle");
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source, json!(native), "SPOT actual-source fixture floor");
}
