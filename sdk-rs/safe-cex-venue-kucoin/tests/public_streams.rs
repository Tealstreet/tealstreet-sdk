use safe_cex_venue_kucoin::public_streams::{
    self as port, StreamCallback, StreamFuture, StreamOps,
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
    message: &'static str,
    code: &'static str,
    ids: [u8; 2],
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
    Cb(String, usize, StreamCallback<V, E>),
    P(Rc<Promise>),
    Iter(Vec<V>),
    Api,
}
struct Promise {
    released: Cell<bool>,
    result: Result<V, E>,
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
        Native::Cb(name, _, _) if k == "name" => s(name),
        Native::Cb(_, arity, _) if k == "length" => n(*arity as f64),
        Native::Iter(_) => s("999"),
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
        Native::Cb(..) => json!({"cb":true}),
        _ => panic!("pack"),
    }
}
fn copy(v: &V) -> V {
    match v.as_ref() {
        Native::O(x) => Rc::new(Native::O(RefCell::new(
            x.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), copy(v)))
                .collect(),
        ))),
        Native::A(x) => array(x.borrow().iter().map(copy).collect()),
        _ => v.clone(),
    }
}
struct State {
    method: &'static str,
    emissions: Cell<usize>,
    frames: Cell<usize>,
    case: &'static str,
    connected: Cell<bool>,
    disposed: Cell<bool>,
    handlers: V,
    ticker: V,
    updates: RefCell<Vec<V>>,
    outputs: RefCell<Vec<V>>,
    output_refs: RefCell<Vec<V>>,
    events: RefCell<Vec<Value>>,
    captures: RefCell<Vec<String>>,
    timers: RefCell<Vec<(usize, V)>>,
    next_timer: Cell<usize>,
    promises: RefCell<Vec<Rc<Promise>>>,
    tasks: RefCell<Vec<StreamFuture<E>>>,
    errors: RefCell<Vec<E>>,
    requests: Cell<usize>,
    data_reads: Cell<usize>,
    coercions: Cell<usize>,
    date_calls: Cell<usize>,
    changed: Cell<bool>,
    err: E,
}
fn emit(t: &State, v: Value) {
    t.events.borrow_mut().push(v)
}
fn spawn(t: &Rc<State>, mut f: StreamFuture<E>) {
    let w = Waker::noop();
    match f.as_mut().poll(&mut Context::from_waker(w)) {
        Poll::Pending => t.tasks.borrow_mut().push(f),
        Poll::Ready(Ok(())) => {}
        Poll::Ready(Err(e)) => t.errors.borrow_mut().push(e),
    }
}
fn flush(t: &Rc<State>) {
    for _ in 0..8 {
        for p in t.promises.borrow().iter() {
            p.released.set(true)
        }
        let tasks = std::mem::take(&mut *t.tasks.borrow_mut());
        for f in tasks {
            spawn(t, f)
        }
        if t.tasks.borrow().is_empty() {
            return;
        }
    }
    panic!("ASYNC independent eight-wave floor exhausted")
}
fn callback(t: &State, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Cb(_, _, f) = callee.as_ref() {
        f(args.first().cloned().unwrap_or_else(u))?;
        return Ok(u());
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("callee")
    };
    match name.as_str() {
        "parseFloat" => Ok(n(safe_cex_num::coercion::parse_float(&text(&args[0])))),
        "parseInt" => {
            assert_eq!(num(&args[1]), 10., "PARSE native radix");
            Ok(n(safe_cex_num::coercion::parse_int(
                &text(&args[0]),
                Some(10),
            )))
        }
        "parseNumber" => {
            let input = if null(&args[0]) {
                String::new()
            } else {
                text(&args[0])
            };
            let x = safe_cex_num::coercion::parse_float(&input);
            Ok(if x.is_finite() { n(x) } else { u() })
        }
        "normalizeTimestamp" => {
            let input = if null(&args[0]) {
                "123".into()
            } else {
                text(&args[0])
            };
            let x = safe_cex_num::coercion::parse_int(&input, Some(10));
            Ok(n(if x > 10000000000. {
                (x / 1000.).floor()
            } else {
                x
            }))
        }
        "this.tickerForApiSymbol" => {
            emit(t, json!(["lookup", pack(&args[0])]));
            Ok(if text(&args[0]) == "API" {
                t.ticker.clone()
            } else {
                u()
            })
        }
        "this.parent.store.updateTicker" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.ticker),
                "TICKER native original ticker identity"
            );
            t.updates.borrow_mut().push(args[1].clone());
            emit(t, json!(["ticker", pack(&args[1])]));
            Ok(u())
        }
        "this.parent.toApiSymbol" => {
            assert_eq!(text(&args[0]), "SYM", "TOPIC native original symbol");
            emit(t, json!(["api", pack(&args[0])]));
            Ok(if t.case == "lazy-topic" {
                Rc::new(Native::Api)
            } else {
                s("API")
            })
        }
        "Date.now" => {
            t.date_calls.set(t.date_calls.get() + 1);
            emit(t, json!(["now"]));
            if t.case == "callee-reenter" && t.date_calls.get() == 2 {
                t.changed.set(true)
            }
            Ok(n(123.))
        }
        "stringify:old" | "stringify:new" => {
            if t.case == "callee-reenter" {
                assert_eq!(
                    name, "stringify:old",
                    "FRAME native original stringify callee"
                );
            }
            let Native::O(fields) = args[0].as_ref() else {
                panic!("frame")
            };
            assert_eq!(
                fields
                    .borrow()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>(),
                ["id", "type", "topic", "privateChannel", "response"],
                "FRAME native ordered keys"
            );
            assert!(matches!(
                get(&args[0], "privateChannel").as_ref(),
                Native::B(false)
            ));
            assert!(matches!(
                get(&args[0], "response").as_ref(),
                Native::B(true)
            ));
            assert_eq!(num(&get(&args[0], "id")), 123.);
            t.frames.set(t.frames.get() + 1);
            let index = t.frames.get();
            let kind = if t.method == "book" && t.case == "disposed" {
                "unsubscribe"
            } else if t.case == "duplicates" {
                if index <= 2 {
                    "subscribe"
                } else {
                    "unsubscribe"
                }
            } else if index == 1 {
                "subscribe"
            } else {
                "unsubscribe"
            };
            assert!(
                matches!(get(&args[0],"type").as_ref(),Native::S(value) if value==kind),
                "FRAME native operation value"
            );
            let topic = if t.method == "book" {
                "/contractMarket/level2:API".to_owned()
            } else if t.case == "lazy-topic" {
                format!("/contractMarket/limitCandle:API{index}_1min")
            } else {
                "/contractMarket/limitCandle:API_1min".to_owned()
            };
            assert!(
                matches!(get(&args[0],"topic").as_ref(),Native::S(value) if value==&topic),
                "FRAME native channel value"
            );

            emit(
                t,
                json!(["stringify", name.ends_with("new"), pack(&args[0])]),
            );
            Ok(args[0].clone())
        }
        "send:old" | "send:new" => {
            if t.case == "callee-reenter" {
                assert_eq!(name, "send:old", "FRAME native original send callee");
            }
            emit(t, json!(["send", name.ends_with("new"), pack(&args[0])]));
            Ok(u())
        }
        "this.subscribeTopic"
        | "this.unsubscribeTopic"
        | "this.subscribeActiveTicker"
        | "this.unsubscribeActiveTicker"
        | "this.cleanupMessageHandlerStats" => {
            let topic = if name.ends_with("subscribeTopic") {
                if t.method.ends_with("candle") {
                    "/market/candles:SYM_1min"
                } else {
                    "/spotMarket/level2Depth50:SYM"
                }
            } else if name.ends_with("ActiveTicker") {
                "API"
            } else if t.method.ends_with("candle") {
                "candle_SYM_1min"
            } else {
                "orderBook_SYM"
            };
            assert!(
                matches!(args[0].as_ref(),Native::S(value) if value==topic),
                "TOPIC native helper argument"
            );
            emit(t, json!([name, pack(&args[0])]));
            Ok(u())
        }
        "setTimeout" => {
            assert_eq!(num(&args[1]), 100., "TIMER native delay");
            let id = t.next_timer.get() + 1;
            t.next_timer.set(id);
            t.timers.borrow_mut().push((id, args[0].clone()));
            emit(t, json!(["timer", id]));
            Ok(n(id as f64))
        }
        "clearTimeout" => {
            let id = num(&args[0]) as usize;
            t.timers.borrow_mut().retain(|(i, _)| *i != id);
            emit(t, json!(["clear", id]));
            Ok(u())
        }
        "sink" => {
            t.outputs.borrow_mut().push(copy(&args[0]));
            t.output_refs.borrow_mut().push(args[0].clone());
            emit(t, json!(["callback", pack(&args[0])]));
            Ok(u())
        }
        "sortOrderBook" => {
            emit(t, json!(["sort"]));
            for side in ["bids", "asks"] {
                let a = get(&args[0], side);
                let Native::A(rows) = a.as_ref() else {
                    panic!("levels")
                };
                rows.borrow_mut().sort_by(|a, b| {
                    let a = num(&get(a, "price"));
                    let b = num(&get(b, "price"));
                    if side == "bids" {
                        b.total_cmp(&a)
                    } else {
                        a.total_cmp(&b)
                    }
                })
            }
            Ok(u())
        }
        "calcOrderBookTotal" => {
            emit(t, json!(["total"]));
            for side in ["bids", "asks"] {
                let mut sum = 0.;
                for row in items(&get(&args[0], side)) {
                    sum += num(&get(&row, "amount"));
                    put(&row, "total", n(sum))
                }
            }
            Ok(u())
        }
        "updateOrderBookByPrice" => {
            emit(
                t,
                json!(["update", pack(&args[1]), pack(&args[2]), pack(&args[3])]),
            );
            let side = text(&args[1]);
            let rows = get(&args[0], &side);
            let Native::A(rows) = rows.as_ref() else {
                panic!("rows")
            };
            let mut rows = rows.borrow_mut();
            let pos = rows
                .iter()
                .position(|r| num(&get(r, "price")) == num(&args[2]));
            if num(&args[3]) == 0. {
                if let Some(i) = pos {
                    rows.remove(i);
                }
            } else if let Some(i) = pos {
                put(&rows[i], "amount", args[3].clone())
            } else {
                rows.push(object(vec![
                    ("price", args[2].clone()),
                    ("amount", args[3].clone()),
                    ("total", n(0.)),
                ]))
            }
            Ok(u())
        }
        "this.parent.xhr.get" => {
            assert_eq!(
                text(&args[0]),
                "/api/v1/level2/snapshot",
                "BOOK native endpoint argument"
            );
            t.requests.set(t.requests.get() + 1);
            assert_eq!(
                text(&get(&get(&args[1], "params"), "symbol")),
                "API",
                "BOOK native API request symbol"
            );
            emit(t, json!(["xhr", pack(&args[1])]));
            let response = if t.case == "reject" {
                Err(t.err.clone())
            } else {
                let seq = if t.requests.get() == 1 {
                    100.
                } else if t.case == "buffer-gap" {
                    105.
                } else {
                    104.
                };
                Ok(object(vec![(
                    "data",
                    object(vec![
                        (
                            "code",
                            s(if t.case == "invalid-code" {
                                "500"
                            } else {
                                "200000"
                            }),
                        ),
                        ("msg", s("invalid")),
                        (
                            "data",
                            object(vec![
                                ("sequence", n(seq)),
                                (
                                    "bids",
                                    array(vec![array(vec![
                                        s(if t.requests.get() == 1 { "100" } else { "99" }),
                                        s("2"),
                                    ])]),
                                ),
                                (
                                    "asks",
                                    array(vec![array(vec![
                                        s(if t.requests.get() == 1 { "110" } else { "111" }),
                                        s("3"),
                                    ])]),
                                ),
                            ]),
                        ),
                    ]),
                )]))
            };
            let p = Rc::new(Promise {
                released: Cell::new(false),
                result: response,
            });
            t.promises.borrow_mut().push(p.clone());
            Ok(Rc::new(Native::P(p)))
        }
        "this.parent.emitAndLogError" => {
            assert!(
                matches!(args[0].as_ref(),Native::S(value) if value==if t.case=="reject"{"down"}else{"invalid"}),
                "ERROR native emitted snapshot message"
            );
            t.emissions.set(t.emissions.get() + 1);
            emit(t, json!(["error", pack(&args[0])]));
            Ok(u())
        }
        _ => panic!("unknown invoke {name}"),
    }
}
fn intrinsic(t: &State, name: &str, args: Vec<V>) -> Result<V, E> {
    Ok(match name {
        "false" => b(false),
        "true" => b(true),
        "null" => Rc::new(Native::Null),
        "get-index" => get(&args[0], &text(&args[1])),
        "set-index" => {
            put(&args[0], &text(&args[1]), args[2].clone());
            u()
        }
        "delete-index" => {
            let Native::O(x) = args[0].as_ref() else {
                panic!("delete")
            };
            x.borrow_mut().retain(|(k, _)| k != &text(&args[1]));
            u()
        }
        "Object.keys-length" => {
            let Native::O(x) = args[0].as_ref() else {
                panic!("keys")
            };
            n(x.borrow().len() as f64)
        }
        "multiply-100" => n(num(&args[0]) * 100.),
        "divide-1000" => n(num(&args[0]) / 1000.),
        "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
        "Number.isFinite" => b(num(&args[0]).is_finite()),
        "typeof-string" => b(matches!(args[0].as_ref(), Native::S(_))),
        "less-equal" => b(num(&args[0]) <= num(&args[1])),
        "greater-than" => b(num(&args[0]) > num(&args[1])),
        "greater-last-plus-one" => b(num(&args[0]) > num(&args[1]) + 1.),
        "split-colon-index-one" | "template-split-colon-index-one" => {
            text(&args[0]).split(':').nth(1).map(s).unwrap_or_else(u)
        }
        "split-comma" => array(text(&args[0]).split(',').map(s).collect()),
        "template-string" => s(&text(&args[0])),
        "destructure-two" => {
            if let Native::Iter(values) = args[0].as_ref() {
                emit(t, json!(["iterator-close"]));
                array(values[..2].to_vec())
            } else {
                let values = items(&args[0]);
                array(vec![
                    values.first().cloned().unwrap_or_else(u),
                    values.get(1).cloned().unwrap_or_else(u),
                ])
            }
        }
        "candle-topic" => s(&format!("candle_{}_{}", text(&args[0]), text(&args[1]))),
        "book-topic" => s(&format!("orderBook_{}", text(&args[0]))),
        "futures-candle-topic" | "futures-book-topic" => {
            let api = if matches!(args[0].as_ref(), Native::Api) {
                t.coercions.set(t.coercions.get() + 1);
                emit(t, json!(["api-coerce", t.coercions.get()]));
                format!("API{}", t.coercions.get())
            } else {
                text(&args[0])
            };
            s(&if name == "futures-candle-topic" {
                format!("/contractMarket/limitCandle:{api}_{}", text(&args[1]))
            } else {
                format!("/contractMarket/level2:{api}")
            })
        }
        "spot-candle-topic" => s(&format!(
            "/market/candles:{}_{}",
            text(&args[0]),
            text(&args[1])
        )),
        "spot-book-topic" => s(&format!("/spotMarket/level2Depth50:{}", text(&args[0]))),
        "snapshot-error" => s(&format!(
            "Failed to fetch orderbook snapshot for {}",
            text(&args[0])
        )),
        "book-error" => s(&format!("Failed to fetch orderbook {}", text(&args[0]))),
        "array-spread" => array(items(&args[0])),
        "spread-append" => {
            let mut a = items(&args[0]);
            a.push(args[1].clone());
            array(a)
        }
        "array-push" => {
            let Native::A(a) = args[0].as_ref() else {
                panic!("push")
            };
            a.borrow_mut().push(args[1].clone());
            u()
        }
        _ => panic!("intrinsic {name}"),
    })
}
fn ops(t: Rc<State>) -> Rc<StreamOps<V, E>> {
    let read = t.clone();
    let cap = t.clone();
    let call = t.clone();
    let oper = t.clone();
    let awaiter = t.clone();
    let spawner = t.clone();
    let err = t.clone();
    Rc::new(StreamOps {
        get: Rc::new(move |v, k| {
            if k == "data" && text_marker(v) == "data-reenter" {
                let count = read.data_reads.get() + 1;
                read.data_reads.set(count);
                emit(&read, json!(["data-read", count]));
                return Ok(object(vec![
                    ("time", n(3000.)),
                    ("open", s("101")),
                    ("high", s("103")),
                    ("low", s("97")),
                    ("close", s("102")),
                    ("volume", s("6")),
                ]));
            }
            if k == "fundingRate" && text_marker(v) == "getter-error" {
                return Err(read.err.clone());
            }
            Ok(get(v, k))
        }),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            cap.captures.borrow_mut().push(name.into());
            Ok(match name {
                "this.isConnected" => b(cap.connected.get()),
                "this.isDisposed" => b(cap.disposed.get()),
                "this.messageHandlers" => cap.handlers.clone(),
                "this.parent.store.tickersBySymbol" => object(vec![("API", cap.ticker.clone())]),
                "ENDPOINTS" => object(vec![("ORDERBOOK", s("/api/v1/level2/snapshot"))]),
                "WS_INTERVAL" => object(vec![("1m", s("1min"))]),
                "KUCOIN_SPOT_INTERVAL" => object(vec![("1m", s("1min"))]),
                "this.send" => Rc::new(Native::Call(format!(
                    "send:{}",
                    if cap.changed.get() { "new" } else { "old" }
                ))),
                "JSON.stringify" => Rc::new(Native::Call(format!(
                    "stringify:{}",
                    if cap.changed.get() { "new" } else { "old" }
                ))),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        invoke: Rc::new(move |c, a| callback(&call, c, a)),
        intrinsic: Rc::new(move |name, args| intrinsic(&oper, name, args)),
        truthy: Rc::new(truth),
        nullish: Rc::new(null),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        callback: Rc::new(|name, arity, f| Rc::new(Native::Cb(name.into(), arity, f))),
        map: Rc::new(|v, f| {
            let mut out = vec![];
            for value in items(&v) {
                out.push(f(value)?)
            }
            Ok(array(out))
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
            for value in items(&v) {
                f(value)?
            }
            Ok(())
        }),
        await_value: Rc::new(move |v| {
            let Native::P(p) = v.as_ref() else {
                panic!("promise")
            };
            let p = p.clone();
            let keep = awaiter.clone();
            Box::pin(std::future::poll_fn(move |_| {
                let _keep = &keep;
                if p.released.get() {
                    Poll::Ready(p.result.clone())
                } else {
                    Poll::Pending
                }
            }))
        }),
        spawn: Rc::new(move |f| spawn(&spawner, f)),
        error_value: Rc::new(move |e| {
            assert!(Rc::ptr_eq(e, &err.err), "ERROR native original identity");
            assert_eq!(
                (e.name, e.message, e.code, e.ids),
                ("TypeError", "down", "STREAM", [7, 2]),
                "ERROR native full value"
            );
            object(vec![("message", s(e.message))])
        }),
    })
}
fn text_marker(v: &V) -> String {
    if let Native::S(x) = get(v, "marker").as_ref() {
        x.clone()
    } else {
        String::new()
    }
}
fn ticker_json(method: &str, case: &str) -> V {
    let mut data = match method {
        "ticker" => vec![
            ("symbol", s("API")),
            ("bestBidPrice", s("11")),
            ("bestAskPrice", s("12")),
        ],
        "snapshot" => vec![
            ("symbol", s("API")),
            ("lastPrice", s("13")),
            ("priceChgPct", s("0.04")),
            ("fundingRate", s("0.005")),
            ("volume", s("15")),
            ("turnover", s("16")),
        ],
        "instrument" => vec![("markPrice", s("17")), ("indexPrice", s("18"))],
        _ => vec![
            ("bestBid", s("21")),
            ("bestAsk", s("22")),
            ("price", s("23")),
            ("changeRate", s("0.06")),
            ("volume", s("25")),
            ("turnover", s("26")),
        ],
    };
    if case == "empty" {
        data = if matches!(method, "ticker" | "snapshot") {
            vec![("symbol", s("API"))]
        } else {
            vec![]
        }
    }
    if case == "zero" {
        for (k, v) in &mut data {
            if *k != "symbol" {
                *v = n(0.)
            }
        }
    }
    if case == "invalid" {
        for (k, v) in &mut data {
            if *k != "symbol" {
                *v = s("bad")
            }
        }
    }
    if case == "alias" && method == "spot-ticker" {
        data = vec![
            ("symbol", s("API")),
            ("buy", s("31")),
            ("sell", s("32")),
            ("lastPrice", s("33")),
            ("changeRate", s("0.04")),
            ("volume24h", s("34")),
            ("turnover24h", s("35")),
        ]
    }
    if case == "getter-error" {
        data.push(("marker", s("getter-error")))
    }
    if case == "missing-ticker" {
        if let Some((_, v)) = data.iter_mut().find(|(k, _)| *k == "symbol") {
            *v = s("BAD")
        }
    }
    object(vec![
        ("data", if case == "missing" { u() } else { object(data) }),
        (
            "subject",
            s(if case == "missing-ticker" {
                "BAD"
            } else {
                "API"
            }),
        ),
        (
            "topic",
            s(if method == "instrument" {
                if case == "missing-ticker" {
                    "/contract/instrument:BAD"
                } else {
                    "/contract/instrument:API"
                }
            } else {
                "/market/ticker:all"
            }),
        ),
    ])
}
fn candle_message(case: &str) -> V {
    if case == "data-reenter" {
        return object(vec![("marker", s("data-reenter"))]);
    }
    if case == "missing" {
        return object(vec![]);
    }
    if case == "object" {
        return object(vec![(
            "data",
            object(vec![
                ("time", n(3000.)),
                ("open", s("101")),
                ("high", s("103")),
                ("low", s("97")),
                ("close", s("102")),
                ("volume", s("6")),
            ]),
        )]);
    }
    let timestamp = if case == "array-ms" {
        "1700000000123"
    } else {
        "1700000000"
    };
    object(vec![(
        "data",
        object(vec![(
            "candles",
            array(
                [timestamp, "101", "102", "103", "97", "6"]
                    .into_iter()
                    .map(s)
                    .collect(),
            ),
        )]),
    )])
}
fn book_message(case: &str) -> V {
    if case == "missing" {
        return object(vec![]);
    }
    let level = if case == "iterable" {
        Rc::new(Native::Iter(vec![s("100"), s("2"), s("unused")]))
    } else {
        array(vec![s("100"), s("2")])
    };
    let bids = if case == "iterable" {
        vec![level]
    } else if case == "invalid-level" {
        vec![
            level,
            array(vec![s("bad"), s("4")]),
            array(vec![s("99"), s("bad")]),
        ]
    } else {
        vec![array(vec![s("99"), s("4")]), level]
    };
    object(vec![(
        "data",
        object(vec![
            ("bids", array(bids)),
            (
                "asks",
                array(vec![
                    array(vec![s("111"), s("3")]),
                    array(vec![s("110"), s("5")]),
                ]),
            ),
        ]),
    )])
}
fn increment(seq: f64, price: &str, amount: &str) -> V {
    object(vec![
        ("type", s("update")),
        (
            "data",
            object(vec![
                ("sequence", n(seq)),
                ("change", s(&format!("{price},buy,{amount}"))),
            ]),
        ),
    ])
}
fn deliver(t: &State, topic: &str, message: V) -> Result<(), E> {
    for parser in items(&get(&t.handlers, topic)) {
        callback(t, parser, vec![message.clone()])?;
    }
    Ok(())
}
fn assert_callback_metadata(value: &V, name: &str, arity: f64) {
    assert!(
        matches!(value.as_ref(), Native::Cb(..)),
        "STREAM native callback kind"
    );
    assert_eq!(
        text(&get(value, "name")),
        name,
        "STREAM native callback name"
    );
    assert_eq!(
        num(&get(value, "length")),
        arity,
        "STREAM native callback arity"
    );
}
fn fixture(method: &'static str, case: &'static str) -> Value {
    let t = Rc::new(State {
        method,
        emissions: Cell::new(0),
        frames: Cell::new(0),
        case,
        connected: Cell::new(case != "disconnected" && case != "delayed"),
        disposed: Cell::new(case == "disposed"),
        handlers: object(vec![]),
        ticker: object(vec![("bid", n(7.)), ("ask", n(8.))]),
        updates: RefCell::new(vec![]),
        outputs: RefCell::new(vec![]),
        output_refs: RefCell::new(vec![]),
        events: RefCell::new(vec![]),
        captures: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        next_timer: Cell::new(0),
        promises: RefCell::new(vec![]),
        tasks: RefCell::new(vec![]),
        errors: RefCell::new(vec![]),
        requests: Cell::new(0),
        data_reads: Cell::new(0),
        coercions: Cell::new(0),
        date_calls: Cell::new(0),
        changed: Cell::new(false),
        err: Rc::new(Error {
            name: "TypeError",
            message: "down",
            code: "STREAM",
            ids: [7, 2],
        }),
    });
    let h = ops(t.clone());
    let sink = Rc::new(Native::Call("sink".into()));
    let mut failure = None;
    if matches!(method, "ticker" | "snapshot" | "instrument" | "spot-ticker") {
        let message = ticker_json(method, case);
        let out = match method {
            "ticker" => port::handle_ticker_v2(&h, message),
            "snapshot" => port::handle_snapshot(&h, message),
            "instrument" => port::handle_instrument(&h, message),
            _ => port::handle_spot_ticker_snapshot(&h, message),
        };
        assert_eq!(
            out.is_err(),
            case == "getter-error",
            "ERROR native synchronous failure scope"
        );
        if let Err(e) = out {
            assert!(
                Rc::ptr_eq(&e, &t.err),
                "ERROR native original ticker failure"
            );
            assert_eq!(
                (e.name, e.message, e.code, e.ids),
                ("TypeError", "down", "STREAM", [7, 2])
            );
            failure = Some(json!([e.name, e.message, e.code, e.ids]));
        }
        let expected = usize::from(
            !matches!(case, "missing" | "missing-ticker" | "getter-error")
                && !(case == "empty" && method != "ticker")
                && !(case == "invalid" && method == "spot-ticker"),
        );
        assert_eq!(
            t.updates.borrow().len(),
            expected,
            "TICKER native independent update count"
        );
        if expected == 1 {
            let update = &t.updates.borrow()[0];
            let expected_keys = match method {
                "ticker" => vec!["bid", "ask"],
                "snapshot" => vec!["last", "percentage", "fundingRate", "volume", "quoteVolume"],
                "instrument" => vec!["mark", "index"],
                _ => vec![
                    "bid",
                    "ask",
                    "last",
                    "mark",
                    "index",
                    "percentage",
                    "volume",
                    "quoteVolume",
                ],
            };
            let Native::O(fields) = update.as_ref() else {
                panic!("update")
            };
            assert_eq!(
                fields
                    .borrow()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>(),
                expected_keys,
                "TICKER native disjoint fields"
            );
            let values = match method {
                "ticker" => vec![
                    if case == "empty" || case == "zero" {
                        7.
                    } else {
                        11.
                    },
                    if case == "empty" || case == "zero" {
                        8.
                    } else {
                        12.
                    },
                ],
                "snapshot" => vec![13., 4., 0.005, 15., 16.],
                "instrument" => vec![17., 18.],
                _ => {
                    if case == "alias" {
                        vec![31., 32., 33., 33., 33., 4., 34., 35.]
                    } else {
                        vec![21., 22., 23., 23., 23., 6., 25., 26.]
                    }
                }
            };
            for ((_, value), expected) in fields.borrow().iter().zip(values) {
                let actual = num(value);
                if case == "invalid" {
                    assert!(actual.is_nan(), "TICKER native NaN retained")
                } else {
                    let expected: f64 = if case == "zero" && method != "ticker" {
                        0.
                    } else {
                        expected
                    };
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "TICKER native independent numeric bits"
                    )
                }
            }
        }
    } else {
        let candles = method.ends_with("candle");
        let spot = method.starts_with("spot");
        let topic = if candles {
            "candle_SYM_1min"
        } else {
            "orderBook_SYM"
        };
        let opts = object(vec![("symbol", s("SYM")), ("interval", s("1m"))]);
        let start = match method {
            "fut-candle" => port::listen_ohlcv(h.clone(), opts.clone(), sink.clone()),
            "spot-candle" => port::listen_spot_ohlcv(h.clone(), opts.clone(), sink.clone()),
            "book" => port::listen_order_book(h.clone(), s("SYM"), sink.clone()),
            _ => port::listen_spot_order_book(h.clone(), s("SYM"), sink.clone()),
        };
        let cleanup = start.unwrap_or_else(|_| panic!("listener unexpectedly threw"));
        assert_callback_metadata(&cleanup, "", 0.);
        if case != "disposed" && case != "disconnected" && case != "delayed" {
            let installed = items(&get(&t.handlers, topic));
            assert_eq!(installed.len(), 1, "STREAM native initial parser count");
            assert_callback_metadata(&installed[0], "parser", 1.);
        }
        for (_, timer) in t.timers.borrow().iter() {
            assert_callback_metadata(timer, "", 0.);
        }
        let duplicate = if case == "duplicates" {
            Some(if method == "fut-candle" {
                port::listen_ohlcv(h.clone(), opts, sink.clone())
                    .unwrap_or_else(|_| panic!("duplicate"))
            } else if candles {
                port::listen_spot_ohlcv(h.clone(), opts, sink.clone())
                    .unwrap_or_else(|_| panic!("duplicate"))
            } else {
                port::listen_spot_order_book(h.clone(), s("SYM"), sink.clone())
                    .unwrap_or_else(|_| panic!("duplicate"))
            })
        } else {
            None
        };
        if case == "delayed" {
            t.connected.set(true);
            let (_, timer) = t.timers.borrow_mut().remove(0);
            callback(&t, timer, vec![]).unwrap_or_else(|_| panic!("timer"));
        }
        if method == "book" && matches!(case, "buffer" | "buffer-gap") {
            let seq = if case == "buffer" { 101. } else { 105. };
            deliver(&t, topic, increment(seq, "100", "5")).unwrap_or_else(|_| panic!("buffer"));
            if case == "buffer-gap" {
                deliver(&t, topic, increment(106., "99", "7"))
                    .unwrap_or_else(|_| panic!("buffer gap"));
            }
        }
        if case == "cleanup-before-snapshot" {
            callback(&t, cleanup.clone(), vec![]).unwrap_or_else(|_| panic!("cleanup"));
        }
        flush(&t);
        if candles {
            if truth(&get(&t.handlers, topic)) {
                deliver(&t, topic, candle_message(case)).unwrap_or_else(|_| panic!("candle"));
            }
        } else if spot {
            deliver(&t, topic, book_message(case)).unwrap_or_else(|_| panic!("spot book"));
        } else if matches!(case, "normal" | "gap") {
            deliver(&t, topic, increment(101., "100", "5")).unwrap_or_else(|_| panic!("increment"));
            deliver(&t, topic, increment(101., "100", "9")).unwrap_or_else(|_| panic!("stale"));
            if case == "gap" {
                deliver(&t, topic, increment(105., "99", "7")).unwrap_or_else(|_| panic!("gap"));
                flush(&t);
            }
        }
        if let Some(second) = duplicate {
            callback(&t, cleanup.clone(), vec![]).unwrap_or_else(|_| panic!("first cleanup"));
            assert_eq!(
                items(&get(&t.handlers, topic)).len(),
                1,
                "HANDLER native retained subscriber"
            );
            let cleanups = t
                .events
                .borrow()
                .iter()
                .filter(|e| e[0] == "this.cleanupMessageHandlerStats")
                .count();
            assert_eq!(
                cleanups,
                usize::from(!spot),
                "HANDLER native original cleanup timing"
            );
            callback(&t, second, vec![]).unwrap_or_else(|_| panic!("second cleanup"));
        } else if case != "cleanup-before-snapshot" {
            callback(&t, cleanup, vec![]).unwrap_or_else(|_| panic!("cleanup"));
        }
        flush(&t);
        let expected = if candles {
            if case == "disconnected" && !spot
                || case == "disposed" && !spot
                || case == "missing"
                || spot && case == "object"
            {
                0
            } else if case == "duplicates" {
                2
            } else {
                1
            }
        } else if spot {
            if case == "missing" {
                0
            } else if case == "duplicates" {
                2
            } else {
                1
            }
        } else {
            match case {
                "normal" => 2,
                "gap" => 3,
                "disconnected" | "disposed" | "reject" | "invalid-code" => 0,
                _ => 1,
            }
        };
        assert_eq!(
            t.outputs.borrow().len(),
            expected,
            "STREAM native independent callback floor"
        );
        if candles && expected > 0 {
            for value in t.outputs.borrow().iter() {
                let keys = if !spot && matches!(case, "object" | "data-reenter") {
                    vec!["timestamp", "open", "high", "low", "close", "volume"]
                } else {
                    vec!["timestamp", "open", "close", "high", "low", "volume"]
                };
                let Native::O(fields) = value.as_ref() else {
                    panic!("candle")
                };
                assert_eq!(
                    fields
                        .borrow()
                        .iter()
                        .map(|(k, _)| k.clone())
                        .collect::<Vec<_>>(),
                    keys,
                    "CANDLE native ordered keys"
                );
                for (key, expected) in [
                    ("open", 101.),
                    ("high", 103.),
                    ("low", 97.),
                    ("close", 102.),
                    ("volume", 6.),
                ] {
                    assert_eq!(
                        num(&get(value, key)),
                        expected,
                        "CANDLE native distinct OHLCV fields"
                    )
                }
                let time = if matches!(case, "object" | "data-reenter") {
                    3.
                } else if case == "array-ms" {
                    if spot { 1700000000. } else { 1700000000123. }
                } else {
                    1700000000.
                };
                assert_eq!(
                    num(&get(value, "timestamp")),
                    time,
                    "CANDLE native timestamp mode"
                );
            }
        }
        if !candles && expected > 0 {
            let out = t.outputs.borrow();
            let last = out.last().unwrap();
            let bids = items(&get(last, "bids"));
            let asks = items(&get(last, "asks"));
            assert_eq!(
                bids.len(),
                if spot && !matches!(case, "invalid-level" | "iterable") {
                    2
                } else {
                    1
                },
                "BOOK native valid level floor"
            );
            assert_eq!(
                asks.len(),
                if spot { 2 } else { 1 },
                "BOOK native asks floor"
            );
            let bid = if matches!(case, "gap" | "buffer-gap") {
                (99., 7.)
            } else if matches!(case, "normal" | "buffer") && !spot {
                (100., 5.)
            } else {
                (100., 2.)
            };
            assert_eq!(
                (num(&get(&bids[0], "price")), num(&get(&bids[0], "amount"))),
                bid,
                "BOOK native final bid"
            );
            assert_eq!(
                num(&get(&bids[0], "total")),
                bid.1,
                "BOOK native decoded total"
            );
            assert_eq!(
                num(&get(&asks[0], "price")),
                if spot {
                    110.
                } else if matches!(case, "gap" | "buffer-gap") {
                    111.
                } else {
                    110.
                },
                "BOOK native ask order"
            );
            if !spot {
                for original in t.output_refs.borrow().iter() {
                    assert!(
                        Rc::ptr_eq(original, &t.output_refs.borrow()[0]),
                        "BOOK native persistent identity"
                    )
                }
            }
        }
        if method == "book" {
            assert_eq!(
                t.requests.get(),
                if matches!(case, "disposed" | "disconnected") {
                    0
                } else if matches!(case, "gap" | "buffer-gap") {
                    2
                } else {
                    1
                },
                "BOOK native independent snapshot request floor"
            );
            assert!(
                t.tasks.borrow().is_empty(),
                "BOOK native no pending profile tasks"
            );
        }
        let remaining = get(&t.handlers, topic);
        if spot {
            assert!(
                matches!(remaining.as_ref(), Native::U),
                "HANDLER native deleted final topic"
            )
        } else {
            assert!(
                items(&remaining).is_empty(),
                "HANDLER native retained empty topic"
            )
        }
        if case == "data-reenter" {
            assert_eq!(
                t.data_reads.get(),
                8,
                "CANDLE native repeated data getter floor"
            )
        }
        if case == "lazy-topic" {
            assert_eq!(
                t.coercions.get(),
                2,
                "TOPIC native deferred repeated coercion"
            );
            let frames = t
                .events
                .borrow()
                .iter()
                .filter(|e| e[0] == "send")
                .map(|e| e[2].clone())
                .collect::<Vec<_>>();
            assert_eq!(
                frames.len(),
                2,
                "FRAME native independent subscribe unsubscribe count"
            );
        }
        if case == "callee-reenter" {
            let sends = t
                .events
                .borrow()
                .iter()
                .filter(|e| e[0] == "send")
                .map(|e| e[1].clone())
                .collect::<Vec<_>>();
            assert_eq!(
                sends,
                vec![json!(false), json!(false)],
                "FRAME native captured inline unsubscribe callee"
            );
        }
        let closes = t
            .events
            .borrow()
            .iter()
            .filter(|e| e[0] == "iterator-close")
            .count();
        assert_eq!(
            closes,
            usize::from(case == "iterable"),
            "LEVEL native iterator close floor"
        );
    }
    assert_eq!(
        t.emissions.get(),
        usize::from(method == "book" && matches!(case, "reject" | "invalid-code")),
        "ERROR native independent emission count"
    );
    let frames = if method == "fut-candle" {
        if case == "disconnected" {
            0
        } else if case == "duplicates" {
            4
        } else {
            2
        }
    } else if method == "book" {
        if case == "disconnected" {
            0
        } else if case == "disposed" {
            1
        } else {
            2
        }
    } else {
        0
    };
    assert_eq!(
        t.frames.get(),
        frames,
        "FRAME native independent frame count"
    );
    assert_eq!(t.errors.borrow().len(), 0, "ASYNC native rejection floor");
    json!({"method":method,"case":case,"failure":failure,"events":t.events.borrow().clone(),"requests":t.requests.get(),"remaining":pack(&t.handlers)})
}
#[test]
fn public_streams_native_before_source() {
    let mut cases = vec![];
    for method in ["ticker", "snapshot", "instrument", "spot-ticker"] {
        for case in [
            "normal",
            "zero",
            "invalid",
            "empty",
            "missing",
            "missing-ticker",
            "alias",
        ] {
            cases.push(fixture(method, case));
        }
    }
    cases.push(fixture("snapshot", "getter-error"));
    for case in [
        "normal",
        "array-ms",
        "object",
        "missing",
        "disconnected",
        "duplicates",
        "callee-reenter",
        "data-reenter",
        "lazy-topic",
    ] {
        cases.push(fixture("fut-candle", case));
    }
    for case in [
        "normal",
        "array-ms",
        "object",
        "missing",
        "disconnected",
        "duplicates",
        "disposed",
    ] {
        cases.push(fixture("spot-candle", case));
    }
    for case in [
        "normal",
        "gap",
        "buffer",
        "buffer-gap",
        "disconnected",
        "delayed",
        "disposed",
        "cleanup-before-snapshot",
        "reject",
        "invalid-code",
    ] {
        cases.push(fixture("book", case));
    }
    for case in [
        "normal",
        "invalid-level",
        "missing",
        "duplicates",
        "disconnected",
        "iterable",
    ] {
        cases.push(fixture("spot-book", case));
    }
    assert_eq!(cases.len(), 61, "independent stream fixture floor");
    let input = json!({"cases":cases});
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/public-streams-oracle.mjs"),
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
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        61,
        "independent source fixture floor"
    );
    for (index, (native, source)) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(source.as_array().unwrap())
        .enumerate()
    {
        assert!(
            native == source,
            "source mismatch at {index} {}/{}",
            native["method"],
            native["case"]
        );
    }
}
