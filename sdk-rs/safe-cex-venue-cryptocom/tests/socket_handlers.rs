use safe_cex_venue_cryptocom::{
    native_bindings::BindingOps,
    socket_classes::{self as classes, SocketClassOps},
    socket_handlers::{self as port, SocketCallback, SocketOps},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = Rc<String>;
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    Symbol(String),
    O(RefCell<Vec<(V, V)>>),
    A(RefCell<Vec<V>>),
    F(String, usize, SocketCallback<V, E>),
    Builtin(String),
    Set,
    Map,
    Class {
        name: String,
        length: usize,
        base: V,
        body: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    },
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
        x.into_iter().map(|(k, v)| (s(k), v)).collect(),
    )))
}
fn keys(mut x: Vec<(V, V)>) -> V {
    x.sort_by_key(|(key, _)| matches!(key.as_ref(), Native::Symbol(_)));
    Rc::new(Native::O(RefCell::new(x)))
}
fn text(v: &V) -> String {
    match v.as_ref() {
        Native::U => "undefined".into(),
        Native::Null => "null".into(),
        Native::S(x) => String::from_utf16_lossy(x),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::B(x) => x.to_string(),
        _ => "[object Object]".into(),
    }
}
fn number(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::U => f64::NAN,
        Native::Null => 0.,
        Native::B(x) => {
            if *x {
                1.
            } else {
                0.
            }
        }
        Native::S(x) => String::from_utf16_lossy(x).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::U | Native::Null => false,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::B(x) => *x,
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        (Native::B(a), Native::B(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::S(a), Native::S(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn raw(v: &V, key: &V) -> V {
    match v.as_ref() {
        Native::O(fields) => fields
            .borrow()
            .iter()
            .find(|(k, _)| equal(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(a) => {
            let key = text(key);
            if key == "length" {
                n(a.borrow().len() as f64)
            } else {
                key.parse::<usize>()
                    .ok()
                    .and_then(|i| a.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        _ => u(),
    }
}
fn get(v: &V, key: &str) -> V {
    raw(v, &s(key))
}
fn put(v: &V, key: V, value: V) {
    let Native::O(fields) = v.as_ref() else {
        panic!("native object assignment")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| equal(k, &key)) {
        *v = value
    } else {
        fields.push((key, value));
    }
}
fn values(v: &V) -> Vec<V> {
    let Native::A(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn copy(v: &V) -> V {
    match v.as_ref() {
        Native::A(a) => array(a.borrow().iter().map(copy).collect()),
        Native::O(a) => keys(
            a.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), copy(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}
fn decode(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Native::Null),
        Value::Bool(x) => b(*x),
        Value::Number(x) => n(x.as_f64().unwrap()),
        Value::String(x) => s(x),
        Value::Array(x) => array(x.iter().map(decode).collect()),
        Value::Object(x) => {
            if x.get("$undefined") == Some(&json!(true)) {
                return u();
            }
            if let Some(x) = x.get("$number").and_then(Value::as_str) {
                return n(match x {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    "-0" => -0.,
                    _ => panic!("number tag"),
                });
            }
            keys(x.iter().map(|(k, v)| (s(k), decode(v))).collect())
        }
    }
}
struct Host {
    spec: Value,
    events: RefCell<Vec<V>>,
    timers: RefCell<Vec<V>>,
    receiver: V,
    payload: V,
    error: E,
    last_book: RefCell<Option<V>>,
    coerces: Cell<usize>,
    parent_argument: RefCell<Option<V>>,
    target_argument: RefCell<Option<V>>,
    super_receiver: RefCell<Option<V>>,
    loaded_symbol: RefCell<Option<V>>,
    reads: Cell<usize>,
}
impl Host {
    fn event(&self, kind: &str, args: Vec<V>) {
        self.events
            .borrow_mut()
            .push(array(std::iter::once(s(kind)).chain(args).collect()));
    }
    fn fail(&self, key: &str) -> Result<(), E> {
        if self.spec["fail"] == key {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn property(&self, v: &V, key: &str) -> Result<V, E> {
        self.reads.set(self.reads.get() + 1);
        if matches!(v.as_ref(), Native::U | Native::Null) {
            return Err(Rc::new(format!(
                "Cannot read properties of {} (reading '{key}')",
                text(v)
            )));
        }
        if key == "recordMessageHandlerStats" && self.spec["statsChange"] == true {
            put(&get(&self.payload, "result"), s("channel"), s("trade"));
        }
        let found = get(v, key);
        if !matches!(found.as_ref(), Native::U) {
            return Ok(found);
        }
        if matches!(v.as_ref(), Native::A(_))
            && matches!(key, "map" | "filter" | "forEach" | "includes")
        {
            return Ok(Rc::new(Native::Builtin(key.into())));
        }
        if matches!(v.as_ref(), Native::S(_)) && key == "startsWith" {
            return Ok(Rc::new(Native::Builtin(key.into())));
        }
        Ok(found)
    }
    fn invoke(self: &Rc<Self>, callee: V, receiver: V, args: Vec<V>) -> Result<V, E> {
        match callee.as_ref() {
            Native::F(_, _, f) => f(args),
            Native::Builtin(name) => self.builtin(name, receiver, args),
            _ => Err(Rc::new("not a function".into())),
        }
    }
    fn builtin(self: &Rc<Self>, name: &str, receiver: V, args: Vec<V>) -> Result<V, E> {
        let arg = |i| args.get(i).cloned().unwrap_or_else(u);
        match name {
            "jsonParse" => {
                self.fail("json")?;
                Ok(arg(0))
            }
            "parseFloat" => Ok(n(safe_cex_num::parse_float(&text(&arg(0))))),
            "getCurrentTimeMs" => {
                self.event("clock", vec![]);
                Ok(n(777.))
            }
            "stringify" => {
                self.event("frame", vec![arg(0)]);
                Ok(s("encoded"))
            }
            "send" => {
                assert_eq!(text(&arg(0)), "encoded", "NATIVE-SEND-DEPENDENCY");
                self.event("send", vec![]);
                self.fail("send")?;
                Ok(u())
            }
            "emitAndLogError" => {
                self.event("error", vec![arg(0)]);
                self.fail("emit")?;
                Ok(u())
            }
            "trackLastReceived" => {
                self.event("track", vec![]);
                self.fail("track")?;
                Ok(u())
            }
            "recordMessageHandlerStats" => {
                self.event("stats", vec![arg(0)]);
                self.fail("stats")?;
                Ok(u())
            }
            "resubscribe" => {
                self.event("resubscribe", vec![get(&self.receiver, "isAuthenticated")]);
                self.fail("resubscribe")?;
                Ok(s("ignored"))
            }
            "startsWith" => Ok(b(text(&receiver).starts_with(&text(&arg(0))))),
            "max" => Ok(n(number(&arg(0)).max(number(&arg(1))))),
            "abs" => Ok(n(number(&arg(0)).abs())),
            "getPositionMode" => {
                self.event("mode", vec![]);
                self.fail("mode")?;
                if self.spec["modeRejected"] == true {
                    Ok(Rc::new(Native::Builtin("rejected".into())))
                } else {
                    Ok(s(self.spec["mode"].as_str().unwrap_or("hedged")))
                }
            }
            "createPositionId" => Ok(if text(&arg(2)) == "oneway" {
                arg(0)
            } else {
                s(&format!("{}:{}", text(&arg(0)), text(&arg(1))))
            }),
            "updateTicker" => {
                self.event(
                    "ticker",
                    vec![
                        b(Rc::ptr_eq(
                            &arg(0),
                            &get(
                                &get(&get(&self.receiver, "store"), "tickersBySymbol"),
                                "BTC-SYMBOL",
                            ),
                        )),
                        arg(1),
                    ],
                );
                self.fail("ticker")?;
                Ok(u())
            }
            "addOrUpdateOrder" => {
                self.event("order", vec![arg(0)]);
                self.fail("order")?;
                Ok(u())
            }
            "addOrUpdatePosition" => {
                self.event("position", vec![arg(0)]);
                self.fail("position")?;
                Ok(u())
            }
            "update" => {
                self.event("store", vec![arg(0)]);
                self.fail("store")?;
                Ok(u())
            }
            "mapOrderType" => Ok(s(match text(&arg(0)).as_str() {
                "MARKET" => "market",
                "STOP_LIMIT" => "stop_market",
                _ => "limit",
            })),
            "mapOrderStatus" => Ok(s(match text(&arg(0)).as_str() {
                "FILLED" => "closed",
                "CANCELED" => "canceled",
                _ => "open",
            })),
            "mapTimeInForce" => Ok(s(if text(&arg(0)) == "POST_ONLY" {
                "PostOnly"
            } else {
                "GoodTillCancel"
            })),
            "setTimeout" => {
                self.event("timer", vec![arg(1)]);
                self.timers.borrow_mut().push(arg(0));
                Ok(n(self.timers.borrow().len() as f64))
            }
            "mapCandlestickInterval" => Ok(arg(0)),
            "subscribeToCandlestick" | "subscribeToOrderBook" => {
                self.event(
                    if name == "subscribeToCandlestick" {
                        "subscribe-candle"
                    } else {
                        "subscribe-book"
                    },
                    args,
                );
                self.fail("subscribe")?;
                Ok(u())
            }
            "unsubscribe" => {
                self.event("unsubscribe", vec![arg(0)]);
                self.fail("unsubscribe")?;
                Ok(u())
            }
            "cleanupMessageHandlerStats" => {
                self.event("cleanup", vec![arg(0)]);
                self.fail("cleanup")?;
                Ok(u())
            }
            "includes" => Ok(b(values(&receiver).iter().any(|v| equal(v, &arg(0))))),
            "map" | "filter" | "forEach" => {
                let input = values(&receiver);
                let mut result = vec![];
                for v in input {
                    let out = self.invoke(arg(0), u(), vec![v.clone()])?;
                    match name {
                        "map" => result.push(out),
                        "filter" if truthy(&out) => result.push(v),
                        _ => {}
                    }
                }
                Ok(if name == "forEach" {
                    u()
                } else {
                    array(result)
                })
            }
            "throttle" => {
                self.event(
                    "throttle",
                    vec![arg(1), get(&arg(2), "leading"), get(&arg(2), "trailing")],
                );
                let host = self.clone();
                let callback = arg(0);
                let first = Cell::new(true);
                Ok(Rc::new(Native::F(
                    "debounced".into(),
                    0,
                    Rc::new(move |args| {
                        if first.replace(false) {
                            host.invoke(callback.clone(), u(), args)
                        } else {
                            let h = host.clone();
                            let f = callback.clone();
                            host.timers.borrow_mut().push(Rc::new(Native::F(
                                "timerExpired".into(),
                                0,
                                Rc::new(move |_| h.invoke(f.clone(), u(), args.clone())),
                            )));
                            Ok(u())
                        }
                    }),
                )))
            }
            "sortOrderBook" => {
                for side in ["asks", "bids"] {
                    let rows = get(&arg(0), side);
                    let Native::A(rows) = rows.as_ref() else {
                        panic!("levels")
                    };
                    rows.borrow_mut().sort_by(|a, b| {
                        let order = number(&get(a, "price")).total_cmp(&number(&get(b, "price")));
                        if side == "bids" {
                            order.reverse()
                        } else {
                            order
                        }
                    });
                }
                Ok(u())
            }
            "calcOrderBookTotal" => {
                for side in ["bids", "asks"] {
                    let mut total = 0.;
                    for row in values(&get(&arg(0), side)) {
                        total += number(&get(&row, "amount"));
                        put(&row, s("total"), n(total));
                    }
                }
                Ok(u())
            }
            _ if name.starts_with("handle") => {
                self.event(
                    "delegate",
                    vec![
                        s(name),
                        b(Rc::ptr_eq(&arg(0), &get(&self.payload, "result"))),
                    ],
                );
                self.fail("handler")?;
                Ok(s("ignored"))
            }
            _ => panic!("unsupported host builtin {name}"),
        }
    }
    fn coerce(&self, v: &V) -> Result<Vec<u16>, E> {
        if matches!(v.as_ref(),Native::Builtin(x)if x=="coercion") {
            self.event("coerce", vec![]);
            let i = self.coerces.get();
            self.coerces.set(i + 1);
            Ok(if i == 0 { "BTC" } else { "ETH" }.encode_utf16().collect())
        } else {
            Ok(text(v).encode_utf16().collect())
        }
    }
}
fn ops(host: Rc<Host>) -> Rc<SocketOps<V, E>> {
    let h = host.clone();
    let get_h = host.clone();
    let index_h = host.clone();
    let global_h = host.clone();
    let text_h = host.clone();
    let await_h = host.clone();
    Rc::new(SocketOps {
        undefined: Rc::new(u),
        boolean: Rc::new(b),
        number: Rc::new(n),
        literal: Rc::new(s),
        object: Rc::new(object),
        array: Rc::new(array),
        get: Rc::new(move |v, key| get_h.property(v, key)),
        index: Rc::new(move |v, key| index_h.property(v, &text(key))),
        key: Rc::new(|v| Ok(v.clone())),
        set: Rc::new(|v, key, x| {
            put(v, s(key), x);
            Ok(())
        }),
        set_index: Rc::new(|v, key, x| {
            put(v, key.clone(), x);
            Ok(())
        }),
        delete: Rc::new(|v, key| {
            let Native::O(fields) = v.as_ref() else {
                panic!("delete")
            };
            fields.borrow_mut().retain(|(k, _)| !equal(k, key));
            Ok(true)
        }),
        global: Rc::new(move |name| {
            let _ = &global_h;
            Ok(match name {
                "OrderSide" => object(vec![("Buy", s("buy")), ("Sell", s("sell"))]),
                "PositionSide" => object(vec![("Long", s("long")), ("Short", s("short"))]),
                "MarginType" => object(vec![("Isolated", s("isolated")), ("Cross", s("cross"))]),
                "Math" => object(vec![
                    ("max", Rc::new(Native::Builtin("max".into()))),
                    ("abs", Rc::new(Native::Builtin("abs".into()))),
                ]),
                "virtualClock" => object(vec![(
                    "getCurrentTimeMs",
                    Rc::new(Native::Builtin("getCurrentTimeMs".into())),
                )]),
                "JSON" => object(vec![(
                    "stringify",
                    Rc::new(Native::Builtin("stringify".into())),
                )]),
                _ => Rc::new(Native::Builtin(name.into())),
            })
        }),
        invoke: Rc::new(move |f, r, args| h.invoke(f, r, args)),
        function: Rc::new(|name, len, body| Rc::new(Native::F(name.into(), len, body))),
        truthy: Rc::new(truthy),
        nullish: Rc::new(|v| matches!(v.as_ref(), Native::Null | Native::U)),
        equal: Rc::new(equal),
        is_number: Rc::new(|v| matches!(v.as_ref(), Native::N(_))),
        has: Rc::new(|v, key| {
            if matches!(v.as_ref(), Native::O(_)) {
                Ok(!matches!(get(v, key).as_ref(), Native::U))
            } else {
                Err(Rc::new("Cannot use 'in' operator".into()))
            }
        }),
        numeric: Rc::new(|v| Ok(number(v))),
        text: Rc::new(move |v| text_h.coerce(v)),
        utf16: Rc::new(|v| Rc::new(Native::S(v))),
        error_value: Rc::new(|e| s(&format!("Error: {e}"))),
        spread: Rc::new(|v| Ok(values(v))),
        spread_object: Rc::new(|v| {
            let Native::O(fields) = v.as_ref() else {
                return Ok(vec![]);
            };
            Ok(fields.borrow().clone())
        }),
        object_keys: Rc::new(keys),
        tuple_two: Rc::new(|v| {
            let rows = values(v);
            Ok((
                rows.first().cloned().unwrap_or_else(u),
                rows.get(1).cloned().unwrap_or_else(u),
            ))
        }),
        await_value: Rc::new(move |v| {
            let error = await_h.error.clone();
            Box::pin(async move {
                if matches!(v.as_ref(),Native::Builtin(x)if x=="rejected") {
                    Err(error)
                } else {
                    Ok(v)
                }
            })
        }),
    })
}
fn host(spec: &Value) -> Rc<Host> {
    let receiver = decode(
        &json!({"isDisposed":spec["disposed"]==true,"isConnected":spec["disconnected"]!=true,"isAuthenticated":false,"messageHandlers":{},"store":{"marketsById":{"BTC":{"symbol":"BTC-SYMBOL"},"ETH":{"symbol":"ETH-SYMBOL"}},"tickersBySymbol":{"BTC-SYMBOL":{"id":"BTC"},"ETH-SYMBOL":{"id":"ETH"}},"positions":[{"symbol":"BTC-SYMBOL","side":"long"},{"symbol":"ETH-SYMBOL"},{"symbol":"BTC-SYMBOL","side":"short"}],"loaded":{"markets":true}},"parent":{}}),
    );
    let h = Rc::new(Host {
        spec: spec.clone(),
        events: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        receiver,
        payload: decode(spec.get("payload").unwrap_or(&json!({}))),
        error: Rc::new("boom".into()),
        last_book: RefCell::new(None),
        coerces: Cell::new(0),
        parent_argument: RefCell::new(None),
        target_argument: RefCell::new(None),
        super_receiver: RefCell::new(None),
        loaded_symbol: RefCell::new(None),
        reads: Cell::new(0),
    });
    for name in [
        "trackLastReceived",
        "recordMessageHandlerStats",
        "resubscribe",
        "send",
        "handleTickerUpdate",
        "handleTradeUpdate",
        "handleBookUpdate",
        "handleCandlestickUpdate",
        "handleValuationUpdate",
        "handleOrderUpdate",
        "handlePositionUpdate",
        "handleBalanceUpdate",
        "mapOrderType",
        "mapOrderStatus",
        "mapTimeInForce",
        "subscribeToCandlestick",
        "subscribeToOrderBook",
        "unsubscribe",
        "cleanupMessageHandlerStats",
    ] {
        put(&h.receiver, s(name), Rc::new(Native::Builtin(name.into())));
    }
    let store = get(&h.receiver, "store");
    for name in [
        "updateTicker",
        "addOrUpdateOrder",
        "addOrUpdatePosition",
        "update",
    ] {
        put(&store, s(name), Rc::new(Native::Builtin(name.into())));
    }
    let parent = get(&h.receiver, "parent");
    for name in ["emitAndLogError", "getPositionMode"] {
        put(&parent, s(name), Rc::new(Native::Builtin(name.into())));
    }
    if spec["loadedSymbol"] == true {
        put(
            &get(&store, "loaded"),
            {
                let key = Rc::new(Native::Symbol("loaded-marker".into()));
                h.loaded_symbol.replace(Some(key.clone()));
                key
            },
            s("retained"),
        );
    }
    h
}
fn ready<F: Future>(f: F) -> F::Output {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut f = pin!(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(x) => x,
        Poll::Pending => panic!("unexpected pending host promise"),
    }
}
fn class_ops(host: Rc<Host>, body: Rc<SocketOps<V, E>>) -> Rc<SocketClassOps<V, E>> {
    let super_h = host.clone();

    let binding = Rc::new(BindingOps {
        null: Rc::new(|| Rc::new(Native::Null)),
        is_string: Rc::new(|v| matches!(v.as_ref(), Native::S(_))),
        value: body.clone(),
        binary: Rc::new(|op, l, r| {
            Ok(match op {
                "ToNumeric" => n(number(&l)),
                "++" => n(number(&l) + 1.),
                ">" => b(number(&l) > number(&r)),
                ">=" => b(number(&l) >= number(&r)),
                "<" => b(number(&l) < number(&r)),
                "<=" => b(number(&l) <= number(&r)),
                "+" => n(number(&l) + number(&r)),
                "-" => n(number(&l) - number(&r)),
                "/" => n(number(&l) / number(&r)),
                _ => panic!("unexercised native operator {op}"),
            })
        }),
        construct: Rc::new(|ctor, _| {
            Ok(match ctor.as_ref() {
                Native::Builtin(x) if x == "Set" => Rc::new(Native::Set),
                Native::Builtin(x) if x == "Map" => Rc::new(Native::Map),
                _ => panic!("constructor"),
            })
        }),
        throw_value: Rc::new(|v| Rc::new(text(&v))),
        promise: Rc::new(|future| ready(future).expect("test promise rejection")),
        iterator: Rc::new(|v| {
            assert!(
                matches!(v.as_ref(), Native::A(_)),
                "witness iterator supports native arrays"
            );
            Ok(object(vec![("source", v), ("offset", n(0.))]))
        }),
        next: Rc::new(|record| {
            let source = get(record, "source");
            let Native::A(rows) = source.as_ref() else {
                panic!("array iterator")
            };
            let offset = number(&get(record, "offset")) as usize;
            let v = rows.borrow().get(offset).cloned();
            put(record, s("offset"), n((offset + 1) as f64));
            Ok(v)
        }),
        close: Rc::new(|_| Ok(())),
        close_throw: Rc::new(|_, e| e),
    });
    Rc::new(SocketClassOps {
        body,
        define: Rc::new(|r, k, v| {
            put(r, s(k), v);
            Ok(())
        }),
        superclass: Rc::new(|active| {
            let Native::Class { base, .. } = active.as_ref() else {
                panic!("active class")
            };
            Ok(base.clone())
        }),
        super_construct: Rc::new(move |base, target, args| {
            assert!(
                matches!(base.as_ref(), Native::Builtin(name) if name == "BaseWebSocket"),
                "NATIVE-SUPERCLASS"
            );
            assert!(
                Rc::ptr_eq(&target, super_h.target_argument.borrow().as_ref().unwrap()),
                "NATIVE-NEWTARGET"
            );
            assert!(
                args.len() == 1
                    && Rc::ptr_eq(&args[0], super_h.parent_argument.borrow().as_ref().unwrap()),
                "NATIVE-PARENT"
            );
            super_h.event("super", vec![b(true)]);
            super_h.fail("super")?;
            let receiver = object(vec![("parent", args[0].clone())]);
            super_h.super_receiver.replace(Some(receiver.clone()));
            Ok(receiver)
        }),
        construct: Rc::new(|ctor, _| {
            Ok(match ctor.as_ref() {
                Native::Builtin(x) if x == "Set" => Rc::new(Native::Set),
                Native::Builtin(x) if x == "Map" => Rc::new(Native::Map),
                _ => panic!("constructor"),
            })
        }),
        derive: Rc::new(|name, base, length, body| {
            Ok(Rc::new(Native::Class {
                name: name.into(),
                length,
                base,
                body,
            }))
        }),
        binding,
        promise: Rc::new(|future| ready(future).expect("test promise rejection")),
    })
}
fn run(h: Rc<Host>) -> Result<V, E> {
    let spec = &h.spec;
    let ops = ops(h.clone());
    let r = h.receiver.clone();
    let update = decode(spec.get("update").unwrap_or(&json!({})));
    match spec["op"].as_str().unwrap() {
        "route" => {
            let event = object(vec![("data", h.payload.clone())]);
            if spec["private"] == true {
                port::private_on_message(&ops, &r, &event)
            } else {
                port::public_on_message(&ops, &r, &event)
            }
        }
        "ticker" => {
            let f = port::initialize_ticker_handler(ops.clone(), r)?;
            let out = h.invoke(f.clone(), u(), vec![update])?;
            if let Some(second) = spec.get("second") {
                h.invoke(f, u(), vec![decode(second)])?;
                let timer = h.timers.borrow_mut().remove(0);
                h.invoke(timer, u(), vec![])?;
            }
            Ok(out)
        }
        "book" => {
            let host = h.clone();
            let frame = update.clone();
            let callback = Rc::new(Native::F(
                "".into(),
                1,
                Rc::new(move |args| {
                    host.event("fanout", vec![b(Rc::ptr_eq(&args[0], &frame))]);
                    host.fail("fanout")?;
                    Ok(s("retained"))
                }),
            ));
            put(
                &get(&r, "messageHandlers"),
                s("book.BTC"),
                array(vec![callback]),
            );
            let f = port::initialize_book_handler(ops, r)?;
            h.invoke(f, u(), vec![update])
        }
        "trade" => port::trade_update(&ops, &update),
        "order" => port::order_update(&ops, &r, &update),
        "position" => {
            let future = port::position_update(ops, r, update.clone());
            let count = h.events.borrow().len();
            h.event("before-await", vec![n(count as f64)]);
            if let Some(rows) = spec.get("mutateAfterStart") {
                put(&update, s("data"), decode(rows));
            }
            ready(future)
        }
        "balance" => port::balance_update(&ops, &r, &update),
        "candle" | "listenbook" => {
            let options = object(vec![("symbol", s("BTC")), ("interval", s("1m"))]);
            let host = h.clone();
            let callback = Rc::new(Native::F(
                "callback".into(),
                1,
                Rc::new(move |args| {
                    let v = args[0].clone();
                    let same = host
                        .last_book
                        .borrow()
                        .as_ref()
                        .is_none_or(|old| Rc::ptr_eq(old, &v));
                    host.last_book.replace(Some(v.clone()));
                    host.event("callback", vec![b(same), copy(&v)]);
                    host.fail("callback")?;
                    Ok(s("ignored"))
                }),
            ));
            let remove = if spec["op"] == "candle" {
                port::listen_ohlcv(ops.clone(), r.clone(), options.clone(), callback)?
            } else {
                port::listen_order_book(
                    ops.clone(),
                    r.clone(),
                    if spec["coerce"] == true {
                        Rc::new(Native::Builtin("coercion".into()))
                    } else {
                        s("BTC")
                    },
                    callback,
                )?
            };
            if spec["removeBeforeConnect"] == true {
                h.invoke(remove.clone(), u(), vec![])?;
            }
            if spec["changeSymbol"] == true {
                put(&options, s("symbol"), s("ETH"));
            }
            if spec["disconnected"] == true {
                put(&r, s("isConnected"), b(true));
                let timer = h.timers.borrow_mut().remove(0);
                h.invoke(timer, u(), vec![])?;
            }
            let key = if spec["op"] == "candle" {
                "candlestick.1m.BTC"
            } else if spec["coerce"] == true {
                "book.ETH"
            } else {
                "book.BTC"
            };
            let callbacks = get(&get(&r, "messageHandlers"), key);
            for frame in std::iter::once(update).chain(spec.get("second").map(decode)) {
                if matches!(callbacks.as_ref(), Native::A(_)) {
                    for parser in values(&callbacks) {
                        h.invoke(parser, u(), vec![frame.clone()])?;
                    }
                }
            }
            if spec["removeBeforeConnect"] != true {
                h.invoke(remove, u(), vec![])?;
            }
            let handlers = get(&r, "messageHandlers");
            let Native::O(fields) = handlers.as_ref() else {
                unreachable!()
            };
            h.event(
                "handlers",
                vec![array(
                    fields.borrow().iter().map(|(k, _)| k.clone()).collect(),
                )],
            );
            Ok(u())
        }
        "class" => {
            let co = class_ops(h.clone(), ops);
            let c = if spec["private"] == true {
                classes::define_private_socket(co)?
            } else {
                classes::define_public_socket(co)?
            };
            let Native::Class {
                name, length, body, ..
            } = c.as_ref()
            else {
                panic!("class factory")
            };
            let target = if spec["subclass"] == true {
                Rc::new(Native::Class {
                    name: "Child".into(),
                    length: 0,
                    base: c.clone(),
                    body: body.clone(),
                })
            } else {
                c.clone()
            };
            let parent = object(vec![("tag", s("parent"))]);
            h.parent_argument.replace(Some(parent.clone()));
            h.target_argument.replace(Some(target.clone()));
            let socket = body(c.clone(), target, vec![parent, s("ignored")])?;
            assert!(
                Rc::ptr_eq(&socket, h.super_receiver.borrow().as_ref().unwrap()),
                "NATIVE-ORIGINAL-SUPER-RECEIVER"
            );
            h.event("class", vec![s(name), n(*length as f64)]);
            let Native::O(fields) = socket.as_ref() else {
                panic!("receiver")
            };
            assert_eq!(
                fields.borrow().len() - 1,
                if spec["private"] == true { 25 } else { 31 },
                "NATIVE-INDEPENDENT-OWN-FIELD-FLOOR"
            );
            let rows = fields
                .borrow()
                .iter()
                .filter(|(k, _)| text(k) != "parent")
                .map(|(k, v)| array(vec![k.clone(), v.clone()]))
                .collect();
            h.event("fields", vec![array(rows)]);
            Ok(u())
        }
        op => panic!("unknown operation {op}"),
    }
}
fn assert_native(v: &V, expected: &Value, label: &str) {
    match v.as_ref() {
        Native::U => assert_eq!(*expected, json!({"$undefined":true}), "{label} undefined"),
        Native::Null => assert!(expected.is_null(), "{label} null"),
        Native::B(x) => assert_eq!(Some(*x), expected.as_bool(), "{label} bool"),
        Native::N(x) => {
            if let Some(tag) = expected.get("$number").and_then(Value::as_str) {
                match tag {
                    "NaN" => assert!(x.is_nan(), "{label} NaN"),
                    "Infinity" => assert_eq!(*x, f64::INFINITY, "{label} infinity"),
                    "-0" => assert_eq!(x.to_bits(), (-0f64).to_bits(), "{label} signedzero"),
                    _ => panic!("number tag"),
                }
            } else {
                assert_eq!(
                    x.to_bits(),
                    expected.as_f64().unwrap().to_bits(),
                    "{label} number"
                );
            }
        }
        Native::S(x) => assert_eq!(
            *x,
            expected
                .as_str()
                .unwrap()
                .encode_utf16()
                .collect::<Vec<_>>(),
            "{label} UTF16"
        ),
        Native::Symbol(x) => assert_eq!(expected["$symbol"], *x, "{label} symbol"),
        Native::A(rows) => {
            let rows = rows.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(rows.len(), e.len(), "{label} array cardinality");
            for (v, e) in rows.iter().zip(e) {
                assert_native(v, e, label);
            }
        }
        Native::O(fields) => {
            let fields = fields.borrow();
            let e = expected["$object"].as_array().unwrap();
            assert_eq!(fields.len(), e.len(), "{label} object cardinality");
            for ((k, v), e) in fields.iter().zip(e) {
                assert_native(k, &e[0], label);
                assert_native(v, &e[1], label);
            }
        }
        Native::F(name, len, _) => assert_eq!(
            expected["$function"],
            json!([name, len]),
            "{label} function metadata"
        ),
        Native::Set => assert_eq!(*expected, json!({"$set":[]}), "{label} fresh Set"),
        Native::Map => assert_eq!(*expected, json!({"$map":[]}), "{label} fresh Map"),
        Native::Builtin(name) if name == "coercion" => assert_eq!(
            *expected,
            json!({"$object": [["toString", {"$function": ["toString", 0]}]]}),
            "{label} original opaque symbol object"
        ),
        _ => panic!("unexpected native capture {label}"),
    }
}
fn fixtures() -> Vec<Value> {
    let ticker = json!({"instrument_name":"BTC","data":[{"b":"11.5x","k":"12.25","a":"19","c":"0.13","v":"27","vv":"43","oi":"5"}]});
    let book = json!({"instrument_name":"BTC","data":[{"bids":[["11","2","0"],["13","3","0"]],"asks":[["17","4","0"],["15","5","0"]]}]});
    let order = json!({"instrument_name":"BTC","order_id":"one","order_type":"MARKET","side":"BUY","quantity":"12","cumulative_quantity":"3","ref_price":"0","trigger_price":"6","limit_price":"7","status":"FILLED","order_status":"CANCELED","update_time":0,"time_updated":92,"create_time":83,"exec_inst":["REDUCE_ONLY"],"time_in_force":"POST_ONLY"});
    let position = json!({"instrument_name":"BTC","quantity":"-4","cost":"-80","leverage":"0","target_leverage":"7","liquidation_price":"12","mark_price":"21","isolation_type":"ISOLATED_MARGIN","open_position_pnl":"9","isolation_id":"isolated-id","update_timestamp_ms":0});
    let summary = json!({"total_margin_balance":"101","total_available_balance":"83","total_initial_margin":"17","total_session_unrealized_pnl":"-9","position_balances":[{"instrument_name":"BTC","quantity":"12","max_withdrawal_balance":"15"},{"instrument_name":"ETH","quantity":"9","max_withdrawal_balance":"2"}]});
    let candle = json!({"instrument_name":"BTC","data":[{"t":234,"o":"11","h":"19","l":"7","c":"13","v":"23"}]});
    let mut out = vec![];
    for channel in [
        "ticker",
        "trade",
        "book",
        "book.update",
        "candlestick",
        "mark",
        "index",
        "funding",
        "estimatedfunding",
    ] {
        out.push(json!({"name":format!("public-{channel}"),"op":"route","payload":{"code":7,"result":{"channel":channel}}}));
    }
    out.extend([
        json!({"name":"public-error","op":"route","payload":{"code":9,"message":"bad"}}),
        json!({"name":"public-zero","op":"route","payload":{"code":0}}),
        json!({"name":"public-heartbeat","op":"route","payload":{"method":"public/heartbeat","id":17,"result":{"channel":"ticker"}}}),
        json!({"name":"public-heartbeat-nonnumber","op":"route","payload":{"method":"public/heartbeat","id":"17"}}),
        json!({"name":"public-disposed","op":"route","disposed":true}),
        json!({"name":"public-track-throw","op":"route","fail":"track"}),
        json!({"name":"public-handler-throw","op":"route","fail":"handler","payload":{"result":{"channel":"ticker"}}}),
        json!({"name":"public-stats-getter","op":"route","statsChange":true,"payload":{"result":{"channel":"ticker"}}}),
        json!({"name":"public-json-throw","op":"route","fail":"json"}),
    ]);
    for channel in [
        "user.order.extra",
        "user.advance.order.extra",
        "user.positions",
        "user.balance",
    ] {
        out.push(json!({"name":format!("private-{channel}"),"op":"route","private":true,"payload":{"result":{"channel":channel}}}));
    }
    out.extend([
        json!({"name":"private-auth-priority","op":"route","private":true,"payload":{"method":"public/auth","code":0,"result":{"channel":"user.order"}}}),
        json!({"name":"private-auth-failure","op":"route","private":true,"payload":{"method":"public/auth","code":8}}),
        json!({"name":"private-heartbeat","op":"route","private":true,"payload":{"method":"public/heartbeat","id":29}}),
        json!({"name":"ticker-basic","op":"ticker","update":ticker}),
        json!({"name":"ticker-leading-trailing","op":"ticker","update":ticker,"second":{"instrument_name":"BTC","data":[{"b":"31","k":"37","a":"41","c":"0.47","v":"53","vv":"59","oi":""}]}}),
        json!({"name":"ticker-invalid-oi","op":"ticker","update":{"instrument_name":"BTC","data":[{"b":"bad","k":"12","a":"13","c":"bad","v":"7","vv":"8","oi":"bad"}]}}),
        json!({"name":"ticker-empty","op":"ticker","update":{"instrument_name":"BTC","data":[]}}),
        json!({"name":"ticker-missing-market","op":"ticker","update":{"instrument_name":"NONE"}}),
        json!({"name":"book-basic","op":"book","update":book}),
        json!({"name":"book-empty-sides","op":"book","update":{"instrument_name":"BTC","data":[{"bids":[],"asks":[]}]}}),
        json!({"name":"book-fanout-throw","op":"book","update":book,"fail":"fanout"}),
        json!({"name":"trade-noop","op":"trade"}),
        json!({"name":"order-basic","op":"order","private":true,"update":{"data":[{"instrument_name":"NONE"},order]}}),
        json!({"name":"order-fallback","op":"order","private":true,"update":{"data":[{"instrument_name":"BTC","order_id":"fallback","quantity":"bad","order_type":"LIMIT","side":"SELL","time_in_force":"GTC"}]}}),
        json!({"name":"order-overfill","op":"order","private":true,"update":{"data":[{"instrument_name":"ETH","order_id":"sell","quantity":"2","cumulative_quantity":"9","trigger_price":"6","order_type":"STOP_LIMIT","side":"SELL","status":"CANCELED","time_in_force":"GTC"}]}}),
        json!({"name":"order-store-throw","op":"order","private":true,"update":{"data":[order,order]},"fail":"order"}),
        json!({"name":"position-short","op":"position","private":true,"update":{"data":[position]}}),
        json!({"name":"position-zero-remove-all","op":"position","private":true,"update":{"data":[{"instrument_name":"BTC","quantity":"0","cost":"0"}]}}),
        json!({"name":"position-unknown","op":"position","private":true,"update":{"data":[{"instrument_name":"NONE"}]}}),
        json!({"name":"position-mode-reject","op":"position","private":true,"update":{"data":[position]},"modeRejected":true}),
        json!({"name":"position-live-after-await","op":"position","private":true,"update":{"data":[position]},"mutateAfterStart":[{"instrument_name":"BTC","quantity":"3","cost":"60","open_position_pnl":"0"}],"mode":"oneway"}),
        json!({"name":"balance-basic","op":"balance","private":true,"update":{"data":[summary]}}),
        json!({"name":"balance-duplicate","op":"balance","private":true,"update":{"data":[{"position_balances":[{"instrument_name":"BTC","quantity":"4","max_withdrawal_balance":"3"},{"instrument_name":"BTC","quantity":"9","max_withdrawal_balance":"2"}]}]}}),
        json!({"name":"balance-empty","op":"balance","private":true,"update":{"data":[]}}),
        json!({"name":"balance-symbol-spread","op":"balance","private":true,"update":{"data":[summary]},"loadedSymbol":true}),
        json!({"name":"balance-store-throw","op":"balance","private":true,"update":{"data":[summary]},"fail":"store"}),
        json!({"name":"candle-basic","op":"candle","update":candle}),
        json!({"name":"candle-disconnected","op":"candle","update":candle,"disconnected":true}),
        json!({"name":"candle-cleaned-before-connected","op":"candle","update":candle,"disconnected":true,"removeBeforeConnect":true}),
        json!({"name":"candle-live-options","op":"candle","update":{"instrument_name":"ETH","data":[{"t":321,"o":"1","h":"2","l":"3","c":"4","v":"5"}]},"disconnected":true,"changeSymbol":true}),
        json!({"name":"book-snapshot-replacement","op":"listenbook","update":book,"second":{"instrument_name":"BTC","data":[{"bids":[["9","7","0"]],"asks":[["19","8","0"]]}]}}),
        json!({"name":"book-symbol-coercion-order","op":"listenbook","update":book,"coerce":true}),
        json!({"name":"candle-unsubscribe-throw","op":"candle","update":candle,"fail":"unsubscribe"}),
        json!({"name":"book-disconnected","op":"listenbook","update":book,"disconnected":true}),
        json!({"name":"public-class","op":"class"}),
        json!({"name":"private-class","op":"class","private":true}),
        json!({"name":"private-subclass-construction","op":"class","private":true,"subclass":true}),
        json!({"name":"public-super-throw","op":"class","fail":"super"}),
    ]);
    out
}
fn independent_native(h: &Host, outcome: &Result<V, E>, index: usize) {
    let name = h.spec["name"].as_str().unwrap();
    let label = format!("CRYPTOCOM-SOCKET-INDEPENDENT {name}");
    let original = matches!(
        name,
        "public-track-throw"
            | "balance-store-throw"
            | "candle-unsubscribe-throw"
            | "public-super-throw"
    );
    if original {
        assert!(
            outcome.as_ref().is_err_and(|e| Rc::ptr_eq(e, &h.error)),
            "{label} original error"
        );
    } else {
        assert!(
            matches!(outcome.as_ref().unwrap().as_ref(), Native::U),
            "{label} void return"
        );
    }
    const EFFECTS: [usize; 60] = [
        3, 3, 3, 3, 3, 3, 3, 3, 2, 2, 1, 3, 1, 0, 1, 4, 3, 2, 3, 3, 3, 3, 2, 2, 3, 2, 3, 2, 1, 1,
        3, 3, 4, 0, 1, 2, 2, 2, 4, 3, 2, 3, 4, 1, 1, 0, 1, 1, 5, 6, 4, 6, 6, 6, 3, 6, 5, 3, 3, 1,
    ];
    assert_eq!(
        h.events.borrow().len(),
        EFFECTS[index],
        "{label} literal effect count"
    );
    assert_eq!(
        truthy(&get(&h.receiver, "isAuthenticated")),
        name == "private-auth-priority",
        "{label} auth state"
    );
    let events = h.events.borrow();
    let event = |kind: &str| {
        events
            .iter()
            .find(|v| text(&values(v)[0]) == kind)
            .map(values)
            .unwrap()
    };
    let fields = |v: &V, expected: &[(&str, f64)]| {
        for (key, expected) in expected {
            let value = get(v, key);
            let Native::N(actual) = value.as_ref() else {
                panic!("{label} native numeric {key}")
            };
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "{label} numeric field {key}"
            );
        }
    };
    match name {
        "public-heartbeat" | "private-heartbeat" => {
            let frame = event("frame")[1].clone();
            assert_eq!(
                text(&get(&frame, "method")),
                "public/respond-heartbeat",
                "{label} heartbeat method"
            );
            fields(
                &frame,
                &[("id", if name == "public-heartbeat" { 17. } else { 29. })],
            );
        }
        "ticker-basic" => fields(
            &event("ticker")[2],
            &[
                ("bid", 11.5),
                ("ask", 12.25),
                ("last", 19.),
                ("percentage", 13.),
                ("volume", 27.),
                ("quoteVolume", 43.),
                ("openInterest", 5.),
            ],
        ),
        "book-basic" => fields(&event("ticker")[2], &[("bid", 11.), ("ask", 17.)]),
        "trade-noop" => assert_eq!(h.reads.get(), 0, "{label} zero argument/receiver reads"),
        "order-basic" => {
            let order = event("order")[1].clone();
            fields(
                &order,
                &[
                    ("price", 0.),
                    ("amount", 12.),
                    ("filled", 3.),
                    ("remaining", 9.),
                    ("lastUpdate", 83.),
                ],
            );
            assert_eq!(text(&get(&order, "id")), "one", "{label} order id");
            assert_eq!(text(&get(&order, "type")), "market", "{label} type");
            assert_eq!(text(&get(&order, "status")), "closed", "{label} status");
            assert!(truthy(&get(&order, "reduceOnly")), "{label} reduceOnly");
        }
        "position-short" => {
            let p = event("position")[1].clone();
            fields(
                &p,
                &[
                    ("contracts", 4.),
                    ("entryPrice", -20.),
                    ("notional", 80.),
                    ("leverage", 1.),
                    ("liquidationPrice", 12.),
                    ("unrealizedPnl", 9.),
                    ("lastUpdate", 777.),
                ],
            );
            assert_eq!(text(&get(&p, "side")), "short", "{label} side");
            assert_eq!(text(&get(&p, "id")), "BTC-SYMBOL:short", "{label} id");
            fields(&get(&p, "info"), &[("markPrice", 21.)]);
        }
        "position-zero-remove-all" => {
            let positions = values(&get(&event("store")[1], "positions"));
            assert_eq!(
                positions.len(),
                1,
                "{label} remaining unrelated position count"
            );
            assert_eq!(
                text(&get(&positions[0], "symbol")),
                "ETH-SYMBOL",
                "{label} unrelated symbol"
            );
        }
        "balance-basic" => {
            let balance = get(&event("store")[1], "balance");
            fields(
                &balance,
                &[("total", 101.), ("free", 83.), ("used", 17.), ("upnl", -9.)],
            );
            fields(
                &get(&get(&balance, "currencies"), "BTC"),
                &[("total", 12.), ("free", 15.), ("used", 0.), ("upnl", 0.)],
            );
            fields(
                &get(&get(&balance, "currencies"), "ETH"),
                &[("total", 9.), ("free", 2.), ("used", 7.), ("upnl", 0.)],
            );
        }
        "balance-symbol-spread" => {
            let loaded = get(&event("store")[1], "loaded");
            let Native::O(loaded) = loaded.as_ref() else {
                panic!("{label} loaded object")
            };
            let original = h.loaded_symbol.borrow();
            let original = original.as_ref().unwrap();
            assert!(
                loaded
                    .borrow()
                    .iter()
                    .any(|(key, value)| Rc::ptr_eq(key, original) && text(value) == "retained"),
                "{label} original Symbol key identity"
            );
        }
        "candle-basic" => fields(
            &event("callback")[2],
            &[
                ("timestamp", 234.),
                ("open", 11.),
                ("high", 19.),
                ("low", 7.),
                ("close", 13.),
                ("volume", 23.),
            ],
        ),
        "book-snapshot-replacement" => {
            let callbacks = events
                .iter()
                .filter(|v| text(&values(v)[0]) == "callback")
                .map(values)
                .collect::<Vec<_>>();
            assert_eq!(callbacks.len(), 2, "{label} callback count");
            assert!(
                callbacks.iter().all(|c| truthy(&c[1])),
                "{label} one retained original book identity"
            );
            let first = values(&get(&callbacks[0][2], "bids"));
            assert_eq!(first.len(), 2, "{label} initial bid count");
            fields(&first[0], &[("price", 13.), ("amount", 3.), ("total", 3.)]);
            fields(&first[1], &[("price", 11.), ("amount", 2.), ("total", 5.)]);
            let bids = values(&get(&callbacks[1][2], "bids"));
            let asks = values(&get(&callbacks[1][2], "asks"));
            assert_eq!(bids.len(), 1, "{label} replacement bid count");
            assert_eq!(asks.len(), 1, "{label} replacement ask count");
            fields(&bids[0], &[("price", 9.), ("amount", 7.), ("total", 7.)]);
            fields(&asks[0], &[("price", 19.), ("amount", 8.), ("total", 8.)]);
        }
        _ => {}
    }
}

fn source_capture() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let specs = fixtures();
        assert_eq!(specs.len(), 60, "SOCKET-INDEPENDENT-FIXTURE-FLOOR");
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/socket_handlers_oracle.cjs"
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
            .write_all(serde_json::to_string(&specs).unwrap().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "source oracle failed {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            oracle["rows"].as_array().unwrap().len(),
            60,
            "SOCKET-INDEPENDENT-SOURCE-FLOOR"
        );
        oracle
    })
}
fn compare_group(op: &str, private: bool, floor: usize) {
    let specs = fixtures();
    let natives = specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| spec["op"] == op && (spec["private"] == true) == private)
        .map(|(index, spec)| {
            let h = host(spec);
            let outcome = run(h.clone());
            independent_native(&h, &outcome, index);
            (index, spec, h, outcome)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        natives.len(),
        floor,
        "SOCKET-INDEPENDENT-GROUP-FLOOR {op} {private}"
    );
    let oracle = source_capture();
    let mut comparisons = 0;
    for (index, spec, h, outcome) in natives {
        if spec["op"] != op || (spec["private"] == true) != private {
            continue;
        }
        let expected = &oracle["rows"][index];
        let label = format!("CRYPTOCOM-SOCKET-NATIVE {}", spec["name"].as_str().unwrap());

        if expected["result"] == "original" {
            let error = outcome.err().expect("native original throw");
            assert!(
                Rc::ptr_eq(&error, &h.error),
                "{label} original error identity"
            );
        } else {
            assert_native(&outcome.unwrap(), &json!({"$undefined": true}), &label);
        }
        assert_native(
            &array(h.events.borrow().clone()),
            &expected["events"],
            &label,
        );
        assert_native(
            &get(&h.receiver, "isAuthenticated"),
            &expected["authenticated"],
            &label,
        );
        assert_eq!(spec["name"], expected["name"], "{label} admitted identity");
        comparisons += 1;
    }
    assert_eq!(
        comparisons, floor,
        "SOCKET-INDEPENDENT-GROUP-FLOOR {op} {private}"
    );
}
#[test]
fn public_message_routing() {
    compare_group("route", false, 18);
}
#[test]
fn private_message_routing() {
    compare_group("route", true, 7);
}
#[test]
fn ticker_throttle_and_fields() {
    compare_group("ticker", false, 5);
}
#[test]
fn book_throttle_and_fanout() {
    compare_group("book", false, 3);
}
#[test]
fn trade_noop() {
    compare_group("trade", false, 1);
}
#[test]
fn private_orders() {
    compare_group("order", true, 4);
}
#[test]
fn private_positions() {
    compare_group("position", true, 5);
}
#[test]
fn private_balances() {
    compare_group("balance", true, 5);
}
#[test]
fn candle_listener() {
    compare_group("candle", false, 5);
}
#[test]
fn book_listener() {
    compare_group("listenbook", false, 3);
}
#[test]
fn public_class_constructor() {
    compare_group("class", false, 2);
}
#[test]
fn private_class_constructor() {
    compare_group("class", true, 2);
}
