// Reuse finite language operations without constructing a socket in this target.
#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{
    bootstrap_reads::BootstrapOps,
    futures_public_class::{ConstructionOps, Constructor},
    public_streams::StreamOps,
    spot_bodies::SpotBodyOps,
    spot_exchange_class::{self as port, SpotExchangeOps},
    spot_private_class::{PrivateClassOps, Task},
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};
use values::*;
fn at(v: &V, k: &str) -> V {
    get(v, k).unwrap()
}
fn same(a: &V, b: &V) {
    assert!(
        Rc::ptr_eq(a, b),
        "KUCOIN-EXCHANGE original reference identity"
    );
}
fn sn(v: &V, w: f64) {
    let Js::N(n) = v.as_ref() else {
        panic!("native number kind")
    };
    assert_eq!(
        n.to_bits(),
        w.to_bits(),
        "KUCOIN-EXCHANGE native number bits"
    );
}
fn ss(v: &V, w: &str) {
    let Js::S(s) = v.as_ref() else {
        panic!("native string kind")
    };
    assert_eq!(s, w, "KUCOIN-EXCHANGE native string value");
}
fn keys(v: &V) -> Vec<String> {
    let Js::O(f) = v.as_ref() else {
        panic!("native object")
    };
    f.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn rows(v: &V) -> Vec<V> {
    let Js::A(r) = v.as_ref() else {
        panic!("native array")
    };
    r.borrow().clone()
}
fn basic_frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
    let mut h = Rc::try_unwrap(rt.frame(receiver)).ok().unwrap();
    let prior = h.intrinsic.clone();
    let reads = rt.clone();
    h.intrinsic = Rc::new(move |name, a| match name {
        "destructure-data" => reads.read(&a[0], "data"),
        "subtract" => Ok(n(number(&a[0]) - number(&a[1]))),
        _ => prior(name, a),
    });
    let prior = h.get.clone();
    h.get = Rc::new(move |v, key| {
        if key == "reduce" && matches!(v.as_ref(), Js::A(_)) {
            return Ok(function(|receiver, args| {
                let Js::A(values) = receiver.as_ref() else {
                    panic!("reduce array")
                };
                let length = values.borrow().len();
                let mut acc = args[1].clone();
                for i in 0..length {
                    let value = values.borrow().get(i).cloned();
                    if let Some(value) = value {
                        acc = call(
                            args[0].clone(),
                            u(),
                            vec![acc, value, n(i as f64), receiver.clone()],
                        )?;
                    }
                }
                Ok(acc)
            }));
        }
        prior(v, key)
    });
    Rc::new(h)
}
fn promise(task: Task<V, E>) -> V {
    let v = Rc::new(Js::P(Rc::new(Promise {
        task: RefCell::new(Some(task)),
        result: RefCell::new(None),
    })));
    let _started = poll(&v);
    v
}
fn frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
    let mut h = Rc::try_unwrap(basic_frame(rt, receiver)).ok().unwrap();
    let previous = h.intrinsic.clone();
    h.intrinsic = Rc::new(move |name, a| match name {
        "add" => Ok(n(number(&a[0]) + number(&a[1]))),
        "divide" => Ok(n(number(&a[0]) / number(&a[1]))),
        "lte" => Ok(b(number(&a[0]) <= number(&a[1]))),
        "lt" => Ok(b(number(&a[0]) < number(&a[1]))),
        "gte" => Ok(b(number(&a[0]) >= number(&a[1]))),
        "template-unsupported-spot-order" => Ok(s(&format!(
            "KuCoin Spot does not support {} orders",
            text(&a[0])
        ))),
        "template-missing-market" => Ok(s(&format!("Market {} not found", text(&a[0])))),
        _ => previous(name, a),
    });
    let previous = h.get.clone();
    h.get = Rc::new(move |v, key| {
        if key == "catch" && matches!(v.as_ref(), Js::P(_)) {
            return Ok(function(|receiver, args| {
                let handler = args[0].clone();
                Ok(promise(Box::pin(async move {
                    match wait(receiver).await {
                        Ok(v) => Ok(v),
                        Err(e) => call(handler, u(), vec![obj(vec![("message", s(&e))])]),
                    }
                })))
            }));
        }
        if key == "flat" && matches!(v.as_ref(), Js::A(_)) {
            return Ok(function(|receiver, _| {
                let mut out = vec![];
                for v in rows(&receiver) {
                    out.extend(rows(&v));
                }
                Ok(arr(out))
            }));
        }
        if key == "sort" && matches!(v.as_ref(), Js::A(_)) {
            return Ok(function(|receiver, _| {
                assert!(rows(&receiver).is_empty(), "finite empty candle sort");
                Ok(receiver)
            }));
        }
        previous(v, key)
    });
    Rc::new(h)
}
fn body(
    language: Rc<PrivateClassOps<V, E>>,
    receiver: &V,
    mapped: Rc<RefCell<Vec<V>>>,
) -> Rc<SpotBodyOps<V, E>> {
    let h = (language.frame)(receiver);
    let member = h.clone();
    let mapper = language.clone();
    let async_mapper = language.clone();
    let callback = language.clone();
    let assigns = h.clone();
    let error_ctor = (h.capture)("Error").unwrap();
    let base = BootstrapOps {
        value: h,
        member: Rc::new(move |v, k| {
            let f = (member.get)(&v, k)?;
            Ok(function(move |_, a| call(f.clone(), v.clone(), a)))
        }),
        raw_filter: Rc::new(|_, _| panic!("unexercised raw filter")),
        boolean: Rc::new(b),
        mapper: Rc::new(move |f| {
            let mapped = mapped.clone();
            (mapper.arrow)(
                "",
                1,
                Rc::new(move |args| {
                    let result = f(args.first().cloned().unwrap_or_else(u))?;
                    if matches!(result.as_ref(), Js::P(_)) {
                        mapped.borrow_mut().push(result.clone());
                    }
                    Ok(result)
                }),
            )
        }),
        async_mapper: Rc::new(move |f| {
            (async_mapper.async_arrow)(
                "",
                1,
                Rc::new(move |args| f(args.first().cloned().unwrap_or_else(u))),
            )
        }),
        callback: Rc::new(move |f| (callback.arrow)("", 0, Rc::new(move |_| f()))),
        response_pair: Rc::new(|_| panic!("unexercised pair")),
        interval_parts: Rc::new(|value| {
            ss(&value, "1m");
            Ok((s("1"), s("m")))
        }),
        iterator: Rc::new(|array| Ok(obj(vec![("array", array), ("index", n(0.))]))),
        next: Rc::new(|it| {
            let i = number(&at(it, "index")) as usize;
            set(it, "index", n((i + 1) as f64));
            let array = at(it, "array");
            let Js::A(values) = array.as_ref() else {
                panic!("array iterator")
            };
            let value = values.borrow().get(i).cloned();
            Ok(value)
        }),
        close_throw: Rc::new(|_, e| e),
        some: Rc::new(|_, _| panic!("unexercised some")),
    };
    let comparator = language.clone();
    Rc::new(SpotBodyOps {
        base: Rc::new(base),
        async_arrow: language.async_arrow.clone(),
        triple: Rc::new(|v| {
            let rows = rows(&v);
            Ok((rows[0].clone(), rows[1].clone(), rows[2].clone()))
        }),
        has: Rc::new(|k, v| Ok(keys(v).iter().any(|key| key == k))),
        new_error: Rc::new(move |ctor, msg| {
            same(&ctor, &error_ctor);
            Ok(Rc::new(text(&msg)))
        }),
        comparator: Rc::new(move |f| {
            (comparator.arrow)(
                "",
                2,
                Rc::new(move |args| f(args[0].clone(), args[1].clone())),
            )
        }),
        spread_arguments: Rc::new(|v| Ok(rows(&v))),
        assignment_reference: Rc::new(|v, k| Ok(obj(vec![("target", v), ("key", k)]))),
        assign: Rc::new(move |r, v| (assigns.set)(&at(&r, "target"), &text(&at(&r, "key")), v)),
    })
}
const NAMES: [&str; 26] = [
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
];
const LENGTHS: [usize; 26] = [
    0, 0, 0, 0, 0, 1, 1, 1, 3, 2, 2, 0, 1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1, 2, 2,
];
struct Fixture {
    rt: Rc<Runtime>,
    class: V,
    r: V,
    store: V,
    trace: Rc<RefCell<Vec<String>>>,
    requests: Rc<RefCell<Vec<V>>>,
    construct: Rc<dyn Fn() -> V>,
    inner: Rc<RefCell<Vec<V>>>,
    mapped: Rc<RefCell<Vec<V>>>,
    delegated: Rc<RefCell<Vec<(V, Vec<V>, V)>>>,
    limited: V,
    unlimited: V,
    owners: Rc<RefCell<Vec<String>>>,
}
fn fixture() -> Fixture {
    let rt = Rc::new(Runtime::default());
    let trace = Rc::new(RefCell::new(vec![]));
    let requests = Rc::new(RefCell::new(vec![]));
    let inner = Rc::new(RefCell::new(vec![]));
    let mut language = Rc::try_unwrap(rt.operations()).ok().unwrap();
    let frames = rt.clone();
    language.frame = Rc::new(move |r| frame(&frames, r));
    let metadata = rt.clone();
    let captured = inner.clone();
    let retained = rt.clone();
    language.async_arrow = Rc::new(move |name, length, body| {
        let name = name.to_string();
        let is_inner = name == "fetchSymbolOrders";
        let captured = captured.clone();
        let retained = retained.clone();
        let f = function(move |_, args| {
            let result = promise(body(args));
            retained.jobs.borrow_mut().push(result.clone());
            if is_inner {
                captured.borrow_mut().push(result.clone());
            }
            Ok(result)
        });
        metadata
            .metas
            .borrow_mut()
            .push((f.clone(), name, length, true));
        f
    });
    let language = Rc::new(language);
    let get_requests = requests.clone();
    let get = function(move |_, args| {
        get_requests.borrow_mut().push(arr(args.clone()));
        let route = text(&args[0]);
        let data = match route.as_str() {
            "/api/v2/user-info" => obj(vec![
                ("userId", s("23")),
                ("subUserId", s("29")),
                ("referrerUserId", s("31")),
            ]),
            "/api/v1/hf/orders/active/symbols" => obj(vec![("symbols", arr(vec![]))]),
            "/api/v1/hf/orders/active/page" => {
                obj(vec![("items", arr(vec![])), ("totalPage", s("1"))])
            }
            "/api/v1/market/allTickers" => obj(vec![("ticker", arr(vec![]))]),
            _ => arr(vec![]),
        };
        Ok(obj(vec![(
            "data",
            obj(vec![("code", s("200000")), ("data", data)]),
        )]))
    });
    let del_requests = requests.clone();
    let delete = function(move |_, args| {
        del_requests.borrow_mut().push(arr(args));
        Ok(obj(vec![("data", obj(vec![("code", s("200000"))]))]))
    });
    let owners = Rc::new(RefCell::new(Vec::new()));
    let limited = obj(vec![]);
    let unlimited = obj(vec![]);
    for (label, client) in [("limited", &limited), ("unlimited", &unlimited)] {
        for (method, operation) in [("get", &get), ("delete", &delete)] {
            let expected = client.clone();
            let operation = operation.clone();
            let ownership = owners.clone();
            set(
                client,
                method,
                function(move |actual, args| {
                    same(&actual, &expected);
                    ownership.borrow_mut().push(format!("{label}:{method}"));
                    call(operation.clone(), actual, args)
                }),
            );
        }
    }
    let xhr = limited.clone();
    let calls = trace.clone();
    let factory_xhr = unlimited.clone();
    let api = function(move |_, args| {
        assert_eq!(args.len(), 2);
        same(&at(&args[1], "opts"), &args[0]);
        calls.borrow_mut().push("api".into());
        Ok(factory_xhr.clone())
    });
    for (key, value) in [
        ("createKucoinSpotAPI", api.clone()),
        ("KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY", arr(vec![s("every")])),
        (
            "KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC",
            arr(vec![s("periodic")]),
        ),
    ] {
        rt.globals.borrow_mut().insert(key.into(), value);
    }
    let rate_trace = trace.clone();
    rt.globals.borrow_mut().insert(
        "rateLimit".into(),
        function(move |_, args| {
            sn(&args[0], 71.);
            sn(&at(&args[1], "maxRPS"), 5.);
            rate_trace.borrow_mut().push("rateLimit:5".into());
            Ok(s("limited"))
        }),
    );
    let rule_trace = trace.clone();
    rt.globals.borrow_mut().insert(
        "patternClientOrderIdRule".into(),
        function(move |_, args| {
            ss(&args[0], "KuCoin Spot");
            ss(&args[1], "^[A-Za-z0-9_-]{1,40}$");
            rule_trace.borrow_mut().push("rule".into());
            Ok(obj(vec![("rule", s("spot"))]))
        }),
    );
    let create_trace = trace.clone();
    let create_xhr = xhr.clone();
    let create_rt = rt.clone();
    let create_api = api.clone();
    let create = function(move |receiver, args| {
        same(&at(&receiver, "opts"), &args[0]);
        same(&args[1], &create_api);
        let callback = at(&args[2], "rateLimit");
        let metas = create_rt.metas.borrow();
        let (_, name, length, asynchronous) = metas
            .iter()
            .find(|(v, _, _, _)| Rc::ptr_eq(v, &callback))
            .unwrap();
        assert_eq!(
            (name.as_str(), *length, *asynchronous),
            ("rateLimit", 1, false)
        );
        drop(metas);
        ss(&call(callback, u(), vec![n(71.)])?, "limited");
        create_trace.borrow_mut().push("xhr".into());
        Ok(create_xhr.clone())
    });
    let dispose_trace = trace.clone();
    let dispose = function(move |receiver, _| {
        set(&receiver, "isDisposed", b(true));
        dispose_trace.borrow_mut().push("base.dispose".into());
        Ok(u())
    });
    let emit_trace = trace.clone();
    let emit = function(move |_, args| {
        emit_trace
            .borrow_mut()
            .push(format!("error:{}", text(&args[0])));
        Ok(u())
    });
    let init_trace = trace.clone();
    let prototype = obj(vec![
        ("createXhrWithSecondaryKey", create),
        ("dispose", dispose),
        (
            "resolveCallerClientOrderId",
            function(|_, _| Ok(s("caller"))),
        ),
        ("emitAndLogError", emit),
        (
            "initExternalPublicData",
            function(move |_, _| {
                init_trace.borrow_mut().push("external".into());
                Ok(b(false))
            }),
        ),
    ]);
    let base = obj(vec![("prototype", prototype.clone())]);
    rt.globals
        .borrow_mut()
        .insert("BaseExchange".into(), base.clone());
    let delegated = Rc::new(RefCell::new(Vec::new()));
    for label in ["public", "private"] {
        let name = if label == "public" {
            "KucoinSpotPublicWebsocket"
        } else {
            "KucoinSpotPrivateWebsocket"
        };
        let calls = trace.clone();
        let delegates = delegated.clone();
        let socket = function(move |_, args| {
            assert_eq!(args.len(), 1);
            let receiver = args[0].clone();
            calls.borrow_mut().push(label.into());
            let disposed = calls.clone();
            let socket = obj(vec![(
                "dispose",
                function(move |_, _| {
                    assert!(matches!(at(&receiver, "isDisposed").as_ref(), Js::B(true)));
                    disposed.borrow_mut().push(format!("{label}.dispose"));
                    Ok(u())
                }),
            )]);
            let cleanup = function(|_, _| Ok(u()));
            for key in ["listenOHLCV", "listenOrderBook"] {
                let owner = socket.clone();
                let result = cleanup.clone();
                let delegates = delegates.clone();
                set(
                    &socket,
                    key,
                    function(move |actual, args| {
                        same(&actual, &owner);
                        assert_eq!(args.len(), 2);
                        delegates.borrow_mut().push((actual, args, result.clone()));
                        Ok(result.clone())
                    }),
                );
            }
            Ok(socket)
        });
        rt.globals.borrow_mut().insert(name.into(), socket);
    }
    let globals = vec![
        (
            "parseFloat",
            function(|_, a| Ok(n(safe_cex_num::parse_float(&text(&a[0]))))),
        ),
        (
            "parseInt",
            function(|_, a| {
                Ok(n(safe_cex_num::parse_int(
                    &text(&a[0]),
                    Some(number(&a[1]) as i32),
                )))
            }),
        ),
        (
            "Math",
            obj(vec![(
                "max",
                function(|_, a| Ok(n(number(&a[0]).max(number(&a[1]))))),
            )]),
        ),
        ("Date", obj(vec![("now", function(|_, _| Ok(n(1701.))))])),
        (
            "Number",
            obj(vec![("MAX_SAFE_INTEGER", n(9007199254740991.))]),
        ),
        (
            "Array",
            obj(vec![(
                "isArray",
                function(|_, a| Ok(b(matches!(a[0].as_ref(), Js::A(_))))),
            )]),
        ),
        ("Error", function(|_, _| Ok(u()))),
        (
            "OrderSide",
            obj(vec![("Sell", s("sell")), ("Buy", s("buy"))]),
        ),
        (
            "OrderTimeInForce",
            obj(vec![
                ("PostOnly", s("PostOnly")),
                ("GoodTillCancel", s("GoodTillCancel")),
            ]),
        ),
        (
            "KUCOIN_SPOT_ORDER_STATUS",
            obj(vec![
                ("active", s("open")),
                ("done", s("closed")),
                ("canceled", s("canceled")),
            ]),
        ),
        (
            "KUCOIN_SPOT_ORDER_TYPE",
            obj(vec![("limit", s("limit")), ("market", s("market"))]),
        ),
        (
            "KUCOIN_SPOT_TIME_IN_FORCE",
            obj(vec![("IOC", s("ImmediateOrCancel"))]),
        ),
        (
            "KUCOIN_SPOT_ENDPOINTS",
            obj(vec![
                (
                    "ACTIVE_ORDER_SYMBOLS",
                    s("/api/v1/hf/orders/active/symbols"),
                ),
                ("MARKETS", s("/api/v2/symbols")),
                ("TICKERS", s("/api/v1/market/allTickers")),
                ("CANCEL_ORDER", s("/api/v1/hf/orders")),
                ("CANCEL_SYMBOL_ORDERS", s("/api/v1/hf/orders")),
                ("CANCEL_ALL_ORDERS", s("/api/v1/hf/orders/cancelAll")),
            ]),
        ),
    ];
    for (key, value) in globals {
        rt.globals.borrow_mut().insert(key.into(), value);
    }
    for (key, value) in [
        ("PositionSide", obj(vec![("Long", s("long"))])),
        ("MarginType", obj(vec![("Isolated", s("isolated"))])),
        ("PositionMode", obj(vec![("OneWay", s("oneway"))])),
        ("KUCOIN_SPOT_INTERVAL", obj(vec![("1m", s("1min"))])),
        (
            "OrderType",
            obj(vec![("Limit", s("limit")), ("Market", s("market"))]),
        ),
        (
            "omitUndefined",
            function(|_, args| {
                let value = obj(vec![]);
                let Js::O(fields) = args[0].as_ref() else {
                    panic!()
                };
                for (k, v) in fields.borrow().iter() {
                    if !matches!(v.as_ref(), Js::U) {
                        set(&value, k, v.clone());
                    }
                }
                Ok(value)
            }),
        ),
    ] {
        rt.globals.borrow_mut().insert(key.into(), value);
    }
    let endpoints = rt
        .globals
        .borrow()
        .get("KUCOIN_SPOT_ENDPOINTS")
        .unwrap()
        .clone();
    for (key, value) in [
        ("ACCOUNT_SUMMARY", "/api/v2/user-info"),
        ("ACCOUNTS", "/api/v1/accounts"),
        ("OPEN_ORDERS", "/api/v1/hf/orders/active/page"),
        ("KLINE", "/api/v1/market/candles"),
    ] {
        set(&endpoints, key, s(value));
    }
    let numbers = rt.globals.borrow().get("Number").unwrap().clone();
    set(
        &numbers,
        "isFinite",
        function(|_, args| Ok(b(number(&args[0]).is_finite()))),
    );
    let math = rt.globals.borrow().get("Math").unwrap().clone();
    set(
        &math,
        "min",
        function(|_, a| Ok(n(number(&a[0]).min(number(&a[1]))))),
    );
    set(
        &math,
        "floor",
        function(|_, a| Ok(n(number(&a[0]).floor()))),
    );
    rt.globals.borrow_mut().insert(
        "Promise".into(),
        obj(vec![(
            "all",
            function(|_, args| {
                let values = rows(&args[0]);
                Ok(promise(Box::pin(async move {
                    let mut out = vec![];
                    for v in values {
                        out.push(wait(v).await?);
                    }
                    Ok(arr(out))
                })))
            }),
        )]),
    );
    fn date(value: f64) -> V {
        obj(vec![
            ("valueOf", function(move |_, _| Ok(n(value)))),
            (
                "subtract",
                function(move |_, a| {
                    ss(&a[1], "m");
                    Ok(date(value - number(&a[0]) * 60_000.))
                }),
            ),
        ])
    }
    rt.globals
        .borrow_mut()
        .insert("dayjs".into(), function(|_, a| Ok(date(number(&a[0])))));
    let bodies = Rc::new(RefCell::new(Vec::<(V, Constructor<V, E>)>::new()));
    let saved = bodies.clone();
    let derive_rt = rt.clone();
    let super_rt = rt.clone();
    let super_trace = trace.clone();
    let actual_base = base.clone();
    let parent = prototype.clone();
    let construction = Rc::new(ConstructionOps {
        derive: Rc::new(move |name, base, length, body| {
            assert_eq!(name, "KucoinSpotExchange");
            assert_eq!(length, 3);
            let class = obj(vec![("base", base)]);
            let proto = obj(vec![]);
            derive_rt
                .prototypes
                .borrow_mut()
                .push((class.clone(), proto.clone()));
            derive_rt
                .prototype_parents
                .borrow_mut()
                .push((proto, at(&actual_base, "prototype")));
            derive_rt
                .constructors
                .borrow_mut()
                .push((class.clone(), Rc::new(|_| Ok(()))));
            saved.borrow_mut().push((class.clone(), body));
            Ok(class)
        }),
        superclass: Rc::new(|active| Ok(at(active, "base"))),
        super_construct: Rc::new(move |base, target, args| {
            same(&base, &super_rt.globals.borrow()["BaseExchange"]);
            assert!(
                super_rt
                    .prototypes
                    .borrow()
                    .iter()
                    .any(|(c, _)| Rc::ptr_eq(c, &target))
            );
            assert_eq!(args.len(), 3);
            ss(&args[0], "KUCOIN_SPOT");
            super_trace.borrow_mut().push("base".into());
            let receiver = obj(vec![
                ("exchangeName", args[0].clone()),
                ("opts", args[1].clone()),
                ("store", args[2].clone()),
                ("isDisposed", b(false)),
            ]);
            super_rt
                .object_prototypes
                .borrow_mut()
                .push((receiver.clone(), parent.clone()));
            Ok(receiver)
        }),
        construct: Rc::new(|ctor, args| call(ctor, u(), args)),
    });
    let body_language = language.clone();
    let mapped = Rc::new(RefCell::new(Vec::new()));
    let mapped_capture = mapped.clone();
    let ops = Rc::new(SpotExchangeOps {
        language: language.clone(),
        construction,
        body: Rc::new(move |r| body(body_language.clone(), r, mapped_capture.clone())),
        regexp: Rc::new(|pattern| Ok(s(pattern))),
    });
    let class = port::define_spot_exchange_class(ops).unwrap();
    let store = obj(vec![
        ("markets", arr(vec![])),
        ("marketsBySymbol", obj(vec![])),
        (
            "tickersBySymbol",
            obj(vec![("BTC-USDT", obj(vec![("last", n(17.))]))]),
        ),
        ("fetchedLeverage", obj(vec![])),
        ("loaded", obj(vec![])),
    ]);
    let store_ref = store.clone();
    set(
        &store,
        "update",
        function(move |receiver, args| {
            same(&receiver, &store_ref);
            let Js::O(fields) = args[0].as_ref() else {
                panic!()
            };
            for (k, v) in fields.borrow().iter() {
                set(&receiver, k, v.clone());
            }
            Ok(u())
        }),
    );
    let opts = obj(vec![("marker", n(97.))]);
    let c = class.clone();
    let a = opts.clone();
    let st = store.clone();
    let construct = Rc::new(move || {
        let constructor = bodies
            .borrow()
            .iter()
            .find(|(value, _)| Rc::ptr_eq(value, &c))
            .unwrap()
            .1
            .clone();
        constructor(
            c.clone(),
            c.clone(),
            vec![s("KUCOIN_SPOT"), a.clone(), st.clone()],
        )
        .unwrap()
    });
    let r = construct();
    same(&at(&r, "opts"), &opts);
    same(&at(&r, "store"), &store);
    Fixture {
        rt,
        class,
        r,
        store,
        trace,
        requests,
        construct,
        inner,
        mapped,
        delegated,
        limited,
        unlimited,
        owners,
    }
}
fn advance(rt: &Runtime) {
    for _ in 0..16 {
        let jobs = rt.jobs.borrow().clone();
        for job in jobs {
            let Js::P(promise) = job.as_ref() else {
                panic!("retained promise")
            };
            if promise.result.borrow().is_none() {
                let progress = poll(&job);
                if let std::task::Poll::Ready(Err(error)) = progress {
                    let result = promise.result.borrow();
                    let Some(Err(saved)) = result.as_ref() else {
                        panic!("visible rejection state")
                    };
                    assert!(Rc::ptr_eq(saved, &error));
                }
            }
        }
    }
}
fn complete(rt: &Runtime, value: V) -> Result<V, E> {
    for _ in 0..64 {
        advance(rt);
        if let std::task::Poll::Ready(value) = poll(&value) {
            return value;
        }
    }
    panic!("finite promise did not complete")
}
fn metadata(f: &Fixture) -> Vec<Value> {
    NAMES
        .iter()
        .map(|name| {
            let value = at(&f.r, name);
            let metas = f.rt.metas.borrow();
            let (_, actual, length, asynchronous) = metas
                .iter()
                .find(|(v, _, _, _)| Rc::ptr_eq(v, &value))
                .unwrap();
            json!([actual, length, asynchronous])
        })
        .collect()
}
fn layout() -> Value {
    let f = fixture();
    same(&at(&f.r, "xhr"), &f.limited);
    same(&at(&f.r, "unlimitedXHR"), &f.unlimited);
    assert!(
        !Rc::ptr_eq(&f.limited, &f.unlimited),
        "KUCOIN-EXCHANGE distinct API handles"
    );
    let fields = keys(&f.r);
    let expected = [
        "exchangeName",
        "opts",
        "store",
        "isDisposed",
        "clientOrderIdRule",
        "xhr",
        "unlimitedXHR",
        "isSpot",
        "publicWebsocket",
        "privateWebsocket",
        "grafanaEndpointsEvery",
        "grafanaEndpointsPeriodic",
    ]
    .into_iter()
    .chain(NAMES)
    .collect::<Vec<_>>();
    assert_eq!(
        fields, expected,
        "KUCOIN-EXCHANGE complete original own fields"
    );
    let definitions =
        f.rt.definitions
            .borrow()
            .iter()
            .filter(|(v, _)| Rc::ptr_eq(v, &f.r))
            .map(|(_, k)| k.clone())
            .collect::<Vec<_>>();
    assert_eq!(
        definitions,
        expected[4..],
        "KUCOIN-EXCHANGE complete field definitions"
    );
    assert!(matches!(at(&f.r, "isSpot").as_ref(), Js::B(true)));
    let meta = metadata(&f);
    for (i, value) in meta.iter().enumerate() {
        assert_eq!(
            value,
            &json!([
                NAMES[i],
                LENGTHS[i],
                !matches!(
                    NAMES[i],
                    "dispose" | "getUsdPrice" | "mapOrder" | "listenOHLCV" | "listenOrderBook"
                )
            ]),
            "KUCOIN-EXCHANGE native function metadata"
        );
    }
    let other = (f.construct)();
    for name in NAMES {
        assert!(
            !Rc::ptr_eq(&at(&f.r, name), &at(&other, name)),
            "KUCOIN-EXCHANGE fresh per-instance arrow"
        );
    }
    assert_eq!(
        f.trace.borrow().as_slice(),
        [
            "base",
            "rule",
            "rateLimit:5",
            "xhr",
            "api",
            "public",
            "private",
            "base",
            "rule",
            "rateLimit:5",
            "xhr",
            "api",
            "public",
            "private"
        ]
    );
    assert!(f.requests.borrow().is_empty());
    let proto =
        f.rt.prototypes
            .borrow()
            .iter()
            .find(|(class, _)| Rc::ptr_eq(class, &f.class))
            .unwrap()
            .1
            .clone();
    assert!(keys(&proto).is_empty());
    json!({"fields":fields[4..],"metadata":meta,"trace":*f.trace.borrow()})
}
fn bodies() -> Value {
    let f = fixture();
    let mut values = vec![];
    let mut errors = vec![];
    let foreign = obj(vec![("store", obj(vec![]))]);
    set(&f.r, "isDisposed", b(true));
    for name in NAMES {
        let input = match name {
            "getUsdPrice" => vec![s("BTC")],
            "fetchPositions" => vec![],
            "fetchSymbolMaxLeverage" => vec![s("BTC")],
            "setLeverage" => vec![s("BTC"), n(97.), s("sell")],
            "changeMarginType" | "changePositionMode" => vec![b(true), s("BTC")],
            "fetchOrders" => vec![arr(vec![s("BTC")])],
            "mapOrder" => vec![obj(vec![
                ("id", s("id")),
                ("symbol", s("BTC")),
                ("size", s("9")),
                ("dealSize", s("4")),
                ("price", s("11")),
                ("lastUpdatedAt", s("123")),
            ])],
            "placeOrder" => vec![obj(vec![("type", s("conditional")), ("symbol", s("BTC"))])],
            "updateOrder" => vec![obj(vec![
                (
                    "order",
                    obj(vec![("symbol", s("MISSING")), ("id", s("id"))]),
                ),
                ("update", obj(vec![])),
            ])],
            "cancelOrders" => vec![arr(vec![])],
            "cancelSymbolOrders" => vec![s("BTC")],
            "fetchHistoricalFills" => vec![obj(vec![])],
            "fetchOHLCV" => vec![obj(vec![
                ("interval", s("1m")),
                ("symbol", s("BTC")),
                ("limit", n(1.)),
                ("to", n(120_000.)),
            ])],
            "listenOHLCV" => vec![
                obj(vec![("symbol", s("BTC")), ("interval", s("1m"))]),
                function(|_, _| Ok(u())),
            ],
            "listenOrderBook" => vec![s("BTC"), function(|_, _| Ok(u()))],
            _ => vec![],
        };
        let original = input.clone();
        let raw = call(at(&f.r, name), foreign.clone(), input).unwrap();
        let asynchronous = !matches!(
            name,
            "dispose" | "getUsdPrice" | "mapOrder" | "listenOHLCV" | "listenOrderBook"
        );
        assert_eq!(
            matches!(raw.as_ref(), Js::P(_)),
            asynchronous,
            "KUCOIN-EXCHANGE actual async return kind"
        );
        let result = complete(&f.rt, raw);
        match result {
            Err(error) => {
                assert_eq!(
                    error.as_str(),
                    if name == "placeOrder" {
                        "KuCoin Spot does not support conditional orders"
                    } else {
                        assert_eq!(name, "updateOrder");
                        "Market MISSING not found"
                    }
                );
                errors.push(json!([name, error.as_str()]));
                values.push(json!({"error":error.as_str()}));
            }
            Ok(value) => {
                match name {
                    "getAccount" => {
                        assert_eq!(keys(&value), ["userId", "subId", "affiliateId"]);
                        for (key, want) in
                            [("userId", "23"), ("subId", "29"), ("affiliateId", "31")]
                        {
                            ss(&at(&value, key), want);
                        }
                    }
                    "fetchBalance" => {
                        assert_eq!(
                            keys(&value),
                            [
                                "free",
                                "used",
                                "total",
                                "upnl",
                                "currencies",
                                "coins",
                                "info"
                            ]
                        );
                        for key in ["free", "used", "total", "upnl"] {
                            sn(&at(&value, key), 0.);
                        }
                        assert!(keys(&at(&value, "currencies")).is_empty());
                        assert!(keys(&at(&value, "coins")).is_empty());
                        assert!(rows(&at(&value, "info")).is_empty());
                    }
                    "validateAccount" => {
                        assert_eq!(keys(&value), ["error", "balance"]);
                        ss(&at(&value, "error"), "");
                        sn(&at(&at(&value, "balance"), "total"), 0.);
                    }
                    "getUsdPrice" => sn(&value, 17.),
                    "fetchSymbolMaxLeverage" => sn(&value, 1.),
                    "fetchPositions"
                    | "fetchOrders"
                    | "fetchActiveOrderSymbols"
                    | "fetchHistoricalFills"
                    | "fetchMarkets"
                    | "fetchTickers"
                    | "fetchOHLCV" => {
                        assert_eq!(rows(&value).len(), 0, "KUCOIN-EXCHANGE native empty result")
                    }
                    "mapOrder" => {
                        same(&at(&value, "info"), &original[0]);
                        assert_eq!(
                            keys(&value),
                            [
                                "id",
                                "status",
                                "symbol",
                                "type",
                                "side",
                                "price",
                                "amount",
                                "filled",
                                "remaining",
                                "reduceOnly",
                                "timeInForce",
                                "lastUpdate",
                                "info"
                            ]
                        );
                        for (key, want) in [
                            ("amount", 9.),
                            ("filled", 4.),
                            ("remaining", 5.),
                            ("price", 11.),
                            ("lastUpdate", 123.),
                        ] {
                            sn(&at(&value, key), want);
                        }
                        for (key, want) in [
                            ("id", "id"),
                            ("status", "closed"),
                            ("symbol", "BTC"),
                            ("type", "limit"),
                            ("side", "buy"),
                            ("timeInForce", "GoodTillCancel"),
                        ] {
                            ss(&at(&value, key), want);
                        }
                    }
                    "listenOHLCV" | "listenOrderBook" => {
                        assert!(matches!(value.as_ref(), Js::F(_)));
                        let delegated = f.delegated.borrow();
                        let (owner, args, cleanup) = delegated.last().unwrap();
                        same(owner, &at(&f.r, "publicWebsocket"));
                        same(&args[0], &original[0]);
                        same(&args[1], &original[1]);
                        same(&value, cleanup);
                    }
                    _ => assert!(
                        matches!(value.as_ref(), Js::U),
                        "KUCOIN-EXCHANGE native undefined"
                    ),
                }
                values.push(if matches!(value.as_ref(), Js::F(_)) {
                    json!({"cleanup":true})
                } else {
                    literal(&value)
                });
            }
        }
    }
    assert_eq!(values.len(), 26);
    assert_eq!(errors.len(), 2);
    let leverage = at(&at(&f.store, "fetchedLeverage"), "BTC");
    assert_eq!(keys(&leverage), ["buyLeverage", "sellLeverage"]);
    sn(&at(&leverage, "buyLeverage"), 1.);
    sn(&at(&leverage, "sellLeverage"), 1.);
    assert_eq!(
        f.inner.borrow().len(),
        1,
        "KUCOIN-EXCHANGE one inner async order promise"
    );
    assert_eq!(
        f.mapped.borrow().len(),
        1,
        "KUCOIN-EXCHANGE one ordinary mapper promise"
    );
    same(&f.inner.borrow()[0], &f.mapped.borrow()[0]);
    let metas = f.rt.metas.borrow();
    let inner_index = metas
        .iter()
        .position(|(_, name, _, _)| name == "fetchSymbolOrders")
        .unwrap();
    assert_eq!(
        (
            &metas[inner_index].1,
            metas[inner_index].2,
            metas[inner_index].3
        ),
        (&"fetchSymbolOrders".to_string(), 1, true)
    );
    assert_eq!(
        (
            &metas[inner_index + 1].1,
            metas[inner_index + 1].2,
            metas[inner_index + 1].3
        ),
        (&String::new(), 1, false),
        "KUCOIN-EXCHANGE ordinary outer order mapper"
    );
    drop(metas);
    assert_eq!(
        f.requests.borrow().len(),
        12,
        "KUCOIN-EXCHANGE independent request count"
    );
    assert_eq!(
        f.trace.borrow().as_slice(),
        [
            "base",
            "rule",
            "rateLimit:5",
            "xhr",
            "api",
            "public",
            "private",
            "base.dispose",
            "public.dispose",
            "private.dispose",
            "external",
            "error:KuCoin Spot does not support conditional orders"
        ],
        "KUCOIN-EXCHANGE complete native constructor disposal and diagnostic order"
    );
    assert_eq!(
        f.owners.borrow().as_slice(),
        [
            "limited:get",
            "limited:get",
            "limited:get",
            "limited:get",
            "limited:get",
            "limited:get",
            "limited:get",
            "unlimited:delete",
            "unlimited:delete",
            "limited:get",
            "limited:get",
            "limited:get"
        ],
        "KUCOIN-EXCHANGE native request adapter ownership"
    );
    assert_eq!(
        f.delegated.borrow().len(),
        2,
        "KUCOIN-EXCHANGE independent delegate count"
    );
    let request_rows = f.requests.borrow();
    for (index, route) in [
        "/api/v2/user-info",
        "/api/v1/accounts",
        "/api/v1/accounts",
        "/api/v2/symbols",
        "/api/v1/accounts",
        "/api/v1/hf/orders/active/page",
        "/api/v1/hf/orders/active/symbols",
        "/api/v1/hf/orders",
        "/api/v1/hf/orders/cancelAll",
        "/api/v2/symbols",
        "/api/v1/market/allTickers",
        "/api/v1/market/candles",
    ]
    .iter()
    .enumerate()
    {
        let args = rows(&request_rows[index]);
        ss(&args[0], route);
        let params = match index {
            1 | 2 | 4 => Some(vec![("type", s("trade"))]),
            5 => Some(vec![
                ("symbol", s("BTC")),
                ("pageNum", n(1.)),
                ("pageSize", n(50.)),
            ]),
            7 => Some(vec![("symbol", s("BTC"))]),
            11 => Some(vec![
                ("symbol", s("BTC")),
                ("type", s("1min")),
                ("startAt", n(60.)),
                ("endAt", n(120.)),
            ]),
            _ => None,
        };
        assert_eq!(
            args.len(),
            if params.is_some() { 2 } else { 1 },
            "KUCOIN-EXCHANGE native request arity"
        );
        if let Some(fields) = params {
            assert_eq!(keys(&args[1]), ["params"]);
            let params = at(&args[1], "params");
            assert_eq!(
                keys(&params),
                fields.iter().map(|(key, _)| *key).collect::<Vec<_>>()
            );
            for (key, want) in fields {
                let actual = at(&params, key);
                match want.as_ref() {
                    Js::N(want) => sn(&actual, *want),
                    Js::S(want) => ss(&actual, want),
                    _ => panic!(),
                }
            }
        }
    }
    drop(request_rows);
    json!({"values":values,"requests":f.requests.borrow().iter().map(literal).collect::<Vec<_>>(),"trace":*f.trace.borrow()})
}
fn dropped() -> Value {
    let f = fixture();
    set(&f.r, "isDisposed", b(true));
    let gate = pending();
    let result = gate.clone();
    set(
        &f.r,
        "initExternalPublicData",
        function(move |_, _| Ok(result.clone())),
    );
    let returned = call(at(&f.r, "start"), u(), vec![]).unwrap();
    assert!(matches!(returned.as_ref(), Js::P(_)));
    let weak = Rc::downgrade(&returned);
    drop(returned);
    assert!(
        weak.upgrade().is_some(),
        "KUCOIN-EXCHANGE pending start retained after caller drop"
    );
    assert_eq!(f.requests.borrow().len(), 0);
    resolve(&gate, Ok(b(false)));
    advance(&f.rt);
    let returned = weak.upgrade().expect("retained completion");
    assert!(matches!(complete(&f.rt, returned).unwrap().as_ref(), Js::U));
    assert_eq!(
        f.requests.borrow().len(),
        1,
        "KUCOIN-EXCHANGE dropped start continues after await"
    );
    let args = rows(&f.requests.borrow()[0]);
    assert_eq!(args.len(), 1);
    ss(&args[0], "/api/v2/symbols");
    json!({"requests":f.requests.borrow().iter().map(literal).collect::<Vec<_>>()})
}
#[test]
fn spot_exchange_native_class_before_source() {
    let results = [layout(), bodies(), dropped()];
    assert_eq!(results.len(), 3);
    for (index, kind) in ["layout", "bodies", "dropped"].iter().enumerate() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-exchange-class.cjs"
            ))
            .arg(kind)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source {kind}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(results[index], source, "source class floor {index}/{kind}");
    }
}
