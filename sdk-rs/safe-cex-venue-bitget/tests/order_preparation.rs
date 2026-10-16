use safe_cex_venue_bitget::{
    order_placement::*,
    order_preparation::{self as body, *},
    rest_actions::*,
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
#[derive(Clone)]
enum V {
    Hole,
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
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("PREP native array required"),
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
            "PREP native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "PREP native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "PREP native independent array count {path}"
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
                "PREP native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "PREP native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "PREP native scalar {path}: {actual:?} != {expected:?}"
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
    panic!("PREP bounded fixture future did not settle")
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
            assert_eq!(a.len(), b.len(), "PREP source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "PREP source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("PREP source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "PREP source value {path}"),
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
    assert!(equal(a, b), "PREP native original reference {why}")
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
    opts: V,
    market: V,
    related: V,
    positions: V,
    fetched: V,
    cached: V,
    flight: V,
    state: Rc<RefCell<V>>,
    events: Rc<RefCell<Vec<Event>>>,
    original: E,
    rollback: E,
    format_at: Rc<RefCell<usize>>,
    place_at: Rc<RefCell<usize>>,
    formatted: Rc<RefCell<Vec<V>>>,
}
fn position(side: &str, contracts: f64) -> V {
    o(vec![
        ("symbol", s("BTCUSD")),
        ("side", s(side)),
        ("contracts", V::N(contracts)),
    ])
}
fn fixture(op: &'static str, scene: &'static str) -> H {
    let normal = op == "classic" || op == "uta";
    let strategy = op == "strategy";
    let typ = if normal {
        if scene == "market" {
            "market"
        } else if scene == "unsupported" {
            "conditional"
        } else {
            "limit"
        }
    } else if strategy {
        if scene == "take-profit" {
            "take_profit_market"
        } else if scene == "unsupported" {
            "limit"
        } else {
            "stop_market"
        }
    } else if op.contains("algo") {
        if scene == "take" {
            "take_profit_market"
        } else if scene.contains("trailing") || scene == "missing-ticker" || scene == "margin-error"
        {
            "trailing_stop_market"
        } else {
            "stop_market"
        }
    } else if scene == "trailing" {
        "trailing_stop_market"
    } else {
        "stop_market"
    };
    let opts = o(vec![
        ("symbol", s("BTCUSD")),
        ("type", s(typ)),
        ("side", s(if normal { "buy" } else { "sell" })),
        ("price", V::N(if op.contains("algo") { 80. } else { 101.2 })),
        (
            "amount",
            V::N(
                if ["remainder", "zero-max", "negative-max", "caller-split"].contains(&scene) {
                    13.
                } else if scene == "split" {
                    10.
                } else if scene == "zero" {
                    0.
                } else {
                    4.
                },
            ),
        ),
        ("reduceOnly", V::Bool(true)),
        ("takeProfit", V::N(110.3)),
        ("stopLoss", V::N(90.2)),
        ("tpTriggerType", s("last_price")),
        ("slTriggerType", s("mark_price")),
    ]);
    if scene == "caller-id" || scene == "caller-split" {
        put(&opts, units(&s("clientOid")), s("caller"));
    }
    if ["no-price", "missing-price"].contains(&scene) {
        put(&opts, units(&s("price")), V::N(0.));
    }
    let market = if scene == "missing-market" {
        V::U
    } else {
        o(vec![
            ("symbol", s("BTCUSD")),
            ("id", s("BTCUSDT")),
            (
                "precision",
                o(vec![("price", V::N(0.5)), ("amount", V::N(1.))]),
            ),
            (
                "limits",
                o(vec![(
                    "amount",
                    o(vec![(
                        "max",
                        V::N(if scene == "zero-max" {
                            0.
                        } else if scene == "negative-max" {
                            -5.
                        } else {
                            5.
                        }),
                    )]),
                )]),
            ),
        ])
    };
    let parent = o(vec![
        ("id", s("parent")),
        ("symbol", s("BTCUSD")),
        ("price", V::N(101.2)),
        ("amount", V::N(4.2)),
    ]);
    let stop = o(vec![
        ("id", s("parent__stop_loss")),
        ("symbol", s("BTCUSD")),
        ("price", V::N(90.)),
        ("amount", V::N(4.2)),
    ]);
    let take = o(vec![
        ("id", s("parent__take_profit")),
        ("symbol", s("BTCUSD")),
        ("price", V::N(110.)),
        ("amount", V::N(4.2)),
    ]);
    let related = if scene == "missing-parent" {
        a(vec![stop.clone(), take.clone()])
    } else {
        a(vec![parent.clone(), stop.clone(), take.clone()])
    };
    let opts = if op == "virtual" {
        let order = if scene == "stop" {
            stop
        } else if scene == "take" {
            take
        } else {
            parent
        };
        let update = if scene == "amount" {
            o(vec![("amount", V::N(7.2))])
        } else if scene == "undefined" {
            o(vec![("price", V::U), ("amount", V::U)])
        } else {
            o(vec![("price", V::N(120.2))])
        };
        o(vec![("order", order), ("update", update)])
    } else if op == "replace" {
        let order = o(vec![
            ("id", s("strategy-id")),
            ("symbol", s("BTCUSD")),
            (
                "type",
                s(if scene == "trailing" {
                    "trailing_stop_market"
                } else {
                    "stop_market"
                }),
            ),
            ("side", s("sell")),
            ("price", V::N(91.)),
            ("amount", V::N(3.)),
        ]);
        let update = if scene == "amount" {
            o(vec![("amount", V::N(7.))])
        } else if scene == "undefined" {
            o(vec![("price", V::U), ("amount", V::U)])
        } else {
            o(vec![("price", V::N(120.))])
        };
        o(vec![("order", order), ("update", update)])
    } else {
        opts
    };
    let positions = if op == "cache" && scene == "store" {
        a(vec![V::Hole, position("short", 7.), position("long", 2.)])
    } else if strategy && scene.starts_with("live") {
        a(vec![position("short", 9.), position("long", 3.)])
    } else if strategy && scene == "tiny" {
        a(vec![position("long", 0.2)])
    } else {
        a(vec![])
    };
    let fetched = if scene == "empty" {
        a(vec![])
    } else {
        a(vec![position("long", 5.)])
    };
    let cached = if scene == "cache" || scene == "expired" {
        o(vec![
            ("at", V::N(if scene == "expired" { -4000. } else { 900. })),
            ("value", a(vec![position("long", 6.)])),
        ])
    } else {
        V::Null
    };
    let flight = if scene == "inflight" {
        promise(Ok(a(vec![position("long", 8.)])))
    } else {
        V::Null
    };
    let state = o(vec![
        ("cache", cached.clone()),
        ("inflight", flight.clone()),
    ]);
    H {
        op,
        scene,
        opts,
        market,
        related,
        positions,
        fetched,
        cached,
        flight,
        state: Rc::new(RefCell::new(state)),
        events: Rc::new(RefCell::new(vec![])),
        original: E {
            identity: Rc::new(()),
            value: o(vec![
                ("message", s("original")),
                (
                    "response",
                    o(vec![("data", o(vec![("msg", s("api original"))]))]),
                ),
            ]),
        },
        rollback: E {
            identity: Rc::new(()),
            value: o(vec![("message", s("rollback original"))]),
        },
        format_at: Rc::new(RefCell::new(0)),
        place_at: Rc::new(RefCell::new(0)),
        formatted: Rc::new(RefCell::new(vec![])),
    }
}
impl H {
    fn record(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        });
    }
    fn mode(&self) -> V {
        s(
            if ["hedged", "hedged-trailing", "live-hedged"].contains(&self.scene) {
                "hedged"
            } else {
                "oneway"
            },
        )
    }
    fn calls(&self, name: &str) -> Vec<Vec<V>> {
        self.events
            .borrow()
            .iter()
            .filter(|e| e.name == format!("call:{name}"))
            .map(|e| e.args.clone())
            .collect()
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
            Poll::Ready(self.result.take().expect("native single completion"))
        }
    }
}
impl RestActionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, path: &str) -> Result<V, E> {
        Ok(match path {
            "store.marketsBySymbol" => o(vec![("BTCUSD", self.market.clone())]),
            "store.tickersBySymbol" => o(vec![(
                "BTCUSD",
                if self.scene == "missing-ticker" {
                    V::U
                } else {
                    o(vec![("last", V::N(100.))])
                },
            )]),
            "store.positions" => self.positions.clone(),
            "usesUtaRouting" => V::Bool(self.scene == "uta-routing"),
            "apiUtaCategory" => s("USDT-FUTURES"),
            "apiProductType" => s("umcbl"),
            "apiMarginCoin" => s("USDT"),
            "_planTypePositionsCache" => get(&self.state.borrow(), "cache"),
            "_planTypePositionsInflight" => get(&self.state.borrow(), "inflight"),
            "BitgetExchange.POSITIONING_CACHE_TTL_MS" => V::N(5000.),
            "TIME_IN_FORCE" => o(vec![
                ("normal", s("GoodTillCancel")),
                ("gtc", s("GoodTillCancel")),
                ("post_only", s("PostOnly")),
                ("fok", s("FillOrKill")),
                ("ioc", s("ImmediateOrCancel")),
            ]),
            "ORDER_TYPE" => o(vec![
                ("limit", s("limit")),
                ("market", s("market")),
                ("pos_profit", s("take_profit_market")),
                ("pos_loss", s("stop_market")),
                ("loss_plan", s("stop_market")),
                ("profit_plan", s("take_profit_market")),
                ("track_plan", s("trailing_stop_market")),
                ("tp", s("take_profit_market")),
                ("sl", s("stop_market")),
                ("psl", s("stop_market")),
                ("ptp", s("take_profit_market")),
                ("track", s("trailing_stop_market")),
            ]),
            "OrderType.Limit" => s("limit"),
            "OrderType.Market" => s("market"),
            "OrderType.StopLoss" => s("stop_market"),
            "OrderType.TakeProfit" => s("take_profit_market"),
            "OrderType.TrailingStopLoss" => s("trailing_stop_market"),
            "OrderSide.Buy" => s("buy"),
            "OrderSide.Sell" => s("sell"),
            "PositionSide.Long" => s("long"),
            "PositionSide.Short" => s("short"),
            "PositionMode.Hedged" => s("hedged"),
            "PositionMode.OneWay" => s("oneway"),
            "MarginType.Cross" => s("cross"),
            "OrderStopTriggerType.LastPrice" => s("last_price"),
            "OrderTimeInForce.GoodTillCancel" => s("GoodTillCancel"),
            _ => panic!("PREP unknown receiver {path}"),
        })
    }
    fn set_receiver(&self, path: &str, v: V) -> Result<(), E> {
        let k = match path {
            "_planTypePositionsCache" => "cache",
            "_planTypePositionsInflight" => "inflight",
            _ => panic!("PREP receiver write"),
        };
        self.record(&format!("set:{path}"), vec![v.clone()]);
        put(&self.state.borrow(), units(&s(k)), v);
        Ok(())
    }
    fn capture(&self, path: &str) -> Result<V, E> {
        if ![
            "adjust",
            "uuid",
            "inverseObj",
            "omitUndefined",
            "times",
            "Error",
            "Date.now",
            "Math.ceil",
            "Math.max",
            "Math.min",
            "Math.round",
        ]
        .contains(&path)
        {
            self.record(&format!("capture:{path}"), vec![]);
        }
        Ok(method(path, V::U))
    }
    fn capture_member(&self, value: &V, name: &str) -> Result<V, E> {
        Ok(method(name, value.clone()))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        let path = text(&get(&f, "path"));
        let owner = get(&f, "owner");
        let silent = [
            "inverseObj",
            "omitUndefined",
            "times",
            "Error",
            "Math.ceil",
            "Math.max",
            "Math.min",
            "Math.round",
            "push",
            "includes",
            "endsWith",
        ];
        if !silent.contains(&path.as_str()) {
            self.record(&format!("call:{path}"), args.clone());
        }
        Ok(match path.as_str() {
            "adjust" => {
                if self.scene == "adjust-error" {
                    return Err(self.original.clone());
                }
                V::N(safe_cex_num::adjust(num(&args[0]), num(&args[1])))
            }
            "uuid" | "generateUuid" => s("native-uuid"),
            "Date.now" => V::N(1000.),
            "Math.ceil" => V::N(num(&args[0]).ceil()),
            "Math.round" => V::N((num(&args[0]) + 0.5).floor()),
            "Math.max" => V::N(num(&args[0]).max(num(&args[1]))),
            "Math.min" => V::N(num(&args[0]).min(num(&args[1]))),
            "inverseObj" => {
                let obj = o(vec![]);
                for k in keys(&args[0]) {
                    put(&obj, units(&key(&args[0], &k)), V::S(k));
                }
                obj
            }
            "omitUndefined" => {
                let obj = o(vec![]);
                for k in keys(&args[0]) {
                    let v = key(&args[0], &k);
                    if !matches!(v, V::U) {
                        put(&obj, k, v);
                    }
                }
                obj
            }
            "push" => {
                let V::A(v) = owner else {
                    panic!("PREP push owner")
                };
                v.borrow_mut().extend(args);
                V::N(v.borrow().len() as f64)
            }
            "includes" => V::Bool(text(&owner).contains(&text(&args[0]))),
            "endsWith" => V::Bool(text(&owner).ends_with(&text(&args[0]))),
            "ensureAccountTypeDetected" => V::U,
            "getOrFetchPositionMode" => {
                if self.scene == "position-error" {
                    return Err(self.original.clone());
                }
                self.mode()
            }
            "getOrFetchMarginType" => {
                if self.scene == "margin-error" {
                    return Err(self.original.clone());
                }
                s("cross")
            }
            "getOrderSideV2" => a(vec![s("buy"), s("open")]),
            "getOrderSideUta" => o(vec![
                ("side", s("buy")),
                (
                    "posSide",
                    if text(&self.mode()) == "hedged" {
                        s("long")
                    } else {
                        V::U
                    },
                ),
            ]),
            "getUtaStrategyOrderSide" => o(vec![
                ("side", s("sell")),
                (
                    "posSide",
                    if text(&self.mode()) == "hedged" {
                        s("long")
                    } else {
                        V::U
                    },
                ),
            ]),
            "getUtaStrategyTriggerBy" => s(if text(&args[0]) == "last_price" {
                "last_price"
            } else {
                "mark_price"
            }),
            "getAlgoOrderSide" => s("long"),
            "getInverseAlgoOrderSide" => s("close_long"),
            "getMarginCoinForSymbol" => s("BTC"),
            "getOrderPlanTypeAndAmount" => a(vec![
                s("loss_plan"),
                V::N(
                    if self.scene == "zero"
                        || ["full", "live", "live-hedged", "tiny", "positions-error"]
                            .contains(&self.scene)
                    {
                        0.
                    } else {
                        3.
                    },
                ),
            ]),
            "getOrFetchPositionsForSymbol" => {
                if self.scene == "positions-error" {
                    return Err(self.original.clone());
                }
                self.positions.clone()
            }
            "resolveCallerClientOrderId" => get(&args[0], "clientOid"),
            "assertSingleNativeClientOrderId" => {
                if !matches!(args[0], V::U | V::Null) && num(&args[1]) > 1. {
                    return Err(E {
                        identity: Rc::new(()),
                        value: o(vec![("message", s("caller ID requires a single payload"))]),
                    });
                }
                V::U
            }
            "formatUtaCreateOrder" => a(vec![o(vec![("delegated", V::Bool(true))])]),
            "formatCreateOrder" => {
                let payload = o(vec![
                    ("symbol", s("BTCUSD")),
                    ("size", s("4")),
                    ("price", s("101")),
                    ("clientOid", s("snapshot")),
                ]);
                self.formatted.borrow_mut().push(payload.clone());
                a(vec![payload])
            }
            "buildUtaStrategyPlaceOptsFromOrder" => o(vec![
                ("symbol", get(&args[0], "symbol")),
                ("type", get(&args[0], "type")),
                ("side", get(&args[0], "side")),
                ("price", get(&args[0], "price")),
                ("amount", get(&args[0], "amount")),
            ]),
            "formatUtaStrategyOrder" => {
                let i = *self.format_at.borrow();
                *self.format_at.borrow_mut() += 1;
                if self.scene == "format-original-error" && i == 0
                    || self.scene == "format-replacement-error" && i == 1
                {
                    return Err(self.original.clone());
                }
                let payload = o(vec![
                    ("version", V::N(i as f64)),
                    ("price", get(&args[0], "price")),
                    ("amount", get(&args[0], "amount")),
                ]);
                self.formatted.borrow_mut().push(payload.clone());
                payload
            }
            "cancelOrders" | "cancelUtaStrategyOrders" => {
                if self.scene == "cancel-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            "placeOrderBatch" => a(vec![s("virtual-result")]),
            "placeUtaStrategyOrders" => {
                let i = *self.place_at.borrow();
                *self.place_at.borrow_mut() += 1;
                if ["place-error", "rollback-error"].contains(&self.scene) && i == 0 {
                    return Err(self.original.clone());
                }
                if self.scene == "rollback-error" && i == 1 {
                    return Err(self.rollback.clone());
                }
                a(vec![s("strategy-result")])
            }
            "fetchOpenPositions" => {
                if self.scene == "async-reject" {
                    return Ok(promise(Err(self.original.clone())));
                }
                if ["reject", "emit-error"].contains(&self.scene) {
                    return Err(self.original.clone());
                }
                self.fetched.clone()
            }
            "emitAndLogError" => {
                if self.scene == "emit-error" {
                    return Err(self.original.clone());
                }
                V::U
            }
            _ => panic!("PREP unknown call {path}"),
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
    fn delete(&self, _v: &V, _k: &V) -> Result<(), E> {
        panic!("PREP unexpected delete")
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
    fn operation(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match name {
            "greater" => V::Bool(num(&args[0]) > num(&args[1])),
            "less" => V::Bool(num(&args[0]) < num(&args[1])),
            "subtract" => V::N(num(&args[0]) - num(&args[1])),
            "divide" => V::N(num(&args[0]) / num(&args[1])),
            "multiply" => V::N(num(&args[0]) * num(&args[1])),
            "remainder" => V::N(num(&args[0]) % num(&args[1])),
            "template" => s(&text(&args[0])),
            "objectSpread" => {
                let obj = o(vec![]);
                if matches!(args[0], V::O(_)) {
                    for k in keys(&args[0]) {
                        put(&obj, k.clone(), key(&args[0], &k));
                    }
                }
                obj
            }
            "missing" => s(&format!("{} {} not found", text(&args[0]), text(&args[1]))),
            "parentMissing" => s(&format!("Parent order not found for {}", text(&args[0]))),
            "strategyPriceRequired" => s(&format!(
                "Bitget UTA {} orders require a trigger price.",
                text(&args[0])
            )),
            "restoredMessage" => s(&format!(
                "Bitget UTA update failed: {}. Original stop was restored; please retry the update.",
                text(&args[0])
            )),
            "rollbackMessage" => s(&format!(
                "Bitget UTA update failed and rollback also failed: {} / {}. Your stop is not on the book — please re-add it.",
                text(&args[0]),
                text(&args[1])
            )),
            "positionsFallbackMessage" => s(&format!(
                "Could not load positions to size the {} stop — placing it as a full-position stop: {}",
                text(&args[0]),
                text(&args[1])
            )),
            _ => panic!("PREP unknown operation {name}"),
        })
    }
    fn new_error(&self, ctor: V, msg: V) -> Result<E, E> {
        native(
            &get(&ctor, "path"),
            &s("Error"),
            "Error constructor selector",
        );
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
    fn times<F>(&self, m: V, count: V, mut f: F) -> Result<V, E>
    where
        F: FnMut() -> Result<V, E>,
    {
        native(&get(&m, "path"), &s("times"), "imported times selector");
        let count = num(&count);
        if !count.is_finite() || count < 1. {
            return Ok(a(vec![]));
        }
        let mut out = vec![];
        for _ in 0..count.floor() as usize {
            out.push(f()?);
        }
        Ok(a(out))
    }
    fn launch(&self, mut body: RestActionFuture<'static, V, E>) -> Result<V, E> {
        let w = Waker::noop().clone();
        let mut cx = Context::from_waker(&w);
        Ok(match body.as_mut().poll(&mut cx) {
            Poll::Ready(r) => promise(r),
            Poll::Pending => V::Task(Rc::new(RefCell::new(Some(body)))),
        })
    }
    fn await_value(&self, p: V) -> RestActionFuture<'_, V, E> {
        Box::pin(async move {
            match p {
                V::Task(task) => {
                    let body = task
                        .borrow_mut()
                        .take()
                        .expect("PREP task owned by runtime");
                    body.await
                }
                v => unwrap_promise(v),
            }
        })
    }
}
fn execute(h: &H) -> Result<V, E> {
    run(async {
        match h.op {
            "classic" => body::format_create_order(h, h.opts.clone()).await,
            "uta" => body::format_uta_create_order(h, h.opts.clone()).await,
            "strategy" => body::format_uta_strategy_order(h, h.opts.clone()).await,
            "algo" => body::format_algo_order(h, h.opts.clone()).await,
            "inverse-algo" => body::inverse_format_algo_order(h, h.opts.clone()).await,
            "virtual" => {
                body::update_virtual_tp_sl_order(h, h.opts.clone(), h.related.clone()).await
            }
            "replace" => body::update_uta_strategy_order(h, h.opts.clone()).await,
            "cache" => body::resolve_positions_for_plan_type(h, s("BTCUSD"), s("long")).await,
            _ => panic!("PREP op"),
        }
    })
}
fn count(h: &H, name: &str, n: usize) {
    assert_eq!(
        h.calls(name).len(),
        n,
        "PREP native independent call count {}:{}:{name}",
        h.op,
        h.scene
    )
}
fn success(r: &Result<V, E>) -> &V {
    r.as_ref().expect("PREP expected native success")
}
fn failure(h: &H, r: &Result<V, E>, original: bool, msg: &str) {
    let e = r.as_ref().expect_err("PREP expected native failure");
    assert_eq!(
        Rc::ptr_eq(&e.identity, &h.original.identity),
        original,
        "PREP native original error identity"
    );
    native(&get(&e.value, "message"), &s(msg), "native error message");
}
fn expected_normal(h: &H) -> V {
    let uta = h.op == "uta";
    let hedged = h.scene == "hedged";
    let market = h.scene == "market";
    let zero = h.scene == "zero";
    let sizes: Vec<&str> = match h.scene {
        "split" => vec!["5", "5"],
        "remainder" => vec!["3", "3", "3", "3"],
        "zero-max" | "negative-max" => vec!["13"],
        "zero" => vec!["0"],
        _ => vec!["4"],
    };
    a(sizes
        .into_iter()
        .map(|size| {
            let mut f = vec![];
            if uta {
                f.push(("category", s("USDT-FUTURES")));
            }
            f.push(("symbol", s(if uta { "BTCUSDT" } else { "BTCUSD" })));
            if !zero {
                f.push((if uta { "qty" } else { "size" }, s(size)));
            }
            if !market {
                f.push(("price", s("101")));
            }
            f.push(("side", s("buy")));
            if uta {
                if hedged {
                    f.push(("posSide", s("long")));
                }
            } else {
                f.push(("tradeSide", s("open")));
            }
            f.push(("orderType", s(if market { "market" } else { "limit" })));
            if !market {
                f.push((if uta { "timeInForce" } else { "force" }, s("gtc")));
            }
            if !hedged {
                f.push(("reduceOnly", s(if uta { "yes" } else { "YES" })));
            }
            if uta {
                f.push(("marginMode", s("crossed")));
            }
            f.push((
                if uta {
                    "takeProfit"
                } else {
                    "presetStopSurplusPrice"
                },
                s("110.5"),
            ));
            f.push((
                if uta {
                    "stopLoss"
                } else {
                    "presetStopLossPrice"
                },
                s("90"),
            ));
            if uta {
                f.extend([
                    ("tpTriggerBy", s("last_price")),
                    ("slTriggerBy", s("mark_price")),
                    ("tpOrderType", s("market")),
                    ("slOrderType", s("market")),
                ]);
            }
            if zero {
                f.push((if uta { "qty" } else { "size" }, s(size)));
            }
            f.push((
                "clientOid",
                s(if h.scene == "caller-id" {
                    "caller"
                } else {
                    "native-uuid"
                }),
            ));
            o(f)
        })
        .collect())
}
fn check_normal(h: &H, r: &Result<V, E>) {
    if h.scene == "unsupported" {
        failure(
            h,
            r,
            false,
            "Bitget UTA currently supports only normal market and limit orders.",
        );
        count(h, "emitAndLogError", 1);
        return;
    }
    if h.scene == "missing-market" {
        failure(h, r, false, "Market BTCUSD not found");
        count(h, "emitAndLogError", 0);
        return;
    }
    if h.scene == "position-error" {
        failure(h, r, true, "original");
        count(h, "emitAndLogError", 0);
        return;
    }
    if h.scene == "caller-split" {
        failure(h, r, false, "caller ID requires a single payload");
        count(h, "uuid", 0);
        native(
            &h.calls("assertSingleNativeClientOrderId")[0][1],
            &V::N(4.),
            "independent split guard count",
        );
        return;
    }
    if h.scene == "uta-routing" {
        native(
            success(r),
            &a(vec![o(vec![("delegated", V::Bool(true))])]),
            "UTA formatter delegation",
        );
        count(h, "formatUtaCreateOrder", 1);
        alias(
            &h.calls("formatUtaCreateOrder")[0][0],
            &h.opts,
            "delegated original opts",
        );
        count(h, "uuid", 0);
        return;
    }
    let expected = expected_normal(h);
    native(
        success(r),
        &expected,
        "normal payload fields and independent lots",
    );
    count(
        h,
        "ensureAccountTypeDetected",
        usize::from(h.op == "classic"),
    );
    count(h, "getOrFetchPositionMode", 1);
    count(h, "getOrFetchMarginType", usize::from(h.op == "uta"));
    count(h, "resolveCallerClientOrderId", 1);
    alias(
        &h.calls("resolveCallerClientOrderId")[0][0],
        &h.opts,
        "caller resolver original opts",
    );
    count(h, "assertSingleNativeClientOrderId", 1);
    let len = match h.scene {
        "split" => 2,
        "remainder" => 4,
        _ => 1,
    };
    native(
        &h.calls("assertSingleNativeClientOrderId")[0][1],
        &V::N(len as f64),
        "independent final payload count before IDs",
    );
    count(h, "uuid", if h.scene == "caller-id" { 0 } else { len });
    let payloads = values(success(r));
    for i in 0..payloads.len() {
        for j in i + 1..payloads.len() {
            assert!(
                !equal(&payloads[i], &payloads[j]),
                "PREP native distinct lot objects"
            );
        }
    }
}
fn check_strategy(h: &H, r: &Result<V, E>) {
    let msg = match h.scene {
        "unsupported" => Some(
            "Bitget UTA currently supports only standalone stop-loss and take-profit strategy orders.",
        ),
        "missing-price" => Some("Bitget UTA stop_market orders require a trigger price."),
        "missing-market" => Some("Market BTCUSD not found"),
        "position-error" | "positions-error" => Some("original"),
        _ => None,
    };
    if let Some(msg) = msg {
        failure(
            h,
            r,
            ["position-error", "positions-error"].contains(&h.scene),
            msg,
        );
        count(
            h,
            "emitAndLogError",
            usize::from(["unsupported", "missing-price"].contains(&h.scene)),
        );
        count(h, "uuid", 0);
        return;
    }
    let partial = !matches!(h.scene, "full" | "tiny");
    let qty = if h.scene == "live" { "9" } else { "3" };
    let take = h.scene == "take-profit";
    let mut f = vec![
        ("category", s("USDT-FUTURES")),
        ("symbol", s("BTCUSDT")),
        ("type", s("tpsl")),
        ("side", s("sell")),
    ];
    if h.scene == "hedged" || h.scene == "live-hedged" {
        f.push(("posSide", s("long")));
    }
    f.push(("tpslMode", s(if partial { "partial" } else { "full" })));
    if partial {
        f.push(("qty", s(qty)));
    }
    f.push(("reduceOnly", s("yes")));
    f.push(("clientOid", s("native-uuid")));
    f.push((if take { "takeProfit" } else { "stopLoss" }, s("101")));
    f.push((
        if take { "tpTriggerBy" } else { "slTriggerBy" },
        s(if take { "last_price" } else { "mark_price" }),
    ));
    f.push((
        if take { "tpOrderType" } else { "slOrderType" },
        s("market"),
    ));
    native(success(r), &o(f), "strategy partial/full native payload");
    count(h, "uuid", 1);
    count(
        h,
        "getOrFetchPositionsForSymbol",
        usize::from(["full", "live", "live-hedged", "tiny"].contains(&h.scene)),
    );
    alias(
        &h.calls("getOrderPlanTypeAndAmount")[0][0],
        &h.opts,
        "strategy plan original opts",
    );
}
fn check_algo(h: &H, r: &Result<V, E>) {
    let inverse = h.op == "inverse-algo";
    let error = match h.scene {
        "missing-market" => Some((false, "Market BTCUSD not found")),
        "missing-ticker" => Some((false, "Ticker BTCUSD not found")),
        "position-error" => Some((true, "original")),
        "margin-error" if !inverse => Some((true, "original")),
        _ => None,
    };
    if let Some((original, msg)) = error {
        failure(h, r, original, msg);
        count(h, "emitAndLogError", 0);
        return;
    }
    let trail = h.scene.contains("trailing") || h.scene == "margin-error";
    let mut f = vec![
        ("marginCoin", s(if inverse { "BTC" } else { "USDT" })),
        ("symbol", s("BTCUSDT")),
        ("clientOid", s("native-uuid")),
        ("productType", s("umcbl")),
    ];
    if h.scene != "hedged-trailing" {
        f.push(("reduceOnly", s("YES")));
    }
    if h.scene != "no-price" {
        f.push(("triggerPrice", s("80")));
        if trail {
            f.push(("size", s("4")));
            if inverse {
                f.extend([("side", s("close_long")), ("rangeRate", s("20"))]);
            } else {
                f.extend([
                    ("planType", s("track_plan")),
                    ("marginMode", s("crossed")),
                    ("orderType", s("market")),
                    ("triggerType", s("mark_price")),
                    ("side", s("sell")),
                ]);
                if h.scene == "hedged-trailing" {
                    f.push(("tradeSide", s("close")));
                }
                f.push(("callbackRatio", s("20")));
            }
        } else {
            f.push(("holdSide", s(if inverse { "close_long" } else { "long" })));
            f.push(("planType", s("loss_plan")));
            if h.scene != "zero" {
                f.push(("size", s("3")));
            }
            if !inverse {
                f.push((
                    "triggerType",
                    s(if h.scene == "take" {
                        "fill_price"
                    } else {
                        "mark_price"
                    }),
                ));
            }
        }
    }
    native(success(r), &o(f), "Classic/inverse algo payload");
    count(h, if inverse { "generateUuid" } else { "uuid" }, 1);
    count(h, "getOrFetchMarginType", usize::from(trail && !inverse));
    count(h, "getMarginCoinForSymbol", usize::from(inverse));
}
fn check_virtual(h: &H, r: &Result<V, E>) {
    if h.scene == "missing-market" {
        failure(h, r, false, "Market BTCUSD not found");
        count(h, "cancelOrders", 0);
        return;
    }
    if h.scene == "missing-parent" {
        failure(h, r, false, "Parent order not found for parent");
        count(h, "emitAndLogError", 1);
        count(h, "cancelOrders", 0);
        return;
    }
    if h.scene == "cancel-error" {
        failure(h, r, true, "original");
        count(h, "placeOrderBatch", 0);
        return;
    }
    native(
        success(r),
        &a(vec![s("virtual-result")]),
        "virtual replacement native return",
    );
    count(h, "cancelOrders", 1);
    count(h, "placeOrderBatch", 1);
    alias(
        &values(&h.calls("cancelOrders")[0][0])[0],
        &values(&h.related)[0],
        "cancel exact parent order",
    );
    let expected = o(vec![
        ("symbol", s("BTCUSD")),
        (
            "size",
            s(if h.scene == "amount" {
                "7"
            } else if h.scene == "undefined" {
                "NaN"
            } else {
                "4.2"
            }),
        ),
        (
            "price",
            s(if h.scene == "parent" {
                "120"
            } else if h.scene == "undefined" {
                "NaN"
            } else {
                "101.2"
            }),
        ),
        ("clientOid", s("snapshot")),
        (
            "presetStopSurplusPrice",
            s(if h.scene == "take" { "120" } else { "110" }),
        ),
        (
            "presetStopLossPrice",
            s(if h.scene == "stop" { "120" } else { "90" }),
        ),
    ]);
    let payload = values(&h.calls("placeOrderBatch")[0][0])[0].clone();
    native(
        &payload,
        &expected,
        "virtual TP/SL independently expected payload",
    );
    assert!(
        !equal(&payload, &h.formatted.borrow()[0]),
        "PREP native fresh replacement payload"
    );
    let formatted_opts = &h.calls("formatCreateOrder")[0][0];
    native(
        formatted_opts,
        &values(&h.related)[0],
        "parent spread fields",
    );
    assert!(
        !equal(formatted_opts, &values(&h.related)[0]),
        "PREP native parent spread fresh alias"
    );
    let events = h.events.borrow();
    let cancel = events
        .iter()
        .position(|e| e.name == "call:cancelOrders")
        .unwrap();
    let place = events
        .iter()
        .position(|e| e.name == "call:placeOrderBatch")
        .unwrap();
    assert!(cancel < place, "PREP native cancel precedes replacement");
}
fn check_replace(h: &H, r: &Result<V, E>) {
    if h.scene == "trailing" {
        failure(
            h,
            r,
            false,
            "Bitget UTA trailing-stop orders are not supported yet.",
        );
        count(h, "buildUtaStrategyPlaceOptsFromOrder", 0);
        return;
    }
    let failed = [
        "place-error",
        "rollback-error",
        "cancel-error",
        "format-original-error",
        "format-replacement-error",
    ]
    .contains(&h.scene);
    if failed {
        failure(h, r, true, "original");
    } else {
        native(
            success(r),
            &a(vec![s("strategy-result")]),
            "strategy replacement return",
        );
    }
    let formats = if h.scene == "format-original-error" {
        1
    } else {
        2
    };
    count(h, "buildUtaStrategyPlaceOptsFromOrder", formats);
    count(h, "formatUtaStrategyOrder", formats);
    for call in h.calls("buildUtaStrategyPlaceOptsFromOrder") {
        alias(
            &call[0],
            &get(&h.opts, "order"),
            "strategy opts original order",
        );
    }
    if h.scene == "format-original-error" || h.scene == "format-replacement-error" {
        count(h, "cancelUtaStrategyOrders", 0);
        count(h, "placeUtaStrategyOrders", 0);
        return;
    }
    let fmt = h.calls("formatUtaStrategyOrder");
    assert!(
        !equal(&fmt[0][0], &fmt[1][0]),
        "PREP native separate original and replacement opts"
    );
    native(
        &get(&fmt[0][0], "price"),
        &V::N(91.),
        "original strategy price before cancel",
    );
    native(
        &get(&fmt[1][0], "price"),
        &if h.scene == "undefined" {
            V::U
        } else if h.scene == "amount" {
            V::N(91.)
        } else {
            V::N(120.)
        },
        "replacement strategy price presence",
    );
    native(
        &get(&fmt[1][0], "amount"),
        &if h.scene == "undefined" {
            V::U
        } else if h.scene == "amount" {
            V::N(7.)
        } else {
            V::N(3.)
        },
        "replacement strategy amount presence",
    );
    count(h, "cancelUtaStrategyOrders", 1);
    let cancel = h.calls("cancelUtaStrategyOrders");
    alias(
        &values(&cancel[0][0])[0],
        &get(&h.opts, "order"),
        "cancel original strategy order",
    );
    native(
        &cancel[0][1],
        &o(vec![("treatNotFoundAsSuccess", V::Bool(false))]),
        "strict cancellation flag",
    );
    if h.scene == "cancel-error" {
        count(h, "placeUtaStrategyOrders", 0);
        return;
    }
    let rollback = h.scene == "place-error" || h.scene == "rollback-error";
    count(h, "placeUtaStrategyOrders", if rollback { 2 } else { 1 });
    let calls = h.calls("placeUtaStrategyOrders");
    alias(
        &values(&calls[0][0])[0],
        &h.formatted.borrow()[1],
        "first place exact replacement payload",
    );
    if rollback {
        alias(
            &values(&calls[1][0])[0],
            &h.formatted.borrow()[0],
            "rollback exact original payload",
        );
        count(h, "emitAndLogError", 1);
        native(
            &h.calls("emitAndLogError")[0][0],
            &s(if h.scene == "rollback-error" {
                "Bitget UTA update failed and rollback also failed: original / rollback original. Your stop is not on the book — please re-add it."
            } else {
                "Bitget UTA update failed: original. Original stop was restored; please retry the update."
            }),
            "rollback message",
        );
    } else {
        count(h, "emitAndLogError", 0);
    }
}
fn check_cache(h: &H, r: &Result<V, E>) {
    let names: &[&str] = match h.scene {
        "store" | "inflight" => &[],
        "cache" => &["call:Date.now"],
        "expired" => &[
            "call:Date.now",
            "capture:fetchOpenPositions",
            "call:fetchOpenPositions",
            "set:_planTypePositionsInflight",
            "call:Date.now",
            "set:_planTypePositionsCache",
            "set:_planTypePositionsInflight",
        ],
        "fetch" => &[
            "capture:fetchOpenPositions",
            "call:fetchOpenPositions",
            "set:_planTypePositionsInflight",
            "call:Date.now",
            "set:_planTypePositionsCache",
            "set:_planTypePositionsInflight",
        ],
        "empty" => &[
            "capture:fetchOpenPositions",
            "call:fetchOpenPositions",
            "set:_planTypePositionsInflight",
            "set:_planTypePositionsInflight",
        ],
        "async-reject" => &[
            "capture:fetchOpenPositions",
            "call:fetchOpenPositions",
            "set:_planTypePositionsInflight",
            "capture:emitAndLogError",
            "call:emitAndLogError",
            "set:_planTypePositionsInflight",
        ],
        "reject" | "emit-error" => &[
            "capture:fetchOpenPositions",
            "call:fetchOpenPositions",
            "capture:emitAndLogError",
            "call:emitAndLogError",
            "set:_planTypePositionsInflight",
            "set:_planTypePositionsInflight",
        ],
        _ => panic!("cache floor"),
    };
    let events = h.events.borrow();
    assert_eq!(
        events.len(),
        names.len(),
        "PREP native independent cache event count"
    );
    for (e, n) in events.iter().zip(names) {
        native(&s(&e.name), &s(n), "native cache effect order before wire");
    }
    drop(events);
    let state = h.state.borrow();
    let cache = get(&state, "cache");
    let flight = get(&state, "inflight");
    if h.scene == "emit-error" {
        failure(h, r, true, "original");
        native(&flight, &V::Null, "inflight finally clear on rejection");
        count(h, "emitAndLogError", 1);
        return;
    }
    if h.scene == "store" {
        alias(success(r), &h.positions, "store answer original positions");
        count(h, "fetchOpenPositions", 0);
        alias(&cache, &h.cached, "store keeps cache");
    } else if h.scene == "cache" {
        alias(
            success(r),
            &get(&h.cached, "value"),
            "cached original position array",
        );
        count(h, "fetchOpenPositions", 0);
        alias(&cache, &h.cached, "cache object unchanged");
    } else if h.scene == "inflight" {
        let V::Promise(p) = &h.flight else {
            panic!("fixture promise")
        };
        alias(
            success(r),
            p.as_ref().as_ref().unwrap(),
            "existing inflight array result",
        );
        alias(&flight, &h.flight, "existing inflight promise retained");
        count(h, "fetchOpenPositions", 0);
    } else {
        count(h, "fetchOpenPositions", 1);
        native(&flight, &V::Null, "inflight cleared after owned promise");
        if h.scene == "reject" || h.scene == "async-reject" {
            native(
                success(r),
                &a(vec![]),
                "degrade to independently empty array",
            );
            count(h, "emitAndLogError", 1);
            native(
                &h.calls("emitAndLogError")[0][0],
                &s(
                    "Could not load positions to size the BTCUSD stop — placing it as a full-position stop: api original",
                ),
                "fetch failure fallback message",
            );
            native(&cache, &V::Null, "failure not cached");
        } else {
            alias(success(r), &h.fetched, "fetched original position array");
            if h.scene == "empty" {
                native(&cache, &V::Null, "empty fetch not cached");
            } else {
                native(&get(&cache, "at"), &V::N(1000.), "cache native clock stamp");
                alias(
                    &get(&cache, "value"),
                    &h.fetched,
                    "cache stores fetched array alias",
                );
            }
        }
    }
}
fn check(h: &H, r: &Result<V, E>) {
    match h.op {
        "classic" | "uta" => check_normal(h, r),
        "strategy" => check_strategy(h, r),
        "algo" | "inverse-algo" => check_algo(h, r),
        "virtual" => check_virtual(h, r),
        "replace" => check_replace(h, r),
        "cache" => check_cache(h, r),
        _ => panic!("PREP group"),
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
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/order-preparation-capture.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(group)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(cases.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "PREP source assertion failed {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("PREP actual source JSON")
}
fn suite(cases: &[(&'static str, &'static str)], group: &str, floor: usize) {
    assert_eq!(cases.len(), floor, "PREP independent native input floor");
    let mut native_cases = vec![];
    for &(op, scene) in cases {
        let h = fixture(op, scene);
        let r = execute(&h);
        check(&h, &r);
        native_cases.push((h, r));
    }
    assert_eq!(
        native_cases.len(),
        floor,
        "PREP independent native output floor"
    );
    // Complete every native assertion above before the first wire/source assertion.
    let inputs=native_cases.iter().map(|(h,_)|json!({"op":h.op,"scene":h.scene,"opts":wire(&h.opts),"market":wire(&h.market),"related":wire(&h.related),"positions":wire(&h.positions),"fetched":wire(&h.fetched),"cached":wire(&h.cached)})).collect::<Vec<_>>();
    let actual=native_cases.iter().map(|(h,r)|json!({"result":result_wire(h,r),"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"state":wire(&h.state.borrow())})).collect::<Vec<_>>();
    let expected = source(json!(inputs), group);
    compare(&json!(actual), &expected, "actual eight TS bodies");
}
const NORMAL: [&str; 12] = [
    "normal",
    "market",
    "hedged",
    "split",
    "remainder",
    "zero-max",
    "negative-max",
    "zero",
    "caller-id",
    "caller-split",
    "missing-market",
    "position-error",
];
#[test]
fn native_classic_then_source() {
    let mut cases = NORMAL.iter().map(|&s| ("classic", s)).collect::<Vec<_>>();
    cases.push(("classic", "uta-routing"));
    suite(&cases, "classic", 13)
}
#[test]
fn native_uta_then_source() {
    let mut cases = NORMAL.iter().map(|&s| ("uta", s)).collect::<Vec<_>>();
    cases.push(("uta", "unsupported"));
    suite(&cases, "uta", 13)
}
#[test]
fn native_strategy_then_source() {
    let cases = [
        "normal",
        "take-profit",
        "hedged",
        "full",
        "live",
        "live-hedged",
        "tiny",
        "unsupported",
        "missing-price",
        "missing-market",
        "position-error",
        "positions-error",
    ]
    .map(|s| ("strategy", s));
    suite(&cases, "strategy", 12)
}
#[test]
fn native_algo_then_source() {
    let mut cases = vec![];
    for op in ["algo", "inverse-algo"] {
        for scene in [
            "stop",
            "take",
            "trailing",
            "hedged-trailing",
            "no-price",
            "zero",
            "missing-market",
            "missing-ticker",
            "position-error",
        ] {
            cases.push((op, scene));
        }
    }
    cases.push(("algo", "margin-error"));
    suite(&cases, "algo", 19)
}
#[test]
fn native_virtual_then_source() {
    let cases = [
        "parent",
        "stop",
        "take",
        "amount",
        "undefined",
        "missing-market",
        "missing-parent",
        "cancel-error",
    ]
    .map(|s| ("virtual", s));
    suite(&cases, "virtual", 8)
}
#[test]
fn native_replace_then_source() {
    let cases = [
        "normal",
        "price",
        "amount",
        "undefined",
        "place-error",
        "rollback-error",
        "cancel-error",
        "trailing",
        "format-original-error",
        "format-replacement-error",
    ]
    .map(|s| ("replace", s));
    suite(&cases, "replace", 10)
}
#[test]
fn native_cache_then_source() {
    let cases = [
        "store",
        "cache",
        "expired",
        "inflight",
        "fetch",
        "empty",
        "reject",
        "emit-error",
        "async-reject",
    ]
    .map(|s| ("cache", s));
    suite(&cases, "cache", 9)
}
