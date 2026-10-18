use safe_cex_venue_weex::public_feed::{self as feed, WeexPublicCallback, WeexPublicFeedRuntime};
use serde_json::Value as Json;
use std::{cell::RefCell, path::Path, process::Command, rc::Rc};
#[derive(Debug, Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(Rc<RefCell<Vec<(String, V)>>>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Set(Rc<RefCell<Vec<V>>>),
    Function(String),
    Callback(usize),
    Iterator(Rc<RefCell<(V, usize)>>),
    Symbol,
    Fault(Vec<u16>),
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.iter().find(|(x, _)| x == k).is_some_and(|(_, w)| v == w))
            }
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            (Self::Set(a), Self::Set(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
struct Error {
    name: &'static str,
    units: Vec<u16>,
}
type H = RefCell<State>;
type RT = WeexPublicFeedRuntime<H, V, Error>;
struct State {
    input: Json,
    parsed: Vec<V>,
    instance: V,
    json: V,
    disposed: bool,
    events: Vec<String>,
    callbacks: Vec<WeexPublicCallback<V>>,
    handlers: Vec<(V, V)>,
    jobs: Vec<(WeexPublicCallback<V>, WeexPublicCallback<V>)>,
    values: Vec<(String, Vec<V>)>,
    primary: V,
    symbol_reads: usize,
}
fn text(s: &str) -> V {
    V::String(s.encode_utf16().collect())
}
fn object(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v.into_iter().map(Some).collect())))
}
fn from_json(j: &Json) -> V {
    match j {
        Json::Null => V::Null,
        Json::Bool(b) => V::Bool(*b),
        Json::Number(n) => V::Number(n.as_f64().unwrap()),
        Json::String(s) => text(s),
        Json::Array(a) => array(a.iter().map(from_json).collect()),
        Json::Object(o) => V::Object(Rc::new(RefCell::new(
            o.iter().map(|(k, v)| (k.clone(), from_json(v))).collect(),
        ))),
    }
}
fn decode(j: &Json) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::Undefined,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"] == true),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => V::String(serde_json::from_value(j["units"].clone()).unwrap()),
        "function" => V::Function(j["name"].as_str().unwrap().into()),
        "object" => V::Object(Rc::new(RefCell::new(
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                .collect(),
        ))),
        "array" => V::Array(Rc::new(RefCell::new(
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    if v["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(v))
                    }
                })
                .collect(),
        ))),
        "set" => V::Set(Rc::new(RefCell::new(
            j["items"].as_array().unwrap().iter().map(decode).collect(),
        ))),
        s => panic!("decode {s}"),
    }
}
fn error(name: &'static str, s: &str) -> Error {
    Error {
        name,
        units: s.encode_utf16().collect(),
    }
}
fn record(h: &H, s: &str) -> Result<(), Error> {
    h.borrow_mut().events.push(s.into());
    if h.borrow().input["fail"] == s {
        Err(error("RangeError", &format!("fail:{s}")))
    } else {
        Ok(())
    }
}
fn push(h: &H, k: &str, v: V) {
    let mut h = h.borrow_mut();
    if let Some((_, a)) = h.values.iter_mut().find(|(x, _)| x == k) {
        a.push(v)
    } else {
        h.values.push((k.into(), vec![v]));
    }
}
fn values(h: &H, k: &str) -> V {
    array(
        h.borrow()
            .values
            .iter()
            .find(|(x, _)| x == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default(),
    )
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        (V::Set(a), V::Set(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn raw(v: &V, key: &str) -> V {
    match v {
        V::Object(o) => o
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined),
        V::Array(a) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| a.borrow().get(i).cloned().flatten())
            .unwrap_or(V::Undefined),
        _ => V::Undefined,
    }
}
fn property(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(error(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ),
        ));
    }
    if same(v, &h.borrow().json) && h.borrow().input["customSymbol"] == true {
        if key == "d" {
            push(h, "reads", text("d:get"));
        }
        if key == "s" {
            return Ok(V::Symbol);
        }
    }
    if let V::Fault(s) = v {
        if key == "message" {
            push(h, "reads", text("error:message"));
            return Ok(V::String(s.clone()));
        }
    }
    if same(v, &h.borrow().instance) && key == "data" {
        record(h, "data:get")?;
        return Ok(from_json(&h.borrow().input["data"]));
    }
    Ok(raw(v, key))
}
fn assign(_: &H, v: &V, k: &'static str, x: V) -> Result<(), Error> {
    set(v, k, x);
    Ok(())
}
fn set(v: &V, k: &str, x: V) {
    let V::Object(o) = v else {
        panic!("set nonobject")
    };
    let mut o = o.borrow_mut();
    if let Some((_, old)) = o.iter_mut().find(|(a, _)| a == k) {
        *old = x
    } else {
        o.push((k.into(), x));
    }
}
fn this(h: &H, k: &'static str) -> Result<V, Error> {
    let v = h.borrow().instance.clone();
    property(h, &v, k)
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(b) => *b,
        V::Number(n) => *n != 0.0 && !n.is_nan(),
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn stringify(h: &H, v: &V) -> Result<Vec<u16>, Error> {
    Ok(match v {
        V::String(s) => s.clone(),
        V::Undefined => "undefined".encode_utf16().collect(),
        V::Null => "null".encode_utf16().collect(),
        V::Bool(b) => b.to_string().encode_utf16().collect(),
        V::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
        V::Object(_) => "[object Object]".encode_utf16().collect(),
        V::Symbol => {
            push(h, "reads", text("symbol:string"));
            let replace = {
                let mut s = h.borrow_mut();
                s.symbol_reads += 1;
                s.input["replaceRoute"] == true && s.symbol_reads == 2
            };
            if replace {
                let instance = h.borrow().instance.clone();
                set(
                    &instance,
                    "handleRoutedMessage",
                    V::Function("overrideRoute".into()),
                );
            }
            h.borrow().input["symbol"]
                .as_str()
                .unwrap()
                .encode_utf16()
                .collect()
        }
        V::Array(a) => {
            let list = a.borrow().clone();
            let mut out = Vec::new();
            for (i, v) in list.iter().enumerate() {
                if i > 0 {
                    out.push(44)
                }
                if let Some(v) = v {
                    if !matches!(v, V::Null | V::Undefined) {
                        out.extend(stringify(h, v)?);
                    }
                }
            }
            out
        }
        _ => panic!("stringify {v:?}"),
    })
}
fn number(h: &H, v: V, fallback: V) -> Result<V, Error> {
    let fallback = if matches!(fallback, V::Undefined) {
        V::Number(0.0)
    } else {
        fallback
    };
    if matches!(v, V::Null | V::Undefined) || matches!(&v,V::String(s)if s.is_empty()) {
        return Ok(fallback);
    }
    let n = safe_cex_num::parse_float(&String::from_utf16_lossy(&stringify(h, &v)?));
    Ok(if n.is_finite() {
        V::Number(n)
    } else {
        fallback
    })
}
fn js_numeric(h: &H, v: &V) -> Result<f64, Error> {
    Ok(match v {
        V::Number(n) => *n,
        V::Null => 0.0,
        V::Undefined => f64::NAN,
        V::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => {
            let s = String::from_utf16_lossy(&stringify(h, v)?)
                .trim()
                .to_owned();
            if s.is_empty() {
                0.0
            } else {
                s.parse().unwrap_or(f64::NAN)
            }
        }
    })
}
fn lookup(h: &H, o: &V, k: &V) -> Result<V, Error> {
    let k = String::from_utf16_lossy(&stringify(h, k)?);
    property(h, o, &k)
}
fn clone_value(h: &H, v: &V) -> Result<V, Error> {
    Ok(match v {
        V::Object(o) => {
            let fields = o.borrow().clone();
            let mut result = Vec::new();
            for (k, v0) in fields {
                let v0 = if same(v, &h.borrow().json)
                    && h.borrow().input["customSymbol"] == true
                    && k == "d"
                {
                    property(h, v, "d")?
                } else {
                    v0
                };
                result.push((k, clone_value(h, &v0)?));
            }
            V::Object(Rc::new(RefCell::new(result)))
        }
        V::Array(a) => V::Array(Rc::new(RefCell::new(
            a.borrow()
                .iter()
                .map(|v| v.as_ref().map(|v| clone_value(h, v).unwrap()))
                .collect(),
        ))),
        V::Symbol => object(vec![]),
        _ => v.clone(),
    })
}
fn disposed(h: &H) -> Result<bool, Error> {
    record(h, "disposed")?;
    Ok(h.borrow().disposed)
}
fn base(h: &H, parent: V) -> Result<V, Error> {
    record(h, "baseConstruct")?;
    let instance = object(vec![
        ("parent", parent),
        ("onClose", V::Function("onClose".into())),
    ]);
    h.borrow_mut().instance = instance.clone();
    Ok(instance)
}
fn call_event(h: &H, method: V, _: V, event: &'static str, handler: V) -> Result<(), Error> {
    let V::Function(method) = method else {
        panic!("method")
    };
    record(h, &format!("event:{method}"))?;
    let V::Function(handler) = handler else {
        panic!("handler")
    };
    push(
        h,
        "parentEvents",
        object(vec![("event", text(event)), ("handler", text(&handler))]),
    );
    Ok(())
}
fn create(h: &H, url: V, protocols: V, options: V) -> Result<V, Error> {
    record(h, "create")?;
    push(
        h,
        "created",
        object(vec![
            ("url", url),
            ("protocols", protocols),
            ("options", options),
        ]),
    );
    Ok(object(vec![(
        "addEventListener",
        V::Function("addEventListener".into()),
    )]))
}
fn listeners(h: &H, _: V, _: V, event: &'static str, handler: V) -> Result<(), Error> {
    record(h, &format!("listener:{event}"))?;
    let V::Function(handler) = handler else {
        panic!("handler")
    };
    push(
        h,
        "listeners",
        object(vec![("event", text(event)), ("handler", text(&handler))]),
    );
    Ok(())
}
fn parse(h: &H, s: Vec<u16>) -> Result<V, Error> {
    record(h, "parse")?;
    push(h, "parseArgs", V::String(s));
    let value = h
        .borrow()
        .parsed
        .first()
        .expect("imported parser boundary fixture")
        .clone();
    h.borrow_mut().json = value.clone();
    if h.borrow().input["customSymbol"] == true {
        set(&value, "s", V::Symbol);
    }
    Ok(value)
}
fn pong(h: &H, id: f64) -> Result<(), Error> {
    record(h, "sendPong")?;
    h.borrow_mut().primary = object(vec![("method", text("PONG")), ("id", V::Number(id))]);
    Ok(())
}
fn warning(h: &H, _: V, s: Vec<u16>) -> Result<(), Error> {
    record(h, "warning")?;
    push(h, "warnings", V::String(s));
    Ok(())
}
fn route(h: &H, callee: V, key: V, json: &V) -> Result<(), Error> {
    if callee == V::Function("overrideRoute".into()) {
        return record(h, "overrideRoute");
    }
    record(h, "route")?;
    let cloned = clone_value(h, json)?;
    push(h, "routes", object(vec![("key", key), ("json", cloned)]));
    Ok(())
}
fn make_callback(h: &H, callback: WeexPublicCallback<V>) -> Result<V, Error> {
    let mut h = h.borrow_mut();
    let index = h.callbacks.len();
    h.callbacks.push(callback);
    Ok(V::Callback(index))
}
fn add_handler(h: &H, key: &V, callback: &V) -> Result<(), Error> {
    record(h, "addHandler")?;
    push(
        h,
        "handlerOps",
        object(vec![("op", text("add")), ("key", key.clone())]),
    );
    h.borrow_mut()
        .handlers
        .push((key.clone(), callback.clone()));
    Ok(())
}
fn remove_handler(h: &H, key: &V, callback: &V) -> Result<(), Error> {
    record(h, "removeHandler")?;
    push(
        h,
        "handlerOps",
        object(vec![("op", text("remove")), ("key", key.clone())]),
    );
    let mut h = h.borrow_mut();
    let i = h
        .handlers
        .iter()
        .position(|(k, c)| {
            k == key && matches!((c,callback),(V::Callback(a),V::Callback(b))if a==b)
        })
        .expect("same native handler identity");
    h.handlers.remove(i);
    Ok(())
}
fn subscription(h: &H, op: &str, topic: &V) -> Result<(), Error> {
    record(h, op)?;
    push(
        h,
        "subscriptions",
        object(vec![("op", text(op)), ("topic", topic.clone())]),
    );
    Ok(())
}
fn snapshot(h: &H, symbol: &V, limit: f64) -> Result<V, Error> {
    record(h, "fetch")?;
    let expected = if h.borrow().input["customSymbol"] == true {
        V::Symbol
    } else {
        from_json(&h.borrow().input["symbol"])
    };
    assert!(
        matches!((symbol, &expected), (V::Symbol, V::Symbol)) || symbol == &expected,
        "public_feed::snapshot_native_symbol"
    );
    assert_eq!(
        limit.to_bits(),
        200.0_f64.to_bits(),
        "public_feed::snapshot_native_limit"
    );
    Ok(V::Number(1.0))
}
fn put_book(h: &H, books: &V, symbol: &V, value: V) -> Result<(), Error> {
    let key = String::from_utf16_lossy(&stringify(h, symbol)?);
    set(books, &key, value);
    Ok(())
}
fn invoke(h: &H, callback: &V, value: V) -> Result<(), Error> {
    match callback {
        V::Function(name) if name == "handleTicker" => feed::handle_ticker(h, &runtime(), value),
        V::Function(name) if name == "handleDepth" => feed::handle_depth(h, &runtime(), value),
        V::Function(name) if name == "overrideTicker" || name == "overrideDepth" => record(h, name),
        V::Function(name) if name == "callback" => {
            record(h, "callback")?;
            push(h, "callbacks", clone_value(h, &value)?);
            if h.borrow().input["append"] == true {
                let count = h
                    .borrow()
                    .values
                    .iter()
                    .find(|(k, _)| k == "callbacks")
                    .unwrap()
                    .1
                    .len();
                if count == 1 {
                    let rows = raw(&h.borrow().json, "d");
                    let V::Array(a) = rows else { panic!("rows") };
                    a.borrow_mut().push(Some(object(vec![(
                        "normalized",
                        object(vec![("close", V::Number(9.0))]),
                    )])));
                }
            }
            Ok(())
        }
        V::Callback(index) => {
            let desc = h.borrow().callbacks[*index].clone();
            feed::run_public_callback(h, &runtime(), &desc, value)
        }
        _ => panic!("invoke {callback:?}"),
    }
}
fn iterator(_: &H, v: &V) -> Result<V, Error> {
    if !matches!(v, V::Array(_)) {
        return Err(error("TypeError", "values is not iterable"));
    }
    Ok(V::Iterator(Rc::new(RefCell::new((v.clone(), 0)))))
}
fn next(_: &H, v: &V) -> Result<Option<V>, Error> {
    let V::Iterator(it) = v else {
        panic!("iterator")
    };
    let mut it = it.borrow_mut();
    let V::Array(a) = &it.0 else { panic!("array") };
    let result = a.borrow().get(it.1).cloned();
    if let Some(v) = result {
        it.1 += 1;
        Ok(Some(v.unwrap_or(V::Undefined)))
    } else {
        Ok(None)
    }
}
fn pair(_: &H, v: V) -> Result<(V, V), Error> {
    if !matches!(v, V::Array(_)) {
        return Err(error("TypeError", ".for is not iterable"));
    }
    Ok((raw(&v, "0"), raw(&v, "1")))
}
fn update_ticker(h: &H, _: V, ticker: V, patch: V) -> Result<(), Error> {
    record(h, "updateTicker")?;
    push(
        h,
        "tickerUpdates",
        object(vec![
            ("ticker", clone_value(h, &ticker)?),
            ("patch", patch.clone()),
        ]),
    );
    let V::Object(o) = patch else { panic!("patch") };
    for (k, v) in o.borrow().iter() {
        set(&ticker, k, v.clone());
    }
    Ok(())
}
fn update_price(h: &H, book: &V, side: &'static str, price: V, amount: V) -> Result<(), Error> {
    record(h, &format!("updatePrice:{side}"))?;
    push(
        h,
        "priceUpdates",
        object(vec![
            ("side", text(side)),
            ("price", price.clone()),
            ("amount", amount.clone()),
        ]),
    );
    let V::Number(price) = price else {
        panic!("price")
    };
    let V::Number(amount) = amount else {
        panic!("amount")
    };
    let V::Array(levels) = raw(book, side) else {
        panic!("levels")
    };
    let mut levels = levels.borrow_mut();
    let mut left = 0isize;
    let mut right = levels.len() as isize - 1;
    let mut found = None;
    while left <= right {
        let mid = ((left + right) as usize) >> 1;
        let V::Number(p) = raw(levels[mid].as_ref().unwrap(), "price") else {
            panic!("price")
        };
        if p == price {
            found = Some(mid);
            break;
        }
        if if side == "bids" { p > price } else { p < price } {
            left = mid as isize + 1
        } else {
            right = mid as isize - 1
        }
    }
    if let Some(i) = found {
        if amount == 0.0 {
            levels.remove(i);
        } else {
            set(levels[i].as_ref().unwrap(), "amount", V::Number(amount));
        }
    } else if amount != 0.0 {
        levels.insert(
            left as usize,
            Some(object(vec![
                ("price", V::Number(price)),
                ("amount", V::Number(amount)),
                ("total", V::Number(0.0)),
            ])),
        );
    }
    drop(levels);
    if h.borrow().input["append"] == true {
        let count = h
            .borrow()
            .values
            .iter()
            .find(|(k, _)| k == "priceUpdates")
            .unwrap()
            .1
            .len();
        if count == 1 {
            let rows = raw(&h.borrow().json, "b");
            let V::Array(a) = rows else { panic!("rows") };
            a.borrow_mut()
                .push(Some(array(vec![V::Number(98.0), V::Number(9.0)])));
        }
    }
    Ok(())
}
fn totals(h: &H, book: &V) -> Result<(), Error> {
    record(h, "calcTotals")?;
    for side in ["bids", "asks"] {
        let V::Array(a) = raw(book, side) else {
            panic!("side")
        };
        let mut sum = 0.0;
        for v in a.borrow().iter().flatten() {
            let V::Number(n) = raw(v, "amount") else {
                panic!("amount")
            };
            sum += n;
            set(v, "total", V::Number(sum));
        }
    }
    Ok(())
}
fn runtime() -> RT {
    RT {
        base_construct: base,
        define: assign,
        bind_field: |_, _, name| Ok(V::Function(name.into())),
        new_set: |_| Ok(V::Set(Rc::new(RefCell::new(vec![])))),
        object,
        array,
        number: V::Number,
        boolean: V::Bool,
        text,
        string_units: V::String,
        undefined: || V::Undefined,
        this_property: this,
        property,
        optional_property: |h, v, k| {
            if matches!(v, V::Null | V::Undefined) {
                Ok(V::Undefined)
            } else {
                property(h, v, k)
            }
        },
        lookup,
        assign,
        remove: |h, o, k| {
            let k = String::from_utf16_lossy(&stringify(h, k)?);
            let V::Object(o) = o else { panic!("books") };
            o.borrow_mut().retain(|(s, _)| s != &k);
            Ok(())
        },
        truthy,
        is_undefined: |v| matches!(v, V::Undefined),
        equals_text: |v, s| *v == text(s),
        is_false: |v| matches!(v, V::Bool(false)),
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        string_constructor: stringify,
        template_string: stringify,
        to_number: number,
        number_value: |v| {
            let V::Number(n) = v else {
                panic!("number {v:?}")
            };
            *n
        },
        abs: |h, v| Ok(js_numeric(h, v)?.abs()),
        multiply: |h, v, n| Ok(V::Number(js_numeric(h, v)? * n)),
        to_integer_string: |h, v| {
            Ok(V::String(if matches!(v, V::Null | V::Undefined) {
                vec![]
            } else {
                stringify(h, &v)?
            }))
        },
        method: |h, v, k| property(h, v, k),
        call_event,
        disposed,
        create_socket: create,
        set_socket: |h, v| {
            let instance = h.borrow().instance.clone();
            set(&instance, "ws", v);
            Ok(())
        },
        add_listener: listeners,
        track_received: |h| record(h, "track"),
        parse,
        subscription_id: |h| {
            let V::Number(n) = this(h, "subscriptionId")? else {
                panic!("id")
            };
            Ok(n)
        },
        set_subscription_id: |h, n| {
            let instance = h.borrow().instance.clone();
            set(&instance, "subscriptionId", V::Number(n));
            Ok(())
        },
        send_pong: pong,
        handle_pong: |h, n| {
            record(h, "pong")?;
            push(
                h,
                "reads",
                text(&format!("pong:{}", safe_cex_num::js_number_string(n))),
            );
            Ok(())
        },
        warning_sink: |_| Ok(V::Function("warning".into())),
        warning,
        record_stats: |h, v| {
            record(h, "stats")?;
            push(h, "stats", v);
            Ok(())
        },
        route_method: |h| this(h, "handleRoutedMessage"),
        route,
        interval: |_, v| {
            let mapping = safe_cex_venue_weex::types::INTERVAL;
            Ok(mapping
                .iter()
                .find(|(k, _)| v == text(k))
                .map(|(_, v)| text(v))
                .unwrap_or(V::Undefined))
        },
        supported_interval: |v| {
            safe_cex_venue_weex::types::SUPPORTED_WS_INTERVALS
                .iter()
                .any(|s| *v == text(s))
        },
        make_callback,
        add_handler,
        remove_handler,
        subscribe: |h, v| subscription(h, "subscribe", v),
        unsubscribe: |h, v| subscription(h, "unsubscribe", v),
        snapshot,
        observe_snapshot: |h, _, ok, err| {
            h.borrow_mut().jobs.push((ok, err));
            Ok(())
        },
        books: |h| this(h, "orderbooks"),
        put_book,
        invoke,
        normalize_candle: |h, v| {
            record(h, "normalizeCandle")?;
            let v = raw(&v, "normalized");
            Ok(if matches!(v, V::Undefined | V::Null) {
                V::Null
            } else {
                v
            })
        },
        iterator,
        next,
        close_on_error: |h, _, _| {
            if h.borrow().input["customIterator"] == true {
                record(h, "iteratorClose").expect("fixture close succeeds");
            }
        },
        pair,
        tickers_by_symbol: |h| {
            let store = this(h, "store")?;
            property(h, &store, "tickersBySymbol")
        },
        ticker_updater: |_| Ok(V::Function("updateTicker".into())),
        update_ticker,
        update_price,
        calc_totals: totals,
        base_dispose: |h| {
            record(h, "baseDispose")?;
            h.borrow_mut().disposed = true;
            Ok(())
        },
        new_error: |_, units| Error {
            name: "Error",
            units,
        },
    }
}
fn input_flag(h: &H, k: &str) -> bool {
    h.borrow().input[k] == true
}
fn settle(h: &H) -> Result<(), Error> {
    let jobs = std::mem::take(&mut h.borrow_mut().jobs);
    for (ok, err) in jobs {
        let result = if h.borrow().input["fail"] == "fetch:reject" {
            Err(V::Fault("fail:fetch:reject".encode_utf16().collect()))
        } else {
            let snapshot = from_json(&h.borrow().input["snapshot"]);
            feed::run_public_callback(h, &runtime(), &ok, snapshot)
                .map_err(|e| object(vec![("message", V::String(e.units))]))
        };
        if let Err(error) = result {
            feed::run_public_callback(h, &runtime(), &err, error)?;
        }
    }
    Ok(())
}
fn output(h: &H, constructor_value: V, error: Option<Error>, constructor_failed: bool) -> V {
    let instance = h.borrow().instance.clone();
    let method = h.borrow().input["method"].as_str().unwrap().to_owned();
    let symbol = from_json(&h.borrow().input["symbol"]);
    let id = if constructor_failed {
        V::Undefined
    } else {
        raw(&instance, "subscriptionId")
    };
    let (book, ticker) = if method == "constructor" {
        (V::Undefined, V::Undefined)
    } else {
        (
            lookup(h, &raw(&instance, "orderbooks"), &symbol).unwrap(),
            lookup(
                h,
                &raw(&raw(&instance, "store"), "tickersBySymbol"),
                &symbol,
            )
            .unwrap(),
        )
    };
    object(vec![
        ("constructorValue", constructor_value),
        (
            "error",
            error
                .map(|e| {
                    object(vec![
                        ("name", text(e.name)),
                        ("message", V::String(e.units)),
                    ])
                })
                .unwrap_or(V::Null),
        ),
        ("disposed", V::Bool(h.borrow().disposed)),
        ("id", id),
        ("created", values(h, "created")),
        ("listeners", values(h, "listeners")),
        ("warnings", values(h, "warnings")),
        ("callbacks", values(h, "callbacks")),
        ("routes", values(h, "routes")),
        ("stats", values(h, "stats")),
        ("subscriptions", values(h, "subscriptions")),
        ("handlerOps", values(h, "handlerOps")),
        ("tickerUpdates", values(h, "tickerUpdates")),
        ("priceUpdates", values(h, "priceUpdates")),
        ("parentEvents", values(h, "parentEvents")),
        ("reads", values(h, "reads")),
        ("parseArgs", values(h, "parseArgs")),
        ("primary", h.borrow().primary.clone()),
        ("book", book),
        ("ticker", ticker),
    ])
}
fn run(row: &Json) -> (V, Vec<String>) {
    let input = row["input"].clone();
    let json = from_json(&input["json"]);
    let h = H::new(State {
        disposed: input["disposed"] == true,
        input,
        parsed: row["parsed"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect(),
        instance: V::Undefined,
        json,
        events: vec![],
        callbacks: vec![],
        handlers: vec![],
        jobs: vec![],
        values: vec![],
        primary: V::Undefined,
        symbol_reads: 0,
    });
    let method = h.borrow().input["method"].as_str().unwrap().to_owned();
    let parent = object(vec![
        (
            "options",
            object(vec![("testnet", from_json(&h.borrow().input["testnet"]))]),
        ),
        ("on", V::Function("on".into())),
        ("off", V::Function("off".into())),
    ]);
    let construct = feed::construct_public_websocket(&h, &runtime(), parent);
    let failed = construct.is_err();
    let mut error = construct.err();
    let mut constructor_value = V::Null;
    if !failed && method == "constructor" {
        let V::Object(o) = h.borrow().instance.clone() else {
            panic!("instance")
        };
        constructor_value = V::Object(Rc::new(RefCell::new(
            o.borrow()
                .iter()
                .filter(|(k, _)| k != "parent" && k != "onClose")
                .cloned()
                .collect(),
        )));
        let topics = raw(&constructor_value, "topics");
        let ticker_topics = raw(&constructor_value, "tickerTopics");
        assert!(
            !same(&topics, &ticker_topics),
            "public_feed::constructor_native_fresh_sets"
        );
        assert!(
            !same(
                &raw(&constructor_value, "messageHandlers"),
                &raw(&constructor_value, "orderbooks")
            ),
            "public_feed::constructor_native_fresh_objects"
        );
    }
    if !failed && method != "constructor" {
        h.borrow_mut().events.clear();
        h.borrow_mut().values.clear();
        let instance = h.borrow().instance.clone();
        let books = object(vec![]);
        let symbol = h.borrow().input["symbol"].as_str().unwrap().to_owned();
        if !h.borrow().input["book"].is_null() {
            set(&books, &symbol, from_json(&h.borrow().input["book"]));
        }
        set(&instance, "orderbooks", books);
        let tickers = object(vec![]);
        if !h.borrow().input["ticker"].is_null() {
            set(&tickers, &symbol, from_json(&h.borrow().input["ticker"]));
        }
        set(
            &instance,
            "store",
            object(vec![("tickersBySymbol", tickers)]),
        );
        set(
            &instance,
            "handleRoutedMessage",
            V::Function("route".into()),
        );
        if h.borrow().input["override"] == "ticker" {
            set(
                &instance,
                "handleTicker",
                V::Function("overrideTicker".into()),
            );
        }
        if h.borrow().input["override"] == "depth" {
            set(
                &instance,
                "handleDepth",
                V::Function("overrideDepth".into()),
            );
        }
        let holes = h.borrow().input["holes"].as_array().unwrap().clone();
        if !holes.is_empty() {
            let V::Array(a) = raw(&h.borrow().json, "d") else {
                panic!("array")
            };
            for i in holes {
                a.borrow_mut()[i.as_u64().unwrap() as usize] = None;
            }
        }
        let result = (|| {
            let json = h.borrow().json.clone();
            let mut disposer = V::Undefined;
            match method.as_str() {
                "connect" => feed::connect_and_subscribe(&h, &runtime())?,
                "message" => {
                    feed::on_message(&h, &runtime(), instance.clone())?;
                }
                "candle" => {
                    let opts = from_json(&h.borrow().input["opts"]);
                    disposer =
                        feed::listen_ohlcv(&h, &runtime(), opts, V::Function("callback".into()))?;
                    if input_flag(&h, "dispatch") {
                        let handler = h.borrow().handlers[0].1.clone();
                        invoke(&h, &handler, json)?;
                    }
                }
                "book" => {
                    let symbol = if input_flag(&h, "customSymbol") {
                        V::Symbol
                    } else {
                        text(&symbol)
                    };
                    disposer = feed::listen_order_book(
                        &h,
                        &runtime(),
                        symbol,
                        V::Function("callback".into()),
                    )?;
                }
                "ticker" => feed::handle_ticker(&h, &runtime(), json)?,
                "depth" => feed::handle_depth(&h, &runtime(), json)?,
                "dispose" => feed::dispose(&h, &runtime())?,
                _ => panic!("method"),
            };
            if input_flag(&h, "unsubscribeBefore") {
                invoke(&h, &disposer, V::Undefined)?;
            }
            if input_flag(&h, "disposeAfterStart") {
                h.borrow_mut().disposed = true;
            }
            if input_flag(&h, "settle") {
                settle(&h)?;
            }
            if method == "book" && input_flag(&h, "dispatch") {
                let handler = h.borrow().handlers.first().map(|(_, v)| v.clone());
                if let Some(handler) = handler {
                    invoke(&h, &handler, V::Undefined)?;
                }
            }
            if input_flag(&h, "unsubscribe") {
                invoke(&h, &disposer, V::Undefined)?;
            }
            Ok(())
        })();
        error = result.err();
    }
    let value = output(&h, constructor_value, error, failed);
    let events = h.borrow().events.clone();
    (value, events)
}
#[test]
fn public_feed_native_source_floor() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/public-feed-oracle.cjs");
    let result = Command::new("node")
        .arg(script)
        .output()
        .expect("node oracle");
    assert!(
        result.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rows: Vec<Json> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(rows.len(), 71, "public_feed::fixture_floor");
    let native: Vec<_> = rows.iter().map(run).collect();
    // Compare decoded native values across the whole floor before comparing event traces.
    for (row, (actual, _)) in rows.iter().zip(&native) {
        let expected = decode(&row["native"]);
        let method = row["input"]["method"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        if id == "constructor_defaults" {
            let V::Object(a) = raw(actual, "constructorValue") else {
                panic!("native constructor object");
            };
            let V::Object(b) = raw(&expected, "constructorValue") else {
                panic!("source constructor object");
            };
            let native_names: Vec<_> = a.borrow().iter().map(|(k, _)| k.clone()).collect();
            let source_names: Vec<_> = b.borrow().iter().map(|(k, _)| k.clone()).collect();
            assert_eq!(
                native_names.len(),
                28,
                "public_feed::constructor_native_field_floor"
            );
            assert_eq!(
                native_names, source_names,
                "public_feed::constructor_native_field_order"
            );
        }
        assert_eq!(*actual, expected, "public_feed::{method}_native {id}");
    }
    for (row, (_, events)) in rows.iter().zip(&native) {
        let actual = events.clone();
        let expected: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
        assert_eq!(actual, expected, "public_feed::event_trace {}", row["id"]);
    }
}
