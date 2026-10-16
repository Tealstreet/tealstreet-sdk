// Reuse finite language operations without constructing a socket in this target.
#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{
    bootstrap_reads::BootstrapOps, public_streams::StreamOps, spot_bodies::SpotBodyOps,
    spot_native_order_reads as port,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
};
use values::*;
fn at(v: &V, k: &str) -> V {
    get(v, k).unwrap()
}
fn same(a: &V, b: &V) {
    assert!(Rc::ptr_eq(a, b), "KUCOIN-READ original reference identity");
}
fn sn(v: &V, w: f64) {
    let Js::N(n) = v.as_ref() else {
        panic!("native number kind")
    };
    assert_eq!(n.to_bits(), w.to_bits(), "KUCOIN-READ native number bits");
}
fn ss(v: &V, w: &str) {
    let Js::S(s) = v.as_ref() else {
        panic!("native string kind")
    };
    assert_eq!(s, w, "KUCOIN-READ native string value");
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
fn block(f: impl std::future::Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut f = Box::pin(f);
    for _ in 0..64 {
        if let std::task::Poll::Ready(value) = f
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            return value;
        }
    }
    panic!("finite fixture exceeded 64 polls")
}
fn frame(rt: &Rc<Runtime>, receiver: &V) -> Rc<StreamOps<V, E>> {
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
fn body(
    h: Rc<StreamOps<V, E>>,
    closes: Rc<Cell<usize>>,
    constructor: V,
    constructed: Rc<RefCell<Vec<E>>>,
) -> Rc<SpotBodyOps<V, E>> {
    let base = BootstrapOps {
        value: h,
        member: Rc::new(|v, k| get(&v, k)),
        raw_filter: Rc::new(|_, _| panic!("unused filter")),
        boolean: Rc::new(b),
        mapper: Rc::new(|_| panic!("unused mapper")),
        async_mapper: Rc::new(|_| panic!("unused mapper")),
        callback: Rc::new(|_| panic!("unused callback")),
        response_pair: Rc::new(|_| panic!("unused pair")),
        interval_parts: Rc::new(|_| panic!("unused interval")),
        iterator: Rc::new(|array| Ok(obj(vec![("array", array), ("index", n(0.))]))),
        next: Rc::new(|it| {
            let i = number(&at(it, "index")) as usize;
            set(it, "index", n((i + 1) as f64));
            let array = at(it, "array");
            let Js::A(values) = array.as_ref() else {
                panic!("iterator array")
            };
            let value = values.borrow().get(i).cloned();
            Ok(value)
        }),
        close_throw: Rc::new(move |_, e| {
            closes.set(closes.get() + 1);
            e
        }),
        some: Rc::new(|_, _| panic!("unused some")),
    };
    Rc::new(SpotBodyOps {
        async_arrow: Rc::new(|_, _, _| panic!("unused async arrow")),
        base: Rc::new(base),
        triple: Rc::new(|_| panic!("unused triple")),
        has: Rc::new(|_, _| panic!("unused has")),
        new_error: Rc::new(move |actual, msg| {
            same(&actual, &constructor);
            let handle = Rc::new(text(&msg));
            constructed.borrow_mut().push(handle.clone());
            Ok(handle)
        }),
        comparator: Rc::new(|_| panic!("unused comparator")),
        spread_arguments: Rc::new(|_| panic!("unused spread")),
        assignment_reference: Rc::new(|_, _| panic!("unused ref")),
        assign: Rc::new(|_, _| panic!("unused assign")),
    })
}
struct Outcome {
    value: V,
    calls: Rc<RefCell<Vec<V>>>,
    emitted: Rc<RefCell<Vec<V>>>,
    closed: usize,
    reads: usize,
    error: Option<String>,
}
impl Outcome {
    fn project(&self) -> Value {
        json!({"value":if matches!(self.value.as_ref(),Js::F(_)){json!({"function":true})}else{literal(&self.value)},"calls":self.calls.borrow().iter().map(literal).collect::<Vec<_>>(),"emitted":self.emitted.borrow().iter().map(literal).collect::<Vec<_>>(),"closed":self.closed,"reads":self.reads,"error":self.error})
    }
}
fn run(kind: &str) -> Outcome {
    let rt = Rc::new(Runtime::default());
    let ops = rt.operations();
    let fallback = arr(vec![s("cached")]);
    let store = obj(vec![
        ("markets", fallback.clone()),
        ("tickers", fallback.clone()),
        (
            "marketsBySymbol",
            obj(vec![(
                "BTC",
                obj(vec![("id", s("API")), ("symbol", s("BTC"))]),
            )]),
        ),
    ]);
    let receiver = obj(vec![("store", store)]);
    let calls = Rc::new(RefCell::new(vec![]));
    let emitted = Rc::new(RefCell::new(vec![]));
    let closes = Rc::new(Cell::new(0));
    let reads = Rc::new(Cell::new(0));
    let sink = emitted.clone();
    set(
        &receiver,
        "emitAndLogError",
        function(move |_, a| {
            sink.borrow_mut().push(a[0].clone());
            Ok(u())
        }),
    );
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
    let error_ctor = rt.globals.borrow().get("Error").unwrap().clone();
    let constructed = Rc::new(RefCell::new(Vec::<E>::new()));
    let h = frame(&rt, &receiver);
    let mut error = None;
    let value;
    if kind.starts_with("map-") {
        let order = if kind == "map-default" {
            obj(vec![
                ("orderId", s("fallback")),
                ("symbol", s("BTC")),
                ("originSize", s("9units")),
                ("filledSize", s("4units")),
                ("matchPrice", s("17.5suffix")),
                ("active", b(true)),
            ])
        } else {
            obj(vec![
                ("id", s("id")),
                ("symbol", s("BTC")),
                ("size", s("3")),
                ("dealSize", s("5")),
                ("remainSize", n(0.)),
                ("price", s("11")),
                ("type", s("market")),
                ("status", s("canceled")),
                ("side", s("sell")),
                ("postOnly", b(true)),
                ("lastUpdatedAt", s("123tail")),
            ])
        };
        value = port::map_order(&ops, &h, order.clone()).unwrap();
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
        same(&at(&value, "info"), &order);
        ss(&at(&value, "symbol"), "BTC");
        assert!(matches!(at(&value, "reduceOnly").as_ref(), Js::B(false)));
        let default = kind == "map-default";
        for (key, want) in [
            ("price", if default { 17.5 } else { 11. }),
            ("amount", if default { 9. } else { 3. }),
            ("filled", if default { 4. } else { 5. }),
            ("remaining", if default { 5. } else { 0. }),
            ("lastUpdate", if default { 1701. } else { 123. }),
        ] {
            sn(&at(&value, key), want);
        }
        for (key, want) in [
            ("id", if default { "fallback" } else { "id" }),
            ("status", if default { "open" } else { "canceled" }),
            ("type", if default { "limit" } else { "market" }),
            ("side", if default { "buy" } else { "sell" }),
            (
                "timeInForce",
                if default {
                    "GoodTillCancel"
                } else {
                    "PostOnly"
                },
            ),
        ] {
            ss(&at(&value, key), want);
        }
    } else if kind.starts_with("delegate-") {
        let external = kind.ends_with("external");
        let candle = kind.contains("candle");
        let input = obj(vec![("marker", n(71.))]);
        let callback = function(|_, _| Ok(u()));
        let cleanup = function(|_, _| Ok(u()));
        let got = calls.clone();
        let a = input.clone();
        let b = callback.clone();
        let c = cleanup.clone();
        let provider = obj(vec![]);
        let owner = provider.clone();
        set(
            &provider,
            if candle {
                if external {
                    "subscribeCandle"
                } else {
                    "listenOHLCV"
                }
            } else if external {
                "subscribeOrderbook"
            } else {
                "listenOrderBook"
            },
            function(move |actual, args| {
                same(&actual, &owner);
                assert_eq!(args.len(), 2);
                same(&args[0], &a);
                same(&args[1], &b);
                got.borrow_mut().push(s("delegate"));
                Ok(c.clone())
            }),
        );
        set(
            &receiver,
            if external {
                "externalPublicDataProvider"
            } else {
                "publicWebsocket"
            },
            provider,
        );
        value = if candle {
            port::listen_ohlcv(&h, input, callback)
        } else {
            port::listen_order_book(&h, input, callback)
        }
        .unwrap();
        same(&value, &cleanup);
        assert_eq!(calls.borrow().len(), 1);
    } else {
        let raw = if kind.starts_with("active") {
            arr(vec![s("BTC"), obj(vec![("raw", n(71.))])])
        } else if kind.starts_with("markets") {
            obj(vec![
                ("symbol", s("BTC")),
                ("baseCurrency", s("B")),
                ("quoteCurrency", s("Q")),
                ("enableTrading", b(true)),
                ("baseIncrement", s("0.25")),
                ("priceIncrement", s("0.5")),
                ("baseMinSize", s("2")),
                ("baseMaxSize", s("19")),
            ])
        } else {
            obj(vec![
                ("symbol", s("BTC")),
                ("last", s("17.5")),
                ("buy", s("11")),
                ("sell", s("13")),
                ("changeRate", s("0.125")),
                ("vol", s("19")),
                ("volValue", s("23")),
            ])
        };
        let data = obj(vec![(
            "code",
            if kind.ends_with("bad") {
                n(200000.)
            } else {
                s("200000")
            },
        )]);
        if kind.starts_with("active") {
            let inner = obj(vec![]);
            if kind == "active-live" {
                let count = reads.clone();
                let first = raw.clone();
                rt.getters.borrow_mut().push((
                    inner.clone(),
                    "symbols".into(),
                    Rc::new(move || {
                        count.set(count.get() + 1);
                        Ok(if count.get() == 1 {
                            first.clone()
                        } else {
                            s("live-raw")
                        })
                    }),
                ));
            } else {
                set(&inner, "symbols", raw.clone());
            }
            set(&data, "data", inner);
        } else if kind.starts_with("markets") {
            set(&data, "data", arr(vec![raw.clone()]));
        } else {
            set(
                &data,
                "data",
                obj(vec![(
                    "ticker",
                    arr(vec![obj(vec![("symbol", s("MISSING"))]), raw.clone()]),
                )]),
            );
        }
        let orders = arr(vec![
            obj(vec![("id", s("a")), ("symbol", s("BTC"))]),
            obj(vec![("id", s("b")), ("symbol", s("OTHER"))]),
        ]);
        let append = kind == "cancel-append";
        let delayed = kind == "cancel-pending";
        let gate = pending();
        let pending_gate = gate.clone();
        let fail = kind.ends_with("throw");
        let original_error = Rc::new("transport".to_string());
        let saved_error = original_error.clone();
        let response = obj(vec![("data", data)]);
        let pending_response = response.clone();
        let got = calls.clone();
        let live = orders.clone();
        let xhr = function(move |_, args| {
            got.borrow_mut().push(arr(args));
            if append && got.borrow().len() == 1 {
                let Js::A(rows) = live.as_ref() else { panic!() };
                rows.borrow_mut()
                    .push(obj(vec![("id", s("c")), ("symbol", s("OTHER"))]));
            }
            if delayed && got.borrow().len() == 1 {
                Ok(gate.clone())
            } else if fail {
                Err(original_error.clone())
            } else {
                Ok(response.clone())
            }
        });
        set(&receiver, "xhr", obj(vec![("get", xhr.clone())]));
        set(&receiver, "unlimitedXHR", obj(vec![("delete", xhr)]));
        let result = if kind.starts_with("active") {
            block(port::fetch_active_order_symbols(h.clone()))
        } else if kind.starts_with("markets") {
            block(port::fetch_markets(ops.clone(), h.clone()))
        } else if kind.starts_with("tickers") {
            block(port::fetch_tickers(ops.clone(), h.clone()))
        } else {
            let body = body(h.clone(), closes.clone(), error_ctor, constructed.clone());
            if kind.starts_with("cancel-symbol") {
                block(port::cancel_symbol_orders(ops, body, s("BTC")))
            } else if kind.starts_with("cancel-all") {
                block(port::cancel_all_orders(body))
            } else {
                if delayed {
                    let original = orders.clone();
                    let mut task = Box::pin(port::cancel_orders(ops, body, orders));
                    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                    for _ in 0..2 {
                        assert!(
                            task.as_mut().poll(&mut context).is_pending(),
                            "KUCOIN-READ pending DELETE must retain await"
                        );
                    }
                    assert_eq!(
                        calls.borrow().len(),
                        1,
                        "KUCOIN-READ no second dispatch before settlement"
                    );
                    let Js::A(rows) = original.as_ref() else {
                        panic!("live orders")
                    };
                    rows.borrow_mut()
                        .push(obj(vec![("id", s("c")), ("symbol", s("OTHER"))]));
                    resolve(&pending_gate, Ok(pending_response));
                    block(task)
                } else {
                    block(port::cancel_orders(ops, body, orders))
                }
            }
        };
        value = match result {
            Ok(v) => v,
            Err(e) => {
                if fail {
                    assert!(
                        Rc::ptr_eq(&e, &saved_error),
                        "KUCOIN-READ original error identity"
                    );
                }
                if kind.ends_with("bad") {
                    assert_eq!(constructed.borrow().len(), 1);
                    assert!(
                        Rc::ptr_eq(&e, &constructed.borrow()[0]),
                        "KUCOIN-READ constructed error handle identity"
                    );
                }
                error = Some(e.to_string());
                u()
            }
        };
        if kind.starts_with("active") || kind.starts_with("markets") || kind.starts_with("tickers")
        {
            assert_eq!(calls.borrow().len(), 1);
            let args = rows(&calls.borrow()[0]);
            assert_eq!(args.len(), 1, "KUCOIN-READ GET arity");
            ss(
                &args[0],
                if kind.starts_with("active") {
                    "/api/v1/hf/orders/active/symbols"
                } else if kind.starts_with("markets") {
                    "/api/v2/symbols"
                } else {
                    "/api/v1/market/allTickers"
                },
            );
        }
        if kind.starts_with("active") {
            assert_eq!(calls.borrow().len(), 1);
            assert!(emitted.borrow().is_empty());
            if kind == "active-live" {
                ss(&value, "live-raw");
                assert_eq!(reads.get(), 2);
            } else if kind == "active-ok" {
                same(&value, &raw);
                assert_eq!(rows(&value).len(), 2);
            } else {
                assert_eq!(rows(&value).len(), 0);
            }
        } else if kind.starts_with("markets") || kind.starts_with("tickers") {
            assert_eq!(calls.borrow().len(), 1);
            if kind.ends_with("bad") || fail {
                same(&value, &fallback);
                assert_eq!(emitted.borrow().len(), 1);
                ss(
                    &emitted.borrow()[0],
                    if fail {
                        "transport"
                    } else if kind.starts_with("markets") {
                        "Failed to fetch KuCoin Spot markets"
                    } else {
                        "Failed to fetch KuCoin Spot tickers"
                    },
                );
            } else {
                let values = rows(&value);
                assert_eq!(values.len(), 1, "KUCOIN-READ independent decoded row count");
                let row = &values[0];
                same(&at(row, "info"), &raw);
                ss(
                    &at(row, "id"),
                    if kind.starts_with("markets") {
                        "BTC"
                    } else {
                        "API"
                    },
                );
                ss(&at(row, "symbol"), "BTC");
                if kind.starts_with("markets") {
                    assert_eq!(
                        keys(row),
                        [
                            "id",
                            "symbol",
                            "prettySymbol",
                            "base",
                            "quote",
                            "active",
                            "precision",
                            "limits",
                            "contractSize",
                            "isSpot",
                            "isInverse",
                            "info"
                        ]
                    );
                    ss(&at(row, "prettySymbol"), "BQ");
                    ss(&at(row, "base"), "B");
                    ss(&at(row, "quote"), "Q");
                    for (key, want) in [("active", true), ("isSpot", true), ("isInverse", false)] {
                        assert!(matches!(at(row,key).as_ref(),Js::B(v)if *v==want));
                    }
                    sn(&at(&at(row, "precision"), "amount"), 0.25);
                    sn(&at(&at(row, "precision"), "price"), 0.5);
                    let limits = at(row, "limits");
                    for (key, want) in [("min", 2.), ("max", 19.)] {
                        sn(&at(&at(&limits, "amount"), key), want);
                    }
                    for key in ["min", "max"] {
                        sn(&at(&at(&limits, "leverage"), key), 1.);
                    }
                    sn(&at(row, "contractSize"), 1.);
                } else {
                    assert_eq!(
                        keys(row),
                        [
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
                            "info"
                        ]
                    );
                    for (key, want) in [
                        ("bid", 11.),
                        ("ask", 13.),
                        ("last", 17.5),
                        ("mark", 17.5),
                        ("index", 17.5),
                        ("percentage", 12.5),
                        ("fundingRate", 0.),
                        ("fundingTime", 0.),
                        ("openInterest", 0.),
                        ("volume", 19.),
                        ("quoteVolume", 23.),
                    ] {
                        sn(&at(row, key), want);
                    }
                }
            }
        } else {
            assert!(matches!(value.as_ref(), Js::U));
            let count = if kind == "cancel-append" || kind == "cancel-pending" {
                3
            } else if kind == "cancel-ok" {
                2
            } else {
                1
            };
            assert_eq!(
                calls.borrow().len(),
                count,
                "KUCOIN-READ sequential call count"
            );
            assert_eq!(
                closes.get(),
                usize::from(kind == "cancel-throw" || kind == "cancel-bad")
            );
            if kind.ends_with("bad") || fail {
                assert_eq!(emitted.borrow().len(), 1);
                let want = if fail {
                    "transport"
                } else if kind.starts_with("cancel-all") {
                    "Failed to cancel all KuCoin Spot orders"
                } else if kind.starts_with("cancel-symbol") {
                    "Failed to cancel KuCoin Spot orders for BTC"
                } else {
                    "Failed to cancel KuCoin Spot order a"
                };
                assert_eq!(
                    error.as_deref(),
                    Some(want),
                    "KUCOIN-READ native rejection message"
                );
                ss(&emitted.borrow()[0], want);
            } else {
                assert!(error.is_none());
                assert!(emitted.borrow().is_empty());
            }
            let got = calls.borrow();
            if kind.starts_with("cancel-all") {
                assert_eq!(rows(&got[0]).len(), 1);
                ss(&rows(&got[0])[0], "/api/v1/hf/orders/cancelAll");
            } else {
                let args = rows(&got[0]);
                assert_eq!(args.len(), 2);
                assert_eq!(keys(&args[1]), ["params"]);
                assert_eq!(keys(&at(&args[1], "params")), ["symbol"]);
                ss(&at(&at(&args[1], "params"), "symbol"), "API");
                ss(
                    &args[0],
                    if kind.starts_with("cancel-symbol") {
                        "/api/v1/hf/orders"
                    } else {
                        "/api/v1/hf/orders/a"
                    },
                );
                if count == 3 {
                    let args = rows(&got[2]);
                    assert_eq!(args.len(), 2);
                    ss(&args[0], "/api/v1/hf/orders/c");
                    assert_eq!(keys(&args[1]), ["params"]);
                    assert_eq!(keys(&at(&args[1], "params")), ["symbol"]);
                    ss(&at(&at(&args[1], "params"), "symbol"), "OTHER");
                }
                if count > 1 {
                    let args = rows(&got[1]);
                    ss(&args[0], "/api/v1/hf/orders/b");
                    ss(&at(&at(&args[1], "params"), "symbol"), "OTHER");
                }
            }
        }
    }
    Outcome {
        value,
        calls,
        emitted,
        closed: closes.get(),
        reads: reads.get(),
        error,
    }
}
#[test]
fn native_spot_reads_cancels_and_delegates_before_source() {
    let cases = [
        "map-default",
        "map-explicit",
        "active-ok",
        "active-live",
        "active-bad",
        "active-throw",
        "markets-ok",
        "markets-bad",
        "markets-throw",
        "tickers-ok",
        "tickers-bad",
        "tickers-throw",
        "cancel-ok",
        "cancel-append",
        "cancel-pending",
        "cancel-bad",
        "cancel-throw",
        "cancel-symbol-ok",
        "cancel-symbol-bad",
        "cancel-all-ok",
        "cancel-all-bad",
        "delegate-candle-external",
        "delegate-candle-socket",
        "delegate-book-external",
        "delegate-book-socket",
    ];
    assert_eq!(cases.len(), 25);
    let outcomes = cases.map(run);
    for (i, outcome) in outcomes.iter().enumerate() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-native-order-reads.cjs"
            ))
            .arg(cases[i])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source {}: {}",
            cases[i],
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(outcome.project(), source, "source floor {i}/{}", cases[i]);
    }
}
