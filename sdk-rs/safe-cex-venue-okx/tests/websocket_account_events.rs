use safe_cex_venue_okx::{
    market_account_reads::MarketAccountOps,
    websocket_account_events::{self as port, SocketEventOps},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type V = Rc<Native>;
type E = Rc<Error>;
struct Error {
    name: &'static str,
    message: String,
    fresh: bool,
    code: Option<&'static str>,
    ids: Option<[u8; 2]>,
}
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    Call(String),
    Member(V, String),
    Callback(Rc<dyn Fn(V) -> Result<V, E>>),
    Map(RefCell<Vec<(V, V)>>),
    Set(RefCell<Vec<V>>),
    Wire(V),
    Coerce(E),
}
fn u() -> V {
    Rc::new(Native::U)
}
fn s(x: &str) -> V {
    Rc::new(Native::S(x.encode_utf16().collect()))
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
        Native::S(s) => String::from_utf16_lossy(s),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::U => "undefined".into(),
        Native::Null => "null".into(),
        Native::B(x) => x.to_string(),
        _ => panic!("primitive"),
    }
}
fn num(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::S(x) => safe_cex_num::number_from_string(&String::from_utf16_lossy(x)),
        Native::Null => 0.,
        Native::U => f64::NAN,
        _ => panic!("number"),
    }
}
fn truth(v: &V) -> bool {
    match v.as_ref() {
        Native::U | Native::Null => false,
        Native::B(x) => *x,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn null(v: &V) -> bool {
    matches!(v.as_ref(), Native::U | Native::Null)
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        (Native::S(a), Native::S(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::B(a), Native::B(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::U => json!({"u":true}),
        Native::Null => Value::Null,
        Native::B(x) => json!(x),
        Native::S(x) => json!({"s":x}),
        Native::N(x) => {
            json!({"n":if x.is_nan(){"nan".into()}else{format!("{:016x}",x.to_bits())}})
        }
        Native::O(x) => {
            json!({"o":x.borrow().iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()})
        }
        Native::A(x) => json!(x.borrow().iter().map(pack).collect::<Vec<_>>()),
        Native::Call(x) => json!({"call":x}),
        Native::Map(rows) => json!(
            rows.borrow()
                .iter()
                .filter(|(k, _)| !matches!(k.as_ref(), Native::U))
                .map(|(k, v)| json!([pack(k), pack(v)]))
                .collect::<Vec<_>>()
        ),
        Native::Set(rows) => json!(rows.borrow().iter().map(pack).collect::<Vec<_>>()),
        Native::Wire(v) => pack(v),
        Native::Coerce(_) => json!({"o":[]}),
        _ => panic!("pack"),
    }
}
fn unused<T>() -> T {
    panic!("unused capability in this source family")
}
struct State {
    case: String,
    err: E,
    receiver: V,
    store: V,
    map: V,
    mapped_orders: V,
    positions: V,
    market: V,
    events: RefCell<Vec<Value>>,
    calls: RefCell<Vec<(String, Vec<V>)>>,
    now: Cell<f64>,
    replacement: Cell<bool>,
    sends: Cell<usize>,
}
fn event(t: &State, name: &str, args: Vec<V>) -> Result<(), E> {
    t.events.borrow_mut().push(json!(
        std::iter::once(json!(name))
            .chain(args.iter().map(pack))
            .collect::<Vec<_>>()
    ));
    t.calls.borrow_mut().push((name.into(), args));
    if t.case == format!("fail:{name}") {
        Err(t.err.clone())
    } else {
        Ok(())
    }
}
fn map_entries(v: &V) -> Vec<(V, V)> {
    let Native::Map(entries) = v.as_ref() else {
        panic!("map")
    };
    entries
        .borrow()
        .iter()
        .filter(|(k, _)| !matches!(k.as_ref(), Native::U))
        .cloned()
        .collect()
}
fn units(v: &V, t: &State) -> Result<Vec<u16>, E> {
    match v.as_ref() {
        Native::S(v) => Ok(v.clone()),
        Native::Coerce(e) => {
            event(t, "coerce", vec![])?;
            Err(e.clone())
        }
        _ => Ok(text(v).encode_utf16().collect()),
    }
}
fn parse(v: &V, t: &State) -> Result<f64, E> {
    Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(
        &units(v, t)?,
    )))
}
fn finite(v: &V, t: &State) -> Result<f64, E> {
    let p = if null(v) { 0. } else { parse(v, t)? };
    Ok(if p.is_finite() { p } else { 0. })
}
fn invoke(t: Rc<State>, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Callback(f) = callee.as_ref() {
        return f(args[0].clone());
    }
    if let Native::Member(receiver, key) = callee.as_ref() {
        assert_eq!(key, "slice");
        let rows = items(receiver);
        return Ok(array(
            rows[num(&args[0]) as usize..(num(&args[1]) as usize).min(rows.len())].to_vec(),
        ));
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("callee")
    };
    Ok(match name.as_str() {
        "this.parent.mapOrders" => {
            event(&t, "mapOrders", args)?;
            if t.case == "unmapped" {
                array(vec![])
            } else {
                t.mapped_orders.clone()
            }
        }
        "this.store.markOrdersTerminal" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.mapped_orders),
                "ORDER native terminal original mapped rows"
            );
            event(&t, "terminal", args)?;
            u()
        }
        "this.store.addOrUpdateOrders" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.mapped_orders),
                "ORDER native original mapped rows"
            );
            event(&t, "orders", args)?;
            u()
        }
        "this.isDuplicateFillEvent" => {
            event(&t, "dedupe", args)?;
            b(t.case == "duplicate")
        }
        "this.parent.contractsToBase" => {
            assert!(
                Rc::ptr_eq(&args[1], &t.market),
                "ORDER native original conversion market"
            );
            let amount = num(&args[0]) * 2.;
            event(&t, "base", args)?;
            n(amount)
        }
        "this.emitter.emit" => {
            event(&t, "emit", args)?;
            u()
        }
        "this.parent.mapPositions" => {
            event(&t, "mapPositions", args)?;
            if t.case == "empty" {
                array(vec![])
            } else {
                t.positions.clone()
            }
        }
        "this.parent.convertMarginValueToUsd" => {
            let amount = num(&args[0]) * 2.;
            event(&t, "margin", args)?;
            n(amount)
        }
        "this.parseFinite" => n(finite(&args[0], &t)?),
        "this.sumFinite" => {
            let mut sum = 0.;
            for v in items(&args[0]) {
                sum += finite(&v, &t)?;
            }
            n(sum)
        }
        "parseFloat" => n(parse(&args[0], &t)?),
        "String" => Rc::new(Native::S(units(&args[0], &t)?)),
        "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
        "Number.isFinite" => b(matches!(args[0].as_ref(),Native::N(x) if x.is_finite())),
        "roundUSD" => n(safe_cex_num::round_usd(num(&args[0]))),
        "sumBy" => {
            let values = items(&args[0]);
            n(values
                .iter()
                .skip(1)
                .map(num)
                .fold(num(&values[0]), |a, b| a + b))
        }
        "subtract" => n(safe_cex_num::subtract(num(&args[0]), num(&args[1]))),
        "this.store.addOrUpdatePositions" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.positions),
                "POSITION native original mapped positions"
            );
            event(&t, "positions", args)?;
            if t.case == "balance-reentry" {
                put(
                    &t.store,
                    "balance",
                    object(vec![
                        ("used", n(77.)),
                        ("upnl", n(88.)),
                        ("tag", s("changed")),
                    ]),
                );
            }
            u()
        }
        "this.store.update" => {
            event(&t, "update", args.clone())?;
            put(&t.store, "balance", get(&args[0], "balance"));
            u()
        }
        "Date.now" => {
            event(&t, "clock", args)?;
            n(t.now.get())
        }
        "this.recentFillKeys.delete" => {
            event(&t, "forget", args.clone())?;
            let Native::Map(rows) = t.map.as_ref() else {
                panic!("map")
            };
            let mut rows = rows.borrow_mut();
            let found = rows.iter_mut().find(|(k, _)| equal(k, &args[0]));
            if let Some((k, v)) = found {
                *k = u();
                *v = u();
                b(true)
            } else {
                b(false)
            }
        }
        "this.recentFillKeys.has" => {
            event(&t, "has", args.clone())?;
            b(map_entries(&t.map).iter().any(|(k, _)| equal(k, &args[0])))
        }
        "this.recentFillKeys.set" => {
            event(&t, "remember", args.clone())?;
            let Native::Map(rows) = t.map.as_ref() else {
                panic!("map")
            };
            let mut rows = rows.borrow_mut();
            if let Some((_, v)) = rows.iter_mut().find(|(k, _)| equal(k, &args[0])) {
                *v = args[1].clone()
            } else {
                rows.push((args[0].clone(), args[1].clone()))
            }
            t.map.clone()
        }
        "this.parent.getIndexTickerInstId" => {
            event(&t, "index", vec![get(&args[0], "id")])?;
            let mut id = units(&get(&args[0], "id"), &t)?;
            id.extend("-INDEX".encode_utf16());
            Rc::new(Native::S(id))
        }
        "JSON.stringify" => Rc::new(Native::Wire(args[0].clone())),
        "this.send" => {
            let Native::Wire(payload) = args[0].as_ref() else {
                panic!("packet")
            };
            event(&t, "send", vec![payload.clone()])?;
            t.sends.set(t.sends.get() + 1);
            if t.case == "send-reentry" && t.sends.get() == 1 {
                let topics = get(&t.receiver, "topics");
                let funding = items(&get(&topics, "funding-rate"));
                put(&funding[20], "instId", s("changed"));
            }
            u()
        }
        x if x.starts_with("this.handle") || x == "replacement" => {
            event(&t, x.strip_prefix("this.").unwrap_or(x), args)?;
            u()
        }
        other => panic!("call {other}"),
    })
}
fn host(t: Rc<State>) -> Rc<SocketEventOps<V, E>> {
    let capture = t.clone();
    let call = t.clone();
    let intrinsic = t.clone();
    let value = MarketAccountOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            Ok(match name {
                "this.store.marketsById" => get(&capture.store, "marketsById"),
                "this.store.balance" => get(&capture.store, "balance"),
                "this.store.balance.used" => get(&get(&capture.store, "balance"), "used"),
                "this.store.balance.upnl" => get(&get(&capture.store, "balance"), "upnl"),
                "this.recentFillKeys" => capture.map.clone(),
                "this.fillDedupeWindowMs" => n(60000.),
                "this.topics" => get(&capture.receiver, "topics"),
                "this.ws" => get(&capture.receiver, "ws"),
                "this.isDisposed" => get(&capture.receiver, "disposed"),
                "OrderStatus.Closed" => s("closed"),
                "OrderStatus.Canceled" => s("canceled"),
                "this.handleTickerEvents" if capture.replacement.get() => {
                    Rc::new(Native::Call("replacement".into()))
                }
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        invoke: Rc::new(move |f, args| invoke(call.clone(), f, args)),
        intrinsic: Rc::new(move |name, args| {
            Ok(match name {
                "construct" => {
                    let Native::Call(class) = args[0].as_ref() else {
                        panic!("class")
                    };
                    event(&intrinsic, "construct", vec![s(class)])?;
                    match class.as_str() {
                        "Map" => Rc::new(Native::Map(RefCell::new(vec![]))),
                        "Set" => Rc::new(Native::Set(RefCell::new(vec![]))),
                        "WsDirectFallback" => object(vec![("tag", s("fallback"))]),
                        _ => panic!("constructor"),
                    }
                }
                "get-index" => {
                    let key = String::from_utf16(&units(&args[1], &intrinsic)?).unwrap();
                    get(&args[0], &key)
                }
                "greater-than" => b(num(&args[0]) > num(&args[1])),
                "less-than" => b(num(&args[0]) < num(&args[1])),
                "less-or-equal" => b(num(&args[0]) <= num(&args[1])),
                "subtract" => n(num(&args[0]) - num(&args[1])),
                "template-string" => Rc::new(Native::S(units(&args[0], &intrinsic)?)),
                "colon-append" => {
                    let mut out = units(&args[0], &intrinsic)?;
                    out.push(58);
                    out.extend(units(&args[1], &intrinsic)?);
                    Rc::new(Native::S(out))
                }
                "object-spread" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("spread")
                    };
                    Rc::new(Native::O(RefCell::new(fields.borrow().clone())))
                }
                "define-literal-field" => {
                    let key = String::from_utf16(&units(&args[1], &intrinsic)?).unwrap();
                    put(&args[0], &key, args[2].clone());
                    u()
                }
                "array-spread" => array(args.iter().flat_map(items).collect()),
                other => panic!("intrinsic {other}"),
            })
        }),
        truthy: Rc::new(truth),
        nullish: Rc::new(null),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        boolean: Rc::new(b),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        map: Rc::new(|v, f| {
            Ok(array(
                items(&v)
                    .into_iter()
                    .map(|v| f(v))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }),
        filter: Rc::new(|v, f| {
            let mut out = vec![];
            for value in items(&v) {
                if f(value.clone())? {
                    out.push(value)
                }
            }
            Ok(array(out))
        }),
        for_of: Rc::new(|v, f| {
            let mut i = 0;
            match v.as_ref() {
                Native::Map(rows) => {
                    while i < rows.borrow().len() {
                        let (k, value) = rows.borrow()[i].clone();
                        i += 1;
                        if !matches!(k.as_ref(), Native::U) {
                            f(array(vec![k, value]))?;
                        }
                    }
                }
                Native::A(rows) => {
                    while i < rows.borrow().len() {
                        let row = rows.borrow()[i].clone();
                        i += 1;
                        f(row)?;
                    }
                }
                _ => panic!("iterable"),
            }
            Ok(())
        }),
        bind_first: Rc::new(|_, _| unused()),
        flat_map: Rc::new(|_, _| unused()),
        reduce: Rc::new(|_, _, _| unused()),
        sum_by: Rc::new(|_, _, _| unused()),
        parallel_map: Rc::new(|_, _, _| unused()),
        some: Rc::new(|_, _| unused()),
        await_value: Rc::new(|_| unused()),
        error_value: Rc::new(|_| unused()),
        new_error: Rc::new(|_, _| unused()),
    };
    Rc::new(SocketEventOps {
        value: Rc::new(value),
        callback: Rc::new(|f| Rc::new(Native::Callback(f))),
    })
}
fn fixture(method: &str, case: &str) -> Value {
    let err = Rc::new(Error {
        name: "TypeError",
        message: "boom".into(),
        fresh: false,
        code: Some("SOCKET"),
        ids: Some([4, 9]),
    });
    let balance = object(vec![
        (
            "used",
            n(if case == "invalid-balance" {
                f64::INFINITY
            } else {
                3.
            }),
        ),
        (
            "upnl",
            n(if case == "invalid-balance" {
                f64::NAN
            } else {
                2.
            }),
        ),
        ("tag", s("retained")),
    ]);
    let market = object(vec![("id", s("I")), ("multiplier", n(2.))]);
    let store = object(vec![
        ("marketsById", object(vec![("I", market.clone())])),
        ("balance", balance),
    ]);
    let topics = object(vec![(
        "tickers",
        array(vec![object(vec![
            ("channel", s("old")),
            ("instId", s("old")),
        ])]),
    )]);
    let receiver = object(vec![
        ("topics", topics),
        (
            "ws",
            if case == "no-socket" {
                Rc::new(Native::Null)
            } else {
                object(vec![])
            },
        ),
        ("disposed", b(case == "disposed")),
    ]);
    let t = Rc::new(State {
        case: case.into(),
        err: err.clone(),
        receiver,
        store,
        map: Rc::new(Native::Map(RefCell::new(vec![
            (s("stale"), n(9.)),
            (s("edge"), n(10.)),
            (s("future"), n(11.)),
        ]))),
        mapped_orders: array(vec![object(vec![
            ("id", s("mapped")),
            ("side", s("sell")),
            ("symbol", s("SYM")),
        ])]),
        positions: array(vec![
            object(vec![("id", s("P")), ("unrealizedPnl", n(1.235))]),
            object(vec![("id", s("Q")), ("unrealizedPnl", s("bad"))]),
        ]),
        market,
        events: RefCell::new(vec![]),
        calls: RefCell::new(vec![]),
        now: Cell::new(60010.),
        replacement: Cell::new(false),
        sends: Cell::new(0),
    });
    let h = host(t.clone());
    let row = object(vec![
        ("instId", s(if case == "missing" { "MISSING" } else { "I" })),
        (
            "ordId",
            if case == "lone" {
                Rc::new(Native::S(vec![0xd800]))
            } else {
                s("O")
            },
        ),
        (
            "state",
            s(match case {
                "partial" => "partially_filled",
                "live" => "live",
                "cancel" => "canceled",
                "mmp" => "mmp_canceled",
                "unknown" => "unknown",
                _ => "filled",
            }),
        ),
        (
            "tradeId",
            if case == "zero-trade" {
                n(0.)
            } else if case == "fallback" {
                u()
            } else if case == "string-zero" {
                s("0")
            } else {
                s("T")
            },
        ),
        ("fillTime", s("F")),
        ("uTime", s("U")),
        (
            "fillPx",
            if case == "throw-price" {
                Rc::new(Native::Coerce(err.clone()))
            } else {
                s("12.5x")
            },
        ),
        (
            "fillSz",
            if case == "zero" {
                n(0.)
            } else if case == "nan" {
                s("bad")
            } else {
                s("2")
            },
        ),
    ]);
    let result = match method {
        "class-private" => port::PrivateSocketState::initialize(&h).map(|state| {
            assert_eq!(
                num(&state.fill_dedupe_window_ms).to_bits(),
                60000f64.to_bits(),
                "CLASS native private dedupe window"
            );
            assert!(
                map_entries(&state.recent_fill_keys).is_empty(),
                "CLASS native fresh fill map"
            );
            object(vec![
                ("keys", array(vec![])),
                ("fallback", state.direct_fallback),
                ("window", state.fill_dedupe_window_ms),
            ])
        }),
        "class-business" => port::BusinessSocketState::initialize(&h).map(|state| {
            assert_eq!(
                num(&state.base.fill_dedupe_window_ms),
                60000.,
                "CLASS native inherited dedupe window"
            );
            assert!(
                matches!(state.topics.as_ref(),Native::O(v) if v.borrow().is_empty()),
                "CLASS native fresh business topics"
            );
            object(vec![
                ("keys", array(vec![])),
                ("fallback", state.base.direct_fallback),
                ("window", state.base.fill_dedupe_window_ms),
                ("topics", state.topics),
            ])
        }),
        "class-public" => port::PublicSocketState::initialize(h.clone()).and_then(|state| {
            let channels = [
                "tickers",
                "mark-price",
                "index-tickers",
                "open-interest",
                "funding-rate",
            ];
            let data = object(vec![("tag", s("D"))]);
            for channel in channels {
                let callbacks = items(&get(&state.message_handlers, channel));
                assert_eq!(
                    callbacks.len(),
                    1,
                    "CLASS native independent callback count"
                );
                invoke(t.clone(), callbacks[0].clone(), vec![data.clone()])?;
            }
            t.replacement.set(true);
            invoke(
                t.clone(),
                items(&get(&state.message_handlers, "tickers"))[0].clone(),
                vec![data.clone()],
            )?;
            let calls = t.calls.borrow();
            assert_eq!(
                calls
                    .iter()
                    .filter(|(n, _)| n != "construct")
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>(),
                [
                    "handleTickerEvents",
                    "handleMarkPriceEvents",
                    "handleIndexTickerEvents",
                    "handleOpenInterestEvents",
                    "handleFundingRateEvents",
                    "replacement"
                ],
                "CLASS native handler identity and late member lookup"
            );
            for (_, args) in calls.iter().filter(|(n, _)| n != "construct") {
                assert!(
                    Rc::ptr_eq(&args[0], &data),
                    "CLASS native original callback data"
                );
            }
            Ok(object(vec![
                ("sequences", state.order_book_sequences),
                ("recovering", state.recovering_order_books),
                ("fallback", state.direct_fallback),
                ("topics", state.topics),
                ("channels", array(channels.iter().map(|v| s(v)).collect())),
            ]))
        }),
        "order" => {
            let rows = if case == "batch" {
                let copy = object(vec![
                    ("instId", s("I")),
                    ("ordId", s("O")),
                    ("state", s("canceled")),
                    ("tradeId", s("T")),
                    ("fillTime", s("F")),
                    ("uTime", s("U")),
                    ("fillPx", s("12.5x")),
                    ("fillSz", s("2")),
                ]);
                array(vec![row, copy])
            } else {
                array(vec![row])
            };
            port::handle_order_topic(h.clone(), object(vec![("data", rows)]))
        }
        "dedupe" => {
            let first = port::is_duplicate_fill_event(h.clone(), row.clone());
            first.and_then(|first| {
                let entries = map_entries(&t.map);
                assert!(
                    !entries.iter().any(|(k, _)| equal(k, &s("stale"))),
                    "DEDUPE native removes stale entry"
                );
                assert!(
                    entries.iter().any(|(k, _)| equal(k, &s("edge"))),
                    "DEDUPE native retains exact cutoff"
                );
                t.now.set(120010.);
                let second = port::is_duplicate_fill_event(h.clone(), row.clone())?;
                t.now.set(120011.);
                let third = port::is_duplicate_fill_event(h.clone(), row)?;
                assert!(
                    matches!(first.as_ref(), Native::B(false)),
                    "DEDUPE native first fill is new"
                );
                assert!(
                    matches!(second.as_ref(), Native::B(true)),
                    "DEDUPE native cutoff does not refresh TTL"
                );
                assert!(
                    matches!(third.as_ref(), Native::B(false)),
                    "DEDUPE native expires beyond cutoff"
                );
                Ok(array(vec![first, second, third]))
            })
        }
        "position" => port::handle_position_topic(
            h.clone(),
            object(vec![(
                "data",
                array(vec![
                    object(vec![
                        ("instId", s("I")),
                        (
                            "mmr",
                            s(if case == "nonfinite" {
                                "Infinity"
                            } else {
                                "1.235"
                            }),
                        ),
                        ("ccy", s("USDT")),
                        ("markPx", s("12.5")),
                    ]),
                    object(vec![
                        ("instId", s("MISSING")),
                        ("mmr", s("bad")),
                        ("ccy", s("USD")),
                        ("markPx", s("-0")),
                    ]),
                ]),
            )]),
        ),
        "account" => {
            let data = match case {
                "missing" => u(),
                "object" => object(vec![]),
                "empty" => array(vec![]),
                "invalid" => array(vec![
                    object(vec![("totalEq", s("bad"))]),
                    object(vec![("totalEq", n(f64::INFINITY))]),
                ]),
                "overflow" => array(vec![
                    object(vec![("totalEq", s("1e308"))]),
                    object(vec![("totalEq", s("1e308"))]),
                ]),
                "throw-price" => array(vec![object(vec![(
                    "totalEq",
                    Rc::new(Native::Coerce(err.clone())),
                )])]),
                "negative-zero" => array(vec![object(vec![("totalEq", s("-0"))])]),
                _ => array(vec![
                    object(vec![("totalEq", s("10.235usd"))]),
                    object(vec![("totalEq", s("bad"))]),
                    object(vec![("totalEq", n(0.))]),
                    object(vec![("totalEq", Rc::new(Native::Null))]),
                ]),
            };
            port::handle_account_topic(h.clone(), object(vec![("data", data)]))
        }
        "markets" => {
            let count = if case == "empty" {
                0
            } else if case == "boundary" || case == "send-reentry" {
                21
            } else {
                2
            };
            let added = array(
                (0..count)
                    .map(|i| object(vec![("id", s(&format!("M{i}")))]))
                    .collect(),
            );
            port::on_markets_changed(h.clone(), object(vec![("added", added)]))
        }
        _ => panic!("family"),
    };
    let (failed, result) = match result {
        Ok(v) => (false, v),
        Err(e) => {
            assert!(
                Rc::ptr_eq(&e, &t.err),
                "SOCKET native original error identity"
            );
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "boom");
            assert_eq!(e.code, Some("SOCKET"));
            assert_eq!(e.ids, Some([4, 9]));
            assert!(!e.fresh);
            (true, u())
        }
    };
    if !failed {
        let calls = t.calls.borrow();
        if method == "order" && case == "normal" {
            assert_eq!(
                calls.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
                ["mapOrders", "terminal", "dedupe", "base", "emit"],
                "ORDER native terminal before fill effects"
            );
            assert!(
                equal(&calls[1].1[1], &s("closed")),
                "ORDER native terminal status"
            );
            let (_, args) = calls.last().unwrap();
            assert!(equal(&args[0], &s("fill")), "ORDER native event name");
            let fill = &args[1];
            assert_eq!(
                num(&get(fill, "amount")),
                4.,
                "ORDER native converted amount"
            );
            assert_eq!(
                num(&get(fill, "price")),
                12.5,
                "ORDER native parsed fill price"
            );
            assert!(
                Rc::ptr_eq(&get(fill, "id"), &get(&items(&t.mapped_orders)[0], "id")),
                "ORDER native original mapped ID"
            );
        }
        if method == "position" && case != "empty" {
            let balance = get(&t.store, "balance");
            assert_eq!(
                num(&get(&balance, "used")),
                if case == "nonfinite" { 0. } else { 2.47 },
                "POSITION native rounded margin USD"
            );
            assert_eq!(
                num(&get(&balance, "upnl")),
                1.24,
                "POSITION native finite rounded PnL"
            );
            assert!(
                equal(
                    &get(&balance, "tag"),
                    &s(if case == "balance-reentry" {
                        "changed"
                    } else {
                        "retained"
                    })
                ),
                "POSITION native balance reread after position write"
            );
        }
        if method == "account" && case == "normal" {
            let balance = get(&t.store, "balance");
            assert_eq!(
                num(&get(&balance, "free")),
                7.24,
                "ACCOUNT native free after used"
            );
            assert_eq!(
                num(&get(&balance, "total")),
                8.24,
                "ACCOUNT native total after upnl"
            );
            assert!(
                equal(&get(&balance, "tag"), &s("retained")),
                "ACCOUNT native retained balance fields"
            );
        }
        if method == "markets" && (case == "normal" || case == "boundary" || case == "send-reentry")
        {
            let widths = calls
                .iter()
                .filter(|(n, _)| n == "send")
                .map(|(_, args)| items(&get(&args[0], "args")).len())
                .collect::<Vec<_>>();
            assert_eq!(
                widths,
                if case == "normal" {
                    vec![10]
                } else {
                    vec![100, 5]
                },
                "MARKETS native 100-topic batch widths"
            );
            if case == "send-reentry" {
                let (_, args) = calls.iter().rfind(|(name, _)| name == "send").unwrap();
                let sent = items(&get(&args[0], "args"));
                assert!(
                    equal(&get(sent.last().unwrap(), "instId"), &s("changed")),
                    "MARKETS native later frame observes original topic alias"
                );
            }
            let topics = get(&t.receiver, "topics");
            assert_eq!(
                items(&get(&topics, "tickers")).len(),
                if case == "normal" { 3 } else { 22 },
                "MARKETS native retains existing topic"
            );
            assert!(
                equal(
                    &get(&items(&get(&topics, "index-tickers"))[0], "instId"),
                    &s("M0-INDEX")
                ),
                "MARKETS native index topic identity"
            );
        }
    }
    let mut result = json!({"method":method,"case":case,"events":t.events.borrow().clone(),"failed":failed,"result":pack(&result)});
    if method == "dedupe" {
        result["keys"] = pack(&t.map)
    }
    if method == "account" || method == "position" {
        result["balance"] = pack(&get(&t.store, "balance"))
    }
    if method == "markets" {
        result["topics"] = pack(&get(&t.receiver, "topics"))
    }
    result
}
fn check(method: &str, cases: &[&str], expected: usize) {
    assert_eq!(cases.len(), expected, "native independent fixture floor");
    let native = cases.iter().map(|c| fixture(method, c)).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/websocket-account-events-oracle.mjs"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(json!({"cases":native.iter().map(|r|json!({"method":r["method"],"case":r["case"]})).collect::<Vec<_>>()}).to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        expected,
        "source fixed fixture floor"
    );
    for (i, (n, s)) in native.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(n, s, "maintained source {method}/{i}");
    }
}
#[test]
fn private_business_class_native() {
    check("class-private", &["normal", "fail:construct"], 2);
    check("class-business", &["normal", "fail:construct"], 2)
}
#[test]
fn public_class_native() {
    check("class-public", &["normal", "fail:construct"], 2)
}
#[test]
fn order_events_native() {
    check(
        "order",
        &[
            "normal",
            "partial",
            "live",
            "cancel",
            "mmp",
            "unknown",
            "zero",
            "nan",
            "missing",
            "duplicate",
            "unmapped",
            "batch",
            "fail:terminal",
            "fail:orders",
            "fail:base",
            "fail:emit",
            "fail:mapOrders",
        ],
        17,
    )
}
#[test]
fn dedupe_native() {
    check(
        "dedupe",
        &[
            "trade",
            "fallback",
            "zero-trade",
            "string-zero",
            "lone",
            "throw-price",
            "fail:forget",
            "fail:remember",
        ],
        8,
    )
}
#[test]
fn position_events_native() {
    check(
        "position",
        &[
            "normal",
            "empty",
            "nonfinite",
            "balance-reentry",
            "fail:margin",
            "fail:positions",
            "fail:update",
        ],
        7,
    )
}
#[test]
fn account_events_native() {
    check(
        "account",
        &[
            "normal",
            "missing",
            "object",
            "empty",
            "invalid",
            "overflow",
            "invalid-balance",
            "negative-zero",
            "throw-price",
            "fail:update",
        ],
        10,
    )
}
#[test]
fn market_events_native() {
    check(
        "markets",
        &[
            "normal",
            "empty",
            "no-socket",
            "disposed",
            "boundary",
            "send-reentry",
            "fail:index",
            "fail:send",
        ],
        8,
    )
}
