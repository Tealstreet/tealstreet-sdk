use safe_cex_venue_kucoin::{
    order_control::{self as port, OrderControlOps},
    public_streams::StreamOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
use std::{future::Future, pin::Pin};
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
fn promise(result: Result<V, E>) -> V {
    Rc::new(Native::P(result))
}
fn await_ready(v: V) -> Pin<Box<dyn Future<Output = Result<V, E>>>> {
    Box::pin(async move {
        if let Native::P(result) = v.as_ref() {
            result.clone()
        } else {
            Ok(v)
        }
    })
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready fixture"),
    }
}
struct State {
    method: &'static str,
    case: &'static str,
    err: E,
    order: V,
    update: V,
    orders: V,
    market: V,
    positions: V,
    refreshed: V,
    events: RefCell<Vec<Vec<V>>>,
    captures: RefCell<Vec<String>>,
    fresh: Cell<usize>,
    fetches: Cell<usize>,
    requests: Cell<usize>,
    emits: Cell<usize>,
    changed_min: Cell<bool>,
}
fn event(t: &State, name: &str, args: Vec<V>) {
    let mut row = vec![s(name)];
    row.extend(args);
    t.events.borrow_mut().push(row);
}
fn fixture(method: &'static str, case: &'static str) -> Rc<State> {
    let status = if case.starts_with("retry-") {
        n(404.)
    } else if case == "string-status" {
        s("404")
    } else {
        n(500.)
    };
    let err = Rc::new(Error {
        name: "TypeError",
        message: "DOWN".into(),
        fresh: false,
        code: Some("WRITE"),
        ids: Some([8, 6]),
        response: object(vec![
            ("status", status),
            ("data", object(vec![("msg", s("VENUE"))])),
        ]),
    });
    let order = object(vec![
        ("id", s("ROOT__tp")),
        ("symbol", s("SYM")),
        ("side", s("sell")),
        (
            "type",
            s(if case == "algo" || method == "algo" {
                "stop_market"
            } else {
                "limit"
            }),
        ),
        ("price", n(11.)),
        ("amount", n(4.)),
        ("timeInForce", s("IOC")),
        ("reduceOnly", b(false)),
        ("tag", n(29.)),
    ]);
    let update = if case == "no-fields" {
        object(vec![])
    } else if case == "undefined" {
        object(vec![("price", u()), ("amount", u())])
    } else {
        object(vec![("price", n(17.)), ("amount", n(2.))])
    };
    let sibling = object(vec![("id", s("ROOT__sl")), ("symbol", s("OTHER"))]);
    let other = object(vec![("id", s("SECOND")), ("symbol", s("OTHER"))]);
    let orders = if method == "dispatch" || method == "normal" {
        array(vec![order.clone(), other, sibling])
    } else {
        array(if case == "virtual" {
            vec![order.clone(), sibling]
        } else {
            vec![order.clone()]
        })
    };
    Rc::new(State {
        method,
        case,
        err,
        order,
        update,
        orders,
        market: object(vec![
            ("id", s("INST")),
            (
                "limits",
                object(vec![(
                    "leverage",
                    object(vec![("min", n(1.)), ("max", n(9.))]),
                )]),
            ),
        ]),
        positions: array(vec![object(vec![(
            "contracts",
            if case == "string-zero" {
                s("0")
            } else {
                n(if case == "open" { 1. } else { 0. })
            },
        )])]),
        refreshed: array(vec![object(vec![("id", s("REFRESHED"))])]),
        events: RefCell::new(vec![]),
        captures: RefCell::new(vec![]),
        fresh: Cell::new(0),
        fetches: Cell::new(0),
        requests: Cell::new(0),
        emits: Cell::new(0),
        changed_min: Cell::new(false),
    })
}
fn route(name: &str) -> &'static str {
    match name {
        "ENDPOINTS.CANCEL_ALL_ORDERS" | "ENDPOINTS.CANCEL_SYMBOL_ORDERS" => "/api/v3/orders",
        "ENDPOINTS.CANCEL_ALL_STOP_ORDERS" => "/api/v1/stopOrders",
        "ENDPOINTS.CANCEL_ORDERS" => "/api/v1/orders",
        "ENDPOINTS.CANCEL_ALGO_ORDER" => "/api/v1/stopOrders/{orderId}",
        "ENDPOINTS.SET_LEVERAGE" => "/api/v2/changeCrossUserLeverage",
        "ENDPOINTS.SET_MARGIN_MODE" => "/api/v2/position/changeMarginMode",
        "ENDPOINTS.SET_POSITION_MODE" => "/api/v2/position/switchPositionMode",
        _ => panic!("route {name}"),
    }
}
fn ops(t: Rc<State>) -> Rc<OrderControlOps<V, E>> {
    let tc = t.clone();
    let ti = t.clone();
    let tg = t.clone();
    let value = Rc::new(StreamOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            tc.captures.borrow_mut().push(name.into());
            Ok(match name {
                "this.store.marketsBySymbol" => {
                    if tc.case == "missing" {
                        object(vec![])
                    } else {
                        object(vec![("SYM", tc.market.clone())])
                    }
                }
                "this.store.orders" => tc.orders.clone(),
                "MarginType.Cross" => s("cross"),
                "Math.min" if tc.changed_min.get() => {
                    Rc::new(Native::Call("Math.changedMin".into()))
                }
                _ if name.starts_with("ENDPOINTS.") => s(route(name)),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        invoke: Rc::new(move |callee, args| {
            let Native::Call(name) = callee.as_ref() else {
                panic!("callee")
            };
            let t = &ti;
            Ok(match name.as_str() {
                "partition" => {
                    assert!(
                        matches!(args[1].as_ref(),Native::Call(x) if x=="this.isVirtualTPSLOrder")
                    );
                    let (mut yes, mut no) = (vec![], vec![]);
                    for o in items(&args[0]) {
                        if text(&get(&o, "id")).contains("__") {
                            yes.push(o)
                        } else {
                            no.push(o)
                        }
                    }
                    array(vec![array(yes), array(no)])
                }
                "uniqBy" => {
                    assert_eq!(text(&args[1]), "id");
                    let mut ids = vec![];
                    let mut rows = vec![];
                    for o in items(&args[0]) {
                        let id = text(&get(&o, "id"));
                        if !ids.contains(&id) {
                            ids.push(id);
                            rows.push(o)
                        }
                    }
                    array(rows)
                }
                "this.toApiSymbol" => s(&format!("API-{}", text(&args[0]))),
                "this.isAlgoOrder" => b(text(&get(&args[0], "type")) != "limit"),
                "this.nativeAmountForCancelReplace" => {
                    assert!(Rc::ptr_eq(&args[0], &t.order));
                    event(t, "nativeAmount", vec![]);
                    n(40.)
                }
                "this.updateVirtualTpSlOrder" | "this.updateAlgoOrder" => {
                    assert!(Rc::ptr_eq(&get(&args[0], "order"), &t.order));
                    assert!(Rc::ptr_eq(&get(&args[0], "update"), &t.update));
                    event(t, name, args);
                    promise(Ok(array(vec![s("DELEGATE")])))
                }
                "this.cancelNormalOrders" | "this.cancelOrders" => {
                    event(t, name, args);
                    promise(if t.case == "reject" || t.case == "cancel-reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(u())
                    })
                }
                "this.placeOrder" => {
                    event(t, name, args);
                    promise(if t.case == "place-reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(array(vec![s("PLACED")]))
                    })
                }
                "this.getOrFetchMarginType" => {
                    event(t, "margin", args);
                    promise(Ok(s(if t.case == "isolated" {
                        "isolated"
                    } else {
                        "cross"
                    })))
                }
                "this.fetchPositions" => {
                    t.fetches.set(t.fetches.get() + 1);
                    event(t, "fetch", args);
                    promise(
                        if t.case == "refresh-reject"
                            && (t.method != "mode" || t.fetches.get() == 2)
                        {
                            Err(t.err.clone())
                        } else if t.method == "mode" && t.fetches.get() == 1 {
                            Ok(t.positions.clone())
                        } else {
                            Ok(t.refreshed.clone())
                        },
                    )
                }
                "this.store.addOrUpdatePositions" => {
                    assert!(Rc::ptr_eq(&args[0], &t.refreshed));
                    event(t, name, args);
                    u()
                }
                "this.store.update" => {
                    assert!(Rc::ptr_eq(&get(&args[0], "positions"), &t.refreshed));
                    event(t, name, args);
                    u()
                }
                "this.emitAndLogError" => {
                    t.emits.set(t.emits.get() + 1);
                    event(t, "emit", args);
                    if t.case == "emit-reject" && t.emits.get() == 1 {
                        return Err(t.err.clone());
                    }
                    u()
                }
                "this.unlimitedXHR.delete" | "this.xhr.post" => {
                    t.requests.set(t.requests.get() + 1);
                    event(
                        t,
                        if name.ends_with("delete") {
                            "delete"
                        } else {
                            "post"
                        },
                        args,
                    );
                    let retry = t.case.starts_with("retry-");
                    if t.case == "reject"
                        || t.case == "string-status"
                        || retry
                            && (t.requests.get() == 1
                                || t.case == "retry-reject" && t.requests.get() == 2)
                    {
                        promise(Err(t.err.clone()))
                    } else {
                        let bad = t.case == "bad"
                            || t.case == "bad-empty"
                            || t.case == "emit-reject"
                            || t.case == "retry-bad" && t.requests.get() == 2;
                        promise(Ok(object(vec![(
                            "data",
                            object(vec![
                                ("code", s(if bad { "400000" } else { "200000" })),
                                (
                                    "msg",
                                    s(if t.case == "bad-empty" {
                                        ""
                                    } else if bad {
                                        "DENIED"
                                    } else {
                                        ""
                                    }),
                                ),
                            ]),
                        )])))
                    }
                }
                "Math.max" => {
                    if t.case == "callee-reenter" {
                        t.changed_min.set(true);
                        put(&get(&get(&t.market, "limits"), "leverage"), "max", n(7.));
                    }
                    n(num(&args[0]).max(num(&args[1])))
                }
                "Math.min" => n(num(&args[0]).min(num(&args[1]))),
                "Math.changedMin" => n(777.),
                _ if name.starts_with("replace:") => s(&name
                    .strip_prefix("replace:")
                    .unwrap()
                    .replacen(&text(&args[0]), &text(&args[1]), 1)),
                _ => panic!("invoke {name}"),
            })
        }),
        intrinsic: Rc::new(move |name, args| {
            Ok(match name {
                "get-index" => get(&args[0], &text(&args[1])),
                "market-not-found-message" => s(&format!("Market {} not found", text(&args[0]))),
                "cancel-order-message" => s(&format!("Failed to cancel order {}", text(&args[0]))),
                "cancel-symbol-message" => s(&format!(
                    "Failed to cancel orders for symbol {}",
                    text(&args[0])
                )),
                "split-double-underscore-first" => s(text(&args[0]).split("__").next().unwrap()),
                "template-prefix" => s(&format!("{}/", text(&args[0]))),
                "concat-id" => s(&format!("{}{}", text(&args[0]), text(&args[1]))),
                "capture-replace" => Rc::new(Native::Call(format!("replace:{}", text(&args[0])))),
                "spread-arrays" => {
                    array(items(&args[0]).into_iter().chain(items(&args[1])).collect())
                }
                "spread-object" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("spread")
                    };
                    Rc::new(Native::O(RefCell::new(fields.borrow().clone())))
                }
                "define-field" => {
                    put(&args[0], &text(&args[1]), args[2].clone());
                    u()
                }
                "greater-than-one" => b(num(&args[0]) > 1.),
                "toString-method" => s(&text(&args[0])),
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
                    .map(|x| cb(x))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }),
        filter: Rc::new(|v, cb| {
            let mut rows = vec![];
            for x in items(&v) {
                if cb(x.clone())? {
                    rows.push(x)
                }
            }
            Ok(array(rows))
        }),
        for_of: Rc::new(|_, _| unused()),
        await_value: Rc::new(await_ready),
        spawn: Rc::new(|_| unused()),
        error_value: Rc::new(|e| Rc::new(Native::Err(e.clone()))),
    });
    Rc::new(OrderControlOps {
        value,
        pair: Rc::new(|v| {
            let rows = items(&v);
            Ok((rows[0].clone(), rows[1].clone()))
        }),
        close_throw: Rc::new(|_, e| e),
        has: Rc::new(|key, v| {
            let Native::O(x) = v.as_ref() else {
                panic!("has")
            };
            Ok(x.borrow().iter().any(|(k, _)| k == key))
        }),
        boolean: Rc::new(b),
        new_error: Rc::new(move |ctor, msg| {
            assert!(matches!(ctor.as_ref(),Native::Call(x) if x=="Error"));
            tg.fresh.set(tg.fresh.get() + 1);
            event(&tg, "newError", vec![msg.clone()]);
            Ok(Rc::new(Error {
                name: "Error",
                message: if matches!(msg.as_ref(), Native::U) {
                    "".into()
                } else {
                    text(&msg)
                },
                fresh: true,
                code: None,
                ids: None,
                response: u(),
            }))
        }),
        iterator: Rc::new(|v| Ok(Rc::new(Native::Iter(RefCell::new((items(&v), 0)))))),
        next: Rc::new(|v| {
            let Native::Iter(x) = v.as_ref() else {
                panic!("iter")
            };
            let mut x = x.borrow_mut();
            let value = x.0.get(x.1).cloned();
            x.1 += 1;
            Ok(value)
        }),
        some: Rc::new(|v, cb| {
            for x in items(&v) {
                if cb(x)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
    })
}
const FAMILIES: &[(&str, &[&str])] = &[
    ("all", &["ok", "bad", "bad-empty", "reject", "emit-reject"]),
    ("dispatch", &["ok", "reject"]),
    (
        "normal",
        &[
            "ok",
            "bad",
            "reject",
            "retry-ok",
            "retry-bad",
            "retry-reject",
            "string-status",
        ],
    ),
    ("symbol", &["ok", "bad", "reject"]),
    (
        "update",
        &[
            "ok",
            "no-fields",
            "undefined",
            "virtual",
            "algo",
            "missing",
            "cancel-reject",
            "place-reject",
        ],
    ),
    ("algo", &["ok", "no-fields", "missing", "place-reject"]),
    (
        "leverage",
        &[
            "ok",
            "isolated",
            "missing",
            "bad",
            "reject",
            "refresh-reject",
            "callee-reenter",
        ],
    ),
    (
        "margin",
        &["ok", "cross", "no-symbol", "missing", "bad", "reject"],
    ),
    (
        "mode",
        &[
            "ok",
            "oneway",
            "open",
            "string-zero",
            "bad",
            "refresh-reject",
        ],
    ),
];
fn execute(t: Rc<State>, h: Rc<OrderControlOps<V, E>>) -> Result<V, E> {
    let opts = object(vec![
        ("order", t.order.clone()),
        ("update", t.update.clone()),
    ]);
    match t.method {
        "all" => run(port::cancel_all_orders(&h)),
        "dispatch" => run(port::cancel_orders(&h, t.orders.clone())),
        "normal" => run(port::cancel_normal_orders(h, t.orders.clone())),
        "symbol" => run(port::cancel_symbol_orders(&h, s("SYM"))),
        "update" => run(port::update_order(h, opts)),
        "algo" => run(port::update_algo_order(&h, opts)),
        "leverage" => run(port::set_leverage(&h, s("SYM"), n(50.))),
        "margin" => run(port::change_margin_type(
            &h,
            b(t.case != "cross"),
            if t.case == "no-symbol" { u() } else { s("SYM") },
        )),
        "mode" => run(port::change_position_mode(h, b(t.case != "oneway"))),
        _ => panic!("method"),
    }
}
fn expected_events(m: &str, c: &str) -> Vec<&'static str> {
    match m {
        "all" => match c {
            "ok" => vec!["delete", "delete"],
            "bad" | "bad-empty" => vec!["delete", "emit", "delete", "emit"],
            "reject" => vec!["delete", "emit"],
            "emit-reject" => vec!["delete", "emit", "emit"],
            _ => unreachable!(),
        },
        "dispatch" => vec!["this.cancelNormalOrders"],
        "normal" => match c {
            "ok" => vec!["delete", "delete"],
            "bad" => vec!["delete", "emit", "delete", "emit"],
            "retry-ok" => vec!["delete", "delete", "delete"],
            "retry-bad" | "retry-reject" => vec!["delete", "delete", "emit"],
            _ => vec!["delete", "emit"],
        },
        "symbol" => {
            if c == "ok" {
                vec!["delete"]
            } else {
                vec!["delete", "emit"]
            }
        }
        "update" | "algo" => match c {
            "missing" => vec!["newError"],
            "virtual" => vec!["this.updateVirtualTpSlOrder"],
            "algo" => vec!["this.updateAlgoOrder"],
            "cancel-reject" => vec!["this.cancelOrders", "emit"],
            "place-reject" => vec!["this.cancelOrders", "this.placeOrder", "emit"],
            "no-fields" => vec!["this.cancelOrders", "nativeAmount", "this.placeOrder"],
            _ => vec!["this.cancelOrders", "this.placeOrder"],
        },
        "leverage" => match c {
            "missing" => vec!["newError"],
            "isolated" => vec!["margin", "newError", "emit"],
            "bad" => vec!["margin", "post", "newError", "emit"],
            "reject" => vec!["margin", "post", "emit"],
            "refresh-reject" => vec!["margin", "post", "fetch", "emit"],
            _ => vec!["margin", "post", "fetch", "this.store.addOrUpdatePositions"],
        },
        "margin" => match c {
            "no-symbol" => vec!["emit", "newError"],
            "missing" => vec!["newError", "emit"],
            "bad" => vec!["post", "newError", "emit"],
            "reject" => vec!["post", "emit"],
            _ => vec!["post", "fetch", "this.store.addOrUpdatePositions"],
        },
        "mode" => match c {
            "open" | "string-zero" => vec!["fetch", "newError", "emit", "newError", "emit"],
            "bad" => vec!["fetch", "post", "newError", "emit"],
            "refresh-reject" => vec!["fetch", "post", "fetch", "emit"],
            _ => vec!["fetch", "post", "fetch", "this.store.update"],
        },
        _ => unreachable!(),
    }
}
fn assert_native(t: &State, result: &Result<V, E>) {
    let (m, c) = (t.method, t.case);
    let events = t.events.borrow();
    assert_eq!(
        events.iter().map(|r| text(&r[0])).collect::<Vec<_>>(),
        expected_events(m, c),
        "SEQUENCE native {m}/{c}"
    );
    assert_eq!(
        text(&get(&t.order, "id")),
        "ROOT__tp",
        "COPY native original order id"
    );
    let requests = events
        .iter()
        .filter(|r| matches!(text(&r[0]).as_str(), "delete" | "post"))
        .collect::<Vec<_>>();
    let emits = events
        .iter()
        .filter(|r| text(&r[0]) == "emit")
        .collect::<Vec<_>>();
    let fresh = match (m, c) {
        ("mode", "open" | "string-zero") => 2,
        (_, "missing")
        | ("leverage", "isolated" | "bad")
        | ("margin", "no-symbol" | "bad")
        | ("mode", "bad") => 1,
        _ => 0,
    };
    assert_eq!(
        t.fresh.get(),
        fresh,
        "ERROR native independent constructor count {m}/{c}"
    );
    match m {
        "all" => {
            assert_eq!(
                requests.len(),
                if c == "reject" || c == "emit-reject" {
                    1
                } else {
                    2
                },
                "ALL native independent request count"
            );
            assert_eq!(
                text(&requests[0][1]),
                "/api/v3/orders",
                "ALL native ordinary route"
            );
            if requests.len() == 2 {
                assert_eq!(
                    text(&requests[1][1]),
                    "/api/v1/stopOrders",
                    "ALL native stop route"
                )
            }
            assert!(
                requests.iter().all(|r| r.len() == 2),
                "ALL native no params"
            );
        }
        "dispatch" => {
            let rows = items(&events[0][1]);
            assert_eq!(rows.len(), 3, "DISPATCH native independent order count");
            let original = items(&t.orders);
            for (row, index) in rows.iter().zip([1, 0, 2]) {
                assert!(
                    Rc::ptr_eq(row, &original[index]),
                    "DISPATCH native original order identity and grouping"
                );
            }
        }
        "normal" => {
            let expected = match c {
                "ok" | "bad" | "retry-bad" | "retry-reject" => 2,
                "retry-ok" => 3,
                _ => 1,
            };
            assert_eq!(
                requests.len(),
                expected,
                "NORMAL native independent deduplicated request count"
            );
            assert_eq!(
                text(&requests[0][1]),
                "/api/v1/orders/ROOT",
                "NORMAL native stripped parent path"
            );
            assert_eq!(
                text(&get(&get(&requests[0][2], "params"), "symbol")),
                "API-SYM",
                "NORMAL native symbol query"
            );
            if c.starts_with("retry-") {
                assert_eq!(
                    text(&requests[1][1]),
                    "/api/v1/stopOrders/ROOT",
                    "RETRY native stop-book path"
                );
                assert_eq!(requests[1].len(), 2, "RETRY native no params")
            }
            if c == "ok" || c == "bad" || c == "retry-ok" {
                assert_eq!(
                    text(&requests.last().unwrap()[1]),
                    "/api/v1/orders/SECOND",
                    "NORMAL native second distinct parent"
                );
            }
        }
        "symbol" => {
            assert_eq!(requests.len(), 1, "SYMBOL native independent request count");
            assert_eq!(
                text(&requests[0][1]),
                "/api/v3/orders",
                "SYMBOL native route"
            );
            assert_eq!(
                text(&get(&get(&requests[0][2], "params"), "symbol")),
                "API-SYM",
                "SYMBOL native query"
            );
        }
        "update" | "algo" => {
            if let Some(cancel) = events.iter().find(|r| text(&r[0]) == "this.cancelOrders") {
                let rows = items(&cancel[1]);
                assert_eq!(rows.len(), 1, "REPLACE native cancel count");
                assert!(
                    Rc::ptr_eq(&rows[0], &t.order),
                    "REPLACE native original cancel order"
                );
            }
            if let Some(place) = events.iter().find(|r| text(&r[0]) == "this.placeOrder") {
                let payload = &place[1];
                let Native::O(fields) = payload.as_ref() else {
                    panic!("payload")
                };
                assert_eq!(
                    fields
                        .borrow()
                        .iter()
                        .map(|(k, _)| k.as_str())
                        .collect::<Vec<_>>(),
                    if m == "algo" {
                        vec!["symbol", "side", "type", "price", "amount", "reduceOnly"]
                    } else {
                        vec![
                            "symbol",
                            "side",
                            "type",
                            "price",
                            "amount",
                            "timeInForce",
                            "reduceOnly",
                        ]
                    },
                    "REPLACE native payload fields"
                );
                for (k, v) in [
                    ("symbol", "SYM"),
                    ("side", "sell"),
                    ("type", if m == "algo" { "stop_market" } else { "limit" }),
                ] {
                    assert_eq!(text(&get(payload, k)), v, "REPLACE native {k}");
                }
                if c == "undefined" {
                    assert!(
                        matches!(get(payload, "price").as_ref(), Native::U),
                        "REPLACE native present undefined price"
                    );
                    assert!(
                        matches!(get(payload, "amount").as_ref(), Native::U),
                        "REPLACE native present undefined amount"
                    );
                } else {
                    assert!(
                        matches!(get(payload, "price").as_ref(), Native::N(_)),
                        "REPLACE native numeric price type"
                    );
                    assert!(
                        matches!(get(payload, "amount").as_ref(), Native::N(_)),
                        "REPLACE native numeric amount type"
                    );
                    assert_eq!(
                        num(&get(payload, "price")),
                        if c == "no-fields" { 11. } else { 17. },
                        "REPLACE native price"
                    );
                    assert_eq!(
                        num(&get(payload, "amount")),
                        if c == "no-fields" { 40. } else { 2. },
                        "REPLACE native native-unit amount"
                    );
                }
                assert!(
                    matches!(get(payload,"reduceOnly").as_ref(),Native::B(x) if *x==(m=="algo")),
                    "REPLACE native reduceOnly"
                );
                if m == "update" {
                    assert_eq!(
                        text(&get(payload, "timeInForce")),
                        "IOC",
                        "REPLACE native timeInForce"
                    );
                }
            }
        }
        "leverage" | "margin" | "mode" => {
            let expected = if c == "missing"
                || m == "leverage" && c == "isolated"
                || m == "margin" && c == "no-symbol"
                || m == "mode" && (c == "open" || c == "string-zero")
            {
                0
            } else {
                1
            };
            assert_eq!(
                requests.len(),
                expected,
                "RISK native independent post count"
            );
            if let Some(request) = requests.first() {
                let payload = &request[2];
                match m {
                    "leverage" => {
                        assert_eq!(
                            text(&request[1]),
                            "/api/v2/changeCrossUserLeverage",
                            "LEVERAGE native route"
                        );
                        assert!(
                            matches!(get(payload,"leverage").as_ref(),Native::S(v) if v==if c=="callee-reenter"{"7"}else{"9"}),
                            "LEVERAGE native clamped string"
                        );
                        assert_eq!(
                            text(&get(payload, "symbol")),
                            "INST",
                            "LEVERAGE native instrument"
                        );
                    }
                    "margin" => {
                        assert_eq!(
                            text(&request[1]),
                            "/api/v2/position/changeMarginMode",
                            "MARGIN native route"
                        );
                        assert_eq!(
                            text(&get(payload, "symbol")),
                            "INST",
                            "MARGIN native instrument"
                        );
                        assert_eq!(
                            text(&get(payload, "marginMode")),
                            if c == "cross" { "CROSS" } else { "ISOLATED" },
                            "MARGIN native wire mode"
                        );
                    }
                    "mode" => {
                        assert_eq!(
                            text(&request[1]),
                            "/api/v2/position/switchPositionMode",
                            "MODE native route"
                        );
                        assert!(
                            matches!(get(payload,"positionMode").as_ref(),Native::S(v) if v==if c=="oneway"{"0"}else{"1"}),
                            "MODE native string position mode"
                        );
                    }
                    _ => unreachable!(),
                }
            }
            for fetch in events.iter().filter(|r| text(&r[0]) == "fetch") {
                if m == "mode" {
                    assert_eq!(fetch.len(), 1, "REFRESH native account-wide fetch")
                } else {
                    assert_eq!(fetch.len(), 2, "REFRESH native symbol argument count");
                    let rows = items(&fetch[1]);
                    assert_eq!(rows.len(), 1, "REFRESH native symbol count");
                    assert_eq!(text(&rows[0]), "SYM", "REFRESH native symbol value");
                }
            }
        }
        _ => unreachable!(),
    }
    let original_failure = matches!(
        c,
        "reject"
            | "cancel-reject"
            | "place-reject"
            | "refresh-reject"
            | "retry-bad"
            | "retry-reject"
            | "string-status"
    );
    if original_failure && m != "all" {
        let Err(e) = result else {
            panic!("ERROR native expected original rejection {m}/{c}")
        };
        assert!(
            Rc::ptr_eq(e, &t.err),
            "ERROR native original rejection identity"
        );
        assert_eq!(e.name, "TypeError");
        assert_eq!(e.message, "DOWN");
        assert_eq!(e.code, Some("WRITE"));
        assert_eq!(e.ids, Some([8, 6]));
        assert!(!e.fresh);
    } else if fresh > 0 {
        let Err(e) = result else {
            panic!("ERROR native expected fresh rejection {m}/{c}")
        };
        assert_eq!(e.name, "Error");
        assert!(e.fresh);
        assert_eq!(e.code, None);
        assert_eq!(e.ids, None);
        let expected = if c == "missing" {
            "Market SYM not found"
        } else if c == "isolated" {
            "KuCoin Futures only exposes cross-margin leverage changes through the current REST API"
        } else if c == "no-symbol" {
            "Symbol is required"
        } else if c == "open" || c == "string-zero" {
            "Cannot update position mode with open positions"
        } else {
            "DENIED"
        };
        assert_eq!(e.message, expected, "ERROR native fresh message");
    } else {
        let Ok(v) = result else {
            panic!("RESULT native expected success {m}/{c}")
        };
        if m == "update" || m == "algo" {
            let rows = items(v);
            assert_eq!(
                rows.len(),
                1,
                "RESULT native independent replacement id count"
            );
            assert_eq!(
                text(&rows[0]),
                if c == "virtual" || c == "algo" {
                    "DELEGATE"
                } else {
                    "PLACED"
                },
                "RESULT native replacement id"
            );
        } else {
            assert!(matches!(v.as_ref(), Native::U), "RESULT native undefined");
        }
    }
    for (emit_index, row) in emits.into_iter().enumerate() {
        let two = matches!(m, "update" | "leverage" | "mode");
        assert_eq!(
            row.len(),
            if two { 3 } else { 2 },
            "EMIT native argument count {m}/{c}"
        );
        if two {
            let Native::Err(e) = row[1].as_ref() else {
                panic!("EMIT native Error object")
            };
            if original_failure {
                assert!(Rc::ptr_eq(e, &t.err), "EMIT native original error identity");
            } else if let Err(out) = result {
                if m == "mode" && (c == "open" || c == "string-zero") {
                    assert_eq!(
                        Rc::ptr_eq(e, out),
                        emit_index == 1,
                        "MODE native indexed Error identity"
                    );
                    assert!(e.fresh, "MODE native first/second fresh Error");
                    assert_eq!(e.name, "Error", "MODE native emitted Error name");
                    assert_eq!(
                        e.message, "Cannot update position mode with open positions",
                        "MODE native emitted Error message"
                    );
                    assert_eq!(e.code, None, "MODE native emitted Error code");
                    assert_eq!(e.ids, None, "MODE native emitted Error ids");
                } else {
                    assert!(Rc::ptr_eq(e, out), "EMIT native caught error identity");
                }
            }
        }
        let message = row.last().unwrap();
        assert!(
            matches!(message.as_ref(), Native::S(_)),
            "EMIT native message string"
        );
        let expected = if original_failure || c == "emit-reject" && emit_index == 1 {
            "VENUE"
        } else if c == "bad-empty" {
            if emit_index == 1 {
                "Failed to cancel all stop orders"
            } else {
                "Failed to cancel all orders"
            }
        } else if c == "bad" || c == "emit-reject" {
            "DENIED"
        } else if c == "missing" {
            "Market SYM not found"
        } else if c == "isolated" {
            "KuCoin Futures only exposes cross-margin leverage changes through the current REST API"
        } else if c == "no-symbol" {
            "Symbol is required"
        } else {
            "Cannot update position mode with open positions"
        };
        assert_eq!(
            text(message),
            expected,
            "EMIT native diagnostic value {m}/{c}"
        );
    }
}
#[test]
fn order_control_native_before_source_oracle() {
    let mut records = vec![];
    let mut cases = vec![];
    for &(method, family) in FAMILIES {
        for &case in family {
            let t = fixture(method, case);
            let result = execute(t.clone(), ops(t.clone()));
            assert_native(&t, &result);
            records.push((t, result));
            cases.push(json!({"method":method,"case":case}));
        }
    }
    assert_eq!(
        records.len(),
        48,
        "independent native fixture floor; stopped mode/refresh-reject"
    );
    let actual=records.iter().map(|(t,result)|json!({"method":t.method,"case":t.case,"result":match result{Ok(v)=>json!({"ok":pack(v)}),Err(e)=>json!({"error":{"name":e.name,"message":e.message,"fresh":e.fresh,"code":e.code,"ids":e.ids}})},"events":t.events.borrow().iter().map(|row|row.iter().map(pack).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-control-oracle.mjs"
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
    assert_eq!(expected.len(), 48, "independent source fixture floor");
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!(
            a, &e,
            "wire comparison after every native assertion: {}/{}",
            a["method"], a["case"]
        );
    }
}
#[test]
fn cancel_normal_orders_waits_and_closes_only_abrupt_body() {
    let t = fixture("normal", "ok");
    let mut h = ops(t.clone());
    let original = Rc::get_mut(&mut h).unwrap();
    let value = Rc::get_mut(&mut original.value).unwrap();
    let released = Rc::new(Cell::new(0));
    let waits = Rc::new(Cell::new(0));
    let gate = released.clone();
    let counter = waits.clone();
    value.await_value = Rc::new(move |v| {
        counter.set(counter.get() + 1);
        let index = counter.get();
        let gate = gate.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if gate.get() < index {
                Poll::Pending
            } else {
                Poll::Ready(if let Native::P(r) = v.as_ref() {
                    r.clone()
                } else {
                    Ok(v.clone())
                })
            }
        }))
    });
    let mut future = Box::pin(port::cancel_normal_orders(h, t.orders.clone()));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    assert_eq!(
        t.requests.get(),
        1,
        "AWAIT native one request before first settlement"
    );
    released.set(1);
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    assert_eq!(
        t.requests.get(),
        2,
        "AWAIT native second request only after first settlement"
    );
    released.set(2);
    let Poll::Ready(Ok(v)) = future.as_mut().poll(&mut cx) else {
        panic!("AWAIT native completion")
    };
    assert!(matches!(v.as_ref(), Native::U));
    let t = fixture("normal", "reject");
    let mut h = ops(t.clone());
    let closed = Rc::new(Cell::new(0));
    let counter = closed.clone();
    let expected = t.err.clone();
    Rc::get_mut(&mut h).unwrap().close_throw = Rc::new(move |_, e| {
        assert!(Rc::ptr_eq(&e, &expected), "CLOSE native original throw");
        counter.set(counter.get() + 1);
        e
    });
    let Err(e) = execute(t.clone(), h) else {
        panic!("CLOSE native rejection")
    };
    assert!(Rc::ptr_eq(&e, &t.err));
    assert_eq!(closed.get(), 1, "CLOSE native abrupt body closes once");
    let t = fixture("normal", "ok");
    let mut h = ops(t.clone());
    let closed = Rc::new(Cell::new(0));
    let counter = closed.clone();
    let expected = t.err.clone();
    let hmut = Rc::get_mut(&mut h).unwrap();
    hmut.next = Rc::new(move |_| Err(expected.clone()));
    hmut.close_throw = Rc::new(move |_, e| {
        counter.set(counter.get() + 1);
        e
    });
    let Err(e) = execute(t.clone(), h) else {
        panic!("NEXT native rejection")
    };
    assert!(Rc::ptr_eq(&e, &t.err));
    assert_eq!(closed.get(), 0, "CLOSE native next failure does not close");
    assert_eq!(t.requests.get(), 0, "NEXT native no request");
}
