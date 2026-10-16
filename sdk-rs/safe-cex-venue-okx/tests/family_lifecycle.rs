use safe_cex_venue_okx::{
    family_lifecycle::{self as port, LifecycleOps},
    market_account_reads::{MarketAccountOps, ReadAwait},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
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
    P(Result<V, E>),
    Member(V, String),
    Callback(Rc<dyn Fn(Vec<V>) -> Result<V, E>>),
    Task(usize),
    Map(RefCell<Vec<(V, V)>>),
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
        _ => panic!("pack"),
    }
}
fn unused<T>() -> T {
    panic!("unused capability in this source family")
}
struct Job {
    future: RefCell<Option<ReadAwait<V, E>>>,
    result: RefCell<Option<Result<V, E>>>,
}
struct State {
    case: String,
    api: V,
    err: E,
    receiver: V,
    store: V,
    markets: V,
    tickers: V,
    positions: V,
    orders: V,
    balance: V,
    events: RefCell<Vec<Value>>,
    native_calls: RefCell<Vec<(String, Vec<V>)>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    timer: RefCell<Option<V>>,
}
fn event(t: &State, name: &str, args: Vec<V>) -> Result<(), E> {
    t.events.borrow_mut().push(json!(
        std::iter::once(json!(name))
            .chain(args.iter().map(pack))
            .collect::<Vec<_>>()
    ));
    t.native_calls.borrow_mut().push((name.into(), args));
    if t.case == format!("fail:{name}") {
        Err(t.err.clone())
    } else {
        Ok(())
    }
}
fn promise(value: Result<V, E>) -> V {
    Rc::new(Native::P(value))
}
fn fetch(t: &State, name: &str, value: V) -> Result<V, E> {
    event(t, name, vec![])?;
    if t.case == format!("dispose:{name}") {
        put(&t.receiver, "isDisposed", b(true));
    }
    if t.case == "flip-public" && name == "markets" {
        put(&t.receiver, "isPublicDataOnly", b(true));
    }
    Ok(promise(if t.case == format!("reject:{name}") {
        Err(t.err.clone())
    } else {
        Ok(value)
    }))
}
fn await_value(value: V, t: Rc<State>) -> ReadAwait<V, E> {
    Box::pin(async move {
        match value.as_ref() {
            Native::P(r) => r.clone(),
            Native::Task(i) => {
                poll_fn(|_| {
                    let j = t.jobs.borrow()[*i].clone();
                    if let Some(r) = j.result.borrow().clone() {
                        Poll::Ready(r)
                    } else {
                        Poll::Pending
                    }
                })
                .await
            }
            _ => Ok(value.clone()),
        }
    })
}
fn spawn(t: &State, f: ReadAwait<V, E>) -> V {
    let mut jobs = t.jobs.borrow_mut();
    let id = jobs.len();
    jobs.push(Rc::new(Job {
        future: RefCell::new(Some(f)),
        result: RefCell::new(None),
    }));
    Rc::new(Native::Task(id))
}
fn run<F: Future>(f: F, t: &State) -> F::Output {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..50 {
        let result = f.as_mut().poll(&mut cx);
        let jobs = t.jobs.borrow().clone();
        for j in jobs {
            let mut future = j.future.borrow_mut();
            if let Some(f) = future.as_mut() {
                if let Poll::Ready(r) = f.as_mut().poll(&mut cx) {
                    *j.result.borrow_mut() = Some(r);
                    *future = None;
                }
            }
        }
        if let Poll::Ready(r) = result {
            return r;
        }
    }
    panic!("bounded native scheduling must settle")
}
fn invoke(t: Rc<State>, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Callback(f) = callee.as_ref() {
        return f(args);
    }
    if let Native::Member(receiver, key) = callee.as_ref() {
        assert_eq!(key, "get");
        let Native::Map(rows) = receiver.as_ref() else {
            panic!("map")
        };
        return Ok(rows
            .borrow()
            .iter()
            .find(|(k, _)| equal(k, &args[0]))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u));
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("callable")
    };
    Ok(match name.as_str() {
        "super" => {
            event(&t, "super", args)?;
            t.receiver.clone()
        }
        "super:OKXExchange" | "super:OKXEUExchange" => {
            assert!(
                matches!(get(&args[1], "testnet").as_ref(), Native::B(true)),
                "DEMO native forced testnet"
            );
            put(&t.receiver, "opts", args[1].clone());
            event(
                &t,
                if name.ends_with("OKXEUExchange") {
                    "eu-base"
                } else {
                    "global-base"
                },
                args,
            )?;
            t.receiver.clone()
        }
        "this.createXhrWithSecondaryKey" => {
            assert!(
                Rc::ptr_eq(&args[1], &t.api),
                "CONSTRUCTOR native original API factory"
            );
            event(&t, "secondary", vec![args[0].clone()])?;
            object(vec![("tag", s("xhr"))])
        }
        "this.initExternalPublicData" => fetch(&t, "external", b(t.case == "external"))?,
        "this.fetchMarkets" => fetch(
            &t,
            "markets",
            if t.case == "empty" {
                array(vec![])
            } else {
                t.markets.clone()
            },
        )?,
        "this.fetchTickersRaw" => fetch(&t, "raw", object(vec![("raw", b(true))]))?,
        "this.parseTickers" => {
            event(&t, "parse", args)?;
            t.tickers.clone()
        }
        "this.loadAccountMarketOverlay" => {
            event(&t, "overlay", vec![])?;
            let result = invoke(t.clone(), args[0].clone(), vec![])?;
            let h = t.clone();
            spawn(
                &t,
                Box::pin(async move {
                    await_value(result, h.clone()).await?;
                    if h.case == "overlay-dispose" {
                        put(&h.receiver, "isDisposed", b(true));
                    }
                    Ok(u())
                }),
            )
        }
        "this.fetchAccountMarketOverlay" => fetch(&t, "accountOverlay", u())?,
        "this.getOrFetchPositionMode" => fetch(&t, "mode", u())?,
        "this.fetchBalanceAndPositions" => fetch(
            &t,
            "balancePositions",
            object(vec![
                ("balance", t.balance.clone()),
                ("positions", t.positions.clone()),
            ]),
        )?,
        "this.fetchOrders" => fetch(&t, "orders", t.orders.clone())?,
        "this.publicWebsocket.connectAndSubscribe" => {
            event(&t, "connectPublic", args)?;
            u()
        }
        "this.businessWebsocket.connectAndSubscribe" => {
            event(&t, "connectBusiness", args)?;
            u()
        }
        "this.privateWebsocket.connectAndSubscribe" => {
            event(&t, "connectPrivate", args)?;
            u()
        }
        "this.log" => {
            event(&t, "log", args)?;
            u()
        }
        "this.tick" => {
            event(&t, "tick", args)?;
            promise(Ok(u()))
        }
        "this.store.update" => {
            event(&t, "update", args.clone())?;
            let Native::O(fields) = args[0].as_ref() else {
                panic!("payload")
            };
            for (k, v) in fields.borrow().iter() {
                put(&t.store, k, v.clone())
            }
            u()
        }
        "this.formatBalanceUsedAvail" => {
            assert!(
                Rc::ptr_eq(&args[0], &get(&t.balance, "info")),
                "TICK native original balance info"
            );
            if t.case != "fallback" {
                assert!(
                    Rc::ptr_eq(&args[1], &t.positions),
                    "TICK native original positions"
                );
                assert!(
                    Rc::ptr_eq(&args[2], &t.orders),
                    "TICK native original orders"
                );
            } else {
                assert_eq!(items(&args[1]).len(), 0, "TICK native fallback positions");
                assert_eq!(items(&args[2]).len(), 0, "TICK native fallback orders");
            }
            event(&t, "format", args)?;
            fetch(
                &t,
                "formatted",
                object(vec![("info", object(vec![("new", b(true))]))]),
            )?
        }
        "this.getTickInterval" => {
            event(&t, "interval", args)?;
            n(-0.)
        }
        "loop" => {
            event(&t, "loop", vec![args[1].clone()])?;
            *t.timer.borrow_mut() = Some(args[0].clone());
            u()
        }
        "Math.min" => n(num(&args[0]).min(num(&args[1]))),
        "Promise.all" => {
            let values = items(&args[0]);
            let h = t.clone();
            spawn(
                &t,
                Box::pin(async move {
                    let mut out = Vec::new();
                    for v in values {
                        out.push(await_value(v, h.clone()).await?)
                    }
                    Ok(array(out))
                }),
            )
        }
        other => panic!("unimplemented callable {other}"),
    })
}
fn host(t: Rc<State>) -> Rc<LifecycleOps<V, E>> {
    let get_t = t.clone();
    let invoke_t = t.clone();
    let intrinsic_t = t.clone();
    let await_t = t.clone();
    let spawn_t = t.clone();
    let value = MarketAccountOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            Ok(match name {
                "this" => get_t.receiver.clone(),
                "this.store.loaded" => get(&get_t.store, "loaded"),
                "this.store.balance.info" => get(&get(&get_t.store, "balance"), "info"),
                "this.store.positions" => get(&get_t.store, "positions"),
                "this.store.orders" => get(&get_t.store, "orders"),
                "this.isDisposed" => get(&get_t.receiver, "isDisposed"),
                "this.isPublicDataOnly" => get(&get_t.receiver, "isPublicDataOnly"),
                "this.needsAccountMarketOverlay" => {
                    get(&get_t.receiver, "needsAccountMarketOverlay")
                }
                "createAPI" => get_t.api.clone(),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        invoke: Rc::new(move |f, args| invoke(invoke_t.clone(), f, args)),
        intrinsic: Rc::new(move |name, args| {
            Ok(match name {
                "define-literal-field" => {
                    put(&args[0], &text(&args[1]), args[2].clone());
                    u()
                }
                "object-spread" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("spread")
                    };
                    Rc::new(Native::O(RefCell::new(fields.borrow().clone())))
                }
                "initialize-okx-fields" => {
                    event(&intrinsic_t, "fields", vec![])?;
                    u()
                }
                "construct" => {
                    let Native::Call(name) = args[0].as_ref() else {
                        panic!("constructor")
                    };
                    if name == "Map" {
                        let mut rows: Vec<(V, V)> = Vec::new();
                        for pair in items(&args[1]) {
                            let pair = items(&pair);
                            if let Some((_, v)) = rows.iter_mut().find(|(k, _)| equal(k, &pair[0]))
                            {
                                *v = pair[1].clone()
                            } else {
                                rows.push((pair[0].clone(), pair[1].clone()))
                            }
                        }
                        Rc::new(Native::Map(RefCell::new(rows)))
                    } else {
                        assert!(
                            Rc::ptr_eq(&args[1], &intrinsic_t.receiver),
                            "CONSTRUCTOR native socket parent identity"
                        );
                        let tag = match name.as_str() {
                            "OKXPublicWebsocket" => "public",
                            "OKXPrivateWebsocket" => "private",
                            "OKXBusinessWebsocket" => "business",
                            _ => panic!("socket class"),
                        };
                        event(&intrinsic_t, tag, vec![])?;
                        object(vec![("tag", s(tag))])
                    }
                }
                "null" => Rc::new(Native::Null),
                "loaded-okx-message" => s(&format!("Loaded {} OKX markets", text(&args[0]))),
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
        flat_map: Rc::new(|v, f| {
            let mut out = vec![];
            for v in items(&v) {
                out.extend(items(&f(v)?));
            }
            Ok(array(out))
        }),
        await_value: Rc::new(move |v| await_value(v, await_t.clone())),
        bind_first: Rc::new(|_, _| unused()),
        filter: Rc::new(|_, _| unused()),
        reduce: Rc::new(|_, _, _| unused()),
        for_of: Rc::new(|_, _| unused()),
        sum_by: Rc::new(|_, _, _| unused()),
        parallel_map: Rc::new(|_, _, _| unused()),
        some: Rc::new(|_, _| unused()),
        error_value: Rc::new(|_| unused()),
        new_error: Rc::new(|_, _| unused()),
    };
    Rc::new(LifecycleOps {
        value: Rc::new(value),
        spawn: Rc::new(move |f| Ok(spawn(&spawn_t, f))),
        callback: Rc::new(|f| Rc::new(Native::Callback(f))),
    })
}
fn fixture(method: &str, case: &str) -> Value {
    let positions = array(vec![object(vec![("id", s("P"))])]);
    let orders = array(vec![object(vec![("id", s("O"))])]);
    let balance = object(vec![("info", object(vec![("retained", b(true))]))]);
    let store = object(vec![
        ("loaded", object(vec![("old", b(true))])),
        ("balance", balance.clone()),
        (
            "positions",
            if case == "fallback" {
                Rc::new(Native::Null)
            } else {
                positions.clone()
            },
        ),
        (
            "orders",
            if case == "fallback" {
                Rc::new(Native::Null)
            } else {
                orders.clone()
            },
        ),
    ]);
    let receiver = object(vec![
        ("store", store.clone()),
        ("isDisposed", b(case == "disposed")),
        ("isPublicDataOnly", b(case == "public")),
        (
            "needsAccountMarketOverlay",
            b(case == "overlay" || case == "overlay-dispose"),
        ),
    ]);
    let t = Rc::new(State {
        case: case.into(),
        api: Rc::new(Native::Call("createAPI".into())),
        err: Rc::new(Error {
            name: "TypeError",
            message: "boom".into(),
            fresh: false,
            code: Some("LIFECYCLE"),
            ids: Some([4, 9]),
        }),
        receiver,
        store,
        positions,
        orders,
        balance,
        markets: array(vec![
            object(vec![
                ("id", s("A")),
                ("limits", object(vec![("max", n(7.))])),
            ]),
            object(vec![
                ("id", s("A")),
                ("limits", object(vec![("max", n(9.))])),
            ]),
            object(vec![
                ("id", s("B")),
                ("limits", object(vec![("max", n(11.))])),
            ]),
        ]),
        tickers: array(vec![object(vec![("id", s("T"))])]),
        events: RefCell::new(vec![]),
        native_calls: RefCell::new(vec![]),
        jobs: RefCell::new(vec![]),
        timer: RefCell::new(None),
    });
    let h = host(t.clone());
    let opts = object(vec![
        ("testnet", b(case == "true")),
        ("secret", Rc::new(Native::S(vec![0xd800]))),
    ]);
    let store_arg = object(vec![("tag", s("store"))]);
    let result: Result<V, E> = match method {
        "constructor" => port::new_exchange(
            &h,
            s(if case.starts_with("eu") {
                "okxeudemo"
            } else if case == "unknown" {
                "future-okx"
            } else {
                "okx"
            }),
            opts.clone(),
            store_arg,
        )
        .map(|state| {
            assert!(
                Rc::ptr_eq(&state.receiver, &t.receiver),
                "CONSTRUCTOR native original receiver"
            );
            assert!(
                equal(
                    &state.region,
                    &s(if case.starts_with("eu") {
                        "okxeu"
                    } else {
                        "okx"
                    })
                ),
                "CONSTRUCTOR native region selection"
            );
            object(vec![
                ("region", state.region),
                ("xhr", state.xhr),
                ("public", state.public_websocket),
                ("private", state.private_websocket),
                ("business", state.business_websocket),
            ])
        }),
        "demo" => port::new_demo_exchange(&h, s("name"), opts.clone(), store_arg).map(|state| {
            assert!(
                matches!(state.is_demo.as_ref(), Native::B(true)),
                "DEMO native class flag"
            );
            object(vec![
                ("demo", state.is_demo),
                ("opts", get(&state.base, "opts")),
            ])
        }),
        "eu-demo" => {
            port::new_eu_demo_exchange(&h, s("name"), opts.clone(), store_arg).map(|state| {
                assert!(
                    matches!(state.is_demo.as_ref(), Native::B(true)),
                    "DEMO native EU class flag"
                );
                object(vec![
                    ("demo", state.is_demo),
                    ("opts", get(&state.base.base, "opts")),
                ])
            })
        }
        "start" => run(port::start(h.clone()), &t),
        "tick" => run(port::tick(h.clone()), &t).and_then(|v| {
            if case == "timer" {
                let cb = t.timer.borrow().as_ref().unwrap().clone();
                invoke(t.clone(), cb, vec![])?;
                assert_eq!(
                    t.native_calls.borrow().last().unwrap().0,
                    "tick",
                    "TICK native retained callback resolves current member"
                );
            }
            Ok(v)
        }),
        "overlay" => run(port::fetch_eu_account_market_overlay(h.clone()), &t).and_then(|f| {
            if matches!(f.as_ref(), Native::Null) {
                return Ok(f);
            }
            let original = array(vec![
                object(vec![
                    ("id", s("B")),
                    ("limits", object(vec![("old", b(true))])),
                    ("shared", b(true)),
                ]),
                object(vec![("id", s("X"))]),
                object(vec![
                    ("id", s("A")),
                    ("limits", object(vec![("old", b(true))])),
                ]),
            ]);
            let a = invoke(t.clone(), f.clone(), vec![original.clone()])?;
            let rows = items(&a);
            let accounts = items(&t.markets);
            assert_eq!(rows.len(), 2, "OVERLAY native selected row count");
            assert!(
                equal(&get(&rows[0], "id"), &s("B")),
                "OVERLAY native shared ordering"
            );
            assert!(
                Rc::ptr_eq(&get(&rows[0], "limits"), &get(&accounts[2], "limits")),
                "OVERLAY native original limits identity"
            );
            assert!(
                Rc::ptr_eq(&get(&rows[1], "limits"), &get(&accounts[1], "limits")),
                "OVERLAY native last duplicate wins"
            );
            assert!(
                !Rc::ptr_eq(&rows[0], &items(&original)[0]),
                "OVERLAY native fresh row"
            );
            put(&accounts[1], "limits", object(vec![("max", n(13.))]));
            let second = invoke(t.clone(), f, vec![original])?;
            assert_eq!(
                num(&get(&get(&items(&second)[1], "limits"), "max")),
                13.,
                "OVERLAY native retained live account row"
            );
            Ok(array(vec![a, second]))
        }),
        _ => panic!("method"),
    };
    assert!(
        equal(&get(&opts, "testnet"), &b(case == "true")),
        "DEMO native original options unchanged"
    );
    let (failed, result) = match result {
        Ok(v) => (false, v),
        Err(e) => {
            assert!(
                Rc::ptr_eq(&e, &t.err),
                "LIFECYCLE native original rejection identity"
            );
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "boom");
            assert_eq!(e.code, Some("LIFECYCLE"));
            assert_eq!(e.ids, Some([4, 9]));
            assert!(!e.fresh);
            (true, u())
        }
    };
    if method == "start" && !failed {
        let calls = t.native_calls.borrow();
        let names = calls.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>();
        if case == "normal" {
            assert_eq!(
                names,
                [
                    "external",
                    "markets",
                    "raw",
                    "update",
                    "parse",
                    "update",
                    "log",
                    "connectPublic",
                    "connectBusiness",
                    "mode",
                    "connectPrivate",
                    "balancePositions",
                    "orders",
                    "update",
                    "update",
                    "log",
                    "tick"
                ],
                "START native ordered load and connection effects"
            );
            assert!(
                Rc::ptr_eq(&get(&t.store, "markets"), &t.markets),
                "START native original markets"
            );
            assert!(
                Rc::ptr_eq(&get(&t.store, "positions"), &t.positions),
                "START native original positions"
            );
        }
        if case == "flip-public" {
            assert!(
                names.contains(&"connectPrivate"),
                "START native public-only captured before fetch"
            );
        }
        if case == "disposed" || case == "dispose:markets" || case == "overlay-dispose" {
            assert!(
                !names.contains(&"connectPublic"),
                "START native disposal prevents public connection"
            );
        }
        if case == "external" {
            assert!(
                !names.contains(&"markets"),
                "START native external skips public reads"
            );
        }
    }
    if method == "tick" && !failed && case != "disposed" {
        let timer = t.timer.borrow();
        assert!(timer.is_some(), "TICK native retained timer callback");
        let calls = t.native_calls.borrow();
        let (_, args) = calls.iter().find(|(n, _)| n == "loop").unwrap();
        assert_eq!(
            num(&args[0]).to_bits(),
            (-0f64).to_bits(),
            "TICK native negative-zero interval"
        );
    }
    json!({"method":method,"case":case,"events":t.events.borrow().clone(),"failed":failed,"result":pack(&result)})
}
fn check(method: &str, cases: &[&str]) {
    let cases = cases.iter().map(|c| fixture(method, c)).collect::<Vec<_>>();
    let expected = match method {
        "constructor" => 9,
        "demo" | "eu-demo" | "overlay" => 3,
        "start" => 20,
        "tick" => 8,
        _ => panic!("unlisted family"),
    };
    assert_eq!(cases.len(), expected, "native independent fixture floor");
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/family-lifecycle-oracle.mjs"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(json!({"cases":cases.iter().map(|c|json!({"method":c["method"],"case":c["case"]})).collect::<Vec<_>>()}).to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        cases.len(),
        "source independent case floor"
    );
    for (i, (native, source)) in cases.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(native, source, "maintained source {method}/{i}");
    }
}
#[test]
fn constructor_native_before_source() {
    check(
        "constructor",
        &[
            "normal",
            "eu",
            "unknown",
            "fail:super",
            "fail:fields",
            "fail:secondary",
            "fail:public",
            "fail:private",
            "fail:business",
        ],
    )
}
#[test]
fn demo_native_before_source() {
    check("demo", &["normal", "true", "fail:global-base"]);
    check("eu-demo", &["normal", "true", "fail:eu-base"])
}
#[test]
fn start_native_before_source() {
    check(
        "start",
        &[
            "normal",
            "external",
            "public",
            "disposed",
            "dispose:markets",
            "overlay",
            "overlay-dispose",
            "flip-public",
            "reject:external",
            "reject:markets",
            "reject:raw",
            "reject:mode",
            "reject:balancePositions",
            "reject:orders",
            "fail:parse",
            "fail:update",
            "fail:connectPublic",
            "fail:connectBusiness",
            "fail:connectPrivate",
            "fail:tick",
        ],
    )
}
#[test]
fn tick_native_before_source() {
    check(
        "tick",
        &[
            "normal",
            "disposed",
            "fallback",
            "timer",
            "reject:formatted",
            "fail:update",
            "fail:interval",
            "fail:loop",
        ],
    )
}
#[test]
fn overlay_native_before_source() {
    check("overlay", &["normal", "empty", "reject:markets"])
}
