use safe_cex_venue_kucoin::{
    order_control::OrderControlOps,
    order_lifecycle::{self as port, OrderLifecycleOps},
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
    Member(V, String),
    Mapper(Rc<dyn Fn(V) -> Result<V, E>>),
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

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Error")
            .field("message", &self.message)
            .finish()
    }
}
struct State {
    method: &'static str,
    case: &'static str,
    err: E,
    opts: V,
    parent: V,
    orders: V,
    related: V,
    payloads: V,
    raw: V,
    events: RefCell<Vec<Vec<V>>>,
    posts: Cell<usize>,
    closes: Cell<usize>,
    mapped: RefCell<Option<V>>,
}
fn event(t: &State, name: &str, args: Vec<V>) {
    let mut row = vec![s(name)];
    row.extend(args);
    t.events.borrow_mut().push(row);
}
fn route(key: &str) -> &'static str {
    match key {
        "ENDPOINTS.PLACE_ORDER" => "/api/v1/orders",
        "ENDPOINTS.PLACE_ALGO_ORDER" => "/api/v1/st-orders",
        _ => panic!("route"),
    }
}
fn fixture(method: &'static str, case: &'static str) -> Rc<State> {
    let err = Rc::new(Error {
        name: "TypeError",
        message: "DOWN".into(),
        fresh: false,
        code: Some("WRITE"),
        ids: Some([3, 7]),
        response: object(vec![("data", object(vec![("msg", s("VENUE"))]))]),
    });
    let parent = object(vec![
        ("id", s("P")),
        ("symbol", s("SYM")),
        ("side", s("sell")),
        ("type", s("limit")),
        ("price", n(10.)),
        ("amount", n(5.)),
        ("timeInForce", s("GTC")),
        ("reduceOnly", b(true)),
    ]);
    let sl = object(vec![("id", s("P__stop_loss")), ("price", n(8.))]);
    let tp = object(vec![("id", s("P__take_profit")), ("price", n(12.))]);
    let order = if case == "sl" {
        sl.clone()
    } else if case == "tp" {
        tp.clone()
    } else {
        parent.clone()
    };
    put(&order, "symbol", s("SYM"));
    let opts = if method == "replace" {
        object(vec![
            ("order", order),
            (
                "update",
                object(if case == "undefined" {
                    vec![("price", u()), ("amount", u())]
                } else {
                    vec![("price", n(11.)), ("amount", n(6.))]
                }),
            ),
        ])
    } else {
        object(vec![
            ("symbol", s("SYM")),
            ("stopLoss", if case == "attached" { n(8.) } else { u() }),
        ])
    };
    let payloads = array(vec![
        object(vec![("tag", s("A")), ("stop", s("up"))]),
        object(vec![("tag", s("B"))]),
    ]);
    let orders = array(if case == "empty-mapped" {
        vec![]
    } else {
        vec![parent.clone(), sl.clone(), tp.clone()]
    });
    let raw = object(vec![
        (
            "orderId",
            if case == "missing-data" {
                u()
            } else {
                s("RAW")
            },
        ),
        ("symbol", s("SYM")),
        (
            "type",
            s(match case {
                "cancel" if method == "spot" => "canceled",
                "cancel" | "cancelled" | "filled" | "update" | "open" => case,
                "done" | "done-remaining" => "received",
                _ => "match",
            }),
        ),
        (
            "status",
            s(
                if case == "done" || case == "done-remaining" || case == "cancel" {
                    "done"
                } else if case == "cancelled" {
                    "cancelled"
                } else {
                    "open"
                },
            ),
        ),
        ("matchSize", if case == "filled" { u() } else { s("2") }),
        ("filledSize", s("9")),
        ("matchPrice", s("15")),
        ("price", s("14")),
        (
            "remainSize",
            s(if case == "done-remaining" { "1" } else { "0" }),
        ),
        ("orderTypeV2", s("limit")),
        ("orderSize", s("5")),
        ("ts", s("1234000")),
    ]);
    let related = array(if case == "no-parent" {
        vec![sl, tp]
    } else {
        vec![parent.clone(), sl, tp]
    });
    Rc::new(State {
        method,
        case,
        err,
        opts,
        parent,
        orders,
        related,
        payloads,
        raw,
        events: RefCell::new(vec![]),
        posts: Cell::new(0),
        closes: Cell::new(0),
        mapped: RefCell::new(None),
    })
}
fn ops(t: Rc<State>) -> Rc<OrderLifecycleOps<V, E>> {
    let capture = t.clone();
    let inv = t.clone();
    let close = t.clone();
    let value = Rc::new(StreamOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |path| {
            Ok(match path {
                "this.store.marketsBySymbol" | "this.parent.store.marketsBySymbol" => {
                    object(if capture.case == "missing-market" {
                        vec![]
                    } else {
                        vec![(
                            "SYM",
                            object(vec![(
                                "contractSize",
                                if capture.case == "zero-contract" {
                                    n(0.)
                                } else {
                                    n(0.1)
                                },
                            )]),
                        )]
                    })
                }
                "OrderStatus.Canceled" => s("canceled"),
                "OrderStatus.Closed" => s("closed"),
                _ if path.starts_with("ENDPOINTS.") => s(route(path)),
                _ => Rc::new(Native::Call(path.into())),
            })
        }),
        invoke: Rc::new(move |callee, args| {
            if let Native::Member(target, key) = callee.as_ref() {
                return Ok(match key.as_str() {
                    "includes" => b(text(target).contains(&text(&args[0]))),
                    "endsWith" => b(text(target).ends_with(&text(&args[0]))),
                    "find" => {
                        let Native::Mapper(cb) = args[0].as_ref() else {
                            panic!("find")
                        };
                        let mut result = u();
                        for row in items(target) {
                            if truth(&cb(row.clone())?) {
                                result = row;
                                break;
                            }
                        }
                        result
                    }
                    "forEach" => {
                        let Native::Mapper(cb) = args[0].as_ref() else {
                            panic!("foreach")
                        };
                        for row in items(target) {
                            let returned = cb(row)?;
                            assert!(
                                matches!(returned.as_ref(),Native::S(x)if x=="STORE"),
                                "HANDLER native original callback return"
                            );
                        }
                        u()
                    }
                    "push" => {
                        let Native::A(rows) = target.as_ref() else {
                            panic!("push")
                        };
                        rows.borrow_mut().extend(args);
                        n(rows.borrow().len() as f64)
                    }
                    _ => panic!("member {key}"),
                });
            }
            let Native::Call(name) = callee.as_ref() else {
                panic!("callee")
            };
            let t = &inv;
            Ok(match name.as_str() {
                "parseFloat" => n(safe_cex_num::parse_float(&text(&args[0]))),
                "this.resolveCallerClientOrderId" => {
                    assert!(Rc::ptr_eq(&args[0], &t.opts));
                    event(t, "resolve", args);
                    u()
                }
                "this.isAlgoOrder" => b(t.case == "algo"),
                "this.formatAlgoOrder" => {
                    assert!(
                        Rc::ptr_eq(&args[0], &t.opts),
                        "PLACE native original algo opts"
                    );
                    event(t, "formatAlgo", args);
                    get(&t.payloads, "0")
                }
                "this.formatCreateOrder" => {
                    if t.method == "place" {
                        assert!(
                            Rc::ptr_eq(&args[0], &t.opts),
                            "PLACE native original format opts"
                        );
                    }
                    event(t, "format", args);
                    promise(if t.case == "format-reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(t.payloads.clone())
                    })
                }
                "this.placeOrderBatch" | "this.placeAlgoOrders" => {
                    event(
                        t,
                        if name == "this.placeOrderBatch" {
                            "batch"
                        } else {
                            "algo"
                        },
                        args,
                    );
                    promise(if t.case == "batch-reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(array(vec![s("NEW")]))
                    })
                }
                "this.cancelOrders" => {
                    event(t, "cancel", args);
                    promise(if t.case == "cancel-reject" {
                        Err(t.err.clone())
                    } else {
                        Ok(u())
                    })
                }
                "this.nativeAmountForCancelReplace" => {
                    assert!(Rc::ptr_eq(&args[0], &t.parent));
                    n(4.)
                }
                "this.emitAndLogError" => {
                    event(t, "logError", args);
                    u()
                }
                "this.emitter.emit" => {
                    event(t, "emit", args);
                    if t.case == "emit-reject" {
                        return Err(t.err.clone());
                    }
                    u()
                }
                "this.unlimitedXHR.post" => {
                    event(t, "post", args.clone());
                    let i = t.posts.get();
                    t.posts.set(i + 1);
                    if t.case == "reject" || t.case == "late-reject" && i == 1 {
                        return Ok(promise(Err(t.err.clone())));
                    }
                    let body = object(vec![
                        (
                            "code",
                            if t.case == "falsey-code" {
                                s("")
                            } else if t.case == "missing-code" {
                                u()
                            } else {
                                s(if t.case == "bad" || t.case == "late-bad" && i == 1 {
                                    "400000"
                                } else {
                                    "200000"
                                })
                            },
                        ),
                        ("msg", s(if t.case == "bad" { "DENIED" } else { "" })),
                        (
                            "data",
                            object(if t.case == "no-id" {
                                vec![]
                            } else {
                                vec![("orderId", s(if i == 0 { "ID1" } else { "ID2" }))]
                            }),
                        ),
                    ]);
                    promise(Ok(object(vec![("data", body)])))
                }
                "this.parent.mapOrder" => {
                    event(t, "map", args.clone());
                    *t.mapped.borrow_mut() = Some(args[0].clone());
                    if t.case == "map-reject" {
                        return Err(t.err.clone());
                    }
                    if t.method == "spot" {
                        t.parent.clone()
                    } else {
                        t.orders.clone()
                    }
                }
                "this.store.updateOrderStatus" => {
                    event(t, "status", args);
                    s("STORE")
                }
                "this.store.addOrUpdateOrder" => {
                    event(t, "add", args);
                    s("STORE")
                }
                "this.refreshOrdersSoon" => {
                    event(t, "refresh", vec![]);
                    u()
                }
                "normalizeMillis" => {
                    assert_eq!(text(&args[0]), "1234000");
                    n(1_234_000_000.)
                }
                _ => panic!("invoke {name}"),
            })
        }),
        intrinsic: Rc::new(|name, args| {
            Ok(match name {
                "get-index" => get(&args[0], &text(&args[1])),
                "multiply" => n(num(&args[0]) * num(&args[1])),
                "greater-zero" => b(num(&args[0]) > 0.),
                "rejected-message" => {
                    s(&format!("KuCoin order rejected (code {})", text(&args[0])))
                }
                "market-not-found-message" => s(&format!("Market {} not found", text(&args[0]))),
                "parent-not-found-message" => {
                    s(&format!("Parent order not found for {}", text(&args[0])))
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
                "define-field" => {
                    put(&args[0], &text(&args[1]), args[2].clone());
                    u()
                }
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
    let base = Rc::new(OrderControlOps {
        value,
        pair: Rc::new(|_| unused()),
        close_throw: Rc::new(move |_, e| {
            close.closes.set(close.closes.get() + 1);
            e
        }),
        has: Rc::new(|key, v| {
            let Native::O(fields) = v.as_ref() else {
                panic!("has")
            };
            Ok(fields.borrow().iter().any(|(k, _)| k == key))
        }),
        boolean: Rc::new(b),
        new_error: Rc::new(|ctor, msg| {
            assert!(matches!(ctor.as_ref(),Native::Call(x)if x=="Error"));
            Ok(Rc::new(Error {
                name: "Error",
                message: text(&msg),
                fresh: true,
                code: None,
                ids: None,
                response: u(),
            }))
        }),
        iterator: Rc::new(|v| Ok(Rc::new(Native::Iter(RefCell::new((items(&v), 0)))))),
        next: Rc::new(|v| {
            let Native::Iter(rows) = v.as_ref() else {
                panic!("iter")
            };
            let mut rows = rows.borrow_mut();
            let out = rows.0.get(rows.1).cloned();
            rows.1 += 1;
            Ok(out)
        }),
        some: Rc::new(|_, _| unused()),
    });
    Rc::new(OrderLifecycleOps {
        base,
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        mapper: Rc::new(|cb| Rc::new(Native::Mapper(cb))),
    })
}
const FAMILIES: &[(&str, &[&str])] = &[
    (
        "place",
        &[
            "normal",
            "attached",
            "algo",
            "format-reject",
            "batch-reject",
        ],
    ),
    (
        "batch",
        &[
            "normal",
            "explicit",
            "bad",
            "late-bad",
            "reject",
            "late-reject",
            "no-id",
            "missing-code",
            "falsey-code",
            "empty",
        ],
    ),
    (
        "algo",
        &["normal", "bad", "reject", "late-reject", "no-id", "empty"],
    ),
    (
        "replace",
        &[
            "normal",
            "sl",
            "tp",
            "undefined",
            "no-parent",
            "missing-market",
            "cancel-reject",
            "format-reject",
            "batch-reject",
        ],
    ),
    (
        "futures",
        &[
            "match",
            "filled",
            "cancel",
            "cancelled",
            "done",
            "done-remaining",
            "update",
            "missing-data",
            "empty-mapped",
            "zero-contract",
            "map-reject",
            "emit-reject",
        ],
    ),
    (
        "spot",
        &[
            "match",
            "filled",
            "cancel",
            "cancelled",
            "open",
            "missing-data",
            "map-reject",
            "emit-reject",
        ],
    ),
];
fn execute(t: &Rc<State>) -> Result<V, E> {
    let h = ops(t.clone());
    match t.method {
        "place" => run(port::place_order(&h, t.opts.clone())),
        "batch" => run(port::place_order_batch(
            &h,
            if t.case == "empty" {
                array(vec![])
            } else {
                t.payloads.clone()
            },
            if t.case == "explicit" {
                s("CUSTOM")
            } else {
                u()
            },
        )),
        "algo" => run(port::place_algo_orders(
            &h,
            if t.case == "empty" {
                array(vec![])
            } else {
                t.payloads.clone()
            },
        )),
        "replace" => run(port::update_virtual_tp_sl_order(
            h,
            t.opts.clone(),
            t.related.clone(),
        )),
        "futures" => port::handle_futures_order_topic(
            h,
            object(vec![(
                "data",
                if t.case == "missing-data" {
                    u()
                } else {
                    t.raw.clone()
                },
            )]),
        ),
        "spot" => port::handle_spot_order_topic(&h, object(vec![("data", t.raw.clone())])),
        _ => panic!("method"),
    }
}
fn named(t: &State, name: &str) -> Vec<Vec<V>> {
    t.events
        .borrow()
        .iter()
        .filter(|r| text(&r[0]) == name)
        .cloned()
        .collect()
}
fn number(v: &V, want: f64, label: &str) {
    let Native::N(x) = v.as_ref() else {
        panic!("{label}: native number")
    };
    assert_eq!(x.to_bits(), want.to_bits(), "{label}");
}
fn string(v: &V, want: &str, label: &str) {
    let Native::S(x) = v.as_ref() else {
        panic!("{label}: native string")
    };
    assert_eq!(x, want, "{label}");
}
fn assert_native(t: &State, result: &Result<V, E>) {
    let bad = matches!(
        t.case,
        "bad"
            | "late-bad"
            | "reject"
            | "late-reject"
            | "no-parent"
            | "missing-market"
            | "cancel-reject"
            | "format-reject"
            | "batch-reject"
            | "map-reject"
            | "emit-reject"
    );
    assert_eq!(
        result.is_err(),
        bad,
        "LIFECYCLE native independent rejection"
    );
    if let Err(e) = result {
        if matches!(t.case, "bad" | "late-bad" | "no-parent" | "missing-market") {
            assert!(e.fresh);
            assert_eq!(e.name, "Error");
            assert_eq!(e.code, None);
            assert_eq!(e.ids, None);
            assert!(matches!(e.response.as_ref(), Native::U));
            assert_eq!(
                e.message,
                match t.case {
                    "bad" => "DENIED",
                    "late-bad" => "KuCoin order rejected (code 400000)",
                    "no-parent" => "Parent order not found for P",
                    _ => "Market SYM not found",
                }
            );
        } else {
            assert!(Rc::ptr_eq(e, &t.err), "LIFECYCLE native original error");
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "DOWN");
            assert_eq!(e.code, Some("WRITE"));
            assert_eq!(e.ids, Some([3, 7]));
            assert!(Rc::ptr_eq(&e.response, &t.err.response));
        }
        assert_eq!(
            t.closes.get(),
            usize::from(t.method == "batch" || t.method == "algo"),
            "BATCH native closing on rejection"
        );
    }
    let expected_events: Vec<&str> = match t.method {
        "place" => {
            if t.case == "algo" {
                vec!["resolve", "formatAlgo", "algo"]
            } else if t.case == "format-reject" {
                vec!["resolve", "format"]
            } else {
                vec!["resolve", "format", "batch"]
            }
        }
        "batch" | "algo" => {
            let mut names = vec![
                "post";
                if t.case == "empty" {
                    0
                } else if matches!(t.case, "bad" | "reject") {
                    1
                } else {
                    2
                }
            ];
            if bad {
                names.push("emit");
            }
            names
        }
        "replace" => match t.case {
            "missing-market" => vec![],
            "no-parent" => vec!["logError"],
            "cancel-reject" => vec!["cancel", "logError"],
            "format-reject" => vec!["cancel", "format", "logError"],
            "batch-reject" => vec!["cancel", "format", "batch", "logError"],
            _ => vec!["cancel", "format", "batch"],
        },
        "futures" => {
            if t.case == "missing-data" {
                vec![]
            } else if matches!(t.case, "empty-mapped" | "map-reject") {
                vec!["map"]
            } else {
                let mut names = vec!["map"];
                if t.case != "filled" {
                    names.push("emit");
                }
                if t.case != "emit-reject" && t.case != "done-remaining" {
                    names.extend(vec![
                        if matches!(t.case, "cancel" | "cancelled" | "filled" | "done") {
                            "status"
                        } else {
                            "add"
                        };
                        3
                    ]);
                }
                names
            }
        }
        "spot" => match t.case {
            "missing-data" => vec!["refresh"],
            "map-reject" => vec!["map"],
            "emit-reject" => vec!["map", "emit"],
            "match" => vec!["map", "emit", "add", "refresh"],
            "cancel" | "cancelled" | "filled" => vec!["map", "status", "refresh"],
            _ => vec!["map", "add", "refresh"],
        },
        _ => panic!("native expected family"),
    };
    assert_eq!(
        t.events
            .borrow()
            .iter()
            .map(|row| text(&row[0]))
            .collect::<Vec<_>>(),
        expected_events,
        "LIFECYCLE native independent effect sequence"
    );
    if matches!(t.method, "batch" | "algo") && bad {
        let rows = named(t, "emit");
        assert_eq!(rows.len(), 1, "BATCH native diagnostic count");
        assert_eq!(rows[0].len(), 3, "BATCH native diagnostic arity");
        string(&rows[0][1], "error", "BATCH native diagnostic name");
    }
    for row in named(t, "emit") {
        if text(&row[1]) == "error" {
            string(
                &row[2],
                if t.case == "bad" {
                    "DENIED"
                } else if t.case == "late-bad" {
                    "KuCoin order rejected (code 400000)"
                } else {
                    "VENUE"
                },
                "BATCH native emitted error",
            );
        }
    }
    for row in named(t, "logError") {
        if t.case == "no-parent" {
            string(
                &row[1],
                "Parent order not found for P",
                "REPLACE native missing parent diagnostic",
            );
        } else {
            let Native::Err(e) = row[1].as_ref() else {
                panic!("REPLACE native original error value")
            };
            assert!(Rc::ptr_eq(e, &t.err));
            string(&row[2], "VENUE", "REPLACE native caught message");
        }
    }
    match t.method {
        "batch" | "algo" => {
            let expected = if t.case == "empty" {
                0
            } else if t.case == "bad" || t.case == "reject" {
                1
            } else {
                2
            };
            assert_eq!(
                t.posts.get(),
                expected,
                "BATCH native independent post count"
            );
            for (i, row) in named(t, "post").iter().enumerate() {
                assert!(
                    Rc::ptr_eq(&row[2], &items(&t.payloads)[i]),
                    "BATCH native original payload"
                );
                string(
                    &row[1],
                    if t.case == "explicit" {
                        "CUSTOM"
                    } else if t.method == "algo" && i == 1 {
                        route("ENDPOINTS.PLACE_ALGO_ORDER")
                    } else {
                        route("ENDPOINTS.PLACE_ORDER")
                    },
                    "BATCH native route",
                );
            }
            if let Ok(out) = result {
                let rows = items(out);
                assert_eq!(
                    rows.len(),
                    if t.case == "empty" || t.case == "no-id" {
                        0
                    } else {
                        2
                    },
                    "BATCH native id count"
                );
                if !rows.is_empty() {
                    string(&rows[0], "ID1", "BATCH native first ID");
                    string(&rows[1], "ID2", "BATCH native second ID");
                }
            }
        }
        "place" => {
            assert_eq!(
                named(t, "resolve").len(),
                1,
                "PLACE native caller resolution"
            );
            if !bad {
                assert_eq!(
                    items(result.as_ref().unwrap()).len(),
                    1,
                    "PLACE native independent ID count"
                );
                string(
                    &items(result.as_ref().unwrap())[0],
                    "NEW",
                    "PLACE native ID",
                );
                if t.case == "algo" {
                    assert!(
                        Rc::ptr_eq(&items(&named(t, "algo")[0][1])[0], &items(&t.payloads)[0]),
                        "PLACE native algo payload"
                    );
                } else {
                    let row = &named(t, "batch")[0];
                    assert!(Rc::ptr_eq(&row[1], &t.payloads));
                    string(
                        &row[2],
                        route(if t.case == "attached" {
                            "ENDPOINTS.PLACE_ALGO_ORDER"
                        } else {
                            "ENDPOINTS.PLACE_ORDER"
                        }),
                        "PLACE native attached route",
                    );
                }
            }
        }
        "replace" => {
            assert_eq!(
                named(t, "cancel").len(),
                usize::from(t.case != "no-parent" && t.case != "missing-market"),
                "REPLACE native cancel count"
            );
            if let Some(row) = named(t, "cancel").first() {
                assert!(
                    Rc::ptr_eq(&items(&row[1])[0], &t.parent),
                    "REPLACE native original parent"
                );
            }
            if !bad {
                let ids = items(result.as_ref().unwrap());
                assert_eq!(ids.len(), 1, "REPLACE native independent ID count");
                string(&ids[0], "NEW", "REPLACE native result ID");
                let fields = named(t, "format")[0][1].clone();
                let Native::O(keys) = fields.as_ref() else {
                    panic!("REPLACE native object");
                };
                assert_eq!(
                    keys.borrow()
                        .iter()
                        .map(|(key, _)| key.as_str())
                        .collect::<Vec<_>>(),
                    [
                        "symbol",
                        "side",
                        "type",
                        "price",
                        "amount",
                        "timeInForce",
                        "reduceOnly",
                        "stopLoss",
                        "takeProfit"
                    ],
                    "REPLACE native exact rebuilt keys"
                );
                for (key, want) in [
                    ("symbol", "SYM"),
                    ("side", "sell"),
                    ("type", "limit"),
                    ("timeInForce", "GTC"),
                ] {
                    string(&get(&fields, key), want, "REPLACE native rebuilt string");
                }
                assert!(
                    matches!(get(&fields, "reduceOnly").as_ref(), Native::B(true)),
                    "REPLACE native reduceOnly"
                );

                for (key, value) in [
                    ("price", 11.),
                    ("amount", 6.),
                    ("stopLoss", 8.),
                    ("takeProfit", 12.),
                ] {
                    let expected = match (t.case, key) {
                        ("sl", "price") | ("tp", "price") => 10.,
                        ("sl", "amount") | ("tp", "amount") => 4.,
                        ("sl", "stopLoss") | ("tp", "takeProfit") => 11.,
                        _ => value,
                    };
                    if t.case == "undefined" && (key == "price" || key == "amount") {
                        assert!(
                            matches!(get(&fields, key).as_ref(), Native::U),
                            "REPLACE native present undefined"
                        );
                    } else {
                        number(
                            &get(&fields, key),
                            expected,
                            "REPLACE native rebuilt numeric field",
                        );
                    }
                }
                string(
                    &named(t, "batch")[0][2],
                    route("ENDPOINTS.PLACE_ALGO_ORDER"),
                    "REPLACE native attached endpoint",
                );
            }
        }
        "futures" | "spot" => {
            let fills = named(t, "emit");
            let expect_fill = if t.method == "spot" {
                t.case == "match" || t.case == "emit-reject"
            } else {
                !matches!(
                    t.case,
                    "filled" | "missing-data" | "empty-mapped" | "map-reject"
                )
            };
            assert_eq!(
                fills.len(),
                usize::from(expect_fill),
                "HANDLER native independent fill count"
            );
            if let Some(fill) = fills.first() {
                string(&fill[1], "fill", "HANDLER native fill event");
                string(&get(&fill[2], "id"), "P", "HANDLER native fill ID");
                string(&get(&fill[2], "side"), "sell", "HANDLER native fill side");
                string(
                    &get(&fill[2], "symbol"),
                    "SYM",
                    "HANDLER native fill symbol",
                );
                number(&get(&fill[2], "price"), 15., "HANDLER native fill price");
                number(
                    &get(&fill[2], "amount"),
                    if t.method == "spot" || t.case == "zero-contract" {
                        2.
                    } else {
                        0.2
                    },
                    "HANDLER native base fill",
                );
            }
            if !bad {
                let early = t.case == "missing-data" || t.case == "empty-mapped";
                let status = named(t, "status");
                let terminal = matches!(t.case, "cancel" | "cancelled" | "filled")
                    || t.method == "futures" && t.case == "done";
                assert_eq!(
                    status.len(),
                    if !early && terminal {
                        if t.method == "futures" { 3 } else { 1 }
                    } else {
                        0
                    },
                    "HANDLER native terminal count"
                );
                for (i, row) in status.into_iter().enumerate() {
                    assert!(
                        Rc::ptr_eq(&row[1], &items(&t.orders)[i]),
                        "HANDLER native original status order"
                    );
                    string(
                        &row[2],
                        if t.case == "cancel" || t.case == "cancelled" {
                            "canceled"
                        } else {
                            "closed"
                        },
                        "HANDLER native terminal status",
                    );
                }
                let adds = named(t, "add");
                assert_eq!(
                    adds.len(),
                    if !early && !terminal && t.case != "done-remaining" {
                        if t.method == "futures" { 3 } else { 1 }
                    } else {
                        0
                    },
                    "HANDLER native add count"
                );
                for (i, row) in adds.into_iter().enumerate() {
                    assert!(
                        Rc::ptr_eq(&row[1], &items(&t.orders)[i]),
                        "HANDLER native original added order"
                    );
                }
                assert_eq!(
                    named(t, "refresh").len(),
                    usize::from(t.method == "spot"),
                    "SPOT native refresh count"
                );
                if t.method == "spot" && !early {
                    let mapped = t.mapped.borrow().as_ref().unwrap().clone();
                    assert!(!Rc::ptr_eq(&mapped, &t.raw));
                    string(&get(&mapped, "id"), "RAW", "SPOT native mapped ID");
                    string(&get(&mapped, "type"), "limit", "SPOT native mapped type");
                    number(
                        &get(&mapped, "lastUpdatedAt"),
                        1_234_000_000.,
                        "SPOT native normalized millis",
                    );
                }
            }
        }
        _ => panic!("native method"),
    }
}
#[test]
fn order_lifecycle_native_before_source_oracle() {
    let mut cases = vec![];
    let mut runs = vec![];
    for (method, family) in FAMILIES {
        for case in *family {
            let t = fixture(method, case);
            let result = execute(&t);
            assert_native(&t, &result);
            cases.push(json!({"method":method,"case":case}));
            runs.push((t, result));
        }
    }
    assert_eq!(cases.len(), 50, "LIFECYCLE independent fixture floor");
    let native=runs.into_iter().map(|(t,result)|json!({"method":t.method,"case":t.case,"result":match result{Ok(v)=>json!({"ok":pack(&v)}),Err(e)=>json!({"error":{"name":e.name,"message":e.message,"fresh":e.fresh,"code":e.code,"ids":e.ids}})},"closes":t.closes.get(),"events":t.events.borrow().iter().map(|row|row.iter().map(pack).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/order-lifecycle-oracle.mjs"),
        )
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source, json!(native), "LIFECYCLE actual-source floor");
}
