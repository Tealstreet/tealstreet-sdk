use safe_cex_venue_kucoin::{
    class_construction::{self as port, ClassOps},
    public_streams::StreamOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
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
    Err(E),
    Null,
    Regexp(String),
    Function {
        name: String,
        root: String,
        receiver: Option<V>,
        body: Option<Rc<dyn Fn(Vec<V>) -> Result<V, E>>>,
    },
    Class {
        name: String,
        length: usize,
        base: RefCell<V>,
        body: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
        prototype: RefCell<Vec<(String, String, usize, V)>>,
    },
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
        Native::Null => Value::Null,
        Native::Regexp(x) => json!({"regexp":x}),
        Native::Function { name, .. } => json!({"fn":name}),
        Native::Class { name, .. } => json!({"class":name}),
    }
}
fn unused<T>() -> T {
    panic!("unused capability")
}
fn await_ready(v: V) -> Pin<Box<dyn Future<Output = Result<V, E>>>> {
    Box::pin(async move { Ok(v) })
}
impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Error")
            .field("message", &self.message)
            .finish()
    }
}
struct State {
    class: &'static str,
    case: &'static str,
    err: E,
    args: Vec<V>,
    events: RefCell<Vec<Vec<V>>>,
    receiver: RefCell<Option<V>>,
    target: RefCell<Option<V>>,
    api_epoch: Cell<usize>,
    every: V,
    periodic: V,
}
fn events(t: &State, name: &str, args: Vec<V>) {
    let mut row = vec![s(name)];
    row.extend(args);
    t.events.borrow_mut().push(row);
}
fn function(
    name: &str,
    root: &str,
    receiver: Option<V>,
    body: Option<Rc<dyn Fn(Vec<V>) -> Result<V, E>>>,
) -> V {
    Rc::new(Native::Function {
        name: name.into(),
        root: root.into(),
        receiver,
        body,
    })
}
fn fixture(class: &'static str, case: &'static str) -> Rc<State> {
    let opts = object(if case == "null-options" {
        vec![
            ("role", Rc::new(Native::Null)),
            ("shardIndex", Rc::new(Native::Null)),
            ("shardCount", Rc::new(Native::Null)),
        ]
    } else if case == "options" {
        vec![
            ("role", s("snapshot")),
            ("shardIndex", n(2.)),
            ("shardCount", n(4.)),
        ]
    } else {
        vec![("testnet", b(false))]
    });
    let args = if class.ends_with("Exchange") {
        vec![
            s("NAME"),
            opts,
            object(vec![("tag", s("STORE"))]),
            s("EXTRA"),
        ]
    } else {
        vec![object(vec![("tag", s("PARENT"))]), opts, s("EXTRA")]
    };
    Rc::new(State {
        class,
        case,
        err: Rc::new(Error {
            name: "TypeError",
            message: "DOWN".into(),
            fresh: false,
            code: Some("BOOT"),
            ids: Some([3, 7]),
            response: object(vec![("data", object(vec![("msg", s("VENUE"))]))]),
        }),
        args,
        events: RefCell::new(vec![]),
        receiver: RefCell::new(None),
        target: RefCell::new(None),
        api_epoch: Cell::new(1),
        every: array(vec![s("EVERY")]),
        periodic: array(vec![s("PERIODIC")]),
    })
}
fn ops(t: Rc<State>) -> Rc<ClassOps<V, E>> {
    let capture = t.clone();
    let invoke = t.clone();
    let set = t.clone();
    let define = t.clone();
    let base = t.clone();
    let ctor = t.clone();
    let derive = t.clone();
    let value = Rc::new(StreamOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(move |v, k, x| {
            events(&set, "set", vec![s(k), x.clone()]);
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |path| {
            Ok(match path {
                "GRAFANA_ENDPOINTS_EVERY" | "KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY" => {
                    capture.every.clone()
                }
                "GRAFANA_ENDPOINTS_PERIODIC" | "KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC" => {
                    capture.periodic.clone()
                }
                "createAPI" | "createKucoinSpotAPI" => {
                    Rc::new(Native::Call(format!("{path}:{}", capture.api_epoch.get())))
                }
                _ => Rc::new(Native::Call(path.into())),
            })
        }),
        invoke: Rc::new(move |callee, args| {
            if let Native::Function { body: Some(cb), .. } = callee.as_ref() {
                return cb(args);
            }
            let Native::Call(name) = callee.as_ref() else {
                panic!("native callee")
            };
            let t = &invoke;
            if name == "patternClientOrderIdRule" {
                events(t, "rule", args.clone());
                return Ok(object(vec![
                    ("label", args[0].clone()),
                    ("requirement", s("match /^[A-Za-z0-9_-]{1,40}$/")),
                    ("isValid", function("isValid", "rule", None, None)),
                ]));
            }
            if name == "secondary" {
                assert!(
                    Rc::ptr_eq(&args[0], &t.args[1]),
                    "CTOR native original opts"
                );
                events(t, "secondary", args.clone());
                if t.case == "secondary-reject" {
                    return Err(t.err.clone());
                }
                let cb = get(&args[2], "rateLimit");
                let raw = object(vec![("raw", s("XHR"))]);
                let Native::Function { body: Some(cb), .. } = cb.as_ref() else {
                    panic!("CTOR native rate callback")
                };
                let limited = cb(vec![raw.clone()])?;
                if t.case == "reenter" {
                    t.api_epoch.set(2);
                    put(&t.args[1], "testnet", b(true));
                }
                return Ok(limited);
            }
            if name == "rateLimit" {
                events(t, "rate", args.clone());
                if t.case == "rate-reject" {
                    return Err(t.err.clone());
                }
                return Ok(object(vec![
                    ("limited", args[0].clone()),
                    ("maxRPS", get(&args[1], "maxRPS")),
                ]));
            }
            if name.starts_with("createAPI:") || name.starts_with("createKucoinSpotAPI:") {
                assert!(Rc::ptr_eq(&args[0], &t.args[1]));
                assert!(Rc::ptr_eq(&args[1], t.receiver.borrow().as_ref().unwrap()));
                events(t, "api", vec![s(name), args[0].clone()]);
                if t.case == "unlimited-reject" {
                    return Err(t.err.clone());
                }
                return Ok(object(vec![("api", s(name))]));
            }
            panic!("native invoke {name}")
        }),
        intrinsic: Rc::new(|_, _| unused()),
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
    Rc::new(ClassOps {
        value,
        null: Rc::new(|| Rc::new(Native::Null)),
        boolean: Rc::new(b),
        regexp: Rc::new(|x| Ok(Rc::new(Native::Regexp(x.into())))),
        define: Rc::new(move |v, k, x| {
            events(&define, "define", vec![s(k), x.clone()]);
            if define.case == "define-reject" {
                return Err(define.err.clone());
            }
            put(v, k, x);
            Ok(())
        }),
        member: Rc::new(|_, key| {
            assert_eq!(key, "createXhrWithSecondaryKey");
            Ok(Rc::new(Native::Call("secondary".into())))
        }),
        superclass: Rc::new(|class| {
            let Native::Class { base, .. } = class.as_ref() else {
                panic!("active class")
            };
            Ok(base.borrow().clone())
        }),
        super_construct: Rc::new(move |superclass, target, args| {
            let exchange = base.class.ends_with("Exchange");
            let Native::Call(name) = superclass.as_ref() else {
                panic!("super")
            };
            assert_eq!(
                name,
                if base.case == "reparent" {
                    "OtherBase"
                } else if exchange {
                    "BaseExchange"
                } else {
                    "BaseWebSocket"
                }
            );
            assert!(
                Rc::ptr_eq(&target, base.target.borrow().as_ref().unwrap()),
                "CLASS native retained newTarget"
            );
            let target_name = match target.as_ref() {
                Native::Class { name, .. } => name.as_str(),
                Native::S(name) => name.as_str(),
                _ => panic!("target"),
            };
            events(
                &base,
                "super",
                vec![s(name), s(target_name), array(args.clone())],
            );
            if base.case == "super-reject" {
                return Err(base.err.clone());
            }
            let receiver = object(if exchange {
                vec![
                    ("baseKind", s("exchange")),
                    ("name", args[0].clone()),
                    ("options", args[1].clone()),
                    ("store", args[2].clone()),
                ]
            } else {
                vec![
                    ("baseKind", s("socket")),
                    ("parent", args.first().cloned().unwrap_or_else(u)),
                ]
            });
            *base.receiver.borrow_mut() = Some(receiver.clone());
            Ok(receiver)
        }),
        construct: Rc::new(move |callee, args| {
            let Native::Call(name) = callee.as_ref() else {
                panic!("socket constructor")
            };
            assert!(
                Rc::ptr_eq(&args[0], ctor.receiver.borrow().as_ref().unwrap()),
                "CTOR native original socket parent"
            );
            events(&ctor, "new", vec![s(name), array(args.clone())]);
            let public = name.contains("Public");
            if ctor.case
                == if public {
                    "public-reject"
                } else {
                    "private-reject"
                }
            {
                return Err(ctor.err.clone());
            }
            Ok(object(vec![("socket", s(name))]))
        }),
        function: Rc::new(|name, len, body| {
            assert_eq!(name, "rateLimit");
            assert_eq!(len, 1, "CTOR native callback arity");
            function(name, "callback", None, Some(body))
        }),
        native_member: Rc::new(|receiver, _class, key, root| {
            assert!(!root.is_empty());
            Ok(function(key, root, Some(receiver.clone()), None))
        }),
        prototype_member: Rc::new(|class, _, key, kind, len, root| {
            let Native::Class { prototype, .. } = class.as_ref() else {
                panic!("prototype")
            };
            prototype.borrow_mut().push((
                key.into(),
                kind.into(),
                len,
                function(
                    &if kind == "get" {
                        format!("get {key}")
                    } else {
                        key.into()
                    },
                    root,
                    None,
                    None,
                ),
            ));
            Ok(())
        }),
        derive: Rc::new(move |name, superclass, len, body| {
            if derive.case == "prototype-reject" {
                return Err(derive.err.clone());
            }
            Ok(Rc::new(Native::Class {
                name: name.into(),
                length: len,
                base: RefCell::new(superclass),
                body,
                prototype: RefCell::new(vec![]),
            }))
        }),
    })
}
fn class(t: &Rc<State>) -> Result<V, E> {
    let h = ops(t.clone());
    match t.class {
        "KucoinExchange" => port::define_futures_exchange(h),
        "KucoinSpotExchange" => port::define_spot_exchange(h),
        "KucoinPrivateWebsocket" => port::define_futures_private(h),
        "KucoinPublicWebsocket" => port::define_futures_public(h),
        "KucoinSpotPrivateWebsocket" => port::define_spot_private(h),
        "KucoinSpotPublicWebsocket" => port::define_spot_public(h),
        _ => panic!("class"),
    }
}
fn execute(t: &Rc<State>) -> (Option<V>, Result<V, E>) {
    let class = match class(t) {
        Ok(v) => v,
        Err(e) => return (None, Err(e)),
    };
    let Native::Class { body, .. } = class.as_ref() else {
        panic!("class")
    };
    if t.case == "reparent" {
        let Native::Class { base, .. } = class.as_ref() else {
            panic!("class")
        };
        *base.borrow_mut() = Rc::new(Native::Call("OtherBase".into()));
    }
    let target = if t.case == "subclass" {
        s("Derived")
    } else {
        class.clone()
    };
    *t.target.borrow_mut() = Some(target.clone());
    let result = body(class.clone(), target, t.args.clone());
    (Some(class), result)
}

const CLASSES: &[&str] = &[
    "KucoinExchange",
    "KucoinSpotExchange",
    "KucoinPrivateWebsocket",
    "KucoinPublicWebsocket",
    "KucoinSpotPrivateWebsocket",
    "KucoinSpotPublicWebsocket",
];
fn own_keys(class: &str) -> &'static [&'static str] {
    match class {
        "KucoinExchange" => &[
            "clientOrderIdRule",
            "xhr",
            "unlimitedXHR",
            "publicWebsocket",
            "snapshotWebsockets",
            "privateWebsocket",
            "grafanaEndpointsEvery",
            "grafanaEndpointsPeriodic",
            "toKucoinSymbol",
            "toInternalSymbol",
            "toApiSymbol",
            "contractSizeFor",
            "nativeAmountForCancelReplace",
            "parseMarginType",
            "parsePositionMode",
            "parseRowPositionMode",
            "getAccount",
            "validateAccount",
            "dispose",
            "start",
            "tick",
            "fetchBalance",
            "fetchPositionMetadata",
            "fetchPositions",
            "fetchOpenPositions",
            "fetchMarkets",
            "fetchTickers",
            "fetchOrders",
            "reconcileFetchedOrderRemainders",
            "fetchNormalOrders",
            "fetchAlgoOrders",
            "fetchOHLCV",
            "listenOHLCV",
            "listenOrderBook",
            "cancelAllOrders",
            "cancelOrders",
            "cancelNormalOrders",
            "cancelSymbolOrders",
            "placeOrder",
            "formatCreateOrder",
            "formatAlgoOrder",
            "getOrderPlanType",
            "placeOrderBatch",
            "placeAlgoOrders",
            "updateVirtualTpSlOrder",
            "updateOrder",
            "updateAlgoOrder",
            "setLeverage",
            "changeMarginType",
            "changePositionMode",
            "mapOrder",
            "getOrderSideV2",
            "getOrderSide",
            "getAlgoOrderSide",
            "getPositionForOrder",
            "getKucoinMarginMode",
            "getKucoinPositionSide",
            "mapStopPriceType",
            "getKucoinStopDirection",
            "isVirtualTPSLOrder",
            "isAlgoOrder",
            "getOrderPlanTypeAndAmount",
        ],
        "KucoinSpotExchange" => &[
            "clientOrderIdRule",
            "xhr",
            "unlimitedXHR",
            "isSpot",
            "publicWebsocket",
            "privateWebsocket",
            "grafanaEndpointsEvery",
            "grafanaEndpointsPeriodic",
            "dispose",
            "getAccount",
            "validateAccount",
            "start",
            "fetchBalance",
            "getUsdPrice",
            "fetchPositions",
            "fetchSymbolMaxLeverage",
            "setLeverage",
            "changeMarginType",
            "changePositionMode",
            "tick",
            "fetchOrders",
            "fetchActiveOrderSymbols",
            "mapOrder",
            "placeOrder",
            "updateOrder",
            "cancelOrders",
            "cancelSymbolOrders",
            "cancelAllOrders",
            "fetchHistoricalFills",
            "fetchMarkets",
            "fetchTickers",
            "fetchOHLCV",
            "listenOHLCV",
            "listenOrderBook",
        ],
        "KucoinPrivateWebsocket" => &[
            "token",
            "socketEndpoint",
            "subscribed",
            "snapshotRefreshTimer",
            "connectAndSubscribe",
            "onOpen",
            "ping",
            "onMessage",
            "refreshSnapshotSoon",
            "reconnect",
            "dispose",
            "handleOrderTopic",
            "handleAlgoOrdersTopic",
            "subscribe",
        ],
        "KucoinPublicWebsocket" => &[
            "messageHandlers",
            "token",
            "socketEndpoint",
            "role",
            "shardIndex",
            "shardCount",
            "connectAndSubscribe",
            "reconnect",
            "onOpen",
            "onMarketsChanged",
            "ping",
            "subscribe",
            "snapshotShardSymbols",
            "subscribeActiveTicker",
            "unsubscribeActiveTicker",
            "sendChannelOp",
            "onMessage",
            "tickerForApiSymbol",
            "handleTickerV2",
            "handleSnapshot",
            "handleInstrument",
            "listenOHLCV",
            "listenOrderBook",
        ],
        "KucoinSpotPrivateWebsocket" => &[
            "token",
            "socketEndpoint",
            "pingInterval",
            "orderRefreshTimeout",
            "balanceRefreshTimeout",
            "connectAndSubscribe",
            "onOpen",
            "ping",
            "onMessage",
            "handleOrderTopic",
            "subscribe",
            "refreshOrdersSoon",
            "refreshBalanceSoon",
        ],
        "KucoinSpotPublicWebsocket" => &[
            "token",
            "socketEndpoint",
            "pingInterval",
            "connectAndSubscribe",
            "onOpen",
            "ping",
            "subscribe",
            "subscribeTopic",
            "unsubscribeTopic",
            "onMessage",
            "handleTickerSnapshot",
            "listenOHLCV",
            "listenOrderBook",
            "getCandleHandlerTopic",
        ],
        _ => panic!("class keys"),
    }
}
fn value_count(class: &str) -> usize {
    match class {
        "KucoinExchange" => 8,
        "KucoinSpotExchange" => 8,
        "KucoinPrivateWebsocket" => 4,
        "KucoinPublicWebsocket" => 6,
        "KucoinSpotPrivateWebsocket" => 5,
        "KucoinSpotPublicWebsocket" => 3,
        _ => panic!("class count"),
    }
}
fn own_count(class: &str) -> usize {
    match class {
        "KucoinExchange" => 62,
        "KucoinSpotExchange" => 34,
        "KucoinPrivateWebsocket" => 14,
        "KucoinPublicWebsocket" => 23,
        "KucoinSpotPrivateWebsocket" => 13,
        "KucoinSpotPublicWebsocket" => 14,
        _ => panic!("class count"),
    }
}
fn rows(t: &State, name: &str) -> Vec<Vec<V>> {
    t.events
        .borrow()
        .iter()
        .filter(|r| text(&r[0]) == name)
        .cloned()
        .collect()
}
fn string(v: &V, want: &str, label: &str) {
    let Native::S(x) = v.as_ref() else {
        panic!("{label}: native string")
    };
    assert_eq!(x, want, "{label}");
}
fn number(v: &V, want: f64, label: &str) {
    let Native::N(x) = v.as_ref() else {
        panic!("{label}: native number")
    };
    assert_eq!(x.to_bits(), want.to_bits(), "{label}");
}
fn native_assert(t: &Rc<State>, class: &Option<V>, result: &Result<V, E>) {
    let bad = t.case.ends_with("reject");
    assert_eq!(result.is_err(), bad, "CLASS native independent rejection");
    if let Err(e) = result {
        assert!(
            Rc::ptr_eq(e, &t.err),
            "CLASS native original error identity"
        );
        assert_eq!(e.name, "TypeError");
        assert_eq!(e.message, "DOWN");
        assert_eq!(e.code, Some("BOOT"));
        assert_eq!(e.ids, Some([3, 7]));
        assert!(Rc::ptr_eq(&e.response, &t.err.response));
    }
    if let Some(class) = class {
        let Native::Class {
            name,
            length,
            base,
            prototype,
            ..
        } = class.as_ref()
        else {
            panic!("CLASS native class")
        };
        assert_eq!(name, t.class);
        assert_eq!(
            *length,
            if t.class.ends_with("Exchange") {
                3
            } else if t.class == "KucoinPublicWebsocket" {
                2
            } else {
                0
            },
            "CLASS native constructor arity"
        );
        let base = base.borrow();
        let Native::Call(base) = base.as_ref() else {
            panic!("base")
        };
        assert_eq!(
            base,
            if t.case == "reparent" {
                "OtherBase"
            } else if t.class.ends_with("Exchange") {
                "BaseExchange"
            } else {
                "BaseWebSocket"
            }
        );
        let prototypes = prototype.borrow();
        let want: &[(&str, &str, usize)] = match t.class {
            "KucoinExchange" => &[
                ("apiMarginCoin", "get", 0),
                ("getOrFetchPositionMode", "method", 1),
                ("getOrFetchMarginType", "method", 1),
            ],
            "KucoinSpotPrivateWebsocket" => &[("dispose", "method", 0)],
            _ => &[],
        };
        assert_eq!(prototypes.len(), want.len(), "CLASS native prototype count");
        for ((key, kind, len, body), (name, kind_want, len_want)) in prototypes.iter().zip(want) {
            assert_eq!(key, name);
            assert_eq!(kind, kind_want);
            assert_eq!(len, len_want);
            let Native::Function { receiver, root, .. } = body.as_ref() else {
                panic!("prototype function")
            };
            assert!(
                receiver.is_none(),
                "CLASS native dynamic prototype receiver"
            );
            assert!(!root.is_empty());
        }
    } else {
        assert_eq!(t.case, "prototype-reject");
    }
    let super_rows = rows(t, "super");
    assert_eq!(
        super_rows.len(),
        usize::from(t.case != "prototype-reject"),
        "CLASS native super count"
    );
    if let Some(row) = super_rows.first() {
        let args = items(&row[3]);
        let explicit = t.class.ends_with("Exchange") || t.class == "KucoinPublicWebsocket";
        assert_eq!(
            args.len(),
            if t.class.ends_with("Exchange") {
                3
            } else if explicit {
                1
            } else {
                3
            },
            "CLASS native argument forwarding count"
        );
        for (i, arg) in args.iter().enumerate() {
            assert!(
                Rc::ptr_eq(arg, &t.args[i]),
                "CLASS native original super arguments"
            );
        }
    }
    let expected_define = if matches!(t.case, "prototype-reject" | "super-reject") {
        0
    } else if t.case == "define-reject" {
        1
    } else {
        own_count(t.class)
    };
    let definitions = rows(t, "define");
    assert_eq!(
        definitions.len(),
        expected_define,
        "CLASS native own definition count"
    );
    for (i, row) in definitions.iter().enumerate() {
        string(
            &row[1],
            own_keys(t.class)[i],
            "CLASS native own definition order",
        );
        let key = own_keys(t.class)[i];
        let undefined = (t.class.ends_with("Exchange")
            && ["xhr", "unlimitedXHR", "publicWebsocket", "privateWebsocket"].contains(&key))
            || (t.class == "KucoinPublicWebsocket"
                && ["role", "shardIndex", "shardCount"].contains(&key))
            || [
                "snapshotRefreshTimer",
                "orderRefreshTimeout",
                "balanceRefreshTimeout",
            ]
            .contains(&key);
        if undefined {
            assert!(
                matches!(row[2].as_ref(), Native::U),
                "CLASS native initial undefined field"
            );
        }
    }
    if let Some(receiver) = t.receiver.borrow().as_ref() {
        if t.class.ends_with("Exchange") {
            assert!(Rc::ptr_eq(&get(receiver, "options"), &t.args[1]));
            assert!(Rc::ptr_eq(&get(receiver, "store"), &t.args[2]));
        } else {
            assert!(Rc::ptr_eq(&get(receiver, "parent"), &t.args[0]));
        }
        if expected_define == own_count(t.class) {
            for key in &own_keys(t.class)[value_count(t.class)..] {
                let member = get(receiver, key);
                let Native::Function {
                    name,
                    receiver: Some(bound),
                    root,
                    ..
                } = member.as_ref()
                else {
                    panic!("CLASS native bound own arrow {key}")
                };
                assert_eq!(name, key);
                assert!(Rc::ptr_eq(bound, receiver), "CLASS native lexical receiver");
                assert!(!root.is_empty());
            }
            if t.class.ends_with("Exchange") {
                let rule = get(receiver, "clientOrderIdRule");
                string(
                    &get(&rule, "label"),
                    if t.class == "KucoinExchange" {
                        "KuCoin"
                    } else {
                        "KuCoin Spot"
                    },
                    "CLASS native rule label",
                );
                string(
                    &get(&rule, "requirement"),
                    "match /^[A-Za-z0-9_-]{1,40}$/",
                    "CLASS native rule requirement",
                );
                assert!(Rc::ptr_eq(
                    &get(receiver, "grafanaEndpointsEvery"),
                    &t.every
                ));
                assert!(Rc::ptr_eq(
                    &get(receiver, "grafanaEndpointsPeriodic"),
                    &t.periodic
                ));
                if t.class == "KucoinExchange" {
                    assert_eq!(
                        items(&get(receiver, "snapshotWebsockets")).len(),
                        0,
                        "CLASS native empty snapshots"
                    );
                } else {
                    assert!(
                        matches!(get(receiver, "isSpot").as_ref(), Native::B(true)),
                        "CLASS native Spot flag"
                    );
                }
            } else {
                for key in ["token", "socketEndpoint"] {
                    assert!(
                        matches!(get(receiver, key).as_ref(), Native::Null),
                        "CLASS native null socket state"
                    );
                }
                if t.class == "KucoinPrivateWebsocket" {
                    assert!(
                        matches!(get(receiver, "subscribed").as_ref(), Native::B(false)),
                        "CLASS native subscription flag"
                    );
                    assert!(matches!(
                        get(receiver, "snapshotRefreshTimer").as_ref(),
                        Native::U
                    ));
                } else if t.class.starts_with("KucoinSpot") {
                    number(
                        &get(receiver, "pingInterval"),
                        5000.,
                        "CLASS native Spot ping interval",
                    );
                    if t.class.contains("Private") {
                        for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
                            assert!(matches!(get(receiver, key).as_ref(), Native::U));
                        }
                    }
                } else {
                    let handlers_value = get(receiver, "messageHandlers");
                    let Native::O(handlers) = handlers_value.as_ref() else {
                        panic!("CLASS native handlers object")
                    };
                    assert_eq!(handlers.borrow().len(), 0);
                    for (key, normal, custom) in [
                        ("role", s("detail"), s("snapshot")),
                        ("shardIndex", n(0.), n(2.)),
                        ("shardCount", n(1.), n(4.)),
                    ] {
                        assert!(
                            equal(
                                &get(receiver, key),
                                &if t.case == "options" { custom } else { normal }
                            ),
                            "CLASS native public options"
                        );
                    }
                }
            }
        }
    }
    if t.class.ends_with("Exchange")
        && !matches!(
            t.case,
            "prototype-reject" | "super-reject" | "define-reject"
        )
    {
        assert_eq!(rows(t, "secondary").len(), 1, "CTOR native secondary count");
        let rates = rows(t, "rate");
        let expected = usize::from(t.case != "secondary-reject");
        assert_eq!(rates.len(), expected, "CTOR native rate count");
        if let Some(row) = rates.first() {
            string(&get(&row[1], "raw"), "XHR", "CTOR native original rate xhr");
            number(
                &get(&row[2], "maxRPS"),
                if t.class == "KucoinExchange" { 3. } else { 5. },
                "CTOR native maxRPS",
            );
        }
        assert_eq!(
            rows(t, "api").len(),
            usize::from(!matches!(t.case, "secondary-reject" | "rate-reject")),
            "CTOR native unlimited count"
        );
        let requests = rows(t, "new");
        let expected = if matches!(
            t.case,
            "secondary-reject" | "rate-reject" | "unlimited-reject"
        ) {
            0
        } else if t.case == "public-reject" {
            1
        } else {
            2
        };
        assert_eq!(requests.len(), expected, "CTOR native socket count");
        if let Some(row) = requests.first() {
            string(
                &row[1],
                if t.class == "KucoinExchange" {
                    "KucoinPublicWebsocket"
                } else {
                    "KucoinSpotPublicWebsocket"
                },
                "CTOR native public constructor",
            );
            let args = items(&row[2]);
            assert_eq!(args.len(), if t.class == "KucoinExchange" { 2 } else { 1 });
            if args.len() == 2 {
                string(&get(&args[1], "role"), "detail", "CTOR native detail role");
            }
        }
        if requests.len() == 2 {
            string(
                &requests[1][1],
                if t.class == "KucoinExchange" {
                    "KucoinPrivateWebsocket"
                } else {
                    "KucoinSpotPrivateWebsocket"
                },
                "CTOR native private constructor",
            );
            assert_eq!(items(&requests[1][2]).len(), 1);
        }
    }
    if let Ok(receiver) = result {
        assert!(
            Rc::ptr_eq(receiver, t.receiver.borrow().as_ref().unwrap()),
            "CLASS native original constructed receiver"
        );
        if t.class.ends_with("Exchange") {
            number(
                &get(&get(receiver, "xhr"), "maxRPS"),
                if t.class == "KucoinExchange" { 3. } else { 5. },
                "CTOR native stored limited xhr",
            );
            string(
                &get(&get(receiver, "unlimitedXHR"), "api"),
                &format!(
                    "{}:{}",
                    if t.class == "KucoinExchange" {
                        "createAPI"
                    } else {
                        "createKucoinSpotAPI"
                    },
                    if t.case == "reenter" { 2 } else { 1 }
                ),
                "CTOR native live unlimited callee",
            );
            for (key, want) in [
                (
                    "publicWebsocket",
                    if t.class == "KucoinExchange" {
                        "KucoinPublicWebsocket"
                    } else {
                        "KucoinSpotPublicWebsocket"
                    },
                ),
                (
                    "privateWebsocket",
                    if t.class == "KucoinExchange" {
                        "KucoinPrivateWebsocket"
                    } else {
                        "KucoinSpotPrivateWebsocket"
                    },
                ),
            ] {
                string(
                    &get(&get(receiver, key), "socket"),
                    want,
                    "CTOR native stored socket",
                );
            }
        }
    }
    let mut sequence: Vec<&str> = vec![];
    if t.case != "prototype-reject" {
        sequence.push("super");
        if t.case != "super-reject" {
            if t.class.ends_with("Exchange") {
                sequence.push("rule");
            }
            sequence.extend(vec!["define"; expected_define]);
            if t.case != "define-reject" {
                if t.class.ends_with("Exchange") {
                    sequence.push("secondary");
                    if t.case != "secondary-reject" {
                        sequence.push("rate");
                        if t.case != "rate-reject" {
                            sequence.extend(["set", "api"]);
                            if t.case != "unlimited-reject" {
                                sequence.extend(["set", "new"]);
                                if t.case != "public-reject" {
                                    sequence.extend(["set", "new"]);
                                    if t.case != "private-reject" {
                                        sequence.push("set");
                                    }
                                }
                            }
                        }
                    }
                } else if t.class == "KucoinPublicWebsocket" {
                    sequence.extend(["set", "set", "set"]);
                }
            }
        }
    }
    assert_eq!(
        t.events
            .borrow()
            .iter()
            .map(|row| text(&row[0]))
            .collect::<Vec<_>>(),
        sequence,
        "CLASS native independent effect sequence"
    );
    if t.case == "normal" {
        let class = class.as_ref().unwrap();
        let Native::Class { body, .. } = class.as_ref() else {
            panic!("class")
        };
        let first = result.as_ref().unwrap();
        let len = t.events.borrow().len();
        let second = body(class.clone(), class.clone(), t.args.clone()).unwrap();
        assert!(!Rc::ptr_eq(first, &second), "CLASS native fresh instances");
        for key in &own_keys(t.class)[value_count(t.class)..] {
            assert!(
                !Rc::ptr_eq(&get(first, key), &get(&second, key)),
                "CLASS native fresh arrows"
            );
        }
        for key in ["snapshotWebsockets", "messageHandlers"] {
            if own_keys(t.class).contains(&key) {
                assert!(
                    !Rc::ptr_eq(&get(first, key), &get(&second, key)),
                    "CLASS native fresh containers"
                );
            }
        }
        t.events.borrow_mut().truncate(len);
        *t.receiver.borrow_mut() = Some(first.clone());
    }
}
fn class_snapshot(class: &V) -> Value {
    let Native::Class {
        name,
        length,
        base,
        prototype,
        ..
    } = class.as_ref()
    else {
        panic!("class snapshot")
    };
    let base = base.borrow();
    let Native::Call(base) = base.as_ref() else {
        panic!("base")
    };
    json!({"name":name,"length":length,"base":base,"prototype":prototype.borrow().iter().map(|(name,kind,length,body)|json!({"name":name,"kind":kind,"length":length,"function":pack(body)})).collect::<Vec<_>>()})
}
#[test]
fn class_construction_native_before_source_oracle() {
    let mut runs = vec![];
    let mut cases = vec![];
    for class in CLASSES {
        let mut family = vec![
            "normal",
            "super-reject",
            "define-reject",
            "prototype-reject",
            "reparent",
        ];
        if class.ends_with("Exchange") {
            family.extend([
                "secondary-reject",
                "rate-reject",
                "unlimited-reject",
                "public-reject",
                "private-reject",
                "subclass",
                "reenter",
            ]);
        }
        if *class == "KucoinPublicWebsocket" {
            family.extend(["null-options", "options", "subclass"]);
        }
        for case in family {
            let t = fixture(class, case);
            let (c, result) = execute(&t);
            native_assert(&t, &c, &result);
            cases.push(json!({"class":class,"case":case}));
            runs.push((t, c, result));
        }
    }
    assert_eq!(cases.len(), 47, "CLASS independent fixture floor");
    let native=runs.into_iter().map(|(t,c,result)|json!({"class":t.class,"case":t.case,"definition":c.as_ref().map(class_snapshot),"result":match result{Ok(v)=>json!({"ok":pack(&v)}),Err(e)=>json!({"error":{"name":e.name,"message":e.message,"code":e.code,"ids":e.ids}})},"events":t.events.borrow().iter().map(|row|row.iter().map(pack).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/class-construction-oracle.mjs"),
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
    assert_eq!(
        source,
        json!(native),
        "CLASS actual-source construction floor"
    );
}
