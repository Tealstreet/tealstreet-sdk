use safe_cex_venue_okx::{
    market_account_reads::{MarketAccountOps, ReadAwait},
    order_composition::{self as port, CompositionOps},
    order_mutations::OrderMutationOps,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = Rc<Error>;
struct Error {
    name: &'static str,
    message: Vec<u16>,
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
    Map(RefCell<Vec<(V, V)>>),
    Big(Decimal),
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
        Native::Call(x) => json!({"fn":x}),
        Native::Map(rows) => json!(
            rows.borrow()
                .iter()
                .map(|(k, v)| json!([pack(k), pack(v)]))
                .collect::<Vec<_>>()
        ),
        Native::Big(x) => json!({"big":x.fixed()}),
        _ => panic!("pack"),
    }
}
fn unused<T>() -> T {
    panic!("unused capability in this source family")
}
#[derive(Clone)]
struct Decimal {
    coefficient: i128,
    scale: u32,
}
impl Decimal {
    fn from_number(x: f64) -> Self {
        let text = safe_cex_num::js_number_string(x);
        let mut parts = text.split('.');
        let whole = parts.next().unwrap();
        let fraction = parts.next().unwrap_or("");
        Self {
            coefficient: format!("{whole}{fraction}").parse().unwrap(),
            scale: fraction.len() as u32,
        }
        .normalize()
    }
    fn normalize(mut self) -> Self {
        while self.scale > 0 && self.coefficient % 10 == 0 {
            self.coefficient /= 10;
            self.scale -= 1
        }
        self
    }
    fn aligned(&self, other: &Self) -> (i128, i128, u32) {
        let scale = self.scale.max(other.scale);
        (
            self.coefficient * 10i128.pow(scale - self.scale),
            other.coefficient * 10i128.pow(scale - other.scale),
            scale,
        )
    }
    fn minus(&self, other: &Self) -> Self {
        let (a, b, scale) = self.aligned(other);
        Self {
            coefficient: a - b,
            scale,
        }
        .normalize()
    }
    fn times(&self, other: &Self) -> Self {
        Self {
            coefficient: self.coefficient * other.coefficient,
            scale: self.scale + other.scale,
        }
        .normalize()
    }
    fn quotient(&self, other: &Self) -> Self {
        let (a, b, _) = self.aligned(other);
        Self {
            coefficient: a / b,
            scale: 0,
        }
    }
    fn le(&self, other: &Self) -> bool {
        let (a, b, _) = self.aligned(other);
        a <= b
    }
    fn fixed(&self) -> String {
        let negative = self.coefficient < 0;
        let digits = self.coefficient.abs().to_string();
        let body = if self.scale == 0 {
            digits
        } else {
            let padded = format!("{:0>width$}", digits, width = self.scale as usize + 1);
            let split = padded.len() - self.scale as usize;
            format!("{}.{}", &padded[..split], &padded[split..])
        };
        format!("{}{body}", if negative { "-" } else { "" })
    }
}
fn big(x: Decimal) -> V {
    Rc::new(Native::Big(x))
}
fn decimal(x: &V) -> Decimal {
    if let Native::Big(x) = x.as_ref() {
        x.clone()
    } else {
        Decimal::from_number(num(x))
    }
}
struct State {
    case: String,
    prototype_constructor: V,
    err: E,
    opts: V,
    market: V,
    store: V,
    positions: V,
    receiver: V,
    effects: RefCell<Vec<Value>>,
    calls: RefCell<Vec<(String, Vec<V>)>>,
    uuid: Cell<usize>,
    recent: V,
    history: V,
}
fn event(t: &State, name: &str, args: Vec<V>) -> Result<(), E> {
    t.effects.borrow_mut().push(json!(
        std::iter::once(json!(name))
            .chain(args.iter().map(pack))
            .collect::<Vec<_>>()
    ));
    t.calls.borrow_mut().push((name.into(), args));
    if t.case == format!("fail:{name}") {
        Err(t.err.clone())
    } else {
        Ok(())
    }
}
fn promise(x: Result<V, E>) -> V {
    Rc::new(Native::P(x))
}
fn await_value(x: V) -> ReadAwait<V, E> {
    Box::pin(async move {
        if let Native::P(r) = x.as_ref() {
            r.clone()
        } else {
            Ok(x.clone())
        }
    })
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(x) => x,
        Poll::Pending => panic!("fixture promises are ready"),
    }
}
fn table(name: &str) -> V {
    let entries = match name {
        "ORDER_SIDE" => vec![("buy", s("buy")), ("sell", s("sell"))],
        "ORDER_TYPE" => vec![
            ("market", s("market")),
            ("limit", s("limit")),
            ("post_only", s("limit")),
            ("fok", s("limit")),
            ("ioc", s("limit")),
            ("optimal_limit_ioc", s("market")),
        ],
        "ORDER_STATUS" => vec![("live", s("open")), ("partially_filled", s("open"))],
        "REVERSE_ORDER_TYPE" => vec![
            ("market", s("market")),
            ("limit", s("limit")),
            ("conditional", s("trigger")),
            ("stop_market", s("trigger")),
            ("take_profit_market", s("trigger")),
            ("trailing_stop_market", s("move_order_stop")),
        ],
        _ => panic!("table"),
    };
    object(entries)
}
fn table_get(o: &V, key: &V, constructor: &V) -> V {
    let key = text(key);
    let x = get(o, &key);
    if !matches!(x.as_ref(), Native::U) {
        return x;
    }
    if key == "constructor" {
        constructor.clone()
    } else if key == "__proto__" {
        object(vec![])
    } else {
        u()
    }
}
fn fresh_error(t: &State, message: V) -> Result<E, E> {
    event(t, "newError", vec![message.clone()])?;
    let Native::S(units) = message.as_ref() else {
        panic!("message")
    };
    Ok(Rc::new(Error {
        name: "Error",
        message: units.clone(),
        fresh: true,
        code: None,
        ids: None,
    }))
}
fn invoke(t: Rc<State>, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Member(receiver, name) = callee.as_ref() {
        return Ok(match name.as_str() {
            "slice" => {
                if let Native::S(units) = receiver.as_ref() {
                    Rc::new(Native::S(
                        units[num(&args[0]) as usize..(num(&args[1]) as usize).min(units.len())]
                            .to_vec(),
                    ))
                } else {
                    let rows = items(receiver);
                    array(
                        rows[num(&args[0]) as usize
                            ..(num(&args[1]).max(0.).min(rows.len() as f64) as usize)]
                            .to_vec(),
                    )
                }
            }
            "push" => {
                let Native::A(rows) = receiver.as_ref() else {
                    panic!("array")
                };
                rows.borrow_mut().push(args[0].clone());
                n(rows.borrow().len() as f64)
            }
            "toFixed" => s(&decimal(receiver).fixed()),
            "dividedToIntegerBy" => big(decimal(receiver).quotient(&decimal(&args[0]))),
            "multipliedBy" => big(decimal(receiver).times(&decimal(&args[0]))),
            "minus" => big(decimal(receiver).minus(&decimal(&args[0]))),
            "lte" => b(decimal(receiver).le(&decimal(&args[0]))),
            "gt" => b(!decimal(receiver).le(&decimal(&args[0]))),
            "toLowerCase" => s(&text(receiver).to_lowercase()),
            "toUpperCase" => s(&text(receiver).to_uppercase()),
            "set" => {
                let Native::Map(rows) = receiver.as_ref() else {
                    panic!("map")
                };
                let mut rows = rows.borrow_mut();
                if let Some((_, v)) = rows.iter_mut().find(|(k, _)| equal(k, &args[0])) {
                    *v = args[1].clone()
                } else {
                    rows.push((args[0].clone(), args[1].clone()))
                }
                receiver.clone()
            }
            "values" => {
                let Native::Map(rows) = receiver.as_ref() else {
                    panic!("map")
                };
                array(rows.borrow().iter().map(|(_, v)| v.clone()).collect())
            }
            other => panic!("member {other}"),
        });
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("callee")
    };
    Ok(match name.as_str() {
        "this.placeOrders" => {
            event(&t, "placeOrders", args.clone())?;
            let row = items(&args[0]);
            assert!(Rc::ptr_eq(&row[0], &t.opts), "SINGLE native original opts");
            promise(Ok(array(vec![s("single")])))
        }
        "this.assertCallerClientOrderIds" => {
            event(&t, "preflight", args)?;
            u()
        }
        "this.getOrFetchPositionMode" => promise(event(&t, "mode", args).map(|_| u())),
        "this.isAlgoOrder" => b(matches!(
            text(&args[0]).as_str(),
            "conditional" | "stop_market" | "take_profit_market" | "trailing_stop_market"
        )),
        "this.getAlgoOrderPlacementPriority" => n(match text(&args[0]).as_str() {
            "stop_market" => 0.,
            "take_profit_market" => 1.,
            "conditional" => 2.,
            _ => 3.,
        }),
        "this.formatCreateOrder" => {
            event(&t, "formatCreate", args.clone())?;
            array(vec![object(vec![
                ("sz", s(if t.case == "small" { "0" } else { "1" })),
                ("tag", get(&args[0], "tag")),
            ])])
        }
        "this.formatAlgoOrder" => {
            event(&t, "formatAlgo", vec![args[0].clone()])?;
            object(vec![(
                "tag",
                if truth(&get(&args[0], "tag")) {
                    get(&args[0], "tag")
                } else {
                    get(&args[0], "type")
                },
            )])
        }
        "this.deriveAlgoOrdersFromNormalOrdersOpts" => {
            event(&t, "derive", args)?;
            if t.case == "derived" {
                array(vec![object(vec![
                    ("type", s("stop_market")),
                    ("tag", s("derived")),
                ])])
            } else {
                array(vec![])
            }
        }
        "this.placeOrderBatch" | "this.placeAlgoOrderBatch" => {
            let tag = if name == "this.placeOrderBatch" {
                "normalBatch"
            } else {
                "algoBatch"
            };
            promise(event(&t, tag, args).map(|_| {
                array(vec![s(if tag == "normalBatch" {
                    "normal"
                } else {
                    "algo"
                })])
            }))
        }
        "this.emitAndLogError" => {
            event(&t, "emit", args)?;
            u()
        }
        "this.contractsToBase" => {
            assert!(
                Rc::ptr_eq(&args[1], &t.market),
                "CONVERSION native original market"
            );
            let a = if t.case == "zero-base" {
                0.
            } else {
                num(&args[0]) * 2.
            };
            event(&t, "base", vec![args[0].clone(), args[2].clone()])?;
            n(a)
        }
        "this.contractsToNotional" => {
            assert!(
                Rc::ptr_eq(&args[1], &t.market),
                "CONVERSION native notional market"
            );
            let a = num(&args[0]) * num(&args[2]) * 2.;
            event(&t, "notional", vec![args[0].clone(), args[2].clone()])?;
            n(a)
        }
        "this.toFiniteNumber" => {
            let x = safe_cex_num::parse_float(&text(&args[0]));
            n(if x.is_finite() { x } else { 0. })
        }
        "this.convertMarginValueToUsd" => {
            assert!(
                Rc::ptr_eq(&args[2], &t.market),
                "CONVERSION native margin market"
            );
            let a = num(&args[0]) * 2.;
            event(
                &t,
                "margin",
                vec![args[0].clone(), args[1].clone(), args[3].clone()],
            )?;
            n(a)
        }
        "this.resolveTriggerReferencePrice" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.market),
                "TRIGGER native original market"
            );
            event(&t, "reference", vec![args[1].clone(), args[2].clone()])?;
            n(if t.case == "no-reference" {
                0.
            } else if t.case == "negative-reference" {
                -1.
            } else {
                10.
            })
        }
        "this.getReduceOnlyTriggerOrderType" => {
            assert!(
                Rc::ptr_eq(&args[1], &t.market),
                "MAPPER native trigger market"
            );
            event(&t, "reduceType", vec![args[0].clone()])?;
            s(if t.case == "take-trigger" {
                "take_profit_market"
            } else {
                "stop_market"
            })
        }
        "this.getPositionSide" => {
            event(&t, "positionSide", args)?;
            s("net")
        }
        "this.resolveCallerClientOrderId" => {
            event(&t, "resolveCaller", args.clone())?;
            get(&args[0], "clientOrderId")
        }
        "this.assertSingleNativeClientOrderId" => {
            event(&t, "singleId", args)?;
            if t.case == "caller-split" {
                return Err(t.err.clone());
            }
            u()
        }
        "uuid" => {
            event(&t, "uuid", args)?;
            t.uuid.set(t.uuid.get() + 1);
            s(&format!("id{}", t.uuid.get()))
        }
        "parseFloat" => n(safe_cex_num::parse_float(&text(&args[0]))),
        "String" => {
            if matches!(args[0].as_ref(), Native::S(_)) {
                args[0].clone()
            } else {
                s(&text(&args[0]))
            }
        }
        "Number.parseInt" => n(safe_cex_num::parse_int(&text(&args[0]), Some(10))),
        "Number.isFinite" => b(matches!(args[0].as_ref(),Native::N(x)if x.is_finite())),
        "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
        "Math.abs" => n(num(&args[0]).abs()),
        "Math.floor" => n(num(&args[0]).floor()),
        "Math.min" => n(safe_cex_num::math_min(num(&args[0]), num(&args[1]))),
        "Math.max" => n(safe_cex_num::math_max(num(&args[0]), num(&args[1]))),
        "adjust" => n(safe_cex_num::adjust(num(&args[0]), num(&args[1]))),
        "subtract" => n(safe_cex_num::subtract(num(&args[0]), num(&args[1]))),
        "add" => n(safe_cex_num::add(num(&args[0]), num(&args[1]))),
        "omitUndefined" => {
            let Native::O(fields) = args[0].as_ref() else {
                panic!("object")
            };
            Rc::new(Native::O(RefCell::new(
                fields
                    .borrow()
                    .iter()
                    .filter(|(_, v)| !matches!(v.as_ref(), Native::U))
                    .cloned()
                    .collect(),
            )))
        }
        "inverseObj" => {
            let Native::O(fields) = args[0].as_ref() else {
                panic!("table")
            };
            object(
                fields
                    .borrow()
                    .iter()
                    .map(|(k, v)| (if text(v) == "buy" { "buy" } else { "sell" }, s(k)))
                    .collect(),
            )
        }
        "getOKXTriggerPxType" => s(match text(&args[0]).as_str() {
            "mark_price" => "mark",
            "index_price" => "index",
            _ => "last",
        }),
        "parseOKXTriggerPxType" => s(match text(&args[0]).as_str() {
            "mark" => "mark_price",
            "index" => "index_price",
            _ => "last_price",
        }),
        "virtualClock.getCurrentTimeMs" => {
            event(&t, "clock", args)?;
            n(-0.)
        }
        "createPositionId" => {
            let mode = text(&args[2]);
            let side = text(&args[1]);
            let symbol = text(&args[0]);
            s(if mode == "oneway" || !truth(&args[1]) {
                &symbol
            } else {
                return Ok(s(&format!("{symbol}:{side}")));
            })
        }
        "normalizeExecutionPositionSide" => {
            s(match text(&args[0]).trim().to_lowercase().as_str() {
                "long" | "open long" | "close long" => "long",
                "short" | "open short" | "close short" => "short",
                _ => "both",
            })
        }
        "BigNumber.minimum" => {
            let a = decimal(&args[0]);
            let b = decimal(&args[1]);
            big(if a.le(&b) { a } else { b })
        }
        "this.getMarketInstrumentType" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.market),
                "FILLS native instrument market identity"
            );
            s("SWAP")
        }
        "this.xhr.get" => {
            let route = if text(&args[0]) == "recent" {
                "recent"
            } else {
                "history"
            };
            let params = get(&args[1], "params");
            let result = event(&t, "get", vec![s(route), params]).map(|_| {
                object(vec![(
                    "data",
                    object(vec![(
                        "data",
                        if t.case == "bad-data" {
                            object(vec![])
                        } else if route == "recent" {
                            t.recent.clone()
                        } else {
                            t.history.clone()
                        },
                    )]),
                )])
            });
            promise(result)
        }
        other => panic!("call {other}"),
    })
}
fn host(t: Rc<State>) -> Rc<CompositionOps<V, E>> {
    let capture = t.clone();
    let inv = t.clone();
    let errors = t.clone();
    let intrinsic = t.clone();
    let value = MarketAccountOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            Ok(match name {
                "this.store.marketsBySymbol" => get(&capture.store, "marketsBySymbol"),
                "this.store.marketsById" => get(&capture.store, "marketsById"),
                "this.store.tickersById" => get(&capture.store, "tickersById"),
                "this.store.positions" => capture.positions.clone(),
                "this._accountPositionMode" => get(&capture.receiver, "mode"),
                "this.isPublicDataOnly" => b(capture.case == "public"),
                "ORDER_SIDE" | "ORDER_TYPE" | "ORDER_STATUS" | "REVERSE_ORDER_TYPE" => table(name),
                "BROKER_ID" => s("e847386590ce4dBC"),
                "OrderType.Limit" => s("limit"),
                "OrderType.Conditional" => s("conditional"),
                "OrderType.StopLoss" => s("stop_market"),
                "OrderType.TakeProfit" => s("take_profit_market"),
                "OrderType.TrailingStopLoss" => s("trailing_stop_market"),
                "OrderSide.Sell" => s("sell"),
                "OrderSide.Buy" => s("buy"),
                "OrderStatus.Open" => s("open"),
                "OrderTimeInForce.PostOnly" => s("PostOnly"),
                "OrderTimeInForce.GoodTillCancel" => s("GoodTillCancel"),
                "OrderStopTriggerType.MarkPrice" => s("mark_price"),
                "PositionSide.Long" => s("long"),
                "PositionSide.Short" => s("short"),
                "PositionMode.OneWay" => s("oneway"),
                "MarginType.Cross" => s("cross"),
                "MarginType.Isolated" => s("isolated"),
                "ENDPOINTS.FILLS" => s("recent"),
                "ENDPOINTS.FILLS_HISTORY" => s("history"),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        invoke: Rc::new(move |f, args| invoke(inv.clone(), f, args)),
        intrinsic: Rc::new(move |name, args| {
            Ok(match name {
                "get-index" => table_get(&args[0], &args[1], &intrinsic.prototype_constructor),
                "destructure-two" => args[0].clone(),
                "null" => Rc::new(Native::Null),
                "template-string" => {
                    if matches!(args[0].as_ref(), Native::S(_)) {
                        args[0].clone()
                    } else {
                        s(&text(&args[0]))
                    }
                }
                "template-concat" | "template-suffix" | "colon-append" => {
                    let mut units = if let Native::S(x) = args[0].as_ref() {
                        x.clone()
                    } else {
                        text(&args[0]).encode_utf16().collect()
                    };
                    if name == "colon-append" {
                        units.push(58)
                    }
                    units.extend(if let Native::S(x) = args[1].as_ref() {
                        x.clone()
                    } else {
                        text(&args[1]).encode_utf16().collect()
                    });
                    Rc::new(Native::S(units))
                }
                "id-prefix" => {
                    let mut units = "id:".encode_utf16().collect::<Vec<_>>();
                    if let Native::S(x) = args[0].as_ref() {
                        units.extend(x)
                    } else {
                        units.extend(text(&args[0]).encode_utf16())
                    }
                    Rc::new(Native::S(units))
                }
                "market-error-message" => s(&format!("Market {} not found on OKX", text(&args[0]))),
                "market-validation-message" => {
                    s(&format!("Market {} {}", text(&args[0]), text(&args[1])))
                }
                "amount-validation-message" => s(&format!(
                    "Order amount for {} must be finite and positive after adjustment",
                    text(&args[0])
                )),
                "greater-than" => b(num(&args[0]) > num(&args[1])),
                "greater-or-equal" => b(num(&args[0]) >= num(&args[1])),
                "less-than" => b(num(&args[0]) < num(&args[1])),
                "less-or-equal" => b(num(&args[0]) <= num(&args[1])),
                "loose-equal" => b(equal(&args[0], &args[1])),
                "unary-plus" => n(num(&args[0])),
                "subtract" => n(num(&args[0]) - num(&args[1])),
                "multiply" => n(num(&args[0]) * num(&args[1])),
                "divide" => n(num(&args[0]) / num(&args[1])),
                "array-spread" => array(args.iter().flat_map(items).collect()),
                "object-spread" => {
                    let Native::O(fields) = args[0].as_ref() else {
                        panic!("object")
                    };
                    Rc::new(Native::O(RefCell::new(fields.borrow().clone())))
                }
                "define-literal-field" => {
                    put(&args[0], &text(&args[1]), args[2].clone());
                    u()
                }
                "construct" => {
                    let Native::Call(name) = args[0].as_ref() else {
                        panic!("class")
                    };
                    match name.as_str() {
                        "BigNumber" => big(Decimal::from_number(num(&args[1]))),
                        "Map" => Rc::new(Native::Map(RefCell::new(vec![]))),
                        _ => panic!("constructor"),
                    }
                }
                other => {
                    panic!("intrinsic {other}")
                }
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
                    .map(|x| f(x))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }),
        flat_map: Rc::new(|v, f| {
            let mut out = vec![];
            for x in items(&v) {
                out.extend(items(&f(x)?))
            }
            Ok(array(out))
        }),
        reduce: Rc::new(|v, mut acc, f| {
            for x in items(&v) {
                acc = f(acc, x)?
            }
            Ok(acc)
        }),
        for_of: Rc::new(|v, f| {
            for x in items(&v) {
                f(x)?
            }
            Ok(())
        }),
        some: Rc::new(|v, f| {
            for x in items(&v) {
                if f(x)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
        await_value: Rc::new(await_value),
        error_value: Rc::new(|e| object(vec![("message", Rc::new(Native::S(e.message.clone())))])),
        new_error: Rc::new(move |_, message| fresh_error(&errors, message)),
        bind_first: Rc::new(|_, _| unused()),
        filter: Rc::new(|_, _| unused()),
        sum_by: Rc::new(|_, _, _| unused()),
        parallel_map: Rc::new(|_, _, _| unused()),
    };
    let order = OrderMutationOps {
        is_terminal_order: Rc::new(|order| {
            let status = match get(order, "status").as_ref() {
                Native::S(status) => String::from_utf16(status).ok().and_then(|status| {
                    serde_json::from_value(serde_json::Value::String(status)).ok()
                }),
                _ => None,
            };
            Ok(safe_cex_store::store::is_terminal_order_status(status))
        }),
        value: Rc::new(value),
        has: Rc::new(|_, _| unused()),
        filter: Rc::new(|_, _| unused()),
        find: Rc::new(|v, f| {
            for x in items(&v) {
                if truth(&f(x.clone())?) {
                    return Ok(x);
                }
            }
            Ok(u())
        }),
        optional_find: Rc::new(|_, _| unused()),
        partition: Rc::new(|_, v, f| {
            let mut yes = vec![];
            let mut no = vec![];
            for x in items(&v) {
                if truth(&f(x.clone())?) {
                    yes.push(x)
                } else {
                    no.push(x)
                }
            }
            Ok(array(vec![array(yes), array(no)]))
        }),
        for_each: Rc::new(|_, _| unused()),
        series: Rc::new(|_, _, _| unused()),
        adopt_return: Rc::new(await_value),
    };
    let sorter = Rc::new(|callee: V, compare: Rc<dyn Fn(V, V) -> Result<V, E>>| {
        let Native::Member(receiver, key) = callee.as_ref() else {
            panic!("sort")
        };
        assert_eq!(key, "sort");
        let mut rows = items(receiver);
        for i in 1..rows.len() {
            let mut j = i;
            while j > 0 && num(&compare(rows[j].clone(), rows[j - 1].clone())?) < 0. {
                rows.swap(j, j - 1);
                j -= 1;
            }
        }
        let Native::A(out) = receiver.as_ref() else {
            panic!("array")
        };
        *out.borrow_mut() = rows;
        Ok(receiver.clone())
    });
    let inv = t.clone();
    Rc::new(CompositionOps {
        order: Rc::new(order),
        sort: sorter,
        map_bound: Rc::new(move |callee, formatter| {
            let Native::Member(receiver, key) = callee.as_ref() else {
                panic!("map")
            };
            assert_eq!(key, "map");
            let mut out = vec![];
            for (i, x) in items(receiver).into_iter().enumerate() {
                out.push(invoke(
                    inv.clone(),
                    formatter.clone(),
                    vec![x, n(i as f64), receiver.clone()],
                )?)
            }
            Ok(array(out))
        }),
    })
}
fn fill(id: &str, time: &str, amount: &str) -> V {
    object(vec![
        ("tradeId", s(id)),
        ("billId", s(&format!("B{id}"))),
        ("instId", s("I")),
        ("ordId", s("O")),
        ("side", s("Sell")),
        ("fillPx", s("12.5")),
        ("fillSz", s(amount)),
        ("fillTime", s(time)),
        ("fee", s("-.3")),
        ("feeCcy", s("USD")),
        ("fillPnl", s("1.2")),
        ("posSide", s("Close Long")),
        ("execType", s("M")),
    ])
}
fn fixture(method: &str, case: &str) -> Value {
    let err = Rc::new(Error {
        name: "TypeError",
        message: "DOWN".encode_utf16().collect(),
        fresh: false,
        code: Some("ORDER"),
        ids: Some([6, 8]),
    });
    let market = object(vec![
        ("id", s("I")),
        ("symbol", s("SYM")),
        (
            "precision",
            object(vec![
                ("amount", n(if case == "bad-step" { 0. } else { 0.1 })),
                ("price", n(0.5)),
            ]),
        ),
        (
            "limits",
            object(vec![(
                "amount",
                object(vec![(
                    "max",
                    n(if case == "below-step" {
                        0.05
                    } else if case == "bad-max" {
                        f64::INFINITY
                    } else {
                        10.
                    }),
                )]),
            )]),
        ),
    ]);
    let positions = if case == "no-position" {
        array(vec![])
    } else {
        array(vec![object(vec![
            ("id", s("SYM:short")),
            ("symbol", s("SYM")),
            ("side", s("short")),
            ("entryPrice", n(8.)),
            ("leverage", n(7.)),
        ])])
    };
    let store = object(vec![
        (
            "marketsBySymbol",
            if case == "missing" {
                object(vec![])
            } else {
                object(vec![("SYM", market.clone())])
            },
        ),
        (
            "marketsById",
            if case == "missing" {
                object(vec![])
            } else {
                object(vec![
                    ("I", market.clone()),
                    ("OTHER", object(vec![("symbol", s("OTHER"))])),
                ])
            },
        ),
        (
            "tickersById",
            if case == "no-ticker" {
                object(vec![])
            } else {
                object(vec![(
                    "I",
                    object(vec![("mark", n(if case == "no-mark" { 0. } else { 12. }))]),
                )])
            },
        ),
    ]);
    let opts = object(vec![
        ("symbol", s("SYM")),
        (
            "type",
            s(if case == "market" {
                "market"
            } else if case == "stop" {
                "stop_market"
            } else if case == "take" {
                "take_profit_market"
            } else if case == "trailing" {
                "trailing_stop_market"
            } else if method == "algo" {
                "conditional"
            } else {
                "limit"
            }),
        ),
        ("side", s("buy")),
        (
            "amount",
            n(if case == "split" || case == "caller-split" {
                20.3
            } else if case == "zero" {
                0.
            } else if case == "nan" {
                f64::NAN
            } else {
                2.3
            }),
        ),
        ("price", n(if case == "zero-price" { 0. } else { 12.7 })),
        (
            "triggerPrice",
            n(if case == "bad-trigger" { 0. } else { 13.2 }),
        ),
        (
            "timeInForce",
            s(if case == "post-only" {
                "PostOnly"
            } else {
                "GoodTillCancel"
            }),
        ),
        ("reduceOnly", b(case == "reduce")),
    ]);
    if case == "caller" || case == "caller-split" {
        put(&opts, "clientOrderId", s("caller"))
    }
    if case == "lone" {
        put(&opts, "clientOrderId", Rc::new(Native::S(vec![0xd800])))
    }
    let recent = vec![
        fill("a", "1700000000000", "2"),
        fill("b", "1700000001000", "2"),
    ];
    let history = vec![
        fill("a", "1700000000000", "3"),
        fill("c", "1699999999000", "2"),
    ];
    let recent = array(recent);
    let history = array(history);
    if case == "filtered" {
        let Native::A(rows) = recent.as_ref() else {
            panic!("array")
        };
        for (name, field, value) in [
            ("bad", "side", s("invalid")),
            ("wrong", "instId", s("OTHER")),
            ("zero", "fillTime", s("0")),
            ("sz", "fillSz", n(0.)),
        ] {
            let r = fill(
                name,
                if name == "wrong" {
                    "1700000003000"
                } else if name == "bad" {
                    "1700000002000"
                } else {
                    "1700000000000"
                },
                "2",
            );
            put(&r, field, value);
            rows.borrow_mut().push(r)
        }
    }
    if case == "fallback" {
        for r in items(&recent).into_iter().chain(items(&history)) {
            let Native::O(fields) = r.as_ref() else {
                panic!("row")
            };
            fields
                .borrow_mut()
                .retain(|(k, _)| k != "tradeId" && k != "billId");
        }
    }
    if case == "lone" {
        put(
            &items(&recent)[0],
            "tradeId",
            Rc::new(Native::S(vec![0xd800])),
        )
    }
    let t = Rc::new(State {
        case: case.into(),
        prototype_constructor: Rc::new(Native::Call("Object".into())),
        err,
        opts: opts.clone(),
        market: market.clone(),
        store,
        positions,
        receiver: object(vec![(
            "mode",
            if case == "no-mode" {
                Rc::new(Native::Null)
            } else {
                s("hedged")
            },
        )]),
        effects: RefCell::new(vec![]),
        calls: RefCell::new(vec![]),
        uuid: Cell::new(0),
        recent,
        history,
    });
    let h = host(t.clone());
    let mut original_row = None;
    let result = match method {
        "single" => run(port::place_order(h.clone(), opts)),
        "place" => {
            let rows = if case == "empty" {
                array(vec![])
            } else {
                array(vec![
                    object(vec![("type", s("limit")), ("tag", s("L"))]),
                    object(vec![("type", s("conditional")), ("tag", s("C"))]),
                    object(vec![("type", s("take_profit_market")), ("tag", s("T"))]),
                    object(vec![("type", s("stop_market")), ("tag", s("S"))]),
                ])
            };
            run(port::place_orders(h.clone(), rows))
        }
        "algo" => port::format_algo_order(&h, opts),
        "create" => port::format_create_order(&h, opts),
        "trigger" => port::get_reduce_only_trigger_order_type(
            &h,
            object(vec![
                ("side", s(if case == "sell" { "sell" } else { "buy" })),
                (
                    "triggerPx",
                    s(if case == "invalid" {
                        "bad"
                    } else if case == "zero" {
                        "0"
                    } else if case == "equal" {
                        "10"
                    } else if case == "sell" {
                        "12"
                    } else {
                        "8"
                    }),
                ),
                ("triggerPxType", s("mark")),
                ("last", s("9")),
            ]),
            market,
        ),
        "orders" => {
            let row = object(vec![
                ("instId", s("I")),
                (
                    "ordType",
                    s(match case {
                        "trigger" | "take-trigger" | "opening-trigger" => "trigger",
                        "oco" => "oco",
                        "conditional" => "conditional",
                        "trailing" | "no-position" => "move_order_stop",
                        "post-only" => "post_only",
                        "prototype" => "constructor",
                        _ => "limit",
                    }),
                ),
                ("ordId", s("O")),
                (
                    "algoId",
                    if case == "lone" {
                        Rc::new(Native::S(vec![0xd800]))
                    } else {
                        s("A")
                    },
                ),
                (
                    "state",
                    s(if case == "prototype" {
                        "constructor"
                    } else {
                        "live"
                    }),
                ),
                (
                    "side",
                    s(if case == "prototype" {
                        "constructor"
                    } else {
                        "buy"
                    }),
                ),
                ("sz", s("3.5")),
                ("accFillSz", s("1")),
                ("px", s("12.5")),
                ("last", s("12")),
                ("triggerPx", s("13")),
                ("ordPx", s("15")),
                (
                    "reduceOnly",
                    if case == "boolean-reduce" {
                        b(true)
                    } else if case == "opening-trigger" {
                        b(false)
                    } else {
                        s("true")
                    },
                ),
                ("slTriggerPx", s("10")),
                ("tpTriggerPx", s("14")),
                ("triggerPxType", s("index")),
                ("slTriggerPxType", s("mark")),
                ("tpTriggerPxType", s("last")),
                ("callbackSpread", s("1.5")),
                ("moveTriggerPx", s("2.5")),
            ]);
            original_row = Some(row.clone());
            port::map_orders(
                h.clone(),
                if case == "empty" {
                    array(vec![])
                } else {
                    array(vec![row])
                },
            )
        }
        "positions" => {
            let row = object(vec![
                ("instId", s("I")),
                (
                    "pos",
                    s(if case == "zero" {
                        "0"
                    } else if case == "nan" {
                        "bad"
                    } else {
                        "-3.5"
                    }),
                ),
                ("posSide", s(if case == "long" { "long" } else { "net" })),
                ("avgPx", s("11.5")),
                ("upl", s("1.2")),
                ("ccy", s("USD")),
                ("lever", s("")),
                ("liqPx", s("2.5")),
                (
                    "mgnMode",
                    s(if case == "isolated" {
                        "isolated"
                    } else {
                        "cross"
                    }),
                ),
            ]);
            original_row = Some(row.clone());
            port::map_positions(
                h.clone(),
                if case == "empty" {
                    array(vec![])
                } else {
                    array(vec![row])
                },
            )
        }
        "fills" => {
            let opts = object(vec![
                ("symbol", if case == "no-symbol" { u() } else { s("SYM") }),
                (
                    "limit",
                    n(if case == "full" {
                        2.
                    } else if case == "zero-limit" {
                        0.
                    } else {
                        5.
                    }),
                ),
            ]);
            if case == "window" {
                put(&opts, "from", n(1700000000.));
                put(&opts, "to", n(1700000001.));
            }
            if case == "order-filter" {
                put(&opts, "orderId", s("OTHER"));
            }
            run(port::fetch_historical_fills(h.clone(), opts))
        }
        _ => panic!("method"),
    };
    let (failed, value, error) = match result {
        Ok(x) => (false, x, None),
        Err(e) => {
            if !e.fresh {
                assert!(
                    Rc::ptr_eq(&e, &t.err),
                    "ERROR native original cause identity"
                );
                assert_eq!(e.name, "TypeError");
                assert_eq!(e.code, Some("ORDER"));
                assert_eq!(e.ids, Some([6, 8]));
            }
            let mut observed = json!({"original":!e.fresh,"name":e.name,"message":pack(&Rc::new(Native::S(e.message.clone())))});
            if !e.fresh {
                observed["code"] = json!(e.code.unwrap());
                observed["ids"] = json!(e.ids.unwrap());
            }
            (true, u(), Some(observed))
        }
    };
    if !failed {
        let calls = t.calls.borrow();
        if method == "single" {
            assert_eq!(items(&value).len(), 1, "SINGLE native result count");
            assert!(
                equal(&items(&value)[0], &s("single")),
                "SINGLE native awaited result"
            );
        }
        if method == "place" && case == "normal" {
            let names = calls.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>();
            assert_eq!(
                names,
                [
                    "preflight",
                    "mode",
                    "formatCreate",
                    "derive",
                    "formatAlgo",
                    "formatAlgo",
                    "formatAlgo",
                    "normalBatch",
                    "algoBatch"
                ],
                "PLACE native validation and sequential batch effects"
            );
            let tags = calls
                .iter()
                .filter(|(n, _)| n == "formatAlgo")
                .map(|(_, args)| text(&get(&args[0], "tag")))
                .collect::<Vec<_>>();
            assert_eq!(
                tags,
                ["S", "T", "C"],
                "PLACE native algorithm priority order"
            );
        }
        if method == "trigger" && case == "normal" {
            assert!(
                equal(&value, &s("take_profit_market")),
                "TRIGGER native buy below reference type"
            );
        }
        if method == "algo" && case == "normal" {
            assert!(
                equal(&get(&value, "ordType"), &s("trigger")),
                "ALGO native opening trigger type"
            );
            assert!(
                equal(&get(&value, "sz"), &s("2.3")),
                "ALGO native adjusted amount"
            );
            assert!(
                equal(&get(&value, "triggerPx"), &s("13")),
                "ALGO native adjusted trigger price"
            );
        }
        if method == "create" && case == "normal" {
            let rows = items(&value);
            assert_eq!(rows.len(), 1, "CREATE native unsplit count");
            assert!(
                equal(&get(&rows[0], "sz"), &s("2.3")),
                "CREATE native exact decimal size"
            );
            assert!(
                equal(&get(&rows[0], "clOrdId"), &s("e847386590ce4dBCid1")),
                "CREATE native minted broker ID"
            );
        }
        if method == "create" && case == "split" {
            let sizes = items(&value)
                .iter()
                .map(|r| text(&get(r, "sz")))
                .collect::<Vec<_>>();
            assert_eq!(
                sizes,
                ["10", "10", "0.3"],
                "CREATE native decimal split widths"
            );
        }
        if method == "orders" && case == "normal" {
            let rows = items(&value);
            assert_eq!(rows.len(), 1, "ORDERS native mapped count");
            let row = &rows[0];
            assert_eq!(
                num(&get(row, "remaining")),
                5.,
                "ORDERS native converted remaining"
            );
            assert_eq!(
                num(&get(row, "filled")),
                2.,
                "ORDERS native converted filled"
            );
            assert_eq!(
                num(&get(row, "lastUpdate")).to_bits(),
                (-0f64).to_bits(),
                "ORDERS native clock bits"
            );
            assert!(
                Rc::ptr_eq(&get(row, "info"), original_row.as_ref().unwrap()),
                "ORDERS native original info alias"
            );
        }
        if method == "orders" && case == "prototype" {
            let row = items(&value)[0].clone();
            assert!(
                matches!(get(&row,"type").as_ref(),Native::Call(name)if name=="Object"),
                "ORDERS native inherited callable table result"
            );
            assert!(
                Rc::ptr_eq(&get(&row, "type"), &get(&row, "status"))
                    && Rc::ptr_eq(&get(&row, "side"), &get(&row, "type")),
                "ORDERS native inherited constructor identity"
            );
        }
        if method == "positions" && case == "normal" {
            let row = items(&value)[0].clone();
            assert_eq!(
                num(&get(&row, "contracts")),
                7.,
                "POSITIONS native base contracts"
            );
            assert_eq!(
                num(&get(&row, "notional")),
                84.,
                "POSITIONS native notional"
            );
            assert_eq!(
                num(&get(&row, "leverage")),
                7.,
                "POSITIONS native existing leverage"
            );
            assert!(
                Rc::ptr_eq(&get(&row, "info"), original_row.as_ref().unwrap()),
                "POSITIONS native original info alias"
            );
        }
        if method == "fills" && case == "normal" {
            let rows = items(&value);
            assert_eq!(rows.len(), 3, "FILLS native distinct trade count");
            assert!(
                equal(&get(&rows[0], "id"), &s("b")),
                "FILLS native newest first"
            );
            assert_eq!(
                num(&get(&rows[1], "qty")),
                6.,
                "FILLS native later duplicate replaces quantity"
            );
            assert!(
                equal(&get(&rows[1], "id"), &s("a")),
                "FILLS native trade identity survives bill rows"
            );
        }
    }
    let mut record = json!({"method":method,"case":case,"effects":t.effects.borrow().clone(),"failed":failed,"result":pack(&value)});
    if let Some(e) = error {
        record["error"] = e;
    }
    record
}
fn check(method: &str, cases: &[&str], expected: usize) {
    assert_eq!(cases.len(), expected, "native fixed fixture floor");
    let native = cases.iter().map(|c| fixture(method, c)).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/order-composition-oracle.mjs"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(json!({"cases":native.iter().map(|r|json!({"method":r["method"],"case":r["case"]})).collect::<Vec<_>>()}).to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        expected,
        "source independent fixture floor"
    );
    for (i, (n, s)) in native.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(n, s, "maintained source {method}/{i}");
    }
}
#[test]
fn single_native() {
    check("single", &["normal", "fail:placeOrders"], 2)
}
#[test]
fn place_native() {
    check(
        "place",
        &[
            "normal",
            "empty",
            "small",
            "derived",
            "fail:preflight",
            "fail:mode",
            "fail:normalBatch",
            "fail:algoBatch",
            "fail:formatCreate",
        ],
        9,
    )
}
#[test]
fn trigger_native() {
    check(
        "trigger",
        &[
            "normal",
            "sell",
            "invalid",
            "zero",
            "equal",
            "no-reference",
            "negative-reference",
            "fail:reference",
        ],
        8,
    )
}
#[test]
fn algo_native() {
    check(
        "algo",
        &[
            "normal",
            "stop",
            "take",
            "trailing",
            "zero-price",
            "zero",
            "nan",
            "bad-trigger",
            "missing",
            "fail:positionSide",
            "lone",
        ],
        11,
    )
}
#[test]
fn create_native() {
    check(
        "create",
        &[
            "normal",
            "split",
            "caller",
            "caller-split",
            "post-only",
            "market",
            "zero-price",
            "reduce",
            "zero",
            "nan",
            "bad-max",
            "bad-step",
            "below-step",
            "missing",
            "lone",
            "fail:positionSide",
        ],
        16,
    )
}
#[test]
fn orders_native() {
    check(
        "orders",
        &[
            "normal",
            "trigger",
            "take-trigger",
            "opening-trigger",
            "oco",
            "conditional",
            "trailing",
            "no-position",
            "post-only",
            "prototype",
            "boolean-reduce",
            "empty",
            "missing",
            "lone",
            "fail:base",
            "fail:clock",
        ],
        16,
    )
}
#[test]
fn positions_native() {
    check(
        "positions",
        &[
            "normal",
            "empty",
            "no-mode",
            "zero",
            "nan",
            "long",
            "no-mark",
            "no-ticker",
            "missing",
            "isolated",
            "fail:base",
            "fail:margin",
        ],
        12,
    )
}
#[test]
fn fills_native() {
    check(
        "fills",
        &[
            "normal",
            "full",
            "zero-limit",
            "filtered",
            "window",
            "order-filter",
            "public",
            "no-symbol",
            "missing",
            "bad-data",
            "fallback",
            "lone",
            "zero-base",
            "fail:get",
            "fail:base",
        ],
        15,
    )
}
