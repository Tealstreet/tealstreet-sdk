use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost},
    spot_exchange_reads::{self as api, SpotExchangeHost},
    spot_order_writes::SpotOrderHost,
};
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
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(String, V)>>>),
    F(Box<V>, String),
    P(Box<Result<V, E>>),
}
#[derive(Clone, Debug)]
struct E(V);
fn s(v: &str) -> V {
    V::S(v.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(v: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::S(v) => String::from_utf16_lossy(v),
        V::N(n) => safe_cex_num::js_number_string(*n),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::B(b) => b.to_string(),
        _ => "[object Object]".into(),
    }
}
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        V::Null => 0.,
        V::B(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        _ => text(v).parse().unwrap_or(f64::NAN),
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::B(v) => *v,
        V::N(v) => *v != 0. && !v.is_nan(),
        V::S(v) => !v.is_empty(),
        _ => true,
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::B(a), V::B(b)) => a == b,
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        V::A(v) if k == "length" => V::N(v.borrow().len() as f64),
        V::A(v) => k
            .parse::<usize>()
            .ok()
            .and_then(|i| v.borrow().get(i).cloned())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn put(v: &V, k: &str, f: V) {
    let V::O(v) = v else {
        panic!("SPOT native object")
    };
    let mut v = v.borrow_mut();
    if let Some((_, old)) = v.iter_mut().find(|(key, _)| key == k) {
        *old = f
    } else {
        v.push((k.into(), f))
    }
}
fn vals(v: &V) -> Vec<V> {
    let V::A(v) = v else {
        panic!("SPOT native array")
    };
    v.borrow().clone()
}
fn native(v: &V, expected: &V, path: &str) {
    match (v, expected) {
        (V::N(a), V::N(b)) => assert_eq!(
            if a.is_nan() {
                f64::NAN.to_bits()
            } else {
                a.to_bits()
            },
            if b.is_nan() {
                f64::NAN.to_bits()
            } else {
                b.to_bits()
            },
            "SPOT native numeric {path}"
        ),
        (V::S(a), V::S(b)) => assert_eq!(a, b, "SPOT native UTF16 {path}"),
        (V::A(a), V::A(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            assert_eq!(
                a.len(),
                b.len(),
                "SPOT native independent array count {path}"
            );
            for (i, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                native(a, b, &format!("{path}.{i}"))
            }
        }
        (V::O(a), V::O(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            assert_eq!(
                a.len(),
                b.len(),
                "SPOT native independent field count {path}"
            );
            for ((ka, a), (kb, b)) in a.iter().zip(b.iter()) {
                assert_eq!(ka, kb, "SPOT native field order {path}");
                native(a, b, &format!("{path}.{ka}"))
            }
        }
        _ => assert!(
            eq(v, expected),
            "SPOT native scalar {path}: {v:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::B(b) => json!({"b":b}),
        V::N(n) => json!({"n":if n.is_nan() { f64::NAN.to_le_bytes() } else { n.to_le_bytes() }}),
        V::S(s) => json!({"s":s}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k.encode_utf16().collect::<Vec<_>>(),wire(v)])).collect::<Vec<_>>()})
        }
        _ => panic!("wire only decoded values"),
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut f = Box::pin(f);
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("SPOT fixture did not settle")
}
fn from_json(v: &Json) -> V {
    match v {
        Json::Null => V::Null,
        Json::Bool(v) => V::B(*v),
        Json::Number(v) => V::N(v.as_f64().unwrap()),
        Json::String(v) => s(v),
        Json::Array(v) => a(v.iter().map(from_json).collect()),
        Json::Object(v) => V::O(Rc::new(RefCell::new(
            v.iter().map(|(k, v)| (k.clone(), from_json(v))).collect(),
        ))),
    }
}
struct H {
    scene: &'static str,
    op: &'static str,
    global: V,
    owner: V,
    original: V,
    requests: RefCell<Vec<V>>,
    events: RefCell<Vec<V>>,
    recorded: RefCell<Vec<V>>,
    page: Cell<usize>,
    clock: Cell<usize>,
}
impl H {
    fn err(&self) -> E {
        E(self.original.clone())
    }
    fn event(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(a(vec![s(name), a(args)]));
    }
    fn new(op: &'static str, scene: &'static str) -> Self {
        use safe_cex_venue_bybit::types as t;
        let g = o(vec![]);
        for (k, v) in [
            ("ORDER_TYPE", &*t::ORDER_TYPE),
            ("ORDER_SIDE", &*t::ORDER_SIDE),
            ("ORDER_STATUS", &*t::ORDER_STATUS),
            ("ORDER_TIME_IN_FORCE", &*t::ORDER_TIME_IN_FORCE),
            (
                "ORDER_TIME_IN_FORCE_OUTBOUND",
                &*t::ORDER_TIME_IN_FORCE_OUTBOUND,
            ),
            ("ORDER_TRIGGER_TYPE", &*t::ORDER_TRIGGER_TYPE),
            (
                "POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE",
                &*t::POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
            ),
            ("INTERVAL", &*t::INTERVAL),
        ] {
            put(&g, k, from_json(v));
        }
        for (k, v) in [
            (
                "OrderType",
                o(vec![
                    ("Limit", s("limit")),
                    ("Market", s("market")),
                    ("StopLoss", s("stop_market")),
                    ("TakeProfit", s("take_profit_market")),
                    ("TrailingStopLoss", s("trailing_stop_market")),
                ]),
            ),
            ("OrderSide", o(vec![("Buy", s("buy")), ("Sell", s("sell"))])),
            (
                "OrderTimeInForce",
                o(vec![("GoodTillCancel", s("GoodTillCancel"))]),
            ),
            (
                "OrderStopTriggerType",
                o(vec![
                    ("LastPrice", s("last_price")),
                    ("MarkPrice", s("mark_price")),
                ]),
            ),
            ("Math", o(vec![])),
            ("Array", o(vec![])),
            ("virtualClock", o(vec![])),
            (
                "ENDPOINTS",
                o(vec![
                    ("BALANCE", s("BALANCE")),
                    ("UNFILLED_ORDERS", s("UNFILLED_ORDERS")),
                    ("KLINE", s("KLINE")),
                    ("CREATE_ORDER", s("CREATE_ORDER")),
                    ("REPLACE_ORDER", s("REPLACE_ORDER")),
                    ("SET_TRADING_STOP", s("SET_TRADING_STOP")),
                ]),
            ),
        ] {
            put(&g, k, v)
        }
        let market = o(vec![
            (
                "precision",
                o(vec![("price", V::N(0.5)), ("amount", V::N(0.1))]),
            ),
            ("limits", o(vec![("amount", o(vec![("max", V::N(2.))]))])),
            ("isInverse", V::B(scene == "inverse")),
        ]);
        let owner = o(vec![
            ("xhr", s("regular")),
            ("forceUnifiedBalance", V::B(false)),
            ("unifiedMarginStatus", V::N(1.)),
            ("needsReinit", V::B(false)),
            ("accountType", s("CONTRACT")),
            (
                "store",
                o(vec![
                    (
                        "balance",
                        o(vec![
                            ("sentinel", s("balance")),
                            ("info", a(vec![s("prior")])),
                        ]),
                    ),
                    ("markets", a(vec![s("stored")])),
                    (
                        "marketsBySymbol",
                        o(if scene == "no-market" {
                            vec![]
                        } else {
                            vec![("BTC", market)]
                        }),
                    ),
                    (
                        "tickersBySymbol",
                        o(vec![(
                            "BTC",
                            o(vec![(
                                "mark",
                                V::N(if scene == "mark-zero" { 0. } else { 100. }),
                            )]),
                        )]),
                    ),
                    (
                        "orders",
                        a(if scene == "missing-parent" {
                            vec![]
                        } else {
                            vec![o(vec![("id", s("parent"))])]
                        }),
                    ),
                ]),
            ),
        ]);
        Self {
            op,
            scene,
            global: g,
            owner,
            original: o(vec![("message", s("original"))]),
            requests: RefCell::default(),
            events: RefCell::default(),
            recorded: RefCell::default(),
            page: Cell::new(0),
            clock: Cell::new(0),
        }
    }
    fn data(&self, endpoint: &str, index: usize) -> V {
        let msg = if self.scene == "promote" && index == 0 {
            "accountType only support UNIFIED."
        } else if matches!(self.scene, "api-error" | "virtual-api-error") {
            "bad"
        } else if self.scene == "not-modified" {
            "order not modified"
        } else {
            "OK"
        };
        let list = match endpoint {
            "BALANCE" => a(vec![s("account")]),
            "UNFILLED_ORDERS" => {
                if self.scene == "empty" || (self.scene == "pages" && index == 1) {
                    a(vec![])
                } else {
                    a(vec![o(vec![("id", s("raw"))])])
                }
            }
            "KLINE" => {
                if self.scene == "missing-list" {
                    V::U
                } else {
                    a(vec![
                        a(vec![s("2000"), s("21"), s("24"), s("19"), s("23"), s("17")]),
                        a(vec![s("1000"), s("11"), s("14"), s("9"), s("13"), s("7")]),
                    ])
                }
            }
            _ => V::U,
        };
        o(vec![
            ("retMsg", s(msg)),
            (
                "result",
                o(vec![
                    ("list", list),
                    (
                        "nextPageCursor",
                        s(if self.scene == "pages" && index == 0 {
                            "next"
                        } else {
                            ""
                        }),
                    ),
                    ("orderId", s(&format!("id-{index}"))),
                ]),
            ),
        ])
    }
}
impl SessionHost for H {
    type Value = V;
    type Error = E;
    fn get(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::U | V::Null) {
            return Err(self.err());
        }
        if k == "map" && matches!(v, V::A(_)) {
            return Ok(V::F(Box::new(v.clone()), k.into()));
        }
        if k == "mapOrder" && eq(v, &self.owner) {
            return Ok(V::F(Box::new(v.clone()), k.into()));
        }
        Ok(get(v, k))
    }
    fn optional(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::U | V::Null) {
            Ok(V::U)
        } else {
            self.get(v, k)
        }
    }
    fn set(&self, v: &V, k: &str, f: V) -> Result<(), E> {
        put(v, k, f);
        Ok(())
    }
    fn define(&self, v: &V, k: &str, f: V) -> Result<(), E> {
        self.set(v, k, f)
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        eq(a, b)
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn literal(&self, v: &str) -> V {
        s(v)
    }
    fn boolean(&self, v: bool) -> V {
        V::B(v)
    }
    fn number(&self, v: f64) -> V {
        V::N(v)
    }
    fn numeric(&self, v: &V) -> Result<f64, E> {
        Ok(num(v))
    }
    fn units(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(if let V::S(v) = v {
            v.clone()
        } else {
            text(v).encode_utf16().collect()
        })
    }
    fn text(&self, v: Vec<u16>) -> V {
        V::S(v)
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        o(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn global(&self) -> V {
        self.global.clone()
    }
    fn select(&self, owner: V, key: &str) -> Result<V, E> {
        if matches!(owner, V::U | V::Null) {
            Err(self.err())
        } else {
            Ok(V::F(Box::new(owner), key.into()))
        }
    }
    fn receiver_call(&self, owner: V, callable: V, args: Vec<V>) -> Result<V, E> {
        let V::F(_, key) = callable else { panic!() };
        self.apply(V::F(Box::new(owner), key), args)
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, E> {
        let V::F(owner, key) = selected else {
            panic!("selected native callable")
        };
        Ok(match key.as_str() {
            "parseFloat" => V::N(safe_cex_num::parse_float(&text(&args[0]))),
            "getCurrentTimeMs" => {
                let i = self.clock.get();
                self.clock.set(i + 1);
                V::N(1234. + i as f64)
            }
            "min" => V::N(num(&args[0]).min(num(&args[1]))),
            "ceil" => V::N(num(&args[0]).ceil()),
            "adjust" => V::N(safe_cex_num::adjust(num(&args[0]), num(&args[1]))),
            "subtract" if !matches!(get(&owner, "ms"), V::U) => {
                let scale = match text(&args[1]).as_str() {
                    "m" => 60000.,
                    "h" => 3600000.,
                    "d" => 86400000.,
                    _ => panic!("date fixture unit"),
                };
                o(vec![(
                    "ms",
                    V::N(num(&get(&owner, "ms")) - num(&args[0]) * scale),
                )])
            }
            "subtract" => V::N(safe_cex_num::subtract(num(&args[0]), num(&args[1]))),
            "isArray" => V::B(matches!(args[0], V::A(_))),
            "v" => get(&args[0], &text(&args[1])),
            "endsWith" => V::B(text(&owner).ends_with(&text(&args[0]))),
            "replace" => s(&text(&owner).replacen(&text(&args[0]), &text(&args[1]), 1)),
            "split" => {
                let x = text(&owner);
                let n = x.chars().take_while(char::is_ascii_digit).count();
                a(vec![s(""), s(&x[..n]), s(&x[n..])])
            }
            "dayjs" => o(vec![(
                "ms",
                args.first().cloned().unwrap_or(V::N(60000000.)),
            )]),
            "valueOf" => get(&owner, "ms"),
            "bind" => {
                let V::F(_, name) = *owner else { panic!() };
                assert_eq!(name, "mapOrder", "READ native bind callable");
                assert!(eq(&args[0], &self.owner), "READ native bind receiver");
                V::F(Box::new(args[0].clone()), "boundMap".into())
            }
            "flatMap" => {
                let V::F(target, k) = &args[0] else { panic!() };
                assert_eq!(k, "boundMap", "READ bound map method");
                assert!(eq(target, &self.owner), "READ bound owner");
                a(vals(&owner)
                    .into_iter()
                    .map(|v| a(vec![s("mapped"), v]))
                    .flat_map(|v| vals(&v))
                    .collect())
            }
            "flat" => a(vals(&owner)
                .into_iter()
                .flat_map(|v| {
                    if matches!(v, V::A(_)) {
                        vals(&v)
                    } else {
                        vec![v]
                    }
                })
                .collect()),
            "push" => {
                let V::A(v) = *owner else { panic!() };
                v.borrow_mut().extend(args);
                let n = v.borrow().len();
                V::N(n as f64)
            }
            "inverseObj" => {
                let out = o(vec![]);
                let V::O(v) = &args[0] else { panic!() };
                for (k, v) in v.borrow().iter() {
                    put(&out, &text(v), s(k))
                }
                out
            }
            "omitUndefined" | "omit" => {
                let out = o(vec![]);
                let V::O(v) = &args[0] else { panic!() };
                let omit = if key == "omit" {
                    vals(&args[1]).iter().map(text).collect::<Vec<_>>()
                } else {
                    vec![]
                };
                for (k, v) in v.borrow().iter() {
                    if (key != "omitUndefined" || !matches!(v, V::U)) && !omit.contains(k) {
                        put(&out, k, v.clone())
                    }
                }
                out
            }
            "orderBy" => {
                native(&args[1], &a(vec![s("timestamp")]), "candle sort keys");
                native(&args[2], &a(vec![s("asc")]), "candle sort direction");
                let mut v = if matches!(args[0], V::U) {
                    vec![]
                } else {
                    vals(&args[0])
                };
                v.sort_by(|a, b| {
                    num(&get(a, "timestamp"))
                        .partial_cmp(&num(&get(b, "timestamp")))
                        .unwrap()
                });
                a(v)
            }
            "get" | "post" => {
                native(&owner, &s("regular"), "regular HTTP client");
                let endpoint = text(&args[0]);
                let i = self.page.get();
                self.page.set(i + 1);
                self.requests
                    .borrow_mut()
                    .push(a(vec![s(&endpoint), args[1].clone()]));
                if matches!(self.scene, "reject" | "virtual-reject")
                    || self.scene == "unknown"
                    || (self.scene == "partial" && i == 0)
                {
                    V::P(Box::new(Err(self.err())))
                } else {
                    o(vec![("data", self.data(&endpoint, i))])
                }
            }
            "reinitOnThrow" => {
                if let V::P(result) = &args[0] {
                    if result.is_err() {
                        put(&self.owner, "needsReinit", V::B(true));
                    }
                }
                args[0].clone()
            }
            "fetchRawBalance" => {
                self.event(&key, vec![]);
                self.data("BALANCE", 1).pipe_result_list()
            }
            "preserveRecentlyPlacedOrders" => {
                self.event(&key, vec![]);
                args[0].clone()
            }
            "fetchMarketsHelper" => {
                native(&args[0], &s("spot"), "market category");
                if self.scene == "null" {
                    a(vec![a(vec![V::Null])])
                } else if self.scene == "empty" {
                    a(vec![])
                } else {
                    a(vec![a(vec![s("new")]), a(vec![V::B(false)])])
                }
            }
            "emitAndLogError" => {
                self.event(&key, args);
                V::U
            }
            "resolveCallerClientOrderId" => {
                self.event(&key, vec![]);
                get(&args[0], "clientOrderId")
            }
            "getOrderExecutionParams" => {
                self.event(&key, vec![]);
                if self.scene == "execution" {
                    o(vec![("reduceOnly", V::B(true))])
                } else {
                    V::U
                }
            }
            "_hasFullSizedPositionOppositeSide" => V::B(self.scene == "protected"),
            "getOrderPositionIdx" => {
                self.event(&key, vec![]);
                V::N(0.)
            }
            "assertSingleNativeClientOrderId" => {
                self.event(&key, args.clone());
                if !matches!(args[0], V::U) && num(&args[1]) != 1. {
                    return Err(self.err());
                }
                V::U
            }
            "recordRecentlyPlacedOrderFromOpts" => {
                self.recorded.borrow_mut().push(a(args));
                V::U
            }
            "isUnknownOutcome" => V::B(self.scene == "unknown"),
            "placeStopLossOrTakeProfit" | "placeTrailingStopLoss" => {
                self.event(&key, args);
                a(vec![s("delegated")])
            }
            "cancelOrders" | "applyLocalOrderUpdateAfterRestWrite" => {
                self.event(&key, args);
                V::U
            }
            _ => panic!("READ unimplemented call {key}"),
        })
    }
    fn wait(&self, v: V) -> SessionFuture<'_, V, E> {
        Box::pin(async move { if let V::P(v) = v { *v } else { Ok(v) } })
    }
    fn intrinsic(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match name {
            "null" => V::Null,
            "keyGet" => get(&args[0], &text(&args[1])),
            "template" => s(&text(&args[0])),
            "add" => V::N(num(&args[0]) + num(&args[1])),
            "subtract" => V::N(num(&args[0]) - num(&args[1])),
            "multiply" => V::N(num(&args[0]) * num(&args[1])),
            "divide" => V::N(num(&args[0]) / num(&args[1])),
            "remainder" => V::N(num(&args[0]) % num(&args[1])),
            "greater" => V::B(num(&args[0]) > num(&args[1])),
            "lessEqual" => V::B(num(&args[0]) <= num(&args[1])),
            "suffixTemplate" => s(&format!("{}{}", text(&args[0]), text(&args[1]))),
            "missingMarket" => s(&format!("Market {} not found", text(&args[0]))),
            "intervalRegex" => s("digit-regex"),
            "has" => V::B(if let V::O(v) = &args[0] {
                v.borrow().iter().any(|(k, _)| *k == text(&args[1]))
            } else {
                false
            }),
            "arraySpread" => a([vals(&args[0]), vals(&args[1])].concat()),
            "capturedMember" => {
                let V::F(_, k) = &args[1] else { panic!() };
                V::F(Box::new(args[0].clone()), k.clone())
            }
            "copyOwnDataProperties" => {
                let V::O(v) = &args[1] else { panic!() };
                for (k, v) in v.borrow().iter() {
                    put(&args[0], k, v.clone())
                }
                V::U
            }
            _ => panic!("READ intrinsic {name}"),
        })
    }
    fn allocate(&self, _: V) -> Result<V, E> {
        panic!("unused allocation")
    }
    fn callback(&self, _: V, _: &'static str, _: Vec<V>) -> Result<V, E> {
        panic!("unused callback")
    }
    fn promise(&self) -> Result<(V, V, V), E> {
        panic!("unused promise")
    }
    fn error(&self, v: V) -> Result<E, E> {
        Ok(E(o(vec![("message", s(&text(&v)))])))
    }
    fn error_value(&self, e: E) -> V {
        e.0
    }
    fn normalize_error(&self, e: E) -> Result<V, E> {
        Ok(e.0)
    }
}
trait ResultList {
    fn pipe_result_list(&self) -> V;
}
impl ResultList for V {
    fn pipe_result_list(&self) -> V {
        get(&get(self, "result"), "list")
    }
}
fn array_method(method: V, expected: &str) -> Vec<V> {
    let V::F(owner, key) = method else { panic!() };
    assert_eq!(key, expected, "SPOT native selected array method");
    vals(&owner)
}
impl SpotOrderHost for H {
    fn spread(&self, v: V) -> Result<V, E> {
        if let V::O(v) = v {
            Ok(V::O(Rc::new(RefCell::new(v.borrow().clone()))))
        } else {
            Ok(o(vec![]))
        }
    }
    fn map<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        Ok(a(array_method(m, "map")
            .into_iter()
            .enumerate()
            .map(|(i, v)| f(v, i))
            .collect::<Result<Vec<_>, _>>()?))
    }
    fn filter<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        let mut out = vec![];
        for (i, v) in array_method(m, "filter").into_iter().enumerate() {
            if f(v.clone(), i)? {
                out.push(v)
            }
        }
        Ok(a(out))
    }
    fn some<F>(&self, m: V, mut f: F) -> Result<bool, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        for (i, v) in array_method(m, "some").into_iter().enumerate() {
            if f(v, i)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn find<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        for (i, v) in array_method(m, "find").into_iter().enumerate() {
            if f(v.clone(), i)? {
                return Ok(v);
            }
        }
        Ok(V::U)
    }
    fn reduce<F>(&self, m: V, mut acc: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, V) -> Result<V, E>,
    {
        for v in array_method(m, "reduce") {
            acc = f(acc, v)?
        }
        Ok(acc)
    }
    fn for_of<F>(&self, v: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(V) -> Result<(), E>,
    {
        for v in vals(&v) {
            f(v)?
        }
        Ok(())
    }
    fn series<'a, F>(&'a self, callee: V, v: V, mut f: F) -> SessionFuture<'a, V, E>
    where
        F: FnMut(V, usize) -> SessionFuture<'a, V, E> + 'a,
    {
        let V::F(owner, key) = callee else {
            panic!("SPOT native series callee")
        };
        assert!(eq(&owner, &self.global), "SPOT native series owner");
        assert!(
            matches!(key.as_str(), "mapSeries" | "forEachSeries"),
            "SPOT native selected series method"
        );
        Box::pin(async move {
            let mut out = vec![];
            let mut i = 0;
            while i < vals(&v).len() {
                let entry = self.wait(vals(&v)[i].clone()).await?;
                out.push(f(entry, i).await?);
                i += 1;
            }
            Ok(if key == "mapSeries" { a(out) } else { V::U })
        })
    }
}
impl SpotExchangeHost for H {
    fn destructure(&self, v: V, skip: usize, n: usize) -> Result<Vec<V>, E> {
        if self.op == "candles" && n == 2 {
            assert_eq!(skip, 1, "READ native candle iterator elision");
        }
        let mut v = vals(&v).into_iter().skip(skip).collect::<Vec<_>>();
        v.resize(n, V::U);
        v.truncate(n);
        Ok(v)
    }
    fn for_each<F>(&self, m: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(V, usize) -> Result<(), E>,
    {
        for (i, v) in array_method(m, "forEach").into_iter().enumerate() {
            f(v, i)?
        }
        Ok(())
    }
    fn times<F>(&self, m: V, n: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<V, E>,
    {
        let V::F(owner, k) = m else { panic!() };
        assert!(eq(&owner, &self.global), "READ times owner");
        assert_eq!(k, "times", "READ times callable");
        Ok(a((0..num(&n) as usize)
            .map(|i| f(V::N(i as f64)))
            .collect::<Result<Vec<_>, _>>()?))
    }
    fn new_error(&self, m: V, msg: V) -> Result<E, E> {
        let V::F(owner, k) = m else { panic!() };
        assert!(eq(&owner, &self.global), "READ Error owner");
        assert_eq!(k, "Error", "READ Error constructor");
        self.error(msg)
    }
    fn key_set(&self, v: &V, k: V, value: V) -> Result<(), E> {
        put(v, &text(&k), value);
        Ok(())
    }
}
const GROUPS: [(&str, &[&str]); 8] = [
    ("raw", &["normal", "promote", "api-error", "reject"]),
    ("balance", &["normal", "zero-free", "empty", "null"]),
    ("orders", &["normal", "pages", "empty", "api-error"]),
    ("markets", &["normal", "null", "empty"]),
    ("candles", &["normal", "from", "cap", "missing-list"]),
    ("map", &["normal", "virtual", "inverse", "full"]),
    (
        "place",
        &[
            "normal",
            "split",
            "client",
            "split-client",
            "unknown",
            "api-error",
            "protected",
            "trailing",
            "partial",
        ],
    ),
    (
        "update",
        &[
            "normal",
            "virtual",
            "missing-parent",
            "virtual-reject",
            "virtual-api-error",
            "not-modified",
            "api-error",
            "reject",
            "project",
            "trailing",
            "mark-zero",
        ],
    ),
];
fn input(op: &str, scene: &str) -> V {
    match op {
        "balance" => {
            if scene == "empty" {
                a(vec![])
            } else if scene == "null" {
                V::Null
            } else {
                let coin = o(vec![
                    ("coin", s("BTC")),
                    ("equity", s("2.5")),
                    ("usdValue", s("99.5")),
                ]);
                a(vec![o(vec![
                    ("totalEquity", s("101.5")),
                    ("totalMaintenanceMargin", s("3.5")),
                    ("totalInitialMargin", s("7.25")),
                    (
                        "totalAvailableBalance",
                        if scene == "zero-free" {
                            s("0")
                        } else {
                            V::Null
                        },
                    ),
                    ("totalMarginBalance", s("82.25")),
                    ("coin", a(vec![coin])),
                ])])
            }
        }
        "candles" => o(vec![
            ("symbol", s("BTC")),
            ("interval", s("1m")),
            (
                "limit",
                if scene == "cap" {
                    V::N(2000.)
                } else if scene == "from" {
                    V::N(0.)
                } else {
                    V::N(2.)
                },
            ),
            ("from", V::N(10000.)),
            ("to", V::N(60000000.)),
        ]),
        "map" => o(vec![
            ("orderId", s("raw")),
            ("symbol", s("BTC")),
            ("orderType", s("Limit")),
            ("orderStatus", s("New")),
            ("side", s("Buy")),
            ("qty", s("9")),
            ("cumExecQty", s("2")),
            ("price", s("25")),
            ("timeInForce", s("GTC")),
            (
                "stopOrderType",
                s(if scene == "full" {
                    "StopLoss"
                } else {
                    "UNKNOWN"
                }),
            ),
            (
                "tpslMode",
                s(if scene == "full" { "Full" } else { "Partial" }),
            ),
            ("stopLoss", s(if scene == "virtual" { "20" } else { "0" })),
            ("takeProfit", s(if scene == "virtual" { "30" } else { "0" })),
            ("triggerBy", s("LastPrice")),
            ("slTriggerBy", s("MarkPrice")),
            ("tpTriggerBy", s("LastPrice")),
        ]),
        "place" => o(vec![
            ("symbol", s("BTC")),
            ("side", s("buy")),
            (
                "type",
                s(if scene == "protected" {
                    "stop_market"
                } else if scene == "trailing" {
                    "trailing_stop_market"
                } else {
                    "limit"
                }),
            ),
            (
                "amount",
                V::N(if matches!(scene, "split" | "split-client" | "partial") {
                    5.3
                } else {
                    1.3
                }),
            ),
            ("price", V::N(89.8)),
            ("stopLoss", V::N(70.)),
            ("takeProfit", V::N(120.)),
            ("slTriggerType", s("last_price")),
            ("tpTriggerType", s("mark_price")),
            (
                "clientOrderId",
                if matches!(scene, "client" | "split-client" | "unknown") {
                    s("caller")
                } else {
                    V::U
                },
            ),
        ]),
        "update" => o(vec![
            (
                "order",
                o(vec![
                    (
                        "id",
                        s(
                            if matches!(
                                scene,
                                "virtual"
                                    | "missing-parent"
                                    | "virtual-reject"
                                    | "virtual-api-error"
                            ) {
                                "parent__stop_loss"
                            } else {
                                "parent"
                            },
                        ),
                    ),
                    ("symbol", s("BTC")),
                    (
                        "type",
                        s(if scene == "trailing" {
                            "trailing_stop_market"
                        } else if matches!(
                            scene,
                            "project"
                                | "mark-zero"
                                | "virtual"
                                | "missing-parent"
                                | "virtual-reject"
                                | "virtual-api-error"
                        ) {
                            "stop_market"
                        } else {
                            "limit"
                        }),
                    ),
                    ("side", s("sell")),
                    ("info", o(vec![])),
                ]),
            ),
            (
                "update",
                o(vec![("price", V::N(110.)), ("amount", V::N(4.))]),
            ),
        ]),
        _ => V::U,
    }
}
fn expected_map(h: &H, raw: &V, scene: &str) -> V {
    let base = o(vec![
        ("id", s("raw")),
        ("status", s("open")),
        ("symbol", s("BTC")),
        (
            "type",
            s(if scene == "full" {
                "stop_market"
            } else {
                "limit"
            }),
        ),
        ("side", s("buy")),
        (
            "remaining",
            V::N(if scene == "inverse" { 0.28 } else { 7. }),
        ),
        ("extra", o(vec![("remainingNative", V::N(7.))])),
        ("price", V::N(25.)),
        ("amount", V::N(if scene == "full" { 0. } else { 9. })),
        ("filled", V::N(2.)),
        ("reduceOnly", V::B(false)),
        ("info", raw.clone()),
        ("timeInForce", s("GoodTillCancel")),
        ("lastUpdate", V::N(1234.)),
    ]);
    if scene == "full" {
        put(&base, "slTriggerType", s("last_price"));
    }
    let mut out = vec![base.clone()];
    if scene == "virtual" {
        for (i, id, ty, price, trigger_name, trigger) in [
            (
                1,
                "raw__stop_loss",
                "stop_market",
                20.,
                "slTriggerType",
                "mark_price",
            ),
            (
                2,
                "raw__take_profit",
                "take_profit_market",
                30.,
                "tpTriggerType",
                "last_price",
            ),
        ] {
            let row = h.spread(base.clone()).unwrap();
            for (k, v) in [
                ("id", s(id)),
                ("type", s(ty)),
                ("side", s("sell")),
                ("price", V::N(price)),
                ("filled", V::N(0.)),
                ("remaining", V::N(9.)),
                ("lastUpdate", V::N(1234. + i as f64)),
                (trigger_name, s(trigger)),
            ] {
                put(&row, k, v)
            }
            out.push(row)
        }
    }
    a(out)
}
fn native_assert(h: &H, op: &str, scene: &str, input: &V, result: &Result<V, E>) {
    let fails = matches!(
        (op, scene),
        ("raw", "reject")
            | ("orders", "api-error")
            | ("place", "split-client" | "unknown" | "api-error")
            | ("update", "reject" | "virtual-reject" | "virtual-api-error")
    );
    assert_eq!(result.is_err(), fails, "READ native outcome {op}/{scene}");
    if let Err(e) = result {
        let original = matches!(
            (op, scene),
            ("raw", "reject")
                | ("place", "split-client" | "unknown")
                | ("update", "reject" | "virtual-reject")
        );
        assert_eq!(
            eq(&e.0, &h.original),
            original,
            "READ native original error identity {op}/{scene}"
        );
        native(
            &get(&e.0, "message"),
            &s(if original { "original" } else { "bad" }),
            "error message",
        );
        if op == "orders" {
            native(
                &get(&h.owner, "needsReinit"),
                &V::B(true),
                "order reinit flag",
            );
        }
        if op == "update" && scene.starts_with("virtual-") {
            let events = h.events.borrow();
            assert_eq!(
                events.len(),
                if scene == "virtual-api-error" { 2 } else { 1 },
                "READ native virtual error emitter count"
            );
            let last = vals(events.last().unwrap());
            let args = vals(&last[1]);
            assert!(
                eq(&args[0], &e.0),
                "READ native virtual emitter original error identity"
            );
            native(
                &args[1],
                &s(if original { "original" } else { "bad" }),
                "virtual error detail",
            );
        }
        if scene == "split-client" {
            assert_eq!(
                h.requests.borrow().len(),
                0,
                "READ native preflight no request"
            );
        }
        return;
    }
    let out = result.as_ref().unwrap();
    match op {
        "raw" => {
            native(
                out,
                &a(vec![s(if scene == "api-error" {
                    "prior"
                } else {
                    "account"
                })]),
                "raw balance value",
            );
            if scene == "api-error" {
                assert!(
                    eq(out, &get(&get(&get(&h.owner, "store"), "balance"), "info")),
                    "READ native raw fallback alias"
                );
            }
            if scene == "promote" {
                native(
                    &get(&h.owner, "forceUnifiedBalance"),
                    &V::B(true),
                    "promoted flag",
                );
                native(
                    &get(&h.owner, "unifiedMarginStatus"),
                    &V::N(6.),
                    "promoted account status",
                );
            }
            let req = &h.requests.borrow()[0];
            native(
                &get(&vals(req)[1], "params"),
                &o(vec![("accountType", s("CONTRACT"))]),
                "raw request",
            );
        }
        "balance" => {
            if matches!(scene, "empty" | "null") {
                assert!(
                    eq(out, &get(&get(&h.owner, "store"), "balance")),
                    "READ native empty balance alias"
                );
            } else {
                let first = &vals(input)[0];
                native(
                    out,
                    &o(vec![
                        ("total", V::N(101.5)),
                        ("upnl", V::N(0.)),
                        ("used", V::N(10.75)),
                        ("free", V::N(if scene == "zero-free" { 0. } else { 82.25 })),
                        (
                            "coins",
                            o(vec![(
                                "BTC",
                                o(vec![
                                    ("base", s("BTC")),
                                    ("amount", V::N(2.5)),
                                    ("notional", V::N(99.5)),
                                ]),
                            )]),
                        ),
                        ("info", first.clone()),
                    ]),
                    "formatted balance",
                );
                assert!(
                    eq(&get(out, "info"), first),
                    "READ native account info alias"
                );
            }
        }
        "orders" => {
            native(
                out,
                &a(if scene == "empty" {
                    vec![]
                } else {
                    vec![s("mapped"), o(vec![("id", s("raw"))])]
                }),
                "mapped pager result",
            );
            assert_eq!(
                h.requests.borrow().len(),
                if scene == "pages" { 2 } else { 1 },
                "READ native pager request count"
            );
            if scene == "pages" {
                native(
                    &get(&get(&vals(&h.requests.borrow()[1])[1], "params"), "cursor"),
                    &s("next"),
                    "pager next cursor",
                );
            }
            assert_eq!(
                h.events.borrow().len(),
                1,
                "READ native preserve invocation"
            );
        }
        "markets" => {
            if scene == "null" {
                assert!(
                    eq(out, &get(&get(&h.owner, "store"), "markets")),
                    "READ native markets fallback alias"
                );
            }
            native(
                out,
                &a(if scene == "null" {
                    vec![s("stored")]
                } else if scene == "empty" {
                    vec![]
                } else {
                    vec![s("new"), V::B(false)]
                }),
                "market flatten and strict null",
            )
        }
        "candles" => {
            native(
                out,
                &a(if scene == "missing-list" {
                    vec![]
                } else {
                    vec![
                        o(vec![
                            ("timestamp", V::N(1.)),
                            ("open", V::N(11.)),
                            ("high", V::N(14.)),
                            ("low", V::N(9.)),
                            ("close", V::N(13.)),
                            ("volume", V::N(7.)),
                        ]),
                        o(vec![
                            ("timestamp", V::N(2.)),
                            ("open", V::N(21.)),
                            ("high", V::N(24.)),
                            ("low", V::N(19.)),
                            ("close", V::N(23.)),
                            ("volume", V::N(17.)),
                        ]),
                    ]
                }),
                "candles sorted and decoded",
            );
            let limit = if scene == "cap" {
                1000.
            } else if scene == "from" {
                500.
            } else {
                2.
            };
            let start = if scene == "from" {
                10000.
            } else {
                60000000. - limit * 60000.
            };
            native(
                &get(&vals(&h.requests.borrow()[0])[1], "params"),
                &o(vec![
                    ("category", s("spot")),
                    ("symbol", s("BTC")),
                    ("start", V::N(start)),
                    ("end", V::N(60000000.)),
                    ("interval", s("1")),
                    ("limit", V::N(limit)),
                ]),
                "candle request bounds",
            );
        }
        "map" => {
            native(out, &expected_map(h, input, scene), "mapped order fields");
            for row in vals(out) {
                assert!(
                    eq(&get(&row, "info"), input),
                    "READ native order info alias"
                );
            }
            if scene == "virtual" {
                let rows = vals(out);
                assert!(
                    eq(&get(&rows[0], "extra"), &get(&rows[2], "extra")),
                    "READ native virtual extra alias"
                );
            }
        }
        "place" => {
            if matches!(scene, "protected" | "trailing") {
                native(out, &a(vec![s("delegated")]), "placement delegate result");
                assert_eq!(
                    h.requests.borrow().len(),
                    0,
                    "READ native delegated no request"
                );
                return;
            }
            let count = if scene == "partial" {
                3
            } else if scene == "split" {
                4
            } else {
                1
            };
            assert_eq!(vals(out).len(), count, "READ native placement ID count");
            assert_eq!(
                h.requests.borrow().len(),
                if matches!(scene, "partial" | "split") {
                    4
                } else {
                    1
                },
                "READ native split request count"
            );
            native(
                &vals(out)[0],
                &s(if scene == "partial" { "id-1" } else { "id-0" }),
                "placement first ID",
            );
            let first = &vals(&h.requests.borrow()[0])[1];
            native(&get(first, "qty"), &s("1.3"), "placement first qty");
            native(&get(first, "price"), &s("90"), "placement adjusted price");
            native(
                &get(first, "tpTriggerBy"),
                &s("LastPrice"),
                "placement TS attached TP bug",
            );
            if scene == "client" {
                native(
                    &get(first, "orderLinkId"),
                    &s("caller"),
                    "placement client ID",
                );
            }
            if scene == "split" {
                native(
                    &get(&vals(&h.requests.borrow()[3])[1], "qty"),
                    &s("1.3"),
                    "split rest qty",
                );
                native(
                    &get(&vals(&h.requests.borrow()[1])[1], "takeProfit"),
                    &V::U,
                    "later lot omits protection",
                );
            }
            assert_eq!(
                h.recorded.borrow().len(),
                count,
                "READ native recorded count"
            );
            for (i, record) in h.recorded.borrow().iter().enumerate() {
                let slots = vals(record);
                native(
                    &slots[0],
                    &s(&format!(
                        "id-{}",
                        if scene == "partial" { i + 1 } else { i }
                    )),
                    "recorded original compacted ID",
                );
                assert!(eq(&slots[1], input), "READ native recorded opts alias");
                assert!(
                    eq(&slots[2], &vals(&h.requests.borrow()[i])[1]),
                    "READ native recorded original payload alias"
                );
            }
        }
        "update" => {
            if matches!(scene, "trailing" | "project") {
                native(out, &a(vec![s("delegated")]), "update delegated IDs");
                assert_eq!(
                    h.requests.borrow().len(),
                    0,
                    "READ native replacement no HTTP"
                );
                assert_eq!(
                    h.events.borrow().len(),
                    3,
                    "READ native cancel replace local order"
                );
                return;
            }
            let empty = matches!(
                scene,
                "missing-parent" | "not-modified" | "api-error" | "mark-zero"
            );
            native(
                out,
                &a(if empty { vec![] } else { vec![s("id-0")] }),
                "update IDs",
            );
            if scene == "missing-parent" {
                assert_eq!(
                    h.requests.borrow().len(),
                    0,
                    "READ native missing parent no request"
                );
                return;
            }
            assert_eq!(
                h.requests.borrow().len(),
                1,
                "READ native amend request count"
            );
            let req = &h.requests.borrow()[0];
            native(
                &vals(req)[0],
                &s(if scene == "mark-zero" {
                    "SET_TRADING_STOP"
                } else {
                    "REPLACE_ORDER"
                }),
                "update endpoint",
            );
            let payload = &vals(req)[1];
            if scene == "virtual" {
                native(
                    payload,
                    &o(vec![
                        ("category", s("spot")),
                        ("orderId", s("parent")),
                        ("symbol", s("BTC")),
                        ("stopLoss", s("110")),
                    ]),
                    "virtual update payload",
                );
            } else if scene == "mark-zero" {
                native(
                    payload,
                    &o(vec![
                        ("category", s("spot")),
                        ("symbol", s("BTC")),
                        ("positionIdx", V::N(0.)),
                        ("stopLoss", s("110")),
                    ]),
                    "protection update payload",
                );
            } else {
                native(
                    payload,
                    &o(vec![
                        ("category", s("spot")),
                        ("orderId", s("parent")),
                        ("symbol", s("BTC")),
                        ("qty", s("4")),
                        ("price", s("110")),
                    ]),
                    "amend payload",
                );
            }
        }
        _ => unreachable!(),
    }
}
fn source(cases: Json) -> Json {
    let mut c = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-exchange-reads-oracle.mjs"
        ))
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
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "READ source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn spot_exchange_reads_native_before_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for (op, scenes) in GROUPS {
        for scene in scenes {
            let h = H::new(op, scene);
            let input = input(op, scene);
            let result = match op {
                "raw" => run(api::fetch_raw_balance(&h, &h.owner)),
                "balance" => api::format_balance(&h, &h.owner, input.clone()),
                "orders" => run(api::fetch_orders(&h, &h.owner)),
                "markets" => run(api::fetch_markets(&h, &h.owner)),
                "candles" => run(api::fetch_ohlcv(&h, &h.owner, input.clone())),
                "map" => api::map_order(&h, &h.owner, input.clone()),
                "place" => run(api::place_order(&h, &h.owner, input.clone())),
                "update" => run(api::update_order(&h, &h.owner, input.clone())),
                _ => unreachable!(),
            };
            native_assert(&h, op, scene, &input, &result);
            cases.push(json!({"op":op,"scene":scene,"input":wire(&input)}));
            outputs.push((op, scene, result, h));
        }
    }
    assert_eq!(outputs.len(), 43, "READ independent Rust case floor");
    let oracle = source(json!(cases));
    assert_eq!(
        oracle.as_array().unwrap().len(),
        43,
        "READ independent source case floor"
    );
    for (i, (op, scene, result, h)) in outputs.into_iter().enumerate() {
        let result = match result {
            Ok(v) => json!({"ok":wire(&v)}),
            Err(e) => json!({"error":wire(&get(&e.0,"message")),"original":eq(&e.0,&h.original)}),
        };
        let actual = json!({"result":result,"requests":h.requests.borrow().iter().map(wire).collect::<Vec<_>>(),"events":h.events.borrow().iter().map(wire).collect::<Vec<_>>(),"recorded":h.recorded.borrow().iter().map(wire).collect::<Vec<_>>()});
        assert_eq!(actual, oracle[i], "READ source parity {op}/{scene}");
    }
}
