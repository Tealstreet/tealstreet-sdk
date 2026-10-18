// The minified bodies here put `}if` and `}{` straight after a guard `return`,
// which possible_missing_else reads as a dropped `else`. There is none to add.
#![allow(clippy::possible_missing_else)]
use safe_cex_venue_okx::{
    history_stream_api::{self as port, RuntimeOps},
    market_account_reads::{MarketAccountOps, ReadAwait},
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
    original: bool,
    object: V,
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
    Callback(Rc<dyn Fn(Vec<V>) -> Result<V, E>>),
    Date(f64),
    Mac(String, String),
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
        Native::S(_) if k == "toUpperCase" => Rc::new(Native::Member(v.clone(), k.into())),
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
        Native::Call(_) | Native::Callback(_) | Native::Member(_, _) => json!({"fn":true}),
        Native::Map(rows) => json!(
            rows.borrow()
                .iter()
                .map(|(k, v)| json!([pack(k), pack(v)]))
                .collect::<Vec<_>>()
        ),

        _ => panic!("pack"),
    }
}
fn from_json(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Native::Null),
        Value::Bool(x) => b(*x),
        Value::Number(x) => n(x.as_f64().unwrap()),
        Value::String(x) => s(x),
        Value::Array(x) => array(x.iter().map(from_json).collect()),
        Value::Object(x) => Rc::new(Native::O(RefCell::new(
            x.iter().map(|(k, v)| (k.clone(), from_json(v))).collect(),
        ))),
    }
}
fn wire(v: &V) -> Value {
    match v.as_ref() {
        Native::U | Native::Null => Value::Null,
        Native::B(x) => json!(x),
        Native::N(x) => json!(x),
        Native::S(x) => json!(String::from_utf16_lossy(x)),
        Native::O(x) => Value::Object(
            x.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v.as_ref(), Native::U))
                .map(|(k, v)| (k.clone(), wire(v)))
                .collect(),
        ),
        Native::A(x) => Value::Array(x.borrow().iter().map(wire).collect()),
        _ => Value::Null,
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected suspended host"),
    }
}
fn promised(value: Result<V, E>) -> V {
    Rc::new(Native::P(value))
}
fn await_value(v: V) -> ReadAwait<V, E> {
    Box::pin(async move {
        if let Native::P(x) = v.as_ref() {
            x.clone()
        } else {
            Ok(v)
        }
    })
}
struct State {
    input: V,
    receiver: V,
    xhr: V,
    effects: RefCell<Vec<Value>>,
    original: E,
    page: Cell<usize>,
    timers: RefCell<Vec<(usize, V)>>,
    next_timer: Cell<usize>,
}
fn effect(t: &State, name: &str, args: Vec<V>) -> Result<(), E> {
    let mut row = vec![json!(name)];
    row.extend(args.iter().map(pack));
    t.effects.borrow_mut().push(json!(row));
    if text(&get(&t.input, "case")) == format!("fail:{name}") {
        Err(t.original.clone())
    } else {
        Ok(())
    }
}
fn native_error(t: &State, message: V) -> Result<E, E> {
    effect(t, "newError", vec![message.clone()])?;
    Ok(Rc::new(Error {
        name: "Error",
        original: false,
        object: object(vec![("message", message)]),
    }))
}
fn invoke(t: Rc<State>, f: V, args: Vec<V>) -> Result<V, E> {
    match f.as_ref() {
        Native::Callback(cb) => cb(args),
        Native::Call(name) => Ok(match name.as_str() {
            "parseFloat" => n(safe_cex_num::parse_float(&text(&args[0]))),
            "parseInt" => n(safe_cex_num::parse_int(
                &text(&args[0]),
                Some(num(&args[1]) as i32),
            )),
            "Math.min" => n(num(&args[0]).min(num(&args[1]))),
            "Array.isArray" => b(matches!(args[0].as_ref(), Native::A(_))),
            "virtualClock.getCurrentTimeMs" => {
                effect(&t, "clock", vec![])?;
                get(&t.input, "now")
            }
            "virtualClock.getCurrentTime" => {
                effect(&t, "isoClock", vec![])?;
                Rc::new(Native::Date(num(&get(&t.input, "now"))))
            }
            "dayjs" => Rc::new(Native::Date(num(&args[0]))),
            "omitUndefined" => {
                let Native::O(fields) = args[0].as_ref() else {
                    panic!("omit object")
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
            "this.xhr.get" => {
                let result =
                    effect(&t, "get", vec![args[0].clone(), get(&args[1], "params")]).map(|_| {
                        let pages = items(&get(&t.input, "pages"));
                        let i = t.page.get();
                        t.page.set(i + 1);
                        object(vec![(
                            "data",
                            object(vec![("data", pages[i.min(pages.len() - 1)].clone())]),
                        )])
                    });
                promised(result)
            }
            "sleep" => promised(effect(&t, "sleep", args).map(|_| u())),
            "this.emitAndLogError" => {
                effect(&t, "emit", args)?;
                u()
            }
            "this.log" => {
                effect(&t, "log", args)?;
                u()
            }
            "oldestCursorId" => {
                effect(&t, "cursor", vec![args[1].clone()])?;
                let cursor = get(&t.input, "cursor");
                if text(&get(&t.input, "case")) == "cap" {
                    s(&t.page.get().to_string())
                } else if !null(&cursor) {
                    cursor
                } else {
                    items(&args[0])
                        .iter()
                        .map(|r| get(r, &text(&args[1])))
                        .min_by_key(|x| text(x).parse::<u128>().unwrap())
                        .unwrap_or_else(u)
                }
            }
            "mapOkxBillToIncomeRecords" => {
                let symbol = invoke(t.clone(), args[1].clone(), vec![get(&args[0], "instId")])?;
                array(
                    items(&get(&args[0], "records"))
                        .iter()
                        .map(|r| {
                            let out = spread(r);
                            put(&out, "symbol", symbol.clone());
                            out
                        })
                        .collect(),
                )
            }
            "setTimeout" => {
                effect(&t, "timer", vec![args[1].clone()])?;
                let id = t.next_timer.get() + 1;
                t.next_timer.set(id);
                t.timers.borrow_mut().push((id, args[0].clone()));
                n(id as f64)
            }
            "clearTimeout" => {
                effect(&t, "clear", args.clone())?;
                t.timers
                    .borrow_mut()
                    .retain(|(i, _)| *i != num(&args[0]) as usize);
                u()
            }
            "this.listenOrderBook" | "this.listenOHLCV" => {
                effect(&t, "retry", args)?;
                u()
            }
            "this.send" => {
                effect(&t, "send", args)?;
                u()
            }
            "this.orderBookSequenceByHandler.delete" => {
                effect(&t, "seqDelete", args)?;
                u()
            }
            "this.recoveringOrderBooks.delete" => {
                effect(&t, "recoverDelete", args)?;
                u()
            }
            "this.cleanupMessageHandlerStats" => {
                effect(&t, "cleanup", args)?;
                u()
            }
            "multiply" => {
                let a = num(&args[0]);
                let c = num(&args[1]);
                let d = |x: f64| {
                    safe_cex_num::js_number_string(x)
                        .split('.')
                        .nth(1)
                        .map_or(0, |s| s.len())
                };
                let factor = 10f64.powi((d(a) + d(c)) as i32);
                n((a * c * factor + 0.5).floor() / factor)
            }
            "calcOrderBookTotal" => {
                for side in ["bids", "asks"] {
                    let mut total = 0.;
                    for r in items(&get(&args[0], side)) {
                        total += num(&get(&r, "amount"));
                        put(&r, "total", n(total));
                    }
                }
                u()
            }
            "updateOrderBook" => {
                let levels = get(&args[0], &text(&args[1]));
                let Native::A(rows) = levels.as_ref() else {
                    panic!("levels")
                };
                let mut rows = rows.borrow_mut();
                let price = num(&get(&args[2], "price"));
                let amount = num(&get(&args[2], "amount"));
                if let Some(i) = rows.iter().position(|r| num(&get(r, "price")) == price) {
                    if amount == 0. {
                        rows.remove(i);
                    } else {
                        put(&rows[i], "amount", get(&args[2], "amount"));
                    }
                } else if amount != 0. {
                    let descending = text(&args[1]) == "bids";
                    let at = rows
                        .iter()
                        .position(|r| {
                            if descending {
                                num(&get(r, "price")) < price
                            } else {
                                num(&get(r, "price")) > price
                            }
                        })
                        .unwrap_or(rows.len());
                    rows.insert(at, args[2].clone());
                }
                u()
            }
            "JSON.stringify" => {
                if matches!(args[0].as_ref(), Native::U) {
                    u()
                } else {
                    s(&serde_json::to_string(&wire(&args[0])).unwrap())
                }
            }
            "safeCexAxiosCreate" => {
                effect(&t, "factory", vec![args[0].clone()])?;
                t.xhr.clone()
            }
            "networkErrorOnRead" => Rc::new(Native::Call("readCondition".into())),
            "signatureRejectedResponse" => {
                assert!(
                    matches!(args[0].as_ref(), Native::Call(name) if name == "isOkxSignatureRejection")
                );
                Rc::new(Native::Call("signatureCondition".into()))
            }
            // Retry probes carry no response, so the real condition declines them.
            "signatureCondition" => {
                assert!(
                    null(&get(&args[0], "response")),
                    "retry probes carry no response"
                );
                b(false)
            }
            "resendOnSignatureRejection" => {
                assert!(
                    matches!(args[2].as_ref(), Native::Call(name) if name == "isOkxSignatureRejection")
                );
                let data = get(&args[1], "data");
                assert!(
                    !safe_cex_venue_okx::api::is_okx_signature_rejection(&wire(&data)).unwrap(),
                    "no factory fixture is a signature rejection"
                );
                u()
            }
            "retry" => {
                effect(&t, "retryInstall", vec![get(&args[1], "retries")])?;
                put(&t.xhr, "condition", get(&args[1], "retryCondition"));
                u()
            }
            "readCondition" => b(matches!(
                text(&get(&get(&args[0], "config"), "method")).as_str(),
                "get" | "head"
            ) && text(&get(&args[0], "code")) == "ERR_NETWORK"),
            "installTransportAdapter" => {
                effect(&t, "adapter", vec![args[2].clone(), args[3].clone()])?;
                u()
            }
            "isPublicEndpoint" => {
                let url = text(&args[0]);
                b(safe_cex_venue_okx::api::is_public_endpoint(
                    if null(&args[0]) { None } else { Some(&url) },
                ))
            }
            "markSkipRemoteSigning" => {
                effect(&t, "skip", vec![])?;
                put(&args[0], "safeCexSkipRemoteSigning", b(true));
                u()
            }
            "qs.stringify" => {
                let Native::O(fields) = args[0].as_ref() else {
                    panic!("query")
                };
                s(&fields
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k}={}", text(v)))
                    .collect::<Vec<_>>()
                    .join("&"))
            }
            "decodeURIComponent" => args[0].clone(),
            "createHmac" => {
                assert_eq!(text(&args[0]), "sha256");
                Rc::new(Native::Mac(text(&args[1]), String::new()))
            }
            other => panic!("call {other}"),
        }),
        Native::Member(o, key) => Ok(match (o.as_ref(), key.as_str()) {
            (Native::A(rows), "find") => {
                let mut found = u();
                for r in rows.borrow().iter() {
                    if truth(&invoke(t.clone(), args[0].clone(), vec![r.clone()])?) {
                        found = r.clone();
                        break;
                    }
                }
                found
            }
            (Native::A(rows), "sort") => {
                let mut values = rows.borrow().clone();
                let mut error = None;
                values.sort_by(|a, c| {
                    match invoke(t.clone(), args[0].clone(), vec![a.clone(), c.clone()]) {
                        Ok(x) => num(&x)
                            .partial_cmp(&0.)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        Err(e) => {
                            error = Some(e);
                            std::cmp::Ordering::Equal
                        }
                    }
                });
                if let Some(e) = error {
                    return Err(e);
                }
                *rows.borrow_mut() = values;
                o.clone()
            }
            (Native::A(rows), "join") => s(&rows
                .borrow()
                .iter()
                .map(text)
                .collect::<Vec<_>>()
                .join(&text(&args[0]))),
            (Native::Map(rows), "set") => {
                let mut rows = rows.borrow_mut();
                if let Some((_, v)) = rows.iter_mut().find(|(k, _)| equal(k, &args[0])) {
                    *v = args[1].clone();
                } else {
                    rows.push((args[0].clone(), args[1].clone()));
                }
                o.clone()
            }
            (Native::Map(rows), "values") => {
                array(rows.borrow().iter().map(|(_, v)| v.clone()).collect())
            }
            (Native::Date(ms), "subtract") => {
                let unit = text(&args[1]);
                let scale = match unit.as_str() {
                    "m" => 60_000.,
                    "h" => 3_600_000.,
                    "d" => 86_400_000.,
                    "w" => 604_800_000.,
                    _ => panic!("date unit {unit}"),
                };
                Rc::new(Native::Date(ms - num(&args[0]) * scale))
            }
            (Native::Date(ms), "valueOf") => n(*ms),
            (Native::Date(ms), "isBefore") => {
                let Native::Date(other) = args[0].as_ref() else {
                    panic!("date")
                };
                b(ms < other)
            }
            (Native::Date(ms), "toISOString") => {
                assert_eq!(*ms, 1_700_000_000_000.);
                s("2023-11-14T22:13:20.000Z")
            }
            (Native::S(_), "includes") => b(text(o).contains(&text(&args[0]))),
            (Native::S(_), "trim") => s(safe_cex_num::js_trim(&text(o))),
            (Native::S(_), "toUpperCase") => s(&text(o).to_uppercase()),
            (Native::Mac(key, _), "update") => Rc::new(Native::Mac(key.clone(), text(&args[0]))),
            (Native::Mac(key, message), "digest") => {
                assert_eq!(text(&args[0]), "base64");
                s(&safe_cex_crypto::hmac_sha256(
                    key.as_bytes(),
                    message.as_bytes(),
                    safe_cex_crypto::Encoding::Base64,
                ))
            }
            (Native::O(_), "use") => {
                let name = if Rc::ptr_eq(o, &get(&get(&t.xhr, "interceptors"), "response")) {
                    "response"
                } else {
                    "request"
                };
                effect(&t, &format!("{name}Use"), vec![])?;
                put(&t.xhr, name, args[0].clone());
                u()
            }
            _ => panic!("member {key}"),
        }),
        _ => panic!("callee"),
    }
}
fn spread(v: &V) -> V {
    if let Native::O(fields) = v.as_ref() {
        Rc::new(Native::O(RefCell::new(fields.borrow().clone())))
    } else {
        object(vec![])
    }
}
fn concat(args: &[V]) -> V {
    let mut out = vec![];
    for v in args {
        if let Native::S(x) = v.as_ref() {
            out.extend(x)
        } else {
            out.extend(text(v).encode_utf16())
        }
    }
    Rc::new(Native::S(out))
}
fn host(t: Rc<State>) -> Rc<RuntimeOps<V, E>> {
    let cap = t.clone();
    let inv = t.clone();
    let intr = t.clone();
    let errors = t.clone();
    let value = MarketAccountOps {
        get: Rc::new(|v, k| Ok(get(v, k))),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |key| {
            Ok(match key {
                x if x.starts_with("this.") && !x.contains("(") => {
                    let mut v = cap.receiver.clone();
                    for key in x[5..].split('.') {
                        v = get(&v, key);
                    }
                    if null(&v) {
                        Rc::new(Native::Call(x.into()))
                    } else {
                        v
                    }
                }
                "ENDPOINTS.BILLS" => s("/api/v5/account/bills"),
                "ENDPOINTS.BILLS_ARCHIVE" => s("/api/v5/account/bills-archive"),
                "ENDPOINTS.KLINE" => s("/api/v5/market/candles"),
                "ENDPOINTS.KLINE_HISTORY" => s("/api/v5/market/history-candles"),
                "INTERVAL" => object(vec![("1m", s("1m")), ("1h", s("1H")), ("1w", s("1W"))]),
                "BASE_URLS" => object(vec![
                    (
                        "okx",
                        object(vec![
                            ("livenet", s("https://www.okx.com")),
                            ("testnet", s("https://openapi.okx.com")),
                        ]),
                    ),
                    (
                        "okxeu",
                        object(vec![
                            ("livenet", s("https://eea.okx.com")),
                            ("testnet", s("https://eea.okx.com")),
                        ]),
                    ),
                ]),
                "RECV_WINDOW" => n(20000.),
                "OKX_BILL_TYPE_BY_INCOME_TYPE" => {
                    object(vec![("COMMISSION", s("2")), ("FUNDING_FEE", s("8"))])
                }
                "LogSeverity.Warning" => s("warning"),
                _ => Rc::new(Native::Call(key.into())),
            })
        }),
        member: Rc::new(|v, k| Ok(Rc::new(Native::Member(v, k.into())))),
        invoke: Rc::new(move |f, a| invoke(inv.clone(), f, a)),
        intrinsic: Rc::new(move |key, a| {
            Ok(match key {
                "get-index" => get(&a[0], &text(&a[1])),
                "destructure-two" => array(items(&a[0]).into_iter().take(2).collect()),
                "set-index" => {
                    put(&a[0], &text(&a[1]), a[2].clone());
                    u()
                }
                "delete-index" => {
                    let Native::O(fields) = a[0].as_ref() else {
                        panic!("delete object")
                    };
                    fields.borrow_mut().retain(|(k, _)| k != &text(&a[1]));
                    u()
                }
                "null" => Rc::new(Native::Null),
                "add" => n(num(&a[0]) + num(&a[1])),
                "subtract" => n(num(&a[0]) - num(&a[1])),
                "multiply" => n(num(&a[0]) * num(&a[1])),
                "divide" => n(num(&a[0]) / num(&a[1])),
                "greater-than" => b(num(&a[0]) > num(&a[1])),
                "greater-or-equal" => b(num(&a[0]) >= num(&a[1])),
                "less-than" => b(num(&a[0]) < num(&a[1])),
                "less-or-equal" => b(num(&a[0]) <= num(&a[1])),
                "template-concat" => concat(&a),
                "candle-handler" => concat(&[s("candle."), a[0].clone(), s("."), a[1].clone()]),
                "market-error-message" => {
                    concat(&[s("Market "), a[0].clone(), s(" not found on OKX")])
                }
                "is-array" => b(matches!(a[0].as_ref(), Native::A(_))),
                "string" => s(&text(&a[0])),
                "bills-cap-message" => s(&format!(
                    "OKX bills paging hit the 200-page cap after {} rows; older rows in this window were not fetched",
                    text(&a[0])
                )),
                "typeof-string" => b(matches!(a[0].as_ref(), Native::S(_))),
                "signing-url" => {
                    if truth(&a[1]) {
                        concat(&[a[0].clone(), s("?"), a[1].clone()])
                    } else {
                        concat(&[a[0].clone()])
                    }
                }
                "split-interval-digits" => {
                    let value = text(&a[0]);
                    let start = value.find(|c: char| c.is_ascii_digit()).unwrap();
                    let end = value[start..]
                        .find(|c: char| !c.is_ascii_digit())
                        .map_or(value.len(), |i| start + i);
                    array(vec![
                        s(&value[..start]),
                        s(&value[start..end]),
                        s(&value[end..]),
                    ])
                }
                "array-spread" => array(a.iter().flat_map(items).collect()),
                "iterable-spread" => array(items(&a[0])),
                "object-spread" => spread(&a[0]),
                "define-literal-field" => {
                    put(&a[0], &text(&a[1]), a[2].clone());
                    u()
                }
                "construct" => match text_call(&a[0]).as_str() {
                    "Map" => Rc::new(Native::Map(RefCell::new(vec![]))),
                    "AxiosHeaders" => {
                        effect(&intr, "headers", vec![a[1].clone()])?;
                        let h = spread(&a[1]);
                        let Native::O(fields) = h.as_ref() else {
                            panic!("headers")
                        };
                        for (_, v) in fields.borrow_mut().iter_mut() {
                            *v = s(&text(v));
                        }
                        h
                    }
                    x => panic!("construct {x}"),
                },
                x => panic!("intrinsic {x}"),
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
            if !matches!(v.as_ref(), Native::A(_)) {
                return Err(Rc::new(Error {
                    name: "TypeError",
                    original: false,
                    object: object(vec![("message", s("data.map is not a function"))]),
                }));
            }
            Ok(array(
                items(&v)
                    .into_iter()
                    .map(|x| f(x))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }),
        filter: Rc::new(|v, f| {
            let mut out = vec![];
            for x in items(&v) {
                if f(x.clone())? {
                    out.push(x);
                }
            }
            Ok(array(out))
        }),
        reduce: Rc::new(|v, mut acc, f| {
            for x in items(&v) {
                acc = f(acc, x)?;
            }
            Ok(acc)
        }),
        for_of: Rc::new(|v, f| {
            for x in items(&v) {
                f(x)?;
            }
            Ok(())
        }),
        await_value: Rc::new(await_value),
        error_value: Rc::new(|e| e.object.clone()),
        new_error: Rc::new(move |_, m| native_error(&errors, m)),
        bind_first: Rc::new(|_, _| panic!("unused bind")),
        flat_map: Rc::new(|_, _| panic!("unused flatMap")),
        sum_by: Rc::new(|_, _, _| panic!("unused sumBy")),
        parallel_map: Rc::new(|_, _, _| panic!("unused parallel")),
        some: Rc::new(|_, _| panic!("unused some")),
    };
    Rc::new(RuntimeOps {
        value: Rc::new(value),
        callback: Rc::new(|f| Rc::new(Native::Callback(f))),
    })
}
fn text_call(v: &V) -> String {
    let Native::Call(x) = v.as_ref() else {
        panic!("constructor value")
    };
    x.clone()
}
fn fixture(f: &Value) -> Value {
    let input = from_json(f);
    let case = f["case"].as_str().unwrap();
    let method_name = f["method"].as_str().unwrap();
    let market = object(vec![
        ("id", s("I")),
        ("symbol", s("SYM")),
        ("precision", object(vec![("amount", n(0.1))])),
    ]);
    let key = if method_name == "book" {
        "books.I"
    } else {
        "candle.1m.I"
    };
    let existing = Rc::new(Native::Callback(Rc::new(|_| Ok(u()))));
    let handlers = if f["shared"] == true {
        object(vec![(key, array(vec![existing.clone()]))])
    } else {
        object(vec![])
    };
    let xhr = object(vec![(
        "interceptors",
        object(vec![
            ("response", object(vec![])),
            ("request", object(vec![])),
        ]),
    )]);
    let receiver = object(vec![
        ("isPublicDataOnly", b(f["public"] == true)),
        ("isConnected", b(f["wait"] != true)),
        ("isDisposed", b(f["disposed"] == true)),
        ("topics", object(vec![])),
        ("messageHandlers", handlers.clone()),
        ("xhr", xhr.clone()),
        (
            "store",
            object(vec![
                (
                    "loaded",
                    object(vec![("markets", b(f["unloaded"] != true))]),
                ),
                (
                    "markets",
                    array(if f["missing"] == true {
                        vec![]
                    } else {
                        vec![market.clone()]
                    }),
                ),
                (
                    "marketsBySymbol",
                    if f["missing"] == true {
                        object(vec![])
                    } else {
                        object(vec![("SYM", market.clone())])
                    },
                ),
                ("marketsById", object(vec![("I", market)])),
                (
                    "tickersBySymbol",
                    if f["noTicker"] == true {
                        object(vec![])
                    } else {
                        object(vec![(
                            "SYM",
                            object(vec![("last", n(f["last"].as_f64().unwrap_or(10.)))]),
                        )])
                    },
                ),
            ]),
        ),
    ]);
    let original = Rc::new(Error {
        name: "TypeError",
        original: true,
        object: object(vec![("message", s("DOWN")), ("code", s("IO"))]),
    });
    let t = Rc::new(State {
        input: input.clone(),
        receiver: receiver.clone(),
        xhr: xhr.clone(),
        effects: RefCell::new(vec![]),
        original,
        page: Cell::new(0),
        timers: RefCell::new(vec![]),
        next_timer: Cell::new(0),
    });
    let h = host(t.clone());
    let loaded = (|| -> Result<V, E> {
        match method_name {
            "income" => {
                let result = run(port::fetch_income_history(h.clone(), get(&input, "opts")))?;
                if case == "baseline" {
                    let rows = items(&result);
                    assert_eq!(rows.len(), 3, "INCOME native bill decomposition count");
                    assert_eq!(
                        text(&get(&rows[0], "tranId")),
                        "b",
                        "INCOME native newest record first"
                    );
                    assert_eq!(
                        num(&get(&rows[1], "income")),
                        9.,
                        "INCOME native later duplicate replaces amount"
                    );
                }
                if case == "pages" {
                    assert_eq!(
                        t.page.get(),
                        2,
                        "INCOME native bills limit counts input rows"
                    );
                    assert!(
                        t.effects.borrow().iter().any(|e| e[0] == "cursor"),
                        "INCOME native paging cursor effect"
                    );
                }
                if case == "cap" {
                    assert_eq!(t.page.get(), 200, "INCOME native cap request count");
                    assert!(
                        t.effects.borrow().iter().any(|e| e[0] == "log"),
                        "INCOME native visible cap warning"
                    );
                }
                Ok(result)
            }
            "candles" => {
                let result = run(port::fetch_ohlcv(h.clone(), get(&input, "opts")))?;
                if case == "baseline" {
                    let rows = items(&result);
                    assert_eq!(
                        num(&get(&rows[0], "timestamp")),
                        1.,
                        "CANDLES native ascending timestamps"
                    );
                    assert_eq!(
                        num(&get(&rows[0], "volume")),
                        7.5,
                        "CANDLES native base-volume column seven"
                    );
                    assert_eq!(t.page.get(), 1, "CANDLES native recent request count");
                }
                if case == "old" || case == "fallback" {
                    assert_eq!(
                        t.page.get(),
                        2,
                        "CANDLES native historical endpoint selected"
                    );
                }
                Ok(result)
            }
            "book" | "candle-sub" => {
                let observations = Rc::new(RefCell::new(vec![]));
                let refs = Rc::new(RefCell::new(Vec::<V>::new()));
                let obs = observations.clone();
                let rs = refs.clone();
                let cb = Rc::new(Native::Callback(Rc::new(move |a| {
                    obs.borrow_mut().push(pack(&a[0]));
                    rs.borrow_mut().push(a[0].clone());
                    Ok(u())
                })));
                let cleanup = if method_name == "book" {
                    port::listen_order_book(h.clone(), s("SYM"), cb)?
                } else {
                    port::listen_ohlcv(
                        h.clone(),
                        object(vec![("symbol", s("SYM")), ("interval", s("1m"))]),
                        cb,
                    )?
                };
                if f["cancelEarly"] == true {
                    invoke(t.clone(), cleanup.clone(), vec![])?;
                }
                if f["fireTimer"] == true && !t.timers.borrow().is_empty() {
                    put(&receiver, "isConnected", b(true));
                    let (_, timer) = t.timers.borrow_mut().remove(0);
                    invoke(t.clone(), timer, vec![])?;
                }
                if f["cancelEarly"] != true {
                    for frame in f["frames"].as_array().unwrap_or(&vec![]) {
                        let hs = get(&handlers, key);
                        if !null(&hs) {
                            if let Some(p) = items(&hs).last() {
                                if !Rc::ptr_eq(p, &existing) {
                                    invoke(t.clone(), p.clone(), vec![from_json(frame)])?;
                                }
                            }
                        }
                    }
                }
                let same =
                    refs.borrow().len() > 1 && Rc::ptr_eq(&refs.borrow()[0], &refs.borrow()[1]);
                if case == "baseline" || case == "wait-connected" {
                    let observations = observations.borrow();
                    if method_name == "book" {
                        let ob = refs.borrow()[0].clone();
                        let bids = items(&get(&ob, "bids"));
                        assert_eq!(
                            num(&get(&bids[0], "amount")),
                            0.3,
                            "BOOK native contract multiplier"
                        );
                        assert_eq!(
                            num(&get(&bids[0], "total")),
                            0.3,
                            "BOOK native cumulative total"
                        );
                        assert!(same, "BOOK native callback book identity across frames");
                    } else {
                        assert_eq!(observations.len(), 2, "SUBCANDLE native callback count");
                        assert_eq!(
                            num(&get(&refs.borrow()[0], "volume")),
                            7.5,
                            "SUBCANDLE native volume column seven"
                        );
                        assert_eq!(
                            num(&get(&refs.borrow()[0], "timestamp")),
                            2.,
                            "SUBCANDLE native milliseconds to seconds"
                        );
                    }
                }
                if f["cancelEarly"] != true {
                    invoke(t.clone(), cleanup.clone(), vec![])?;
                }
                if f["twice"] == true {
                    invoke(t.clone(), cleanup, vec![])?;
                }
                if case == "shared" {
                    assert_eq!(
                        items(&get(&handlers, key)).len(),
                        1,
                        "STREAM native cleanup preserves other subscriber"
                    );
                    assert!(
                        !t.effects.borrow().iter().any(|e| e[0] == "cleanup"),
                        "STREAM native shared stats retained"
                    );
                }
                let refs = refs.borrow();
                let cleared = if method_name == "book" && !refs.is_empty() {
                    b(items(&get(&refs[0], "bids")).is_empty()
                        && items(&get(&refs[0], "asks")).is_empty())
                } else {
                    Rc::new(Native::Null)
                };
                let hs = get(&handlers, key);
                Ok(object(vec![
                    (
                        "observations",
                        array(observations.borrow().iter().map(unpack).collect()),
                    ),
                    (
                        "shared",
                        b(!null(&hs) && items(&hs).iter().any(|v| Rc::ptr_eq(v, &existing))),
                    ),
                    (
                        "handlers",
                        n(if null(&hs) {
                            0.
                        } else {
                            items(&hs).len() as f64
                        }),
                    ),
                    ("topics", get(&receiver, "topics")),
                    ("timers", n(t.timers.borrow().len() as f64)),
                    ("same", b(same)),
                    ("cleared", cleared),
                ]))
            }
            "factory" => {
                let opts = get(&input, "opts");
                let exchange = get(&input, "exchange");
                let out = port::create_api(h, opts.clone(), exchange.clone())?;
                assert!(
                    Rc::ptr_eq(&out, &xhr),
                    "FACTORY native exact client identity"
                );
                let config = get(&input, "request");
                let mut next = u();
                let mut request_same = false;
                if !null(&config) {
                    next = invoke(t.clone(), get(&xhr, "request"), vec![config.clone()])?;
                    request_same = Rc::ptr_eq(&next, &config);
                }
                if case == "baseline" {
                    let headers = get(&next, "headers");
                    assert_eq!(
                        text(&get(&headers, "OK-ACCESS-KEY")),
                        "key",
                        "FACTORY native signed API key"
                    );
                    assert_eq!(
                        text(&get(&headers, "OK-ACCESS-SIGN")),
                        "KfA/A2JX5Jb4nzChMUyVq2L4pz0MlhW1U5kdwREV+qI=",
                        "FACTORY native HMAC signature"
                    );
                    assert!(!request_same, "FACTORY native signed config clone");
                }
                if case == "public" {
                    assert!(request_same, "FACTORY native public request alias");
                }
                if case == "market" {
                    assert!(
                        truth(&get(&next, "safeCexSkipRemoteSigning")),
                        "FACTORY native market remote skip tag"
                    );
                    assert!(
                        truth(&get(&get(&next, "headers"), "OK-ACCESS-SIGN")),
                        "FACTORY native market still signed locally"
                    );
                }
                let response = if null(&get(&input, "response")) {
                    object(vec![("data", object(vec![("code", s("0"))]))])
                } else {
                    get(&input, "response")
                };
                let original = response.clone();
                let response = invoke(t.clone(), get(&xhr, "response"), vec![response])?;
                let retry = |method: &str| {
                    invoke(
                        t.clone(),
                        get(&xhr, "condition"),
                        vec![object(vec![
                            ("code", s("ERR_NETWORK")),
                            ("config", object(vec![("method", s(method))])),
                        ])],
                    )
                };
                Ok(object(vec![
                    ("config", next),
                    ("response", response.clone()),
                    (
                        "retry",
                        array(vec![retry("get")?, retry("head")?, retry("post")?]),
                    ),
                    (
                        "identity",
                        object(vec![
                            ("options", b(true)),
                            ("exchange", b(true)),
                            ("xhr", b(true)),
                            ("request", b(request_same)),
                            ("response", b(Rc::ptr_eq(&original, &response))),
                        ]),
                    ),
                ]))
            }
            _ => panic!("method"),
        }
    })();
    match loaded {
        Ok(result) => json!({"effects":*t.effects.borrow(),"failed":false,"result":pack(&result)}),
        Err(e) => {
            json!({"effects":*t.effects.borrow(),"failed":true,"result":{"u":true},"error":{"original":e.original,"name":e.name,"message":pack(&get(&e.object,"message"))}})
        }
    }
}
fn unpack(v: &Value) -> V {
    if v.get("u") == Some(&json!(true)) {
        return u();
    }
    if let Some(units) = v.get("s") {
        return Rc::new(Native::S(
            units
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u16)
                .collect(),
        ));
    }
    if let Some(bits) = v.get("n").and_then(Value::as_str) {
        return n(if bits == "nan" {
            f64::NAN
        } else {
            f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
        });
    }
    if let Some(fields) = v.get("o") {
        return Rc::new(Native::O(RefCell::new(
            fields
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (p[0].as_str().unwrap().into(), unpack(&p[1])))
                .collect(),
        )));
    }
    if let Some(a) = v.as_array() {
        return array(a.iter().map(unpack).collect());
    }
    from_json(v)
}
fn income_bill(id: &str, records: Value) -> Value {
    json!({"billId":id,"instId":"I","records":records})
}
fn inputs(method: &str, cases: &[&str]) -> Vec<Value> {
    cases.iter().map(|case|{
        let mut f=json!({"method":method,"case":case,"now":1700000000000f64,"pages":[[]]});
        if method=="income" {
            f["opts"]=json!({"symbol":"SYM","limit":150});
            f["pages"]=json!([[income_bill("1",json!([{"tranId":"a","timestamp":1,"income":1},{"tranId":"b","timestamp":3,"income":2},{"tranId":"a","timestamp":1,"income":9},{"tranId":"c","timestamp":0,"income":3}]))]]);
            if *case=="public"{f["public"]=json!(true);}if *case=="missing"{f["missing"]=json!(true);}if *case=="unscoped"{f.as_object_mut().unwrap().remove("opts");}
            if *case=="archive"{f["opts"]["startTime"]=json!(1690000000000f64);f["opts"]["endTime"]=json!(1700000000000f64);f["opts"]["incomeType"]=json!("FUNDING_FEE");}
            if *case=="recent-zero"{f["opts"]["startTime"]=json!(0);f["opts"]["limit"]=json!(0);}
            if *case=="bad-data"{f["pages"]=json!([{}]);}if *case=="empty"{f["pages"]=json!([[]]);}
            if matches!(*case,"pages"|"cap"|"stalled"|"no-cursor"|"limit-one") {
                let rows=(1..=100).rev().map(|i|income_bill(&i.to_string(),json!([{"tranId":i.to_string(),"timestamp":i,"income":i}]))).collect::<Vec<_>>();
                f["pages"]=json!([rows,rows]);
                if *case=="cap"{f["opts"]["limit"]=json!(30000);f["cursor"]=json!("next");}
                if *case=="stalled"{f["opts"]["limit"]=json!(30000);f["cursor"]=json!("1");}
                if *case=="no-cursor"{f["cursor"]=json!("");}
                if *case=="limit-one"{f["opts"]["limit"]=json!(1);}
            }
        }
        if method=="candles" {
            f["opts"]=json!({"symbol":"SYM","interval":"1m","limit":2});
            f["pages"]=json!([[ ["2000","1","2",".5","1.5","99","88","8.5"], ["1000","2","3","1","2.5","99","88","7.5"] ]]);
            if *case=="old"{f["opts"]["to"]=json!(1690000000000f64);f["opts"]["limit"]=json!(300);}
            if *case=="fallback"{f["pages"]=json!([[],[["1000","1","2","0","1","99","88","7.5"]]]);}
            if *case=="from"{f["opts"].as_object_mut().unwrap().remove("limit");f["opts"]["from"]=json!(1699999990000f64);}
            if *case=="clamp"{f["opts"]["limit"]=json!(900);}
            if *case=="zero"{f["opts"]["limit"]=json!(0);}
            if *case=="week"{f["opts"]["interval"]=json!("1w");}
            if *case=="missing"{f["missing"]=json!(true);}
            if *case=="malformed"{f["pages"]=json!([{}]);}
            if *case=="numbers"{f["pages"]=json!([[ ["-0","-0","1e309","bad","1.25suffix","99","88",".00000001"] ]]);}
        }
        if method=="book"||method=="candle-sub" {
            if method=="book"{f["frames"]=json!([
                {"action":"snapshot","data":[{"bids":[["10","2"],["9","1"],["11","6"],["8","0"]],"asks":[["10","4"],["11","5"],["9","7"]]}]},
                {"action":"update","data":[{"bids":[["10","3"],["9","0"]],"asks":[["11","6"],["12","2"]]}]},
            ]);}else{f["frames"]=json!([{"data":[["2000","1","2",".5","1.5","99","88","7.5"]]},{"data":[["1000","2","3","1","2.5","99","88","8.5"]]}]);}
            if *case=="unloaded-fire"{f["unloaded"]=json!(true);f["fireTimer"]=json!(true);}
            if *case=="unloaded-cancel"{f["unloaded"]=json!(true);f["cancelEarly"]=json!(true);f["fireTimer"]=json!(true);}
            if *case=="wait-connected"{f["wait"]=json!(true);f["fireTimer"]=json!(true);}
            if *case=="wait-cancel"{f["wait"]=json!(true);f["cancelEarly"]=json!(true);}
            for(key,c)in [("missing","missing"),("disposed","disposed"),("shared","shared"),("twice","twice"),("noTicker","no-ticker")]{if *case==c{f[key]=json!(true);}}
            if *case=="empty-frame"{f["frames"]=if method=="book"{json!([{"action":"other"}])}else{json!([{}, {"data":[]}, {"data":null}])};}
            if *case=="zero-last"{f["last"]=json!(0);}
        }
        if method=="factory" {
            f["opts"]=json!({"key":"key","secret":"secret","passphrase":"pass"});f["exchange"]=json!({"name":"okx"});f["request"]=json!({"url":"/api/v5/trade/order","method":"post","params":{"a":"x y"},"data":{"sz":"1"},"headers":{"X":"yes"}});
            if *case=="public"{f["request"]["url"]=json!("/api/v5/public/time");}
            if *case=="market"{f["request"]["url"]=json!("/api/v5/market/tickers");}
            if *case=="remote"{f["opts"]["remoteSigner"]=json!({});}
            if *case=="no-key"{f["opts"].as_object_mut().unwrap().remove("key");}
            if *case=="no-pass"{f["opts"].as_object_mut().unwrap().remove("passphrase");}
            if *case=="public-only"{f["opts"]=json!({"publicDataOnly":true});}
            if *case=="testnet"{f["opts"]["testnet"]=json!(true);f["opts"]["extra"]=json!({"recvWindow":0});}
            if *case=="eu"{f["exchange"]["region"]=json!("okxeu");}
            if *case=="tunnel"{f["opts"]["tealstreetTunnel"]=json!(true);}
            if *case=="string-body"{f["request"]["data"]=json!("raw");}
            if *case=="null-body"{f["request"]["data"]=Value::Null;}
            if *case=="no-body"{f["request"].as_object_mut().unwrap().remove("data");}
            if *case=="no-method"{f["request"].as_object_mut().unwrap().remove("method");}
            if *case=="empty-query"{f["request"]["params"]=json!({});}
            if *case=="code-number"{f["response"]=json!({"data":{"code":1}});}
            if *case=="code-message"{f["response"]=json!({"data":{"code":"7","msg":"bad"}});}
            if *case=="row-error"{f["response"]=json!({"data":{"code":"1","msg":"All operations failed","data":[{"sCode":"0","sMsg":""},{"sCode":"51000","sMsg":" Parameter error "}]}});}
            if *case=="row-message"{f["response"]=json!({"data":{"code":"1","msg":"","data":[{"sCode":"0","sMsg":"partial"}]}});}
            if *case=="row-trimmed-msg"{f["response"]=json!({"data":{"code":"2","msg":"  spaced  ","data":[null,{"sCode":"0"}]}});}
            if *case=="row-code-only"{f["response"]=json!({"data":{"code":"3","msg":"","data":[{"sCode":51001}]}});}
            if *case=="code-zero"{f["response"]=json!({"data":{"code":0}});}
            if *case=="null-response"{f["response"]=json!({"data":null});}
        }
        f
    }).collect()
}
fn check(method: &str, cases: &[&str]) {
    let fixtures = inputs(method, cases);
    let native = fixtures.iter().map(fixture).collect::<Vec<_>>();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/history-stream-api-oracle.mjs"
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
        .write_all(serde_json::to_string(&fixtures).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source {method}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source.len(), cases.len());
    for ((actual, expected), case) in native.iter().zip(&source).zip(cases) {
        assert_eq!(actual, expected, "source comparison {method}:{case}");
    }
    println!("{method}: {} maintained-source fixtures", cases.len());
}
#[test]
fn income_native() {
    check(
        "income",
        &[
            "baseline",
            "public",
            "missing",
            "unscoped",
            "archive",
            "recent-zero",
            "bad-data",
            "empty",
            "pages",
            "stalled",
            "cap",
            "no-cursor",
            "limit-one",
            "fail:get",
            "fail:cursor",
            "fail:sleep",
            "fail:emit",
        ],
    );
}
#[test]
fn candles_native() {
    check(
        "candles",
        &[
            "baseline",
            "old",
            "fallback",
            "from",
            "clamp",
            "zero",
            "week",
            "missing",
            "malformed",
            "numbers",
            "fail:get",
            "fail:emit",
        ],
    );
}
#[test]
fn candle_subscription_native() {
    check(
        "candle-sub",
        &[
            "baseline",
            "unloaded-fire",
            "unloaded-cancel",
            "wait-connected",
            "wait-cancel",
            "missing",
            "disposed",
            "shared",
            "twice",
            "empty-frame",
            "fail:send",
            "fail:cleanup",
        ],
    );
}
#[test]
fn book_subscription_native() {
    check(
        "book",
        &[
            "baseline",
            "unloaded-fire",
            "unloaded-cancel",
            "wait-connected",
            "wait-cancel",
            "missing",
            "disposed",
            "shared",
            "twice",
            "empty-frame",
            "no-ticker",
            "zero-last",
            "fail:send",
            "fail:cleanup",
        ],
    );
}
#[test]
fn factory_native() {
    check(
        "factory",
        &[
            "baseline",
            "public",
            "market",
            "remote",
            "no-key",
            "no-pass",
            "public-only",
            "testnet",
            "eu",
            "tunnel",
            "string-body",
            "null-body",
            "no-body",
            "no-method",
            "empty-query",
            "code-number",
            "code-message",
            "row-error",
            "row-message",
            "row-trimmed-msg",
            "row-code-only",
            "code-zero",
            "null-response",
            "fail:factory",
            "fail:responseUse",
            "fail:requestUse",
            "fail:adapter",
            "fail:newError",
        ],
    );
}

#[test]
fn factory_response_phase_native() {
    use safe_cex_transport::{RestResponseBody, how::big_int_as_string_json_parse};
    let texts = [
        "\"123\"",
        "\"true\"",
        "\"null\"",
        "\"hello\"",
        "{\"x\":1}",
        "[1,2]",
        "null",
    ];
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let parse = |v: Input| match v {
        Input::Text(ref text) => Input::parse_json(text).unwrap_or(v),
        _ => v,
    };
    let mut native = vec![];
    for text in texts {
        for tunnel in [false, true] {
            let body = if tunnel {
                RestResponseBody::Transformed(big_int_as_string_json_parse(text))
            } else {
                RestResponseBody::Encoded(text.as_bytes().to_vec())
            };
            let data = body.transform(parse).into_data();
            if text == "\"123\"" {
                if tunnel {
                    assert_eq!(
                        match &data {
                            Input::Float(value) => value.to_bits(),
                            _ => panic!("RESPONSE native HOW numeric result"),
                        },
                        123f64.to_bits(),
                        "RESPONSE native HOW second parse numeric bits"
                    );
                } else {
                    assert!(
                        matches!(data, Input::Text(_)),
                        "RESPONSE native direct quoted scalar remains string"
                    );
                }
            }
            native.push(data.into_value().unwrap());
        }
    }
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/response-phase-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source.len(), 14);
    for (i, (data, row)) in native.iter().zip(&source).enumerate() {
        assert!(row["error"].is_null(), "response source case {i}");
        let kind = match data {
            Value::Null | Value::Object(_) => "object",
            Value::Array(_) => "object",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
        };
        assert_eq!(
            kind,
            row["result"]["dataType"].as_str().unwrap(),
            "response source type {i}"
        );
        if let Some(number) = data.as_f64() {
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                row["result"]["data"]["numberBits"].as_str().unwrap(),
                "response source number bits {i}"
            );
        } else {
            assert_eq!(
                phase_tag(data),
                row["result"]["data"],
                "response source data {i}"
            );
        }
    }
}

fn phase_tag(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!({"numberBits":format!("{:016x}",n.as_f64().unwrap().to_bits())}),
        Value::Array(a) => json!(a.iter().map(phase_tag).collect::<Vec<_>>()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), phase_tag(v))).collect())
        }
        _ => v.clone(),
    }
}
