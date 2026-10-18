use safe_cex_venue_okx::{
    market_account_reads::{MarketAccountOps, ReadAwait},
    order_mutations::{self as port, OrderMutationOps},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
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
    S(String),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    Call(String),
    P(Result<V, E>),
    Member(V, String),
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
        Native::Null => "null".into(),
        Native::B(x) => x.to_string(),
        _ => panic!("primitive"),
    }
}
fn num(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::S(x) => safe_cex_num::number_from_string(x),
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
        Native::S(x) => json!(x),
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
struct State {
    method: &'static str,
    case: &'static str,
    err: E,
    order: V,
    opts: V,
    market: V,
    markets: V,
    orders: V,
    positions: V,
    refreshed: V,
    receiver: V,
    requests: RefCell<Vec<(String, V)>>,
    dispatches: RefCell<Vec<(String, V)>>,
    stores: RefCell<Vec<(String, V)>>,
    events: RefCell<Vec<Value>>,
    captures: RefCell<Vec<String>>,
    messages: RefCell<Vec<V>>,
    reads: RefCell<Vec<&'static str>>,
    fresh: Cell<usize>,
    fetches: Cell<usize>,
    finds: Cell<usize>,
    changed_min: Cell<bool>,
}
fn promise(x: Result<V, E>) -> V {
    Rc::new(Native::P(x))
}
fn await_ready(value: V) -> ReadAwait<V, E> {
    Box::pin(async move {
        match value.as_ref() {
            Native::P(r) => r.clone(),
            _ => Ok(value.clone()),
        }
    })
}
fn run<F: std::future::Future>(future: F) -> F::Output {
    let mut f = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture scheduling contract requires a ready future"),
    }
}
fn event(t: &State, v: Value) {
    t.events.borrow_mut().push(v);
}
fn string_number(v: &V) -> V {
    s(&text(v))
}
fn fresh_error(t: &State, message: V) -> E {
    t.fresh.set(t.fresh.get() + 1);
    event(t, json!(["newError", pack(&message)]));
    Rc::new(Error {
        name: "Error",
        message: if matches!(message.as_ref(), Native::U) {
            String::new()
        } else {
            text(&message)
        },
        fresh: true,
        code: None,
        ids: None,
    })
}
fn emit_message(t: &State, value: V) {
    t.messages.borrow_mut().push(value.clone());
    event(t, json!(["emit", pack(&value)]));
}
fn is_algo(v: &V) -> bool {
    matches!(
        text(v).as_str(),
        "conditional" | "stop_market" | "take_profit_market" | "trailing_stop_market"
    )
}
fn strip(v: &V) -> V {
    let id = text(v);
    let root = id
        .rsplit_once('_')
        .filter(|(_, tail)| !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphabetic()))
        .map_or(id.as_str(), |(head, _)| head);
    s(root)
}
fn invoke(t: &State, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Member(receiver, key) = callee.as_ref() {
        assert_eq!(key, "startsWith");
        return Ok(b(text(receiver).starts_with(&text(&args[0]))));
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("captured callable")
    };
    let out = match name.as_str() {
        "adjust" => n(safe_cex_num::adjust(num(&args[0]), num(&args[1]))),
        "Number.isFinite" => b(matches!(args[0].as_ref(),Native::N(x)if x.is_finite())),
        "this.toFiniteNumber" => {
            let x = num(&args[0]);
            n(if x.is_finite() { x } else { 0. })
        }
        "getOKXTriggerPxType" => s(match text(&args[0]).as_str() {
            "mark_price" => "mark",
            "index_price" => "index",
            _ => "last",
        }),
        "Math.max" => {
            let x = num(&args[0]).max(num(&args[1]));
            if t.case == "callee-reenter" {
                t.changed_min.set(true);
                put(&get(&get(&t.market, "limits"), "leverage"), "max", n(7.));
            }
            n(x)
        }
        "Math.min" => n(num(&args[0]).min(num(&args[1]))),
        "Math.min.changed" => n(777.),
        "chunk" => {
            let size = num(&args[1]) as usize;
            array(
                items(&args[0])
                    .chunks(size)
                    .map(|x| array(x.to_vec()))
                    .collect(),
            )
        }
        "this.isAlgoOrder" => b(is_algo(&args[0])),
        "this.emitAndLogError" => {
            emit_message(t, args[0].clone());
            u()
        }
        "this.updateAlgoOrder" | "this.updateTriggerAlgoOrder" => {
            assert!(
                Rc::ptr_eq(&get(&args[0], "order"), &t.order),
                "AMEND native original delegated order"
            );
            assert!(
                Rc::ptr_eq(&get(&args[0], "update"), &get(&t.opts, "update")),
                "AMEND native original delegated update"
            );
            t.dispatches
                .borrow_mut()
                .push((name.clone(), args[0].clone()));
            event(t, json!([name, pack(&args[0])]));
            promise(Ok(array(vec![s(if name.ends_with("TriggerAlgoOrder") {
                "TRIGGER"
            } else {
                "ALGO"
            })])))
        }
        "this.cancelAlgoOrders" | "this.cancelNormalOrders" | "this.cancelOrders" => {
            t.dispatches
                .borrow_mut()
                .push((name.clone(), args[0].clone()));
            event(t, json!([name, pack(&args[0])]));
            promise(
                if t.case == "cancel-reject"
                    || t.case == "normal-reject" && name.ends_with("NormalOrders")
                    || t.method == "symbol" && t.case == "reject"
                {
                    Err(t.err.clone())
                } else {
                    Ok(u())
                },
            )
        }
        "this.placeOrder" => {
            t.dispatches
                .borrow_mut()
                .push((name.clone(), args[0].clone()));
            event(t, json!([name, pack(&args[0])]));
            promise(if t.case == "place-reject" {
                Err(t.err.clone())
            } else {
                Ok(array(vec![s("PLACED")]))
            })
        }
        "this.fetchPositions" => {
            t.fetches.set(t.fetches.get() + 1);
            event(t, json!(["fetch"]));
            promise(
                if t.case == "recovery-reject" || t.case == "refresh-reject" && t.fetches.get() == 1
                {
                    Err(t.err.clone())
                } else {
                    Ok(t.refreshed.clone())
                },
            )
        }
        "this.store.update" | "this.store.addOrUpdatePositions" => {
            let positions = if name.ends_with("update") {
                get(&args[0], "positions")
            } else {
                args[0].clone()
            };
            assert!(
                Rc::ptr_eq(&positions, &t.refreshed),
                "RISK native original refreshed positions"
            );
            t.stores.borrow_mut().push((name.clone(), positions));
            event(t, json!([name, pack(&args[0])]));
            u()
        }
        x if x.starts_with("this.xhr.post:ENDPOINTS.") => {
            let route = x.split('.').next_back().unwrap().to_owned();
            t.requests
                .borrow_mut()
                .push((route.clone(), args[0].clone()));
            event(t, json!(["post", route, pack(&args[0])]));
            if t.case == "reject" || t.case == "late-error" || t.case == "recovery-reject" {
                return Ok(promise(Err(t.err.clone())));
            }
            let body = if t.method == "trigger" {
                if t.case == "empty-body" {
                    u()
                } else {
                    object(vec![
                        (
                            "code",
                            s(if t.case == "code-error" { "51000" } else { "0" }),
                        ),
                        ("msg", s(if t.case == "code-error" { "CODE" } else { "" })),
                        (
                            "data",
                            array(vec![object(vec![
                                (
                                    "sCode",
                                    s(if t.case == "row-error" { "51001" } else { "0" }),
                                ),
                                ("sMsg", s(if t.case == "row-error" { "ROW" } else { "" })),
                            ])]),
                        ),
                    ])
                }
            } else if t.method == "mode" {
                object(vec![(
                    "msg",
                    s(if t.case == "msg-error" { "DENIED" } else { "" }),
                )])
            } else if t.method == "normal" || t.method == "algo-cancel" {
                object(vec![(
                    "data",
                    array(vec![
                        object(vec![("sMsg", s(""))]),
                        object(vec![("sMsg", s("WARN"))]),
                    ]),
                )])
            } else {
                object(vec![])
            };
            if t.method == "trigger" && t.case == "raw-predicate" {
                put(
                    &body,
                    "data",
                    object(vec![
                        ("marker", s("custom-find")),
                        (
                            "row",
                            object(vec![("sCode", s("0")), ("sMsg", s("ROW-RAW"))]),
                        ),
                    ]),
                );
            }
            promise(Ok(object(vec![("data", body)])))
        }
        _ => panic!("unhandled callable {name}"),
    };
    Ok(out)
}
fn intrinsic(t: &State, key: &str, args: Vec<V>) -> Result<V, E> {
    Ok(match key {
        "get-index" => get(&args[0], &text(&args[1])),
        "template-string" => string_number(&args[0]),
        "unary-plus" => n(num(&args[0])),
        "less-or-equal" => b(num(&args[0]) <= num(&args[1])),
        "greater-than" => b(num(&args[0]) > num(&args[1])),
        "replace-order-id-suffix" => strip(&args[0]),
        "market-not-found-message" => s(&format!("Market {} not found on OKX", text(&args[0]))),
        "position-not-found-message" => s(&format!("Position {} not found on OKX", text(&args[0]))),
        "object-spread" => {
            let Native::O(rows) = args[0].as_ref() else {
                panic!("spread")
            };
            Rc::new(Native::O(RefCell::new(rows.borrow().clone())))
        }
        "define-literal-field" => {
            put(&args[0], &text(&args[1]), args[2].clone());
            u()
        }
        "destructure-two" => array(items(&args[0]).into_iter().take(2).collect()),
        "array-spread-prefix" => {
            t.reads.borrow_mut().push("spread");
            array(items(&args[0]))
        }
        "array-prefix-append" => {
            let mut prefix = items(&args[0]);
            prefix.push(args[1].clone());
            array(prefix)
        }
        _ => panic!("unhandled language expression {key}"),
    })
}
fn ops(t: Rc<State>) -> Rc<OrderMutationOps<V, E>> {
    let read = t.clone();
    let write = t.clone();
    let capture = t.clone();
    let inv = t.clone();
    let expr = t.clone();
    let err = t.clone();
    let ctor = t.clone();
    let value = Rc::new(MarketAccountOps {
        get: Rc::new(move |v, k| {
            if read.case == "guard-error" && k == "contracts" {
                return Err(read.err.clone());
            }
            if (read.method == "normal" || read.method == "algo-cancel") && k == "id" {
                read.reads
                    .borrow_mut()
                    .push(if Rc::ptr_eq(v, &read.market) {
                        "instId"
                    } else {
                        "orderId"
                    });
            }
            Ok(get(v, k))
        }),
        set: Rc::new(move |v, k, x| {
            if Rc::ptr_eq(v, &write.receiver) && k == "_accountPositionMode" {
                event(&write, json!(["mode", pack(&x)]));
            }
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            capture.captures.borrow_mut().push(name.into());
            Ok(match name {
                "this.store.marketsBySymbol" => capture.markets.clone(),
                "this.store.orders" => capture.orders.clone(),
                "this.store.positions" => capture.positions.clone(),
                "this" => capture.receiver.clone(),
                "OrderType.Limit" => s("limit"),
                "OrderType.Conditional" => s("conditional"),
                "PositionMode.OneWay" => s("oneway"),
                "PositionMode.Hedged" => s("hedged"),
                "Math.min" if capture.changed_min.get() => {
                    Rc::new(Native::Call("Math.min.changed".into()))
                }
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        invoke: Rc::new(move |c, a| invoke(&inv, c, a)),
        intrinsic: Rc::new(move |k, a| intrinsic(&expr, k, a)),
        truthy: Rc::new(truth),
        nullish: Rc::new(null),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        boolean: Rc::new(b),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        map: Rc::new(|a, f| {
            items(&a)
                .into_iter()
                .map(|x| f(x))
                .collect::<Result<Vec<_>, _>>()
                .map(array)
        }),
        bind_first: Rc::new(|a, f| f(items(&a).first().cloned().unwrap_or_else(u))),
        flat_map: Rc::new(|_, _| unused()),
        filter: Rc::new(|_, _| unused()),
        reduce: Rc::new(|a, init, f| items(&a).into_iter().try_fold(init, |acc, x| f(acc, x))),
        for_of: Rc::new(|_, _| unused()),
        sum_by: Rc::new(|_, _, _| unused()),
        parallel_map: Rc::new(|_, _, _| unused()),
        some: Rc::new(|_, _| unused()),
        await_value: Rc::new(await_ready),
        error_value: Rc::new(move |e| {
            object(vec![(
                "message",
                s(
                    if err.case == "late-error" && !err.messages.borrow().is_empty() {
                        "AFTER"
                    } else {
                        &e.message
                    },
                ),
            )])
        }),
        new_error: Rc::new(move |callee, msg| {
            assert!(
                matches!(callee.as_ref(),Native::Call(x)if x=="Error"),
                "ERROR native captured constructor"
            );
            Ok(fresh_error(&ctor, msg))
        }),
    });
    let find = t.clone();
    Rc::new(OrderMutationOps {
        is_terminal_order: Rc::new(|order| {
            let status = match get(order, "status").as_ref() {
                Native::S(status) => {
                    serde_json::from_value(serde_json::Value::String(status.clone())).ok()
                }
                _ => None,
            };
            Ok(safe_cex_store::store::is_terminal_order_status(status))
        }),
        value,
        has: Rc::new(|key, v| {
            let Native::O(fields) = v.as_ref() else {
                panic!("in object")
            };
            Ok(fields.borrow().iter().any(|(k, _)| k == key))
        }),
        filter: Rc::new(|a, f| {
            let mut out = Vec::new();
            for x in items(&a) {
                if truth(&f(x.clone())?) {
                    out.push(x);
                }
            }
            Ok(array(out))
        }),
        find: Rc::new(move |a, f| {
            find.finds.set(find.finds.get() + 1);
            for x in items(&a) {
                if truth(&f(x.clone())?) {
                    return Ok(x);
                }
            }
            Ok(u())
        }),
        optional_find: Rc::new(|a, f| {
            if matches!(get(&a,"marker").as_ref(), Native::S(x) if x=="custom-find") {
                let row = get(&a, "row");
                let returned = f(row.clone())?;
                assert_s(
                    &returned,
                    "ROW-RAW",
                    "TRIGGER native raw find predicate string",
                );
                return Ok(row);
            }
            if null(&a) {
                return Ok(u());
            }
            for x in items(&a) {
                if truth(&f(x.clone())?) {
                    return Ok(x);
                }
            }
            Ok(u())
        }),
        partition: Rc::new(|c, a, f| {
            assert!(
                matches!(c.as_ref(),Native::Call(x)if x=="partition"),
                "CANCEL native captured partition"
            );
            let (mut yes, mut no) = (Vec::new(), Vec::new());
            for x in items(&a) {
                if truth(&f(x.clone())?) {
                    yes.push(x)
                } else {
                    no.push(x)
                }
            }
            Ok(array(vec![array(yes), array(no)]))
        }),
        for_each: Rc::new(|a, f| {
            for x in items(&a) {
                f(x)?;
            }
            Ok(())
        }),
        series: Rc::new(|callee, a, f| {
            Box::pin(async move {
                assert!(
                    matches!(callee.as_ref(),Native::Call(x)if x=="forEachSeries"),
                    "BATCH native captured serial helper"
                );
                for x in items(&a) {
                    f(x).await?;
                }
                Ok(u())
            })
        }),
        adopt_return: Rc::new(await_ready),
    })
}
fn make_order(id: &str, symbol: &str, kind: &str) -> V {
    object(vec![
        ("id", s(id)),
        ("symbol", s(symbol)),
        ("type", s(kind)),
        ("side", s("sell")),
        ("amount", n(4.)),
        ("price", n(11.)),
        ("reduceOnly", b(true)),
        ("extra", object(vec![("conditionalOrderPrice", n(12.6))])),
        ("triggerType", s("index_price")),
        ("tag", n(29.)),
    ])
}
fn setup(method: &'static str, case: &'static str) -> Rc<State> {
    let market = object(vec![
        ("id", s("INST")),
        (
            "precision",
            object(vec![("amount", n(0.25)), ("price", n(0.5))]),
        ),
        (
            "limits",
            object(vec![(
                "leverage",
                object(vec![("min", n(1.)), ("max", n(9.))]),
            )]),
        ),
    ]);
    let kind = if method == "limit" && case != "delegate" {
        "limit"
    } else if method == "trigger" || case == "conditional" {
        "conditional"
    } else {
        "stop_market"
    };
    let order = make_order("ROOT_tp", "SYM", kind);
    let update = object(vec![
        ("price", n(if case == "invalid" { 0. } else { 17.3 })),
        ("amount", n(if case == "tiny" { 0.01 } else { 2.2 })),
    ]);
    if case == "no-price" {
        let Native::O(f) = update.as_ref() else {
            unreachable!()
        };
        f.borrow_mut().retain(|(k, _)| k != "price");
    }
    if case == "zero-original" {
        put(&order, "amount", s("0"));
    }
    if case == "market-order" {
        put(&order, "extra", Rc::new(Native::Null));
    }
    let opts = object(vec![("order", order.clone()), ("update", update)]);
    let orders = if matches!(method, "normal" | "algo-cancel") {
        let algo = method == "algo-cancel";
        let count = if case == "batch-boundary" {
            if algo { 21 } else { 41 }
        } else {
            2
        };
        let mut rows = (0..count)
            .map(|i| {
                make_order(
                    &format!("O{i}_tp"),
                    if case == "unknown" { "UNKNOWN" } else { "SYM" },
                    if algo { "stop_market" } else { "limit" },
                )
            })
            .collect::<Vec<_>>();
        rows.push(make_order(
            "WRONG",
            "SYM",
            if algo { "limit" } else { "conditional" },
        ));
        array(rows)
    } else if case == "empty" {
        array(vec![])
    } else {
        array(vec![
            make_order("ROOT_sl", "SYM", "stop_market"),
            make_order("N1", "SYM", "limit"),
            make_order("ROOTX_tp", "OTHER", "take_profit_market"),
            make_order("OTHER", "SYM", "stop_market"),
        ])
    };
    let positions = array(vec![object(vec![
        ("symbol", s("SYM")),
        ("leverage", n(if case == "same" { 9. } else { 3. })),
        ("contracts", n(if case == "open" { 1. } else { 0. })),
    ])]);
    Rc::new(State {
        method,
        case,
        err: Rc::new(Error {
            name: "TypeError",
            message: "DOWN".into(),
            fresh: false,
            code: Some("WRITE"),
            ids: Some([8, 6]),
        }),
        order,
        opts,
        market: market.clone(),
        markets: if case == "missing" || case == "missing-market" {
            object(vec![])
        } else {
            object(vec![("SYM", market)])
        },
        orders,
        positions,
        refreshed: array(vec![object(vec![
            ("id", s("REFRESHED")),
            ("contracts", n(0.)),
        ])]),
        receiver: object(vec![("_accountPositionMode", s("oneway"))]),
        requests: RefCell::new(vec![]),
        dispatches: RefCell::new(vec![]),
        stores: RefCell::new(vec![]),
        events: RefCell::new(vec![]),
        captures: RefCell::new(vec![]),
        messages: RefCell::new(vec![]),
        reads: RefCell::new(vec![]),
        fresh: Cell::new(0),
        fetches: Cell::new(0),
        finds: Cell::new(0),
        changed_min: Cell::new(false),
    })
}
fn keys(v: &V) -> Vec<String> {
    let Native::O(rows) = v.as_ref() else {
        panic!("native object")
    };
    rows.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn assert_s(v: &V, expected: &str, message: &str) {
    assert!(
        matches!(v.as_ref(),Native::S(x)if x==expected),
        "{message}: expected native string {expected}"
    );
}
fn expected_error(method: &str, case: &str) -> Option<(bool, &'static str)> {
    match case {
        "missing" | "missing-market" => Some((true, "Market SYM not found on OKX")),
        "invalid" => Some((
            true,
            "Trigger price is required for OKX opening conditional orders",
        )),
        "tiny" if method == "trigger" => Some((true, "Order amount is too small")),
        "code-error" => Some((true, "CODE")),
        "row-error" => Some((true, "ROW")),
        "raw-predicate" => Some((true, "ROW-RAW")),
        "msg-error" => Some((true, "DENIED")),
        "late-error" => Some((true, "AFTER")),
        "reject" if method == "limit" => None,
        "reject" | "refresh-reject" => {
            Some((matches!(method, "trigger" | "leverage" | "mode"), "DOWN"))
        }
        "cancel-reject" | "place-reject" | "normal-reject" | "recovery-reject" | "guard-error" => {
            Some((false, "DOWN"))
        }
        _ => None,
    }
}
fn fixture(method: &'static str, case: &'static str) -> Value {
    let t = setup(method, case);
    let h = ops(t.clone());
    let out = run(async {
        match method {
            "limit" => port::update_order(h, t.opts.clone()).await,
            "algo" => port::update_algo_order(h, t.opts.clone()).await,
            "trigger" => port::update_trigger_algo_order(h, t.opts.clone()).await,
            "dispatch" => {
                port::cancel_orders(
                    h,
                    if case == "algo-only" {
                        array(vec![make_order("A", "SYM", "conditional")])
                    } else {
                        t.orders.clone()
                    },
                )
                .await
            }
            "symbol" => {
                port::cancel_symbol_orders(h, s(if case == "empty" { "NO" } else { "SYM" })).await
            }
            "normal" => port::cancel_normal_orders(h, t.orders.clone()).await,
            "algo-cancel" => port::cancel_algo_orders(h, t.orders.clone()).await,
            "leverage" => port::set_leverage(h, s("SYM"), n(50.)).await,
            "mode" => port::change_position_mode(h, b(case != "oneway")).await,
            _ => unreachable!(),
        }
    });
    let expected = expected_error(method, case);
    match (&out, expected) {
        (Err(error), Some((fresh, message))) => {
            assert_eq!(error.fresh, fresh, "ERROR native freshness");
            assert_eq!(error.message, message, "ERROR native message");
            if fresh {
                assert_eq!(error.name, "Error", "ERROR native fresh name");
                assert_eq!(error.code, None, "ERROR native fresh code absent");
                assert_eq!(error.ids, None, "ERROR native fresh ids absent");
                assert!(
                    !Rc::ptr_eq(error, &t.err),
                    "ERROR native replacement identity"
                );
            } else {
                assert!(
                    Rc::ptr_eq(error, &t.err),
                    "ERROR native original rejection identity"
                );
                assert_eq!(error.name, "TypeError");
                assert_eq!(error.code, Some("WRITE"));
                assert_eq!(error.ids, Some([8, 6]));
            }
        }
        (Ok(_), None) => {}
        _ => panic!("unexpected native result {method}/{case}"),
    }
    if let Ok(value) = &out {
        if matches!(method, "limit" | "algo" | "trigger") {
            let ids = items(value);
            let empty = method == "limit" && matches!(case, "tiny" | "reject");
            assert_eq!(
                ids.len(),
                if empty { 0 } else { 1 },
                "AMEND native independent result count"
            );
            if !empty {
                assert_s(
                    &ids[0],
                    if case == "delegate" {
                        "ALGO"
                    } else if case == "conditional" {
                        "TRIGGER"
                    } else if method == "algo" {
                        "PLACED"
                    } else {
                        "ROOT_tp"
                    },
                    "AMEND native returned id",
                );
            }
        } else {
            assert!(
                matches!(value.as_ref(), Native::U),
                "MUTATION native undefined return"
            );
        }
    }
    let requests = t.requests.borrow();
    match method {
        "limit" | "trigger" => {
            let skipped = matches!(case, "delegate" | "tiny" | "missing" | "invalid");
            assert_eq!(
                requests.len(),
                if skipped { 0 } else { 1 },
                "AMEND native independent request count"
            );
            if let Some((route, payload)) = requests.first() {
                assert_eq!(
                    route,
                    if method == "limit" {
                        "UPDATE_ORDER"
                    } else {
                        "UPDATE_ALGO_ORDER"
                    },
                    "AMEND native endpoint"
                );
                assert_s(&get(payload, "instId"), "INST", "AMEND native instrument");
                if method == "limit" {
                    let expected = if case == "no-price" {
                        vec!["instId", "ordId", "newSz"]
                    } else if case == "zero-original" {
                        vec!["instId", "ordId", "newPx"]
                    } else {
                        vec!["instId", "ordId", "newPx", "newSz"]
                    };
                    assert_eq!(keys(payload), expected, "LIMIT native payload keys");
                    assert_s(
                        &get(payload, "ordId"),
                        "ROOT_tp",
                        "LIMIT native original id",
                    );
                    if case != "no-price" {
                        assert_s(
                            &get(payload, "newPx"),
                            "17.3",
                            "LIMIT native unadjusted price",
                        );
                    }
                    if case != "zero-original" {
                        assert_s(
                            &get(payload, "newSz"),
                            "2.25",
                            "LIMIT native adjusted amount",
                        );
                    }
                } else {
                    assert_eq!(
                        keys(payload),
                        [
                            "instId",
                            "algoId",
                            "newTriggerPx",
                            "newOrdPx",
                            "newTriggerPxType",
                            "newSz"
                        ],
                        "TRIGGER native payload keys"
                    );
                    assert_s(
                        &get(payload, "algoId"),
                        "ROOT",
                        "TRIGGER native stripped id",
                    );
                    assert_s(
                        &get(payload, "newTriggerPx"),
                        "17.5",
                        "TRIGGER native adjusted trigger",
                    );
                    assert_s(
                        &get(payload, "newOrdPx"),
                        if case == "market-order" { "-1" } else { "12.5" },
                        "TRIGGER native conditional limit",
                    );
                    assert_s(
                        &get(payload, "newTriggerPxType"),
                        "index",
                        "TRIGGER native reference type",
                    );
                    assert_s(&get(payload, "newSz"), "2.25", "TRIGGER native amount");
                }
            }
        }
        "algo" => {
            let calls = t.dispatches.borrow();
            let expected = if case == "conditional" {
                vec!["this.updateTriggerAlgoOrder"]
            } else if case == "cancel-reject" {
                vec!["this.cancelAlgoOrders"]
            } else {
                vec!["this.cancelAlgoOrders", "this.placeOrder"]
            };
            assert_eq!(
                calls.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                expected,
                "ALGO native cancel before recreation"
            );
            if case != "conditional" {
                let siblings = items(&calls[0].1);
                assert_eq!(
                    siblings.len(),
                    if case == "empty" { 0 } else { 2 },
                    "ALGO native independent prefix siblings"
                );
                if case != "empty" {
                    assert_s(
                        &get(&siblings[0], "id"),
                        "ROOT_sl",
                        "ALGO native first sibling",
                    );
                    assert_s(
                        &get(&siblings[1], "id"),
                        "ROOTX_tp",
                        "ALGO native broad prefix sibling",
                    );
                }
                if let Some((_, new)) = calls.get(1) {
                    assert_eq!(
                        keys(new),
                        [
                            "id",
                            "symbol",
                            "type",
                            "side",
                            "amount",
                            "price",
                            "reduceOnly",
                            "extra",
                            "triggerType",
                            "tag"
                        ],
                        "ALGO native spread keys"
                    );
                    assert_s(&get(new, "id"), "ROOT_tp", "ALGO native retained spread id");
                    assert_s(&get(new, "side"), "sell", "ALGO native side");
                    assert_eq!(
                        num(&get(new, "amount")),
                        2.2,
                        "ALGO native unadjusted replacement amount"
                    );
                    assert_eq!(
                        num(&get(new, "price")),
                        17.3,
                        "ALGO native unadjusted replacement price"
                    );
                    assert!(
                        Rc::ptr_eq(&get(new, "extra"), &get(&t.order, "extra")),
                        "ALGO native spread nested identity"
                    );
                    assert_eq!(num(&get(new, "tag")), 29., "ALGO native extra spread field");
                }
            }
        }
        "dispatch" => {
            let calls = t.dispatches.borrow();
            let expected = if case == "empty" {
                vec![]
            } else if case == "algo-only" {
                vec!["this.cancelAlgoOrders"]
            } else if case == "normal-reject" {
                vec!["this.cancelNormalOrders"]
            } else {
                vec!["this.cancelNormalOrders", "this.cancelAlgoOrders"]
            };
            assert_eq!(
                calls.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                expected,
                "CANCEL native sequential group order"
            );
            if case == "normal" {
                assert_eq!(
                    items(&calls[0].1).len(),
                    1,
                    "CANCEL native normal group count"
                );
                assert!(
                    Rc::ptr_eq(&items(&calls[0].1)[0], &items(&t.orders)[1]),
                    "CANCEL native original normal order"
                );
                assert_eq!(
                    items(&calls[1].1).len(),
                    3,
                    "CANCEL native algo group count"
                );
            }
        }
        "symbol" => {
            let calls = t.dispatches.borrow();
            assert_eq!(calls.len(), 1, "SYMBOL native empty group still delegated");
            let rows = items(&calls[0].1);
            assert_eq!(
                rows.len(),
                if case == "empty" { 0 } else { 3 },
                "SYMBOL native exact selected count"
            );
            if case != "empty" {
                assert!(
                    Rc::ptr_eq(&rows[0], &items(&t.orders)[0]),
                    "SYMBOL native original row identity"
                );
            }
        }
        "normal" | "algo-cancel" => {
            let algo = method == "algo-cancel";
            let expected = if case == "unknown" {
                0
            } else if case == "reject" {
                1
            } else if case == "batch-boundary" {
                3
            } else {
                1
            };
            assert_eq!(
                requests.len(),
                expected,
                "BATCH native independent request count"
            );
            let widths = if case == "unknown" {
                vec![]
            } else if case == "batch-boundary" {
                if algo {
                    vec![10, 10, 1]
                } else {
                    vec![20, 20, 1]
                }
            } else {
                vec![2]
            };
            assert_eq!(
                requests
                    .iter()
                    .map(|(_, v)| items(v).len())
                    .collect::<Vec<_>>(),
                widths,
                "BATCH native independent chunk widths"
            );
            for (route, rows) in requests.iter() {
                assert_eq!(
                    route,
                    if algo {
                        "CANCEL_ALGO_ORDERS"
                    } else {
                        "CANCEL_ORDERS"
                    },
                    "BATCH native endpoint"
                );
                for row in items(rows) {
                    assert_eq!(
                        keys(&row),
                        if algo {
                            vec!["instId", "algoId"]
                        } else {
                            vec!["instId", "ordId"]
                        },
                        "BATCH native request row keys"
                    );
                    assert_s(&get(&row, "instId"), "INST", "BATCH native instrument");
                }
            }
            if case != "unknown" && case != "reject" {
                assert_s(
                    &get(
                        &items(&requests[0].1)[0],
                        if algo { "algoId" } else { "ordId" },
                    ),
                    if algo { "O0" } else { "O0_tp" },
                    "BATCH native id suffix choice",
                );
            }
            let mut expected_reads = Vec::new();
            for _ in 0..if case == "unknown" {
                0
            } else if case == "batch-boundary" {
                if algo { 21 } else { 41 }
            } else {
                2
            } {
                expected_reads.extend(["spread", "instId", "orderId"]);
            }
            assert_eq!(
                *t.reads.borrow(),
                expected_reads,
                "BATCH native spread and property evaluation order"
            );
        }
        "leverage" | "mode" => {
            if method == "leverage" {
                assert_eq!(
                    t.finds.get(),
                    1,
                    "LEVERAGE native position find even with missing market"
                );
            }
            let skipped = case == "same"
                || case == "missing-market"
                || case == "open"
                || case == "guard-error";
            assert_eq!(
                requests.len(),
                if skipped { 0 } else { 1 },
                "RISK native independent request count"
            );
            if let Some((route, payload)) = requests.first() {
                if method == "leverage" {
                    assert_eq!(route, "SET_LEVERAGE");
                    assert_eq!(
                        keys(payload),
                        ["instId", "lever", "mgnMode"],
                        "LEVERAGE native payload keys"
                    );
                    assert_s(
                        &get(payload, "instId"),
                        "INST",
                        "LEVERAGE native instrument",
                    );
                    assert_s(
                        &get(payload, "lever"),
                        if case == "callee-reenter" { "7" } else { "9" },
                        "LEVERAGE native clamped value",
                    );
                    assert_s(
                        &get(payload, "mgnMode"),
                        "cross",
                        "LEVERAGE native margin mode",
                    );
                    let captures = t.captures.borrow();
                    let at = captures.iter().position(|x| x == "Math.min").unwrap();
                    assert_eq!(
                        &captures[at..at + 2],
                        &["Math.min", "Math.max"],
                        "LEVERAGE native nested callee order"
                    );
                } else {
                    assert_eq!(route, "SET_POSITION_MODE");
                    assert_eq!(keys(payload), ["posMode"], "MODE native request keys");
                    assert_s(
                        &get(payload, "posMode"),
                        if case == "oneway" {
                            "net_mode"
                        } else {
                            "long_short_mode"
                        },
                        "MODE native authored code",
                    );
                }
            }
            let success = matches!(case, "normal" | "oneway" | "callee-reenter");
            let unchanged = matches!(case, "same" | "open" | "guard-error");
            let store = t.stores.borrow();
            assert_eq!(
                store.len(),
                if unchanged || case == "recovery-reject" {
                    0
                } else {
                    1
                },
                "RISK native independent store call count"
            );
            if let Some((name, value)) = store.first() {
                assert_eq!(
                    name,
                    if success {
                        "this.store.update"
                    } else {
                        "this.store.addOrUpdatePositions"
                    },
                    "RISK native update versus recovery merge"
                );
                assert!(
                    Rc::ptr_eq(value, &t.refreshed),
                    "RISK native store payload identity"
                );
            }
            assert_eq!(
                t.fetches.get(),
                if unchanged {
                    0
                } else if case == "refresh-reject" {
                    2
                } else {
                    1
                },
                "RISK native independent refresh count"
            );
            if method == "mode" {
                assert_s(
                    &get(&t.receiver, "_accountPositionMode"),
                    if case == "normal" || case == "refresh-reject" {
                        "hedged"
                    } else {
                        "oneway"
                    },
                    "MODE native assignment retained through refresh error",
                );
            }
        }
        _ => unreachable!(),
    }
    if case == "tiny" {
        assert_s(
            &t.messages.borrow()[0],
            "Order amount is too small",
            "AMEND native small amount diagnostic",
        );
    }
    if case == "late-error" {
        assert_s(
            &t.messages.borrow()[0],
            "DOWN",
            "RISK native first error message",
        );
    }
    let expected_messages: Vec<&str> = match (method, case) {
        ("limit", "tiny") | ("trigger", "tiny") => vec!["Order amount is too small"],
        ("trigger", "invalid") => {
            vec!["Trigger price is required for OKX opening conditional orders"]
        }
        ("trigger", "code-error") => vec!["CODE"],
        ("trigger", "row-error") => vec!["ROW"],
        ("trigger", "raw-predicate") => vec!["ROW-RAW"],
        ("normal" | "algo-cancel", "batch-boundary") => vec!["WARN", "WARN", "WARN"],
        ("normal" | "algo-cancel", "normal") => vec!["WARN"],
        ("leverage", "missing-market") => vec!["Market SYM not found on OKX"],
        ("mode", "open") => vec!["Please close all positions before switching position mode"],
        ("mode", "msg-error") => vec!["DENIED"],
        ("limit" | "trigger" | "leverage" | "mode", "reject")
        | ("leverage", "late-error" | "recovery-reject")
        | ("mode", "refresh-reject") => vec!["DOWN"],
        _ => vec![],
    };
    let messages = t.messages.borrow();
    assert_eq!(
        messages.len(),
        expected_messages.len(),
        "DIAGNOSTIC native independent message count"
    );
    for (message, expected) in messages.iter().zip(expected_messages) {
        assert_s(message, expected, "DIAGNOSTIC native message value");
    }
    let expected_fresh = match (method, case) {
        ("limit", "missing") => 1,
        (
            "trigger",
            "invalid" | "tiny" | "reject" | "code-error" | "row-error" | "raw-predicate",
        ) => 1,
        ("leverage", "missing-market") | ("mode", "msg-error") => 2,
        ("leverage", "reject" | "late-error") | ("mode", "reject" | "refresh-reject") => 1,
        _ => 0,
    };
    assert_eq!(
        t.fresh.get(),
        expected_fresh,
        "ERROR native independent fresh constructor count"
    );
    let result = match out {
        Ok(value) => json!({"ok":pack(&value)}),
        Err(error) => {
            json!({"error":{"name":error.name,"message":error.message,"fresh":error.fresh,"code":error.code,"ids":error.ids}})
        }
    };
    json!({"method":method,"case":case,"result":result,"events":t.events.borrow().clone(),"mode":pack(&get(&t.receiver,"_accountPositionMode"))})
}
#[test]
fn order_mutations_native_before_source() {
    let mut cases = Vec::new();
    for case in [
        "normal",
        "no-price",
        "zero-original",
        "tiny",
        "missing",
        "reject",
        "delegate",
    ] {
        cases.push(fixture("limit", case));
    }
    for case in [
        "normal",
        "conditional",
        "empty",
        "cancel-reject",
        "place-reject",
    ] {
        cases.push(fixture("algo", case));
    }
    for case in [
        "normal",
        "market-order",
        "invalid",
        "tiny",
        "reject",
        "code-error",
        "row-error",
        "raw-predicate",
        "empty-body",
    ] {
        cases.push(fixture("trigger", case));
    }
    for case in ["normal", "empty", "normal-reject", "algo-only"] {
        cases.push(fixture("dispatch", case));
    }
    for case in ["normal", "empty", "reject"] {
        cases.push(fixture("symbol", case));
    }
    for case in ["normal", "batch-boundary", "unknown", "reject"] {
        cases.push(fixture("normal", case));
        cases.push(fixture("algo-cancel", case));
    }
    for case in [
        "normal",
        "same",
        "missing-market",
        "reject",
        "late-error",
        "callee-reenter",
        "recovery-reject",
    ] {
        cases.push(fixture("leverage", case));
    }
    for case in [
        "normal",
        "oneway",
        "open",
        "msg-error",
        "reject",
        "refresh-reject",
        "guard-error",
    ] {
        cases.push(fixture("mode", case));
    }
    assert_eq!(cases.len(), 50, "independent mutation fixture floor");
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/order-mutations-oracle.mjs"),
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
        .write_all(json!({"cases":cases}).to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "actual source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        50,
        "independent source fixture floor"
    );
    for (i, (native, source)) in cases.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(
            native, source,
            "source mismatch {i} {}/{}",
            native["method"], native["case"]
        );
    }
}
