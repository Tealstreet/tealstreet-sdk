const PRIVATE_ENTRY_CASES: [(&str, &str); 15] = [
    ("connectAndSubscribe", "get:isDisposed"),
    ("onOpen", "get:isDisposed"),
    (
        "getWatchdogStaleThresholdMs",
        "global:CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS",
    ),
    (
        "getWatchdogPingThresholdMs",
        "global:CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS",
    ),
    ("authenticate", "global:generateWsAuthMessage"),
    ("resubscribe", "get:isAuthenticated"),
    ("subscribeToOrderUpdates", "get:filter"),
    ("subscribeToPositionUpdates", "get:subscriptions"),
    ("subscribeToBalanceUpdates", "get:subscriptions"),
    ("mapOrderType", "global:OrderType"),
    ("mapOrderStatus", "global:OrderStatus"),
    ("mapTimeInForce", "global:OrderTimeInForce"),
    ("unsubscribe", "get:length"),
    ("dispose", "global:clearTimeout"),
    ("shouldEnforcePongMissReconnect", ""),
];
const PUBLIC_ENTRY_CASES: [(&str, &str); 18] = [
    ("connectAndSubscribe", "get:isDisposed"),
    ("onOpen", "get:isDisposed"),
    (
        "getWatchdogStaleThresholdMs",
        "global:CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS",
    ),
    (
        "getWatchdogPingThresholdMs",
        "global:CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS",
    ),
    ("resubscribe", "get:subscriptions"),
    ("sendSubscriptionFrames", "get:length"),
    ("registerAndSubscribe", "iterator"),
    ("subscribeToOrderBook", "get:subscriptions"),
    ("subscribeToTicker", "get:subscriptions"),
    ("subscribeToTickers", "get:registerAndSubscribe"),
    ("subscribeToMarketData", "iterator"),
    ("subscribeToTrades", "get:subscriptions"),
    ("subscribeToCandlestick", "get:subscriptions"),
    ("handleValuationUpdate", "get:data"),
    (
        "getMarketIdsByIndexInstrument",
        "get:marketIdsByIndexInstrument",
    ),
    ("handleCandlestickUpdate", "get:subscription"),
    ("unsubscribe", "get:length"),
    ("dispose", "global:clearTimeout"),
];
const EXCHANGE_ENTRY_CASES: [(&str, &str); 29] = [
    ("dispose", "get:publicWebsocket"),
    ("getAccount", "get:xhr"),
    (
        "validateAccount",
        "get:_suppressCryptocomValidationErrorEmission",
    ),
    ("fetchMarkets", "get:xhr"),
    ("fetchTickers", "get:xhr"),
    ("fetchOHLCV", "get:symbol"),
    ("fetchOrderBook", "get:xhr"),
    ("fetchOrders", "global:Promise"),
    ("fetchPositions", "get:xhr"),
    ("setLeverage", "get:xhr"),
    ("placeOrder", "get:resolveCallerClientOrderId"),
    ("cancelOrder", "get:type"),
    ("cancelOrders", "global:forEachSeries"),
    ("cancelSymbolOrders", "get:xhr"),
    ("cancelAllOrders", "get:xhr"),
    ("updateOrder", "get:order"),
    ("changePositionMode", "get:log"),
    ("changeMarginType", "global:Error"),
    ("placeOrders", "get:assertCallerClientOrderIds"),
    ("fetchBalance", "get:xhr"),
    ("listenOHLCV", "get:publicWebsocket"),
    ("listenOrderBook", "get:publicWebsocket"),
    ("mapOrder", "global:parseFloat"),
    ("mapOrderStatus", "global:OrderStatus"),
    ("mapOrderType", "global:OrderType"),
    ("mapCryptocomOrderType", "global:OrderType"),
    ("mapTimeInForce", "global:OrderTimeInForce"),
    ("mapStopTriggerType", "global:OrderStopTriggerType"),
    ("mapCryptocomTimeInForce", "global:OrderTimeInForce"),
];
use super::*;
use crate::order_writes::WriteHost;
use crate::{class_exchange, class_socket};
use std::cell::{Cell, RefCell};
use std::task::{Context, Poll, Waker};
type V = Rc<Value>;
type E = V;
enum Value {
    U,
    Null,
    B(bool),
    N(f64),
    S(String),
    A(RefCell<Vec<V>>),
    O(RefCell<Vec<(String, V)>>),
    F(crate::socket_handlers::SocketCallback<V, E>),
    P(RefCell<PromiseState>),
    D(Rc<Cell<bool>>, V),
    M(Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>),
}
fn u() -> V {
    Rc::new(Value::U)
}
fn s(v: &str) -> V {
    Rc::new(Value::S(v.into()))
}
fn n(v: f64) -> V {
    Rc::new(Value::N(v))
}
fn b(v: bool) -> V {
    Rc::new(Value::B(v))
}
fn a(v: Vec<V>) -> V {
    Rc::new(Value::A(RefCell::new(v)))
}
fn o(v: Vec<(&str, V)>) -> V {
    Rc::new(Value::O(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn t(v: &V) -> String {
    match v.as_ref() {
        Value::S(s) => s.clone(),
        Value::N(n) => safe_cex_num::js_number_string(*n),
        Value::U => "undefined".into(),
        Value::B(b) => b.to_string(),
        Value::Null => "null".into(),
        _ => "[object Object]".into(),
    }
}
fn num(v: &V) -> f64 {
    match v.as_ref() {
        Value::N(n) => *n,
        _ => t(v).parse().unwrap_or(f64::NAN),
    }
}
fn truth(v: &V) -> bool {
    match v.as_ref() {
        Value::U | Value::Null => false,
        Value::B(b) => *b,
        Value::N(n) => *n != 0. && !n.is_nan(),
        Value::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Value::U, Value::U) | (Value::Null, Value::Null) => true,
        (Value::S(a), Value::S(b)) => a == b,
        (Value::N(a), Value::N(b)) => a == b,
        (Value::B(a), Value::B(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}

fn raw(v: &V, k: &str) -> V {
    match v.as_ref() {
        Value::O(fields) => fields
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Value::A(values) => {
            if k == "length" {
                n(values.borrow().len() as f64)
            } else {
                k.parse::<usize>()
                    .ok()
                    .and_then(|i| values.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        _ => u(),
    }
}
fn property(v: &V, k: &str) -> V {
    if let Value::A(rows) = v.as_ref() {
        match k {
            "map" | "filter" | "forEach" => {
                let rows = v.clone();
                let kind = k.to_owned();
                return function(move |args| {
                    let Value::A(values) = rows.as_ref() else {
                        unreachable!()
                    };
                    let length = values.borrow().len();
                    let mut output = vec![];
                    for index in 0..length {
                        let row = values.borrow().get(index).cloned();
                        if let Some(row) = row {
                            let result = invoke(
                                args[0].clone(),
                                vec![row.clone(), n(index as f64), rows.clone()],
                            )?;
                            if kind == "map" {
                                output.push(result)
                            } else if kind == "filter" && truth(&result) {
                                output.push(row)
                            }
                        }
                    }
                    if kind == "forEach" {
                        Ok(u())
                    } else {
                        Ok(a(output))
                    }
                });
            }
            "push" => {
                let rows = v.clone();
                return function(move |args| {
                    let Value::A(values) = rows.as_ref() else {
                        unreachable!()
                    };
                    values.borrow_mut().extend(args);
                    Ok(n(values.borrow().len() as f64))
                });
            }
            "includes" => {
                let rows = v.clone();
                return function(move |args| {
                    let Value::A(values) = rows.as_ref() else {
                        unreachable!()
                    };
                    Ok(b(values.borrow().iter().any(|row| eq(row, &args[0]))))
                });
            }
            _ => {
                let _ = rows;
            }
        }
    }
    if matches!(v.as_ref(), Value::N(_) | Value::S(_)) && k == "toString" {
        let value = v.clone();
        return function(move |_| Ok(s(&t(&value))));
    }
    raw(v, k)
}
fn put(v: &V, k: &str, x: V) {
    match v.as_ref() {
        Value::O(fields) => {
            let mut f = fields.borrow_mut();
            if let Some((_, old)) = f.iter_mut().find(|(key, _)| key == k) {
                *old = x
            } else {
                f.push((k.into(), x))
            }
        }
        Value::A(values) => {
            let i: usize = k.parse().expect("array index");
            let mut values = values.borrow_mut();
            while values.len() <= i {
                values.push(u())
            }
            values[i] = x
        }
        _ => panic!("test assignment"),
    }
}
fn function(f: impl Fn(Vec<V>) -> Result<V, E> + 'static) -> V {
    Rc::new(Value::F(Rc::new(f)))
}
fn native_invoke(f: V, receiver: V, args: Vec<V>) -> Result<V, E> {
    match f.as_ref() {
        Value::F(f) => f(args),
        Value::M(f) => f(receiver, args),
        _ => Err(s("not callable")),
    }
}
fn invoke(f: V, args: Vec<V>) -> Result<V, E> {
    native_invoke(f, u(), args)
}
fn method(f: impl Fn(V, Vec<V>) -> Result<V, E> + 'static) -> V {
    Rc::new(Value::M(Rc::new(f)))
}
fn entry_argument(name: &str) -> V {
    match name {
        "sendSubscriptionFrames"
        | "registerAndSubscribe"
        | "subscribeToTickers"
        | "subscribeToMarketData"
        | "unsubscribe" => a(vec![o(vec![])]),
        "subscribeToOrderBook"
        | "subscribeToTicker"
        | "subscribeToTrades"
        | "subscribeToCandlestick"
        | "getMarketIdsByIndexInstrument"
        | "mapOrderType"
        | "mapOrderStatus"
        | "mapTimeInForce"
        | "mapCryptocomOrderType"
        | "mapCryptocomTimeInForce"
        | "mapStopTriggerType" => s("UNKNOWN"),
        _ => o(vec![]),
    }
}
fn entry_result(side: &str, name: &str, bound: V, args: Vec<V>) -> Result<V, E> {
    let result = invoke(bound, args);
    let asynchronous = if side == "exchange" {
        !matches!(
            name,
            "dispose"
                | "listenOHLCV"
                | "listenOrderBook"
                | "mapOrder"
                | "mapOrderStatus"
                | "mapOrderType"
                | "mapCryptocomOrderType"
                | "mapTimeInForce"
                | "mapStopTriggerType"
                | "mapCryptocomTimeInForce"
        )
    } else {
        side == "private" && matches!(name, "connectAndSubscribe" | "authenticate" | "resubscribe")
    };
    if asynchronous {
        assert!(
            matches!(&result,Ok(v)if matches!(v.as_ref(),Value::P(_))),
            "NATIVE-EXCHANGE-BINDING-ASYNC-PROMISE {side} {name}"
        );
    }
    result.and_then(settle)
}
fn take<T>(v: Result<T, E>) -> T {
    match v {
        Ok(v) => v,
        Err(e) => panic!("unexpected native error {}", t(&e)),
    }
}
enum PromiseState {
    Polling,
    Running(SocketFuture<V, E>),
    Done(Result<V, E>),
}
fn poll_native(v: &V, cx: &mut Context<'_>) -> Poll<Result<V, E>> {
    match v.as_ref() {
        Value::P(state) => {
            let current = std::mem::replace(&mut *state.borrow_mut(), PromiseState::Polling);
            match current {
                PromiseState::Done(result) => {
                    *state.borrow_mut() = PromiseState::Done(result.clone());
                    Poll::Ready(result)
                }
                PromiseState::Running(mut future) => match future.as_mut().poll(cx) {
                    Poll::Ready(result) => {
                        *state.borrow_mut() = PromiseState::Done(result.clone());
                        Poll::Ready(result)
                    }
                    Poll::Pending => {
                        *state.borrow_mut() = PromiseState::Running(future);
                        Poll::Pending
                    }
                },
                PromiseState::Polling => Poll::Pending,
            }
        }
        Value::D(ready, value) => {
            if ready.get() {
                Poll::Ready(Ok(value.clone()))
            } else {
                Poll::Pending
            }
        }
        _ => Poll::Ready(Ok(v.clone())),
    }
}
struct AwaitNative(V, bool);
impl Future for AwaitNative {
    type Output = Result<V, E>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.1 {
            self.1 = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        poll_native(&self.0, cx)
    }
}
fn settle(v: V) -> Result<V, E> {
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..100 {
        if let Poll::Ready(result) = poll_native(&v, &mut cx) {
            return result;
        }
    }
    panic!("bridge poll floor")
}

struct Model {
    error: V,
    get_hook: RefCell<Option<Rc<dyn Fn(&V, &str) -> Result<(), E>>>>,
    fail_get: Cell<bool>,
    events: RefCell<Vec<String>>,
    globals: V,
    closed: Cell<usize>,
    next_error: Cell<bool>,
    steps: Cell<usize>,
    saved: RefCell<Option<V>>,
    retained: RefCell<Vec<V>>,
    writes: RefCell<Vec<(V, String, V)>>,
}
impl Model {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            error: o(vec![("native", b(true))]),
            get_hook: RefCell::new(None),
            fail_get: Cell::new(false),
            events: RefCell::new(vec![]),
            globals: o(vec![]),
            closed: Cell::new(0),
            next_error: Cell::new(false),
            steps: Cell::new(0),
            saved: RefCell::new(None),
            retained: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
        })
    }
    fn ops(self: &Rc<Self>) -> Rc<BindingOps<V, E>> {
        let get = self.clone();
        let write = self.clone();
        let promise = self.clone();
        let global = self.clone();
        let iterator = self.clone();
        let next = self.clone();
        let close = self.clone();
        let abrupt = self.clone();
        let value = Rc::new(SocketOps {
            undefined: Rc::new(u),
            boolean: Rc::new(b),
            number: Rc::new(n),
            literal: Rc::new(s),
            object: Rc::new(o),
            array: Rc::new(a),
            get: Rc::new(move |v, k| {
                get.events.borrow_mut().push(format!("get:{k}"));
                let hook = get.get_hook.borrow().clone();
                if let Some(hook) = hook {
                    hook(v, k)?;
                }
                if get.fail_get.get() {
                    Err(get.error.clone())
                } else {
                    Ok(property(v, k))
                }
            }),
            index: Rc::new(|v, k| Ok(raw(v, &t(k)))),
            key: Rc::new(|v| Ok(v.clone())),
            set: Rc::new(move |v, k, x| {
                write
                    .writes
                    .borrow_mut()
                    .push((v.clone(), k.into(), x.clone()));
                put(v, k, x);
                Ok(())
            }),
            set_index: Rc::new(|v, k, x| {
                put(v, &t(k), x);
                Ok(())
            }),
            delete: Rc::new(|_, _| Ok(true)),
            global: Rc::new(move |name| {
                global.events.borrow_mut().push(format!("global:{name}"));
                let v = raw(&global.globals, name);
                if matches!(v.as_ref(), Value::U) {
                    Err(global.error.clone())
                } else {
                    Ok(v)
                }
            }),
            invoke: Rc::new(native_invoke),
            function: Rc::new(|_, _, f| Rc::new(Value::F(f))),
            truthy: Rc::new(truth),
            nullish: Rc::new(|v| matches!(v.as_ref(), Value::U | Value::Null)),
            equal: Rc::new(eq),
            is_number: Rc::new(|v| matches!(v.as_ref(), Value::N(_))),
            has: Rc::new(|v, k| Ok(!matches!(raw(v, k).as_ref(), Value::U))),
            numeric: Rc::new(|v| Ok(num(v))),
            text: Rc::new(|v| Ok(t(v).encode_utf16().collect())),
            utf16: Rc::new(|v| s(&String::from_utf16(&v).expect("test valid UTF16"))),
            error_value: Rc::new(|v| v),
            spread: Rc::new(|v| {
                let Value::A(v) = v.as_ref() else {
                    panic!("array spread")
                };
                Ok(v.borrow().clone())
            }),
            spread_object: Rc::new(|v| {
                let Value::O(v) = v.as_ref() else {
                    panic!("object spread")
                };
                Ok(v.borrow().iter().map(|(k, v)| (s(k), v.clone())).collect())
            }),
            object_keys: Rc::new(|fields| {
                let v = o(vec![]);
                for (k, x) in fields {
                    put(&v, &t(&k), x)
                }
                v
            }),
            tuple_two: Rc::new(|v| Ok((raw(v, "0"), raw(v, "1")))),
            await_value: Rc::new(|v| Box::pin(AwaitNative(v, false))),
        });
        Rc::new(BindingOps {
            null: Rc::new(|| Rc::new(Value::Null)),
            is_string: Rc::new(|v| matches!(v.as_ref(), Value::S(_))),
            value,
            binary: Rc::new(|op, l, r| {
                Ok(match op {
                    "+" => n(num(&l) + num(&r)),
                    ">" => b(num(&l) > num(&r)),
                    ">=" => b(num(&l) >= num(&r)),
                    "<=" => b(num(&l) <= num(&r)),
                    "<" => b(num(&l) < num(&r)),
                    "ToNumeric" => n(num(&l)),
                    "++" => n(num(&l) + 1.),
                    "/" => n(num(&l) / num(&r)),
                    "-" => n(num(&l) - num(&r)),
                    _ => panic!("test operator {op}"),
                })
            }),
            construct: Rc::new(invoke),
            throw_value: Rc::new(|v| v),
            promise: Rc::new(move |f| {
                let v = Rc::new(Value::P(RefCell::new(PromiseState::Running(f))));
                promise.retained.borrow_mut().push(v.clone());
                let _ = poll_native(&v, &mut Context::from_waker(Waker::noop()));
                v
            }),
            iterator: Rc::new(move |v| {
                iterator.events.borrow_mut().push("iterator".into());
                Ok(o(vec![("source", v), ("offset", n(0.))]))
            }),
            next: Rc::new(move |record| {
                next.steps.set(next.steps.get() + 1);
                if next.next_error.get() {
                    return Err(next.error.clone());
                }
                let values = raw(record, "source");
                let Value::A(values) = values.as_ref() else {
                    panic!("test iterator")
                };
                let offset = num(&raw(record, "offset")) as usize;
                let v = values.borrow().get(offset).cloned();
                put(record, "offset", n((offset + 1) as f64));
                Ok(v)
            }),
            close: Rc::new(move |_| {
                close.closed.set(close.closed.get() + 1);
                Ok(())
            }),
            close_throw: Rc::new(move |_, e| {
                abrupt.closed.set(abrupt.closed.get() + 1);
                e
            }),
        })
    }
}
#[test]
fn native_destructuring_closes_only_active_binding_errors() {
    for (label, rows, next_error, closes, bindings) in [
        ("exhausted", vec![], false, 0, 1),
        ("live-binding", vec![u()], false, 1, 1),
        ("step-abrupt", vec![u()], true, 0, 0),
    ] {
        let model = Model::new();
        model.next_error.set(next_error);
        let h = BoundHost {
            ops: model.ops(),
            receiver: o(vec![]),
        };
        let binding_error = o(vec![("binding", b(true))]);
        let expected = if next_error {
            model.error.clone()
        } else {
            binding_error.clone()
        };
        let called = Rc::new(Cell::new(0));
        let count = called.clone();
        let result = ReadHost::iterator_pair(&h, a(rows), move |_| {
            count.set(count.get() + 1);
            Err(binding_error.clone())
        });
        assert!(
            matches!(result,Err(ref e)if Rc::ptr_eq(e,&expected)),
            "NATIVE-ITERATOR-ORIGINAL-ERROR {label}"
        );
        assert_eq!(called.get(), bindings, "NATIVE-ITERATOR-BIND-COUNT {label}");
        assert_eq!(model.steps.get(), 1, "NATIVE-ITERATOR-STEP-COUNT {label}");
        assert_eq!(
            model.closed.get(),
            closes,
            "NATIVE-ITERATOR-CLOSE-PHASE {label}"
        );
    }
}
#[test]
fn native_destructuring_success_retains_values_and_stops_after_two() {
    let model = Model::new();
    let h = BoundHost {
        ops: model.ops(),
        receiver: o(vec![]),
    };
    let first = o(vec![("first", b(true))]);
    let second = o(vec![("second", b(true))]);
    let (one, two) = take(ReadHost::iterator_pair(
        &h,
        a(vec![first.clone(), second.clone(), s("must not read")]),
        Ok,
    ));
    assert!(
        Rc::ptr_eq(&one, &first) && Rc::ptr_eq(&two, &second),
        "NATIVE-ITERATOR-TWO-ORIGINAL-VALUES"
    );
    assert_eq!(model.steps.get(), 2, "NATIVE-ITERATOR-TWO-STEP-FLOOR");
    assert_eq!(model.closed.get(), 1, "NATIVE-ITERATOR-EARLY-CLOSE");
}
#[test]
fn native_series_callback_factory_can_reenter() {
    let model = Model::new();
    let imported = model.clone();
    put(
        &model.globals,
        "forEachSeries",
        function(move |args| {
            imported.saved.replace(Some(args[1].clone()));
            invoke(args[1].clone(), vec![u()])
        }),
    );
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let callback_model = model.clone();
    let h = BoundHost {
        ops: model.ops(),
        receiver: o(vec![]),
    };
    let pending = take(WriteHost::series(&h, a(vec![u()]), move |_| {
        let old = count.get();
        count.set(old + 1);
        if old == 0 {
            let callback = callback_model.saved.borrow().as_ref().unwrap().clone();
            take(invoke(callback, vec![u()]));
        }
        Box::pin(async { Ok(u()) })
    }));
    assert_eq!(calls.get(), 2, "NATIVE-SERIES-REENTRANT-FACTORY");
    take(settle((h.ops.promise)(Box::pin(pending))));
}
#[test]
fn native_private_position_channel_is_plural() {
    let model = Model::new();
    let channels = Rc::new(RefCell::new(vec![]));
    let added = channels.clone();
    let set = o(vec![
        ("has", function(|_| Ok(b(false)))),
        (
            "add",
            function(move |args| {
                added.borrow_mut().push(args[0].clone());
                Ok(u())
            }),
        ),
    ]);
    let receiver = o(vec![("subscriptions", set), ("isAuthenticated", b(false))]);
    let bound = class_socket::member(model.ops(), receiver, "subscribeToPositionUpdates", true);
    assert!(
        matches!(take(invoke(bound, vec![])).as_ref(), Value::U),
        "NATIVE-POSITION-UNDEFINED"
    );
    let channels = channels.borrow();
    assert_eq!(channels.len(), 1, "NATIVE-POSITION-CHANNEL-COUNT");
    assert!(
        matches!(channels[0].as_ref(),Value::S(v)if v=="user.positions"),
        "NATIVE-POSITION-CHANNEL-VALUE"
    );
}
#[test]
fn assembled_exchange_fields_reach_real_bodies() {
    // Independent bounded field list, excluding start/tick (already native).

    for (iteration, (name, first)) in EXCHANGE_ENTRY_CASES.into_iter().enumerate() {
        let model = Model::new();
        model.fail_get.set(true);
        let bound = class_exchange::member(model.ops(), o(vec![]), name);
        let result = entry_result(
            "exchange",
            name,
            bound,
            vec![entry_argument(name), u(), u()],
        );
        assert!(
            matches!(result,Err(ref e)if Rc::ptr_eq(e,&model.error)),
            "NATIVE-EXCHANGE-BINDING {iteration} {name}"
        );
        assert_eq!(
            model.events.borrow().first().map(String::as_str),
            Some(first),
            "NATIVE-EXCHANGE-FIRST-OP {iteration} {name}"
        );
        if name == "dispose" {
            let writes = model.writes.borrow();
            assert_eq!(writes.len(), 1, "NATIVE-DISPOSE-WRITE-COUNT");
            assert_eq!(writes[0].1, "isDisposed", "NATIVE-DISPOSE-FIELD");
            assert!(
                matches!(writes[0].2.as_ref(), Value::B(true)),
                "NATIVE-DISPOSE-VALUE"
            );
        }
    }
}
#[test]
fn assembled_socket_fields_reach_real_bodies() {
    for (private, fields) in [
        (false, PUBLIC_ENTRY_CASES.as_slice()),
        (true, PRIVATE_ENTRY_CASES.as_slice()),
    ] {
        for (iteration, (name, first)) in fields.iter().copied().enumerate() {
            let model = Model::new();
            model.fail_get.set(true);
            let bound = class_socket::member(model.ops(), o(vec![]), name, private);
            let result = entry_result(
                if private { "private" } else { "public" },
                name,
                bound,
                vec![entry_argument(name), u()],
            );
            if name == "shouldEnforcePongMissReconnect" {
                assert!(
                    matches!(take(result).as_ref(), Value::B(false)),
                    "NATIVE-PONG-POLICY"
                );
                continue;
            }
            assert!(
                matches!(result,Err(ref e)if Rc::ptr_eq(e,&model.error)),
                "NATIVE-SOCKET-BINDING {private} {iteration} {name}"
            );
            assert_eq!(
                model.events.borrow().first().map(String::as_str),
                Some(first),
                "NATIVE-SOCKET-FIRST-OP {private} {iteration} {name}"
            );
        }
        let model = Model::new();
        let ping = class_socket::member(model.ops(), o(vec![]), "ping", private);
        assert!(
            matches!(take(invoke(ping, vec![])).as_ref(), Value::U),
            "NATIVE-PING-UNDEFINED"
        );
        assert!(model.events.borrow().is_empty(), "NATIVE-PING-NO-EFFECTS");
    }
}
#[test]
fn assembled_get_account_retains_decoded_native_uuid() {
    let model = Model::new();
    let uuid = o(vec![("opaque", b(true))]);
    let expected = uuid.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let endpoint = o(vec![("opaque-endpoint", b(true))]);
    let expected_endpoint = endpoint.clone();
    put(&model.globals, "ENDPOINTS", o(vec![("ACCOUNTS", endpoint)]));
    let receiver = o(vec![(
        "xhr",
        o(vec![(
            "post",
            function(move |args| {
                count.set(count.get() + 1);
                assert!(
                    Rc::ptr_eq(&args[0], &expected_endpoint),
                    "NATIVE-ACCOUNT-ENDPOINT"
                );
                let Value::O(fields) = args[1].as_ref() else {
                    panic!("native request object")
                };
                assert!(fields.borrow().is_empty(), "NATIVE-ACCOUNT-PARAMS");
                Ok(o(vec![(
                    "data",
                    o(vec![("this_account", o(vec![("uuid", uuid.clone())]))]),
                )]))
            }),
        )]),
    )]);
    let bound = class_exchange::member(model.ops(), receiver, "getAccount");
    let output = take(settle(take(invoke(bound, vec![]))));
    assert!(
        Rc::ptr_eq(&raw(&output, "userId"), &expected),
        "NATIVE-ACCOUNT-UUID-IDENTITY"
    );
    assert!(
        matches!(raw(&output, "affiliateId").as_ref(), Value::Null),
        "NATIVE-ACCOUNT-AFFILIATE-NULL"
    );
    assert_eq!(calls.get(), 1, "NATIVE-ACCOUNT-POST-COUNT");
}
#[test]
fn assembled_mapping_reads_live_native_imports() {
    let model = Model::new();
    let first = o(vec![("case", n(1.))]);
    let replacement = o(vec![("case", n(2.))]);
    let imports = o(vec![("Market", first.clone())]);
    put(&model.globals, "OrderType", imports.clone());
    let bound = class_exchange::member(model.ops(), o(vec![]), "mapOrderType");
    assert!(
        matches!(take(invoke(bound.clone(),vec![first.clone()])).as_ref(),Value::S(v)if v=="MARKET"),
        "NATIVE-MAPPING-IMPORTED-CASE"
    );
    put(&imports, "Market", replacement.clone());
    assert!(
        matches!(take(invoke(bound.clone(),vec![replacement])).as_ref(),Value::S(v)if v=="MARKET"),
        "NATIVE-MAPPING-LIVE-CASE"
    );
    for value in [first, o(vec![])] {
        assert!(
            matches!(take(invoke(bound.clone(),vec![value])).as_ref(),Value::S(v)if v=="LIMIT"),
            "NATIVE-MAPPING-NONMATCH-DEFAULT"
        );
    }
}

#[test]
fn bridge_equality_preserves_nan_inequality() {
    let nan = n(f64::NAN);
    assert!(!eq(&nan, &nan), "NATIVE-NAN-NOT-STRICT-EQUAL");
}
#[test]
fn assembled_account_posts_eagerly_then_suspends_with_original_receiver() {
    let model = Model::new();
    let uuid = o(vec![("uuid", b(true))]);
    let expected = uuid.clone();
    let xhr = o(vec![]);
    let expected_xhr = xhr.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    put(
        &xhr,
        "post",
        method(move |receiver, _| {
            assert!(
                Rc::ptr_eq(&receiver, &expected_xhr),
                "NATIVE-ACCOUNT-POST-RECEIVER"
            );
            count.set(count.get() + 1);
            Ok(o(vec![(
                "data",
                o(vec![("this_account", o(vec![("uuid", uuid.clone())]))]),
            )]))
        }),
    );
    put(
        &model.globals,
        "ENDPOINTS",
        o(vec![("ACCOUNTS", s("accounts"))]),
    );
    let bound = class_exchange::member(model.ops(), o(vec![("xhr", xhr)]), "getAccount");
    let returned = take(native_invoke(bound, o(vec![("xhr", o(vec![]))]), vec![]));
    assert_eq!(calls.get(), 1, "NATIVE-ACCOUNT-EAGER-POST");
    assert!(
        !model.events.borrow().iter().any(|e| e == "get:uuid"),
        "NATIVE-ACCOUNT-AWAIT-SUSPENDS"
    );
    let output = take(settle(returned));
    assert!(
        Rc::ptr_eq(&raw(&output, "userId"), &expected),
        "NATIVE-ACCOUNT-EAGER-UUID"
    );
}
#[test]
fn assembled_account_continues_after_observer_drop() {
    let model = Model::new();
    let ready = Rc::new(Cell::new(false));
    let gate = ready.clone();
    let uuid = o(vec![("uuid", b(true))]);
    let expected = uuid.clone();
    let response = o(vec![(
        "data",
        o(vec![("this_account", o(vec![("uuid", uuid)]))]),
    )]);
    let xhr = o(vec![(
        "post",
        function(move |_| Ok(Rc::new(Value::D(gate.clone(), response.clone())))),
    )]);
    put(
        &model.globals,
        "ENDPOINTS",
        o(vec![("ACCOUNTS", s("accounts"))]),
    );
    let bound = class_exchange::member(model.ops(), o(vec![("xhr", xhr)]), "getAccount");
    let observer = take(invoke(bound, vec![]));
    assert!(
        !model.events.borrow().iter().any(|e| e == "get:uuid"),
        "NATIVE-ACCOUNT-SUSPENDED"
    );
    drop(observer);
    ready.set(true);
    let promises = model.retained.borrow().clone();
    assert_eq!(promises.len(), 1, "NATIVE-ACCOUNT-RETAINED-JOB-COUNT");
    let output = take(settle(promises[0].clone()));
    assert!(
        Rc::ptr_eq(&raw(&output, "userId"), &expected),
        "NATIVE-ACCOUNT-DROP-CONTINUATION"
    );
}
#[test]
fn assembled_rest_dispatch_uses_distinct_native_endpoints() {
    const CASES: [(&str, &str); 8] = [
        ("fetchMarkets", "INSTRUMENTS"),
        ("fetchTickers", "TICKERS"),
        ("fetchOrderBook", "BOOK"),
        ("fetchPositions", "POSITIONS"),
        ("fetchBalance", "BALANCE"),
        ("getAccount", "ACCOUNTS"),
        ("setLeverage", "CHANGE_LEVERAGE"),
        ("cancelAllOrders", "CANCEL_ALL_ORDERS"),
    ];
    for (iteration, (name, key)) in CASES.into_iter().enumerate() {
        let model = Model::new();
        let endpoints = o(vec![]);
        for (_, field) in CASES {
            put(&endpoints, field, o(vec![("key", s(field))]));
        }
        let expected = raw(&endpoints, key);
        put(&model.globals, "ENDPOINTS", endpoints);
        put(
            &model.globals,
            "LogSeverity",
            o(vec![("Error", s("Error"))]),
        );
        let error = model.error.clone();
        let calls = Rc::new(RefCell::new(vec![]));
        let observed = calls.clone();
        let xhr = o(vec![]);
        let expected_receiver = xhr.clone();
        put(
            &xhr,
            "post",
            method(move |receiver, args| {
                assert!(
                    Rc::ptr_eq(&receiver, &expected_receiver),
                    "NATIVE-REST-POST-RECEIVER"
                );
                observed.borrow_mut().push(args[0].clone());
                Err(error.clone())
            }),
        );
        let receiver = o(vec![
            ("xhr", xhr),
            ("log", function(|_| Ok(u()))),
            ("store", o(vec![("balance", o(vec![]))])),
        ]);
        let bound = class_exchange::member(model.ops(), receiver, name);
        let result = take(invoke(bound, vec![s("BTC_USD"), n(2.), u()]));
        let _ = settle(result);
        let calls = calls.borrow();
        assert_eq!(
            calls.len(),
            1,
            "NATIVE-REST-DISPATCH-COUNT {iteration} {name}"
        );
        assert!(
            Rc::ptr_eq(&calls[0], &expected),
            "NATIVE-REST-DISPATCH-ENDPOINT {iteration} {name}"
        );
    }
}

#[test]
#[ignore = "requires isolated TypeScript oracle compiler"]
fn source_body_entry_floor_matches_native_after_native_assertions() {
    use serde_json::json;
    native_parse_float_captures_callee_before_order_field_getters();
    assembled_order_retains_imported_side_value();
    assembled_positions_retain_each_live_enum_read();
    let mut native = vec![];
    for (side, private, cases) in [
        ("exchange", false, EXCHANGE_ENTRY_CASES.as_slice()),
        ("public", false, PUBLIC_ENTRY_CASES.as_slice()),
        ("private", true, PRIVATE_ENTRY_CASES.as_slice()),
    ] {
        for (iteration, (name, first)) in cases.iter().copied().enumerate() {
            let model = Model::new();
            model.fail_get.set(true);
            let bound = if side == "exchange" {
                class_exchange::member(model.ops(), o(vec![]), name)
            } else {
                class_socket::member(model.ops(), o(vec![]), name, private)
            };
            let result = entry_result(side, name, bound, vec![entry_argument(name), u(), u()]);
            if name == "shouldEnforcePongMissReconnect" {
                assert!(
                    matches!(take(result).as_ref(), Value::B(false)),
                    "NATIVE-ENTRY-POLICY"
                );
            } else {
                assert!(
                    matches!(result,Err(ref e)if Rc::ptr_eq(e,&model.error)),
                    "NATIVE-ENTRY-ERROR {side} {iteration} {name}"
                );
            }
            let actual = model.events.borrow().first().cloned().unwrap_or_default();
            assert_eq!(
                actual, first,
                "NATIVE-ENTRY-FIRST {side} {iteration} {name}"
            );
            native.push(json!({"side":side,"name":name,"first":actual}));
        }
        if side != "exchange" {
            for name in if private {
                vec!["ping"]
            } else {
                vec!["ping", "shouldEnforcePongMissReconnect"]
            } {
                let model = Model::new();
                let bound = class_socket::member(model.ops(), o(vec![]), name, private);
                let result = take(invoke(bound, vec![]));
                if name == "ping" {
                    assert!(matches!(result.as_ref(), Value::U), "NATIVE-ENTRY-PING");
                } else {
                    assert!(
                        matches!(result.as_ref(), Value::B(false)),
                        "NATIVE-ENTRY-POLICY"
                    );
                }
                assert!(model.events.borrow().is_empty(), "NATIVE-ENTRY-STATELESS");
                native.push(json!({"side":side,"name":name,"first":""}));
            }
        }
    }
    assert_eq!(native.len(), 65, "NATIVE-INDEPENDENT-ENTRY-FLOOR");
    assert_eq!(
        native.last().unwrap()["name"],
        "ping",
        "NATIVE-ENTRY-STOP-INDEX64"
    );
    // All native assertions above precede invoking/comparing the source oracle.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/bridge_oracle.cjs"
        ))
        .output()
        .expect("source entry oracle");
    assert!(
        output.status.success(),
        "SOURCE-BRIDGE-ORACLE {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("source entry JSON");
    assert_eq!(
        source["parserOrder"],
        json!([{ "field": "quantity", "original": 1, "replacement": 0 }, { "field": "ref_price", "original": 2, "replacement": 0 }]),
        "SOURCE-PARSE-FLOAT-CAPTURE"
    );
    assert_eq!(
        source["enumProof"],
        json!({"orderSides": [true,true], "enumCounts": [3,2,2], "positionSides": [true,true,true], "positionMargins": [true,true,true], "positionIds": [true,true,true]}),
        "SOURCE-ENUM-NATIVE-IDENTITY"
    );
    let rows = source["rows"].as_array().expect("source rows");
    assert_eq!(rows.len(), 65, "SOURCE-INDEPENDENT-ENTRY-FLOOR");
    assert_eq!(
        rows.last().unwrap()["name"],
        "dispose",
        "SOURCE-ENTRY-STOP-INDEX64"
    );
    for actual in native {
        let expected = rows
            .iter()
            .find(|row| row["side"] == actual["side"] && row["name"] == actual["name"])
            .expect("source original member");
        assert_eq!(
            actual["first"], expected["first"],
            "SOURCE-BRIDGE-ENTRY {}:{}",
            actual["side"], actual["name"]
        );
    }
}
#[test]
fn assembled_read_callback_survives_return_and_observes_native_value() {
    let model = Model::new();
    let saved = Rc::new(RefCell::new(None));
    let capture = saved.clone();
    let rows = o(vec![(
        "filter",
        function(move |args| {
            capture.replace(Some(args[0].clone()));
            Ok(a(vec![]))
        }),
    )]);
    let response = o(vec![("data", o(vec![("data", rows)]))]);
    let xhr = o(vec![("post", function(move |_| Ok(response.clone())))]);
    put(
        &model.globals,
        "ENDPOINTS",
        o(vec![("TICKERS", s("tickers"))]),
    );
    let bound = class_exchange::member(model.ops(), o(vec![("xhr", xhr)]), "fetchTickers");
    let returned = take(settle(take(invoke(bound, vec![]))));
    let Value::A(result) = returned.as_ref() else {
        panic!("native ticker array")
    };
    assert!(
        result.borrow().is_empty(),
        "NATIVE-RETAINED-CALLBACK-RETURN"
    );
    let original = o(vec![("opaque", b(true))]);
    let expected = original.clone();
    let instrument = o(vec![]);
    let receiver = instrument.clone();
    put(
        &instrument,
        "includes",
        method(move |actual, args| {
            assert!(
                Rc::ptr_eq(&actual, &receiver),
                "NATIVE-RETAINED-CALLBACK-RECEIVER"
            );
            assert!(
                matches!(args[0].as_ref(),Value::S(v)if v=="PERP"),
                "NATIVE-RETAINED-CALLBACK-ARGUMENT"
            );
            Ok(original.clone())
        }),
    );
    let callback = saved
        .borrow()
        .as_ref()
        .expect("retained source callback")
        .clone();
    let value = take(invoke(callback, vec![o(vec![("i", instrument)])]));
    assert!(
        Rc::ptr_eq(&value, &expected),
        "NATIVE-RETAINED-CALLBACK-ORIGINAL-RESULT"
    );
}

#[test]
fn native_parse_float_captures_callee_before_order_field_getters() {
    // Quantity and the nullish price chain are distinct source call expressions.
    for (iteration, key) in ["quantity", "ref_price"].into_iter().enumerate() {
        let model = Model::new();
        let order = o(vec![("quantity", s("7")), ("ref_price", s("11"))]);
        let old_calls = Rc::new(Cell::new(0));
        let seen = old_calls.clone();
        put(
            &model.globals,
            "parseFloat",
            function(move |args| {
                seen.set(seen.get() + 1);
                Ok(n(num(&args[0])))
            }),
        );
        let replacements = Rc::new(Cell::new(0));
        let calls = replacements.clone();
        let replacement = function(move |_| {
            calls.set(calls.get() + 1);
            Ok(n(999.))
        });
        let original_order = order.clone();
        let globals = model.globals.clone();
        let phase = model.error.clone();
        model.get_hook.replace(Some(Rc::new(move |value, field| {
            if Rc::ptr_eq(value, &original_order) {
                if field == key {
                    put(&globals, "parseFloat", replacement.clone());
                }
                if field
                    == if key == "quantity" {
                        "cumulative_quantity"
                    } else {
                        "status"
                    }
                {
                    return Err(phase.clone());
                }
            }
            Ok(())
        })));
        let member = class_exchange::member(model.ops(), o(vec![]), "mapOrder");
        let result = invoke(member, vec![order]);
        assert!(
            matches!(result, Err(ref error) if Rc::ptr_eq(error, &model.error)),
            "NATIVE-PARSE-FLOAT-STOP-PHASE {iteration}"
        );
        assert_eq!(
            old_calls.get(),
            iteration + 1,
            "NATIVE-PARSE-FLOAT-CAPTURE {iteration}"
        );
        assert_eq!(
            replacements.get(),
            0,
            "NATIVE-PARSE-FLOAT-CAPTURE {iteration}"
        );
        // Complete the same order so the parser result and fresh later lookup are checked.
        old_calls.set(0);
        replacements.set(0);
        let original = old_calls.clone();
        put(
            &model.globals,
            "parseFloat",
            function(move |args| {
                original.set(original.get() + 1);
                Ok(n(num(&args[0])))
            }),
        );
        let changes = replacements.clone();
        let replacement = function(move |_| {
            changes.set(changes.get() + 1);
            Ok(n(999.))
        });
        let globals = model.globals.clone();
        model.get_hook.replace(Some(Rc::new(move |_, field| {
            if field == key {
                put(&globals, "parseFloat", replacement.clone());
            }
            Ok(())
        })));
        put(&model.globals, "OrderSide", o(vec![("Sell", s("sell"))]));
        put(
            &model.globals,
            "Math",
            o(vec![(
                "max",
                function(|args| Ok(n(num(&args[0]).max(num(&args[1]))))),
            )]),
        );
        let root = o(vec![
            ("mapCryptocomOrderType", function(|_| Ok(u()))),
            ("mapOrderStatus", function(|_| Ok(u()))),
            ("mapCryptocomTimeInForce", function(|_| Ok(u()))),
        ]);
        let bound = class_exchange::member(model.ops(), root, "mapOrder");
        let output = take(invoke(
            bound,
            vec![o(vec![
                ("quantity", s("7")),
                ("ref_price", s("11")),
                ("update_time", n(1.)),
            ])],
        ));
        assert!(
            matches!(raw(&output,"amount").as_ref(), Value::N(v) if *v == 7.),
            "NATIVE-PARSE-FLOAT-AMOUNT {iteration}"
        );
        let expected_price = if key == "quantity" { 999. } else { 11. };
        assert!(
            matches!(raw(&output,"price").as_ref(), Value::N(v) if *v == expected_price),
            "NATIVE-PARSE-FLOAT-PRICE {iteration}"
        );
        assert_eq!(
            (old_calls.get(), replacements.get()),
            if key == "quantity" { (1, 1) } else { (2, 0) },
            "NATIVE-PARSE-FLOAT-FRESH-CALLEE {iteration}"
        );
    }
}

#[test]
fn assembled_order_retains_imported_side_value() {
    for (iteration, (input, field)) in [("BUY", "Buy"), ("SELL", "Sell")].into_iter().enumerate() {
        let model = Model::new();
        let expected = o(vec![("side", s(field))]);
        put(
            &model.globals,
            "OrderSide",
            o(vec![(field, expected.clone())]),
        );
        put(
            &model.globals,
            "parseFloat",
            function(|args| Ok(n(num(&args[0])))),
        );
        put(
            &model.globals,
            "Math",
            o(vec![(
                "max",
                function(|args| Ok(n(num(&args[0]).max(num(&args[1]))))),
            )]),
        );
        let root = o(vec![
            ("mapCryptocomOrderType", function(|_| Ok(u()))),
            ("mapOrderStatus", function(|_| Ok(u()))),
            ("mapCryptocomTimeInForce", function(|_| Ok(u()))),
        ]);
        let order = o(vec![
            ("quantity", s("2")),
            ("ref_price", s("3")),
            ("side", s(input)),
            ("update_time", n(1.)),
        ]);
        let bound = class_exchange::member(model.ops(), root, "mapOrder");
        let result = take(invoke(bound, vec![order]));
        assert!(
            Rc::ptr_eq(&raw(&result, "side"), &expected),
            "NATIVE-ORDER-SIDE-IMPORT {iteration}"
        );
    }
}

#[test]
fn assembled_positions_retain_each_live_enum_read() {
    let model = Model::new();
    let longs = [o(vec![]), o(vec![]), o(vec![])];
    let shorts = [o(vec![]), o(vec![])];
    let cross = [o(vec![]), o(vec![])];
    let isolated = o(vec![]);
    let sides = o(vec![]);
    let margins = o(vec![("Isolated", isolated.clone())]);
    put(&model.globals, "PositionSide", sides.clone());
    put(&model.globals, "MarginType", margins.clone());
    let counts = Rc::new(RefCell::new([0usize; 3]));
    let observed = counts.clone();
    let expected_longs = longs.clone();
    let expected_shorts = shorts.clone();
    let expected_cross = cross.clone();
    model.get_hook.replace(Some(Rc::new(move |value, key| {
        let index = if Rc::ptr_eq(value, &sides) && key == "Long" {
            Some(0)
        } else if Rc::ptr_eq(value, &sides) && key == "Short" {
            Some(1)
        } else if Rc::ptr_eq(value, &margins) && key == "Cross" {
            Some(2)
        } else {
            None
        };
        if let Some(index) = index {
            let at = observed.borrow()[index];
            let entry = match index {
                0 => &expected_longs[at],
                1 => &expected_shorts[at],
                _ => &expected_cross[at],
            };
            put(value, key, entry.clone());
            observed.borrow_mut()[index] += 1;
        }
        Ok(())
    })));
    put(
        &model.globals,
        "parseFloat",
        function(|args| Ok(n(num(&args[0])))),
    );
    put(
        &model.globals,
        "Math",
        o(vec![("abs", function(|args| Ok(n(num(&args[0]).abs()))))]),
    );
    put(
        &model.globals,
        "virtualClock",
        o(vec![("getCurrentTimeMs", function(|_| Ok(n(1.))))]),
    );
    put(
        &model.globals,
        "ENDPOINTS",
        o(vec![("POSITIONS", s("positions"))]),
    );
    put(
        &model.globals,
        "Set",
        function(|_| Ok(o(vec![("has", function(|_| Ok(b(false))))]))),
    );
    put(
        &model.globals,
        "uniqBy",
        function(|args| Ok(args[0].clone())),
    );
    let ids = Rc::new(RefCell::new(vec![]));
    let id_calls = ids.clone();
    put(
        &model.globals,
        "createPositionId",
        function(move |args| {
            id_calls.borrow_mut().push(args[1].clone());
            Ok(n(id_calls.borrow().len() as f64))
        }),
    );
    let position = o(vec![
        ("quantity", s("2")),
        ("cost", s("6")),
        ("instrument_name", s("ACTUAL")),
        ("open_position_pnl", s("1")),
        ("isolation_type", s("ISOLATED_MARGIN")),
        ("update_timestamp_ms", n(1.)),
    ]);
    let response = o(vec![("data", o(vec![("data", a(vec![position]))]))]);
    let root = o(vec![
        (
            "xhr",
            o(vec![("post", function(move |_| Ok(response.clone())))]),
        ),
        ("getPositionMode", function(|_| Ok(s("hedged")))),
        (
            "store",
            o(vec![(
                "markets",
                a(vec![o(vec![
                    ("symbol", s("FAKE")),
                    ("active", b(true)),
                    ("isSpot", b(false)),
                ])]),
            )]),
        ),
        (
            "log",
            function(|_| panic!("NATIVE-POSITION-UNEXPECTED-CATCH")),
        ),
    ]);
    let bound = class_exchange::member(model.ops(), root, "fetchPositions");
    let result = take(settle(take(invoke(bound, vec![]))));
    let Value::A(rows) = result.as_ref() else {
        panic!("native positions array")
    };
    let rows = rows.borrow();
    assert_eq!(rows.len(), 3, "NATIVE-POSITION-INDEPENDENT-FLOOR");
    assert_eq!(
        *counts.borrow(),
        [3, 2, 2],
        "NATIVE-POSITION-ENUM-READ-COUNTS"
    );
    for (iteration, expected) in [longs[0].clone(), longs[2].clone(), shorts[1].clone()]
        .into_iter()
        .enumerate()
    {
        assert!(
            Rc::ptr_eq(&raw(&rows[iteration], "side"), &expected),
            "NATIVE-POSITION-SIDE-IMPORT {iteration}"
        );
    }
    for (iteration, expected) in [isolated, cross[0].clone(), cross[1].clone()]
        .into_iter()
        .enumerate()
    {
        assert!(
            Rc::ptr_eq(&raw(&rows[iteration], "marginType"), &expected),
            "NATIVE-POSITION-MARGIN-IMPORT {iteration}"
        );
    }
    let ids = ids.borrow();
    assert_eq!(ids.len(), 3, "NATIVE-POSITION-ID-COUNT");
    for (iteration, expected) in [longs[0].clone(), longs[1].clone(), shorts[0].clone()]
        .into_iter()
        .enumerate()
    {
        assert!(
            Rc::ptr_eq(&ids[iteration], &expected),
            "NATIVE-POSITION-ID-SIDE-IMPORT {iteration}"
        );
    }
}

// Include inside the existing native_bindings bridge test module.
// Static-only proposal: four value/order cases, then one throwing-getter case.
fn socket_enum_model() -> Rc<Model> {
    let model = Model::new();
    put(
        &model.globals,
        "parseFloat",
        function(|args| Ok(n(num(&args[0])))),
    );
    put(
        &model.globals,
        "Math",
        o(vec![
            (
                "max",
                function(|args| Ok(n(num(&args[0]).max(num(&args[1]))))),
            ),
            ("abs", function(|args| Ok(n(num(&args[0]).abs())))),
        ]),
    );
    model
}

#[test]
fn socket_order_retains_selected_enum_after_side_getter() {
    // Independent branch floor2; stop index1 SELL/Sell.
    for (iteration, (input, member)) in [("BUY", "Buy"), ("SELL", "Sell")].into_iter().enumerate() {
        let model = socket_enum_model();
        let selected = o(vec![("opaque-order-side", s(member))]);
        let imported = o(vec![("Buy", o(vec![])), ("Sell", o(vec![]))]);
        put(&model.globals, "OrderSide", imported.clone());
        let row = o(vec![
            ("instrument_name", s("BTC")),
            ("quantity", s("2")),
            ("ref_price", s("3")),
            ("side", s(input)),
            ("update_time", n(1.)),
        ]);
        let reads = Rc::new(RefCell::new(Vec::<String>::new()));
        let observed = reads.clone();
        let hooked_row = row.clone();
        let hooked_enum = imported.clone();
        let hooked_math = raw(&model.globals, "Math");
        let replacement = selected.clone();
        model.get_hook.replace(Some(Rc::new(move |value, key| {
            if Rc::ptr_eq(value, &hooked_row) && key == "side" {
                observed.borrow_mut().push("row.side".into());
                // A real source property getter can change the imported enum object.
                put(&hooked_enum, member, replacement.clone());
            }
            if Rc::ptr_eq(value, &hooked_enum) {
                assert_eq!(key, member, "NATIVE-SOCKET-ORDER-SELECTED-ENUM {iteration}");
                observed.borrow_mut().push(format!("OrderSide.{key}"));
            }
            if Rc::ptr_eq(value, &hooked_math) && key == "max" {
                observed.borrow_mut().push("Math.max".into());
            }
            Ok(())
        })));
        let frames = Rc::new(RefCell::new(Vec::<V>::new()));
        let captured = frames.clone();
        let store = o(vec![
            (
                "marketsById",
                o(vec![("BTC", o(vec![("symbol", s("BTC"))]))]),
            ),
            (
                "addOrUpdateOrder",
                function(move |args| {
                    captured.borrow_mut().push(args[0].clone());
                    Ok(u())
                }),
            ),
        ]);
        let receiver = o(vec![
            ("store", store),
            ("mapOrderType", function(|_| Ok(u()))),
            ("mapOrderStatus", function(|_| Ok(u()))),
            ("mapTimeInForce", function(|_| Ok(u()))),
            (
                "parent",
                o(vec![(
                    "emitAndLogError",
                    function(|_| panic!("NATIVE-SOCKET-ORDER-UNEXPECTED-CATCH")),
                )]),
            ),
        ]);
        let output = take(crate::socket_handlers::order_update(
            &model.ops().value,
            &receiver,
            &o(vec![("data", a(vec![row]))]),
        ));
        assert!(
            matches!(output.as_ref(), Value::U),
            "NATIVE-SOCKET-ORDER-UNDEFINED {iteration}"
        );
        let frames = frames.borrow();
        assert_eq!(
            frames.len(),
            1,
            "NATIVE-SOCKET-ORDER-FRAME-COUNT {iteration}"
        );
        assert!(
            Rc::ptr_eq(&raw(&frames[0], "side"), &selected),
            "NATIVE-SOCKET-ORDER-SIDE-IDENTITY {iteration}"
        );
        assert_eq!(
            *reads.borrow(),
            vec![
                "row.side".to_owned(),
                format!("OrderSide.{member}"),
                "Math.max".to_owned()
            ],
            "NATIVE-SOCKET-ORDER-ENUM-ORDER {iteration}"
        );
    }
}

#[test]
fn socket_position_retains_side_and_margin_at_source_order() {
    // Independent branch floor2; stop index1 negative/Cross.
    for (iteration, (quantity, side_member, isolation, margin_member)) in [
        ("2", "Long", "ISOLATED_MARGIN", "Isolated"),
        ("-2", "Short", "CROSS_MARGIN", "Cross"),
    ]
    .into_iter()
    .enumerate()
    {
        let model = socket_enum_model();
        let side = o(vec![("opaque-position-side", s(side_member))]);
        let margin = o(vec![("opaque-margin", s(margin_member))]);
        let imported_side = o(vec![(side_member, side.clone())]);
        let imported_margin = o(vec![("Isolated", o(vec![])), ("Cross", o(vec![]))]);
        put(&model.globals, "PositionSide", imported_side.clone());
        put(&model.globals, "MarginType", imported_margin.clone());
        let row = o(vec![
            ("instrument_name", s("BTC")),
            ("quantity", s(quantity)),
            ("cost", s("6")),
            ("leverage", s("1")),
            ("liquidation_price", s("0")),
            ("mark_price", s("0")),
            ("isolation_type", s(isolation)),
            ("open_position_pnl", s("1")),
            ("update_timestamp_ms", n(1.)),
        ]);
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let observed = trace.clone();
        let hooked_row = row.clone();
        let hooked_side = imported_side.clone();
        let hooked_margin = imported_margin.clone();
        let hooked_math = raw(&model.globals, "Math");
        let replacement = margin.clone();
        model.get_hook.replace(Some(Rc::new(move |value, key| {
            if Rc::ptr_eq(value, &hooked_row)
                && matches!(
                    key,
                    "cost" | "leverage" | "isolation_type" | "open_position_pnl"
                )
            {
                observed.borrow_mut().push(key.into());
                if key == "isolation_type" {
                    put(&hooked_margin, margin_member, replacement.clone());
                }
            }
            if Rc::ptr_eq(value, &hooked_side) {
                assert_eq!(
                    key, side_member,
                    "NATIVE-SOCKET-POSITION-SELECTED-SIDE {iteration}"
                );
                observed.borrow_mut().push("PositionSide".into());
            }
            if Rc::ptr_eq(value, &hooked_margin) {
                assert_eq!(
                    key, margin_member,
                    "NATIVE-SOCKET-POSITION-SELECTED-MARGIN {iteration}"
                );
                observed.borrow_mut().push("MarginType".into());
            }
            if Rc::ptr_eq(value, &hooked_math) && key == "abs" {
                observed.borrow_mut().push("Math.abs".into());
            }
            Ok(())
        })));
        let expected_id_side = side.clone();
        let ids = Rc::new(Cell::new(0));
        let id_count = ids.clone();
        put(
            &model.globals,
            "createPositionId",
            function(move |args| {
                assert!(
                    Rc::ptr_eq(&args[1], &expected_id_side),
                    "NATIVE-SOCKET-POSITION-ID-SIDE {iteration}"
                );
                id_count.set(id_count.get() + 1);
                Ok(s("native-id"))
            }),
        );
        let frames = Rc::new(RefCell::new(Vec::<V>::new()));
        let captured = frames.clone();
        let receiver = o(vec![
            (
                "store",
                o(vec![
                    (
                        "marketsById",
                        o(vec![("BTC", o(vec![("symbol", s("BTC"))]))]),
                    ),
                    (
                        "addOrUpdatePosition",
                        function(move |args| {
                            captured.borrow_mut().push(args[0].clone());
                            Ok(u())
                        }),
                    ),
                ]),
            ),
            (
                "parent",
                o(vec![
                    ("getPositionMode", function(|_| Ok(s("hedged")))),
                    (
                        "emitAndLogError",
                        function(|_| panic!("NATIVE-SOCKET-POSITION-UNEXPECTED-CATCH")),
                    ),
                ]),
            ),
        ]);
        let ops = model.ops();
        let future = crate::socket_handlers::position_update(
            ops.value.clone(),
            receiver,
            o(vec![("data", a(vec![row]))]),
        );
        let output = take(settle((ops.promise)(future)));
        assert!(
            matches!(output.as_ref(), Value::U),
            "NATIVE-SOCKET-POSITION-UNDEFINED {iteration}"
        );
        let frames = frames.borrow();
        assert_eq!(
            frames.len(),
            1,
            "NATIVE-SOCKET-POSITION-FRAME-COUNT {iteration}"
        );
        assert!(
            Rc::ptr_eq(&raw(&frames[0], "side"), &side),
            "NATIVE-SOCKET-POSITION-SIDE-IDENTITY {iteration}"
        );
        assert!(
            Rc::ptr_eq(&raw(&frames[0], "marginType"), &margin),
            "NATIVE-SOCKET-POSITION-MARGIN-IDENTITY {iteration}"
        );
        assert_eq!(ids.get(), 1, "NATIVE-SOCKET-POSITION-ID-COUNT {iteration}");
        assert_eq!(
            *trace.borrow(),
            vec![
                "cost",
                "PositionSide",
                "Math.abs",
                "leverage",
                "Math.abs",
                "isolation_type",
                "MarginType",
                "open_position_pnl"
            ],
            "NATIVE-SOCKET-POSITION-ENUM-ORDER {iteration}"
        );
    }
}

#[test]
fn socket_selected_order_enum_getter_error_prevents_frame() {
    let model = socket_enum_model();
    let imported = o(vec![("Buy", o(vec![]))]);
    put(&model.globals, "OrderSide", imported.clone());
    let getter_error = o(vec![("original-enum-error", b(true))]);
    let getter_calls = Rc::new(Cell::new(0));
    let observed = getter_calls.clone();
    model.get_hook.replace(Some(Rc::new(move |value, key| {
        if Rc::ptr_eq(value, &imported) && key == "Buy" {
            observed.set(observed.get() + 1);
            return Err(getter_error.clone());
        }
        Ok(())
    })));
    let emitted = Rc::new(Cell::new(0));
    let count = emitted.clone();
    let frames = Rc::new(Cell::new(0));
    let frame_count = frames.clone();
    let receiver = o(vec![
        (
            "store",
            o(vec![
                (
                    "marketsById",
                    o(vec![("BTC", o(vec![("symbol", s("BTC"))]))]),
                ),
                (
                    "addOrUpdateOrder",
                    function(move |_| {
                        frame_count.set(frame_count.get() + 1);
                        Ok(u())
                    }),
                ),
            ]),
        ),
        ("mapOrderType", function(|_| Ok(u()))),
        (
            "parent",
            o(vec![(
                "emitAndLogError",
                function(move |_| {
                    count.set(count.get() + 1);
                    Ok(u())
                }),
            )]),
        ),
    ]);
    let row = o(vec![
        ("instrument_name", s("BTC")),
        ("quantity", s("2")),
        ("ref_price", s("3")),
        ("side", s("BUY")),
    ]);
    let output = take(crate::socket_handlers::order_update(
        &model.ops().value,
        &receiver,
        &o(vec![("data", a(vec![row]))]),
    ));
    assert!(
        matches!(output.as_ref(), Value::U),
        "NATIVE-SOCKET-ENUM-ERROR-UNDEFINED"
    );
    assert_eq!(
        getter_calls.get(),
        1,
        "NATIVE-SOCKET-ENUM-GETTER-ERROR-COUNT"
    );
    assert_eq!(emitted.get(), 1, "NATIVE-SOCKET-ENUM-ERROR-EMITTED");
    assert_eq!(frames.get(), 0, "NATIVE-SOCKET-ENUM-ERROR-NO-FRAME");
}

#[test]
fn constructed_socket_fields_call_concrete_bodies() {
    use crate::socket_classes::{self, SocketClassOps};
    // Constructor floor2, stopping at index1 private subscriptions.
    for (iteration, private) in [false, true].into_iter().enumerate() {
        let model = Model::new();
        let added = Rc::new(RefCell::new(vec![]));
        let channels = added.clone();
        let subscriptions = o(vec![
            ("has", function(|_| Ok(b(false)))),
            (
                "add",
                function(move |args| {
                    channels.borrow_mut().push(args[0].clone());
                    Ok(u())
                }),
            ),
        ]);
        put(
            &model.globals,
            "Set",
            function(move |_| Ok(subscriptions.clone())),
        );
        put(&model.globals, "Map", function(|_| Ok(o(vec![]))));
        put(
            &model.globals,
            "throttle",
            function(|args| Ok(args[0].clone())),
        );
        let receiver = o(vec![("isDisposed", b(true))]);
        let expected_receiver = receiver.clone();
        let parent = o(vec![]);
        let original_parent = parent.clone();
        let binding = model.ops();
        let ops = Rc::new(SocketClassOps {
            body: binding.value.clone(),
            binding: binding.clone(),
            promise: binding.promise.clone(),
            define: Rc::new(|value, key, field| {
                put(value, key, field);
                Ok(())
            }),
            superclass: Rc::new(|_| Ok(u())),
            super_construct: Rc::new(move |_, _, args| {
                assert_eq!(args.len(), 1, "NATIVE-ASSEMBLED-SOCKET-SUPER-COUNT");
                assert!(
                    Rc::ptr_eq(&args[0], &original_parent),
                    "NATIVE-ASSEMBLED-SOCKET-PARENT"
                );
                Ok(expected_receiver.clone())
            }),
            construct: Rc::new(invoke),
            derive: Rc::new(|_, _, _, _| panic!("direct constructor witness")),
        });
        let result = take(if private {
            socket_classes::private_constructor(ops, u(), u(), vec![parent])
        } else {
            socket_classes::public_constructor(ops, u(), u(), vec![parent])
        });
        assert!(
            Rc::ptr_eq(&result, &receiver),
            "NATIVE-ASSEMBLED-SOCKET-RECEIVER {iteration}"
        );
        let name = if private {
            "subscribeToPositionUpdates"
        } else {
            "onOpen"
        };
        let member = raw(&result, name);
        assert!(
            matches!(member.as_ref(), Value::F(_)),
            "NATIVE-ASSEMBLED-SOCKET-FIELD {iteration}"
        );
        model.events.borrow_mut().clear();
        let value = take(native_invoke(
            member,
            o(vec![("isDisposed", b(false))]),
            vec![],
        ));
        assert!(
            matches!(value.as_ref(), Value::U),
            "NATIVE-ASSEMBLED-SOCKET-UNDEFINED {iteration}"
        );
        if private {
            let channels = added.borrow();
            assert_eq!(channels.len(), 1, "NATIVE-ASSEMBLED-PRIVATE-CHANNEL-COUNT");
            assert!(
                matches!(channels[0].as_ref(), Value::S(v) if v == "user.positions"),
                "NATIVE-ASSEMBLED-PRIVATE-CHANNEL"
            );
        } else {
            assert_eq!(
                *model.events.borrow(),
                ["get:isDisposed"],
                "NATIVE-ASSEMBLED-PUBLIC-LEXICAL-RECEIVER"
            );
        }
    }
}

#[test]
#[ignore = "requires Node proof-admission unit tests"]
fn proof_admission_requires_independent_control_pairs() {
    let output = std::process::Command::new("node")
        .args([
            "--test",
            "--test-reporter=tap",
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/bridge-proof.test.mjs"),
        ])
        .output()
        .expect("proof-admission unit tests");
    assert!(
        output.status.success(),
        "BRIDGE-PROOF-GUARD stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
