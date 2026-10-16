use safe_cex_venue_weex::market_reads::*;
use serde_json::Value as J;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug, Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(Rc<RefCell<Vec<(String, V)>>>, String),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Symbol(Rc<Vec<u16>>),
    Map(Rc<RefCell<Vec<(V, V)>>>),
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Object(a, _), Self::Object(b, _)) => *a.borrow() == *b.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            (Self::Symbol(a), Self::Symbol(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
struct Error {
    name: &'static str,
    units: Vec<u16>,
    venue: Option<Vec<u16>>,
}
type H = RefCell<State>;
type F<'a, T> = Pin<Box<dyn Future<Output = Result<T, Error>> + 'a>>;
type RT = WeexMarketReadRuntime<H, V, Error>;
struct Task {
    key: String,
    left: u64,
    value: V,
    done: Option<Result<V, Error>>,
}
struct State {
    input: J,
    opts: V,
    symbol: V,
    callback: V,
    markets: V,
    events: Vec<String>,
    requests: Vec<(&'static str, Option<V>, Option<u64>)>,
    reports: Vec<V>,
    calls: Vec<(String, String, V, bool, bool)>,
    tasks: Vec<Task>,
    completed: Vec<usize>,
    counts: std::collections::HashMap<String, usize>,
    provider_index: usize,
    row_symbol_index: usize,
    market_version: &'static str,
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::String(units(s))
}
fn object(fields: Vec<(&'static str, V)>) -> V {
    V::Object(
        Rc::new(RefCell::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )),
        String::new(),
    )
}
fn array(items: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(items.into_iter().map(Some).collect())))
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::Undefined,
        "null" => V::Null,
        "bool" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => V::String(serde_json::from_value(j["units"].clone()).unwrap()),
        "symbol" => V::Symbol(Rc::new(serde_json::from_value(j["units"].clone()).unwrap())),
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
        "object" => V::Object(
            Rc::new(RefCell::new(
                j["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (v[0].as_str().unwrap().into(), decode(&v[1])))
                    .collect(),
            )),
            String::new(),
        ),
        other => panic!("unexpected {other}"),
    }
}
fn get_plain(v: &V, key: &str) -> V {
    match v {
        V::Object(fields, _) => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined),
        V::Array(items) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| items.borrow().get(i).cloned().flatten())
            .unwrap_or(V::Undefined),
        _ => V::Undefined,
    }
}
fn error(stage: &str, venue: Option<Vec<u16>>) -> Error {
    Error {
        name: "RangeError",
        units: units(&format!("fail:{stage}")),
        venue,
    }
}
fn check(h: &H, stage: &str) -> Result<(), Error> {
    let s = h.borrow();
    if s.input["fail"] == stage
        || s.input["fails"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == stage)
    {
        let venue = decode(&s.input["venueMessage"]);
        Err(error(
            stage,
            if let V::String(v) = venue {
                Some(v)
            } else {
                None
            },
        ))
    } else {
        Ok(())
    }
}
fn record(h: &H, stage: &str) -> Result<(), Error> {
    h.borrow_mut().events.push(stage.into());
    check(h, stage)
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(v) => *v,
        V::Number(n) => *n != 0.0 && !n.is_nan(),
        V::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn equals_text(v: &V, text: &str) -> bool {
    matches!(v,V::String(u) if *u==units(text))
}
fn to_string(_: &H, v: &V) -> Result<V, Error> {
    Ok(match v {
        V::String(_) => v.clone(),
        V::Undefined => text("undefined"),
        V::Null => text("null"),
        V::Bool(v) => text(if *v { "true" } else { "false" }),
        V::Number(n) => text(&safe_cex_num::js_number_string(*n)),
        V::Symbol(s) => {
            let mut u = units("Symbol(");
            u.extend(s.iter());
            u.push(41);
            V::String(u)
        }
        _ => text("[object Object]"),
    })
}
fn template(h: &H, v: &V) -> Result<Vec<u16>, Error> {
    if matches!(v, V::Symbol(_)) {
        return Err(Error {
            name: "TypeError",
            units: units("Cannot convert a Symbol value to a string"),
            venue: None,
        });
    }
    let V::String(s) = to_string(h, v)? else {
        panic!("String")
    };
    Ok(s)
}
fn number(h: &H, v: &V) -> Result<f64, Error> {
    match v {
        V::Number(n) => Ok(*n),
        V::Null => Ok(0.0),
        V::Undefined => Ok(f64::NAN),
        V::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        V::String(s) => {
            let s = String::from_utf16_lossy(s);
            if s.trim().is_empty() {
                Ok(0.0)
            } else {
                Ok(s.trim().parse().unwrap_or(f64::NAN))
            }
        }
        V::Symbol(_) => Err(Error {
            name: "TypeError",
            units: units("Cannot convert a Symbol value to a number"),
            venue: None,
        }),
        _ => number(h, &to_string(h, v)?),
    }
}
fn get(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(Error {
            name: "TypeError",
            units: units(&format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )),
            venue: None,
        });
    }
    if let V::Object(_, label) = v {
        if !label.is_empty() && (label != "row" || key == "symbol") {
            record(h, &format!("get:{label}:{key}"))?;
            if h.borrow().input["failGet"] == format!("{label}:{key}") {
                return Err(error(&format!("get:{label}:{key}"), None));
            }
            if label == "opts" {
                let mut s = h.borrow_mut();
                let index = *s.counts.entry(key.into()).or_default();
                *s.counts.get_mut(key).unwrap() += 1;
                if let Some(seq) = s.input["optsSequence"][key].as_array() {
                    return Ok(V::Number(seq[index.min(seq.len() - 1)].as_f64().unwrap()));
                }
            }
            if label == "row" && key == "symbol" {
                let mut s = h.borrow_mut();
                s.row_symbol_index += 1;
                if s.row_symbol_index > 1 {
                    s.market_version = "NEW";
                }
                return Ok(text(if s.row_symbol_index == 1 {
                    "BTC"
                } else {
                    "ETH"
                }));
            }
        }
    }
    Ok(get_plain(v, key))
}
fn optional(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        Ok(V::Undefined)
    } else {
        get(h, v, key)
    }
}
fn array_ref(v: &V) -> Rc<RefCell<Vec<Option<V>>>> {
    let V::Array(a) = v else {
        panic!("bounded fixture expected array")
    };
    a.clone()
}
fn species(h: &H) -> Result<(), Error> {
    if h.borrow().input["speciesFail"] == true {
        record(h, "species")?;
        Err(error("species", None))
    } else {
        Ok(())
    }
}
fn map<'a>(
    h: &'a H,
    v: &V,
    mut callback: Box<dyn FnMut(V, usize, &V) -> Result<V, Error> + 'a>,
) -> Result<V, Error> {
    species(h)?;
    let a = array_ref(v);
    let n = a.borrow().len();
    let mut out = vec![None; n];
    for (i, entry) in out.iter_mut().enumerate() {
        let value = a.borrow().get(i).cloned().flatten();
        if let Some(value) = value {
            *entry = Some(callback(value, i, v)?);
        }
    }
    Ok(V::Array(Rc::new(RefCell::new(out))))
}
fn filter<'a>(
    h: &'a H,
    v: &V,
    mut callback: Box<dyn FnMut(&V, usize, &V) -> Result<bool, Error> + 'a>,
) -> Result<V, Error> {
    species(h)?;
    let a = array_ref(v);
    let n = a.borrow().len();
    let mut out = Vec::new();
    for i in 0..n {
        let value = a.borrow().get(i).cloned().flatten();
        if let Some(value) = value {
            if callback(&value, i, v)? {
                out.push(value);
            }
        }
    }
    Ok(array(out))
}
fn reduce<'a>(
    _: &'a H,
    v: &V,
    mut acc: V,
    mut callback: Box<dyn FnMut(V, V, usize, &V) -> Result<V, Error> + 'a>,
) -> Result<V, Error> {
    let a = array_ref(v);
    let n = a.borrow().len();
    for i in 0..n {
        let value = a.borrow().get(i).cloned().flatten();
        if let Some(value) = value {
            acc = callback(acc, value, i, v)?;
        }
    }
    Ok(acc)
}
fn sort<'a>(
    _: &'a H,
    v: V,
    mut compare: Box<dyn FnMut(&V, &V) -> Result<f64, Error> + 'a>,
) -> Result<V, Error> {
    let a = array_ref(&v);
    let mut values = a.borrow().iter().flatten().cloned().collect::<Vec<_>>();
    for i in 1..values.len() {
        let mut j = i;
        while j > 0 && compare(&values[j], &values[j - 1])? < 0.0 {
            values.swap(j, j - 1);
            j -= 1;
        }
    }
    *a.borrow_mut() = values.into_iter().map(Some).collect();
    Ok(v)
}
fn new_map(_: &H, entries: V) -> Result<V, Error> {
    let a = array_ref(&entries);
    let mut out: Vec<(V, V)> = Vec::new();
    for entry in a.borrow().iter() {
        let Some(V::Array(e)) = entry else {
            return Err(Error {
                name: "TypeError",
                units: units("Iterator value undefined is not an entry object"),
                venue: None,
            });
        };
        let e = e.borrow();
        let key = e[0].clone().unwrap();
        let value = e[1].clone().unwrap();
        if let Some((_, v)) = out.iter_mut().find(|(k, _)| *k == key) {
            *v = value;
        } else {
            out.push((key, value));
        }
    }
    Ok(V::Map(Rc::new(RefCell::new(out))))
}
fn map_lookup(_: &H, map: V, key: V) -> Result<V, Error> {
    let V::Map(m) = map else { panic!("Map") };
    Ok(m.borrow()
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::Undefined))
}
fn push(_: &H, receiver: V, value: V) -> Result<(), Error> {
    array_ref(&receiver).borrow_mut().push(Some(value));
    Ok(())
}
fn copy_properties(h: &H, value: V) -> Result<V, Error> {
    let V::Object(fields, _) = &value else {
        panic!("spread fixture")
    };
    let keys = fields
        .borrow()
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for key in keys {
        out.push((key.clone(), get(h, &value, &key)?));
    }
    Ok(V::Object(Rc::new(RefCell::new(out)), String::new()))
}
fn define(_: &H, target: &V, key: &'static str, value: V) -> Result<(), Error> {
    let V::Object(f, _) = target else {
        panic!("object")
    };
    let mut f = f.borrow_mut();
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        f.push((key.into(), value));
    }
    Ok(())
}
fn extract(h: &H, v: V) -> Result<V, Error> {
    if matches!(v, V::Array(_)) {
        return Ok(v);
    }
    let data = optional(h, &v, "data")?;
    if matches!(data, V::Array(_)) {
        get(h, &v, "data")
    } else {
        Ok(array(vec![]))
    }
}
fn omit(_: &H, v: V) -> Result<V, Error> {
    let V::Object(f, label) = v else {
        panic!("omitUndefined")
    };
    let values = f
        .borrow()
        .iter()
        .filter(|(_, v)| !matches!(v, V::Undefined))
        .cloned()
        .collect();
    Ok(V::Object(Rc::new(RefCell::new(values)), label))
}
fn timestamp(h: &H, v: V) -> Result<f64, Error> {
    if matches!(v, V::Undefined | V::Null) || equals_text(&v, "") {
        return Ok(0.0);
    }
    let n = number(h, &v)?;
    Ok(if n.is_finite() && n > 0.0 {
        if n >= 1e18 {
            (n / 1e6).floor()
        } else if n >= 1e15 {
            (n / 1e3).floor()
        } else if n >= 1e12 {
            n.floor()
        } else {
            (n * 1e3).floor()
        }
    } else {
        0.0
    })
}
fn normalize_market(h: &H, row: V) -> Result<V, Error> {
    record(h, "normalizeMarket")?;
    get(h, &row, "native")
}
fn normalize_ticker(h: &H, ticker: V, market: V, book: V) -> Result<V, Error> {
    record(h, "normalizeTicker")?;
    Ok(object(vec![
        ("symbol", get(h, &market, "symbol")?),
        ("ticker", optional(h, &ticker, "native")?),
        ("book", optional(h, &book, "native")?),
    ]))
}
fn normalize_candle(h: &H, row: V) -> Result<V, Error> {
    record(h, "normalizeCandle")?;
    get(h, &row, "native")
}
fn normalize_book(h: &H, data: V) -> Result<V, Error> {
    record(h, "normalizeOrderBook")?;
    if h.borrow().input["mutateDepth"] == true {
        define(h, &data, "lastUpdateId", text("CHANGED"))?;
    }
    get(h, &data, "native")
}
fn normalize_trade(h: &H, row: V, symbol: &V) -> Result<V, Error> {
    record(h, "normalizePublicTrade")?;
    let v = get(h, &row, "native")?;
    if !truthy(&v) {
        return Ok(v);
    }
    let v = copy_properties(h, v)?;
    define(h, &v, "symbol", symbol.clone())?;
    Ok(v)
}
fn normalize_execution(h: &H, row: V, market: V) -> Result<V, Error> {
    record(h, "normalizeExecution")?;
    let v = get(h, &row, "native")?;
    if !truthy(&v) {
        return Ok(v);
    }
    let v = copy_properties(h, v)?;
    define(h, &v, "market", optional(h, &market, "tag")?)?;
    Ok(v)
}
struct Observation<'a> {
    host: &'a H,
    index: usize,
}
impl Future for Observation<'_> {
    type Output = Result<V, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.host.borrow().tasks[self.index].done {
            Some(v) => Poll::Ready(v.clone()),
            None => Poll::Pending,
        }
    }
}
fn request<'a>(h: &'a H, r: WeexMarketReadRequest<V>) -> Result<F<'a, V>, Error> {
    let index = h.borrow().requests.len() + 1;
    let key = format!("request:{index}");
    record(h, &format!("{key}:start"))?;
    h.borrow_mut()
        .requests
        .push((r.endpoint, r.params, r.timeout.map(f64::to_bits)));
    let data = {
        let s = h.borrow();
        let replies = decode(&s.input["replies"]);
        let a = array_ref(&replies);
        let a = a.borrow();
        if a.is_empty() {
            decode(&s.input["data"])
        } else {
            a[(index - 1).min(a.len() - 1)].clone().unwrap()
        }
    };
    if h.borrow().input["marketSymbolSequence"] == true {
        if let V::Array(a) = &data {
            for v in a.borrow_mut().iter_mut().flatten() {
                if let V::Object(_, label) = v {
                    *label = "row".into();
                }
            }
        }
    }
    let mut response = object(vec![("data", data)]);
    if let V::Object(_, label) = &mut response {
        *label = format!("response{index}");
    }
    let index = {
        let mut s = h.borrow_mut();
        let left = s.input["delays"][&key].as_u64().unwrap_or(1);
        let index = s.tasks.len();
        s.tasks.push(Task {
            key,
            left,
            value: response,
            done: None,
        });
        index
    };
    Ok(Box::pin(Observation { host: h, index }))
}
struct All<'a> {
    host: &'a H,
    a: F<'a, V>,
    b: F<'a, V>,
    results: [Option<Result<V, Error>>; 2],
    indices: [usize; 2],
}
impl Future for All<'_> {
    type Output = Result<(V, V), Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.results[0].is_none() {
            if let Poll::Ready(v) = self.a.as_mut().poll(cx) {
                self.results[0] = Some(v);
            }
        }
        if self.results[1].is_none() {
            if let Poll::Ready(v) = self.b.as_mut().poll(cx) {
                self.results[1] = Some(v);
            }
        }
        let complete = self.host.borrow().completed.clone();
        for index in complete {
            for branch in 0..2 {
                if index == self.indices[branch] {
                    if let Some(Err(e)) = &self.results[branch] {
                        return Poll::Ready(Err(e.clone()));
                    }
                }
            }
        }
        if self.results.iter().all(Option::is_some) {
            let a = self.results[0].take().unwrap();
            let b = self.results[1].take().unwrap();
            Poll::Ready(a.and_then(|a| b.map(|b| (a, b))))
        } else {
            Poll::Pending
        }
    }
}
fn all<'a>(h: &'a H, a: F<'a, V>, b: F<'a, V>) -> Result<F<'a, (V, V)>, Error> {
    record(h, "all")?;
    let n = h.borrow().tasks.len();
    Ok(Box::pin(All {
        host: h,
        a,
        b,
        results: [None, None],
        indices: [n - 2, n - 1],
    }))
}
fn provider(h: &H) -> Result<V, Error> {
    record(h, "provider")?;
    let mut s = h.borrow_mut();
    s.provider_index += 1;
    let flag = if s.provider_index == 1 {
        s.input["provider"].as_bool().unwrap()
    } else {
        s.input["providerSecond"]
            .as_bool()
            .unwrap_or(s.input["provider"].as_bool().unwrap())
    };
    Ok(if flag {
        object(vec![("tag", text("provider"))])
    } else {
        V::Null
    })
}
fn method(h: &H, receiver: &V, name: &'static str) -> Result<V, Error> {
    if matches!(receiver, V::Null | V::Undefined) {
        return get(h, receiver, name);
    }
    record(h, &format!("method:{name}"))?;
    Ok(text(name))
}
fn same_reference(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a, _), V::Object(b, _)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn call(h: &H, method: V, receiver: V, arg: V, callback: V) -> Result<V, Error> {
    let V::String(method) = method else {
        panic!("method")
    };
    let method = String::from_utf16(&method).unwrap();
    record(h, &format!("call:{method}"))?;
    let V::String(tag) = get_plain(&receiver, "tag") else {
        panic!("receiver")
    };
    let tag = String::from_utf16(&tag).unwrap();
    let s = h.borrow();
    let callback_same = same_reference(&callback, &s.callback);
    let argument_same = same_reference(&arg, &s.opts) || arg == s.symbol;
    drop(s);
    let snapshot = if matches!(arg, V::Object(..)) {
        copy_properties(h, arg)?
    } else {
        arg
    };
    h.borrow_mut().calls.push((
        tag.clone(),
        method.clone(),
        snapshot,
        callback_same,
        argument_same,
    ));
    Ok(object(vec![(
        "unsubscribe",
        text(&format!("{tag}:{method}")),
    )]))
}
fn error_value(e: &Error) -> V {
    let mut f = vec![("message", V::String(e.units.clone()))];
    if let Some(venue) = &e.venue {
        f.push((
            "response",
            object(vec![(
                "data",
                object(vec![("msg", V::String(venue.clone()))]),
            )]),
        ));
    }
    object(f)
}
fn runtime() -> RT {
    RT {
        request,
        all,
        get,
        optional_get: optional,
        truthy,
        is_undefined: |v| matches!(v, V::Undefined),
        is_nullish: |v| matches!(v, V::Undefined | V::Null),
        equals_text,
        equals: |a, b| a == b,
        number: V::Number,
        text,
        undefined: || V::Undefined,
        object,
        array,
        to_string,
        template_string: template,
        to_integer_string: |h, v| {
            if matches!(v, V::Null | V::Undefined) {
                Ok(text(""))
            } else {
                to_string(h, v)
            }
        },
        to_number: number,
        less_than: |h, a, b| Ok(number(h, a)? < number(h, b)?),
        greater_than: |h, a, b| Ok(number(h, a)? > number(h, b)?),
        map,
        filter,
        reduce,
        sort,
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        new_map,
        map_getter: |_, v| Ok(v.clone()),
        map_lookup,
        push_method: |_, v| Ok(v.clone()),
        push,
        markets: |h| {
            record(h, "markets")?;
            Ok(h.borrow().markets.clone())
        },
        tickers: |h| {
            record(h, "tickers")?;
            Ok(decode(&h.borrow().input["tickers"]))
        },
        markets_by_symbol: |h| {
            record(h, "marketsBySymbol")?;
            Ok(object(vec![("tag", text(h.borrow().market_version))]))
        },
        market_lookup: |h, captured, key| {
            let V::String(s) = to_string(h, &key)? else {
                panic!("key")
            };
            record(h, &format!("market:{}", serde_json::to_string(&s).unwrap()))?;
            let tag = if h.borrow().input["marketSymbolSequence"] == true {
                let V::String(prefix) = get_plain(&captured, "tag") else {
                    panic!("captured map")
                };
                let mut prefix = prefix;
                prefix.push(58);
                prefix.extend(s);
                V::String(prefix)
            } else {
                V::String(s)
            };
            Ok(object(vec![("tag", tag)]))
        },
        normalize_market,
        normalize_ticker,
        normalize_candle,
        normalize_order_book: normalize_book,
        normalize_public_trade: normalize_trade,
        normalize_execution,
        normalize_timestamp_ms: timestamp,
        interval: |h, v| {
            if matches!(v, V::Symbol(_)) {
                return Ok(V::Undefined);
            }
            let key = to_string(h, &v)?;
            Ok(safe_cex_venue_weex::types::INTERVAL
                .iter()
                .find(|(k, _)| equals_text(&key, k))
                .map(|(_, v)| text(v))
                .unwrap_or(V::Undefined))
        },
        supported_interval: |v| {
            safe_cex_venue_weex::types::SUPPORTED_INTERVALS
                .iter()
                .any(|s| equals_text(v, s))
        },
        copy_properties,
        define_property: define,
        extract_array: extract,
        omit_undefined: omit,
        public_only: |h| Ok(h.borrow().input["publicOnly"] == true),
        provider,
        public_websocket: |h| {
            record(h, "publicWebsocket")?;
            Ok(object(vec![("tag", text("websocket"))]))
        },
        method,
        call,
        error_value,
        error_sink: |_| Ok(V::Undefined),
        emit_error: |h, _, v| {
            record(h, "report")?;
            h.borrow_mut().reports.push(v);
            Ok(())
        },
        new_error: |_, units| Error {
            name: "Error",
            units,
            venue: None,
        },
    }
}
fn tick(h: &H) {
    let mut due = Vec::new();
    {
        let mut s = h.borrow_mut();
        for (i, t) in s.tasks.iter_mut().enumerate() {
            if t.done.is_none() {
                t.left -= 1;
                if t.left == 0 {
                    due.push(i);
                }
            }
        }
        if s.input["reverseDue"] == true {
            due.reverse();
        }
    }
    for index in due {
        let key = h.borrow().tasks[index].key.clone();
        let result = record(h, &format!("{key}:end"));
        let after = decode(&h.borrow().input["afterMarkets"]);
        let mut s = h.borrow_mut();
        if !matches!(after, V::Undefined) {
            s.markets = after;
        }
        s.tasks[index].done = Some(result.map(|_| s.tasks[index].value.clone()));
        s.completed.push(index);
    }
}
fn run(h: &H, mut f: F<'_, V>) -> Result<V, Error> {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut n = 0;
    let result = loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            break v;
        }
        tick(h);
        n += 1;
        assert!(n < 100, "native_scheduler_floor");
    };
    drop(f);
    while h.borrow().tasks.iter().any(|t| t.done.is_none()) {
        tick(h);
        n += 1;
        assert!(n < 100, "native_peer_floor");
    }
    result
}
fn decode_error(j: &J, input: &J) -> Option<Error> {
    if j.is_null() {
        None
    } else {
        let venue = decode(&input["venueMessage"]);
        Some(Error {
            name: match j["name"].as_str().unwrap() {
                "Error" => "Error",
                "RangeError" => "RangeError",
                "TypeError" => "TypeError",
                _ => panic!("error type"),
            },
            units: serde_json::from_value(j["units"].clone()).unwrap(),
            venue: if let V::String(v) = venue {
                Some(v)
            } else {
                None
            },
        })
    }
}
#[test]
fn native_market_reads_match_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/market-reads-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<J> = serde_json::from_slice(&output.stdout).unwrap();
    let expected_ids = [
        "markets_filter",
        "markets_missing",
        "markets_sparse",
        "markets_failure",
        "markets_factory_failure",
        "markets_normalizer_failure",
        "markets_report_failure",
        "markets_latest_fallback",
        "markets_species_failure",
        "tickers_join",
        "tickers_numeric_keys",
        "tickers_empty",
        "tickers_parallel",
        "tickers_failure",
        "tickers_dual_failure",
        "tickers_second_factory_failure",
        "tickers_data_get_failure",
        "tickers_rejection_before_data_get",
        "tickers_sparse_map_failure",
        "candles_recent",
        "candles_historical_zero",
        "candles_to_only",
        "candles_recent_upper",
        "candles_recent_zero",
        "candles_negative_limit",
        "candles_unsupported",
        "candles_unsupported_surrogate",
        "candles_symbol_interval",
        "candles_opts_repeated",
        "candles_non_array",
        "candles_failure",
        "listen_candle_provider",
        "listen_candle_websocket",
        "listen_book_provider",
        "listen_book_websocket",
        "listen_provider_changes",
        "listen_method_failure",
        "depth_default",
        "depth_null_limit",
        "depth_missing_id",
        "depth_mutated_id",
        "depth_failure",
        "public_trades",
        "public_limit_lower",
        "public_limit_upper",
        "public_limit_nan",
        "public_sparse",
        "public_non_array",
        "query_default",
        "query_filters",
        "query_zero_bounds",
        "query_created_time",
        "query_failure",
        "history_order",
        "history_public",
        "history_falsey_filter",
        "history_missing_symbol",
        "history_live_symbol",
        "markets_venue_message",
        "markets_empty_venue_message",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_ids,
        "native_fixture_ids"
    );
    let rt = runtime();
    let mut traces = Vec::new();
    for c in &cases {
        let input = c["input"].clone();
        let mut opts = decode(&input["opts"]);
        if let V::Object(_, label) = &mut opts {
            *label = "opts".into();
        }
        let symbol = decode(&input["symbol"]);
        let callback = object(vec![("callback", text("fixture"))]);
        let h = RefCell::new(State {
            input: input.clone(),
            opts: opts.clone(),
            symbol: symbol.clone(),
            callback: callback.clone(),
            markets: decode(&input["markets"]),
            events: vec![],
            requests: vec![],
            reports: vec![],
            calls: vec![],
            tasks: vec![],
            completed: vec![],
            counts: Default::default(),
            provider_index: 0,
            row_symbol_index: 0,
            market_version: "OLD",
        });
        let limit = decode(&input["limit"]);
        let f = match c["method"].as_str().unwrap() {
            "fetchMarkets" => fetch_markets(&h, &rt),
            "fetchTickers" => fetch_tickers(&h, &rt),
            "fetchOHLCV" => fetch_ohlcv(&h, &rt, opts),
            "listenOHLCV" => Box::pin(std::future::ready(listen_ohlcv(&h, &rt, opts, callback))),
            "listenOrderBook" => Box::pin(std::future::ready(listen_order_book(
                &h, &rt, symbol, callback,
            ))),
            "fetchOrderBook" => fetch_order_book(&h, &rt, symbol, limit),
            "fetchPublicTrades" => fetch_public_trades(&h, &rt, symbol, limit),
            "fetchUserTradeRows" => fetch_user_trade_rows(&h, &rt, opts),
            "fetchHistoricalFills" => fetch_historical_fills(&h, &rt, opts),
            _ => panic!("method"),
        };
        let prefix = h.borrow().events.clone();
        let result = run(&h, f);
        let id = c["id"].as_str().unwrap();
        assert_eq!(
            result.as_ref().err().cloned(),
            decode_error(&c["error"], &input),
            "native_error::{id}"
        );
        if let Ok(v) = &result {
            assert_eq!(*v, decode(&c["result"]), "native_result::{id}");
        }
        let s = h.borrow();
        let requests = c["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let endpoint = match r["endpoint"].as_str().unwrap() {
                    "/capi/v3/market/exchangeInfo" => "/capi/v3/market/exchangeInfo",
                    "/capi/v3/market/ticker/24hr" => "/capi/v3/market/ticker/24hr",
                    "/capi/v3/market/ticker/bookTicker" => "/capi/v3/market/ticker/bookTicker",
                    "/capi/v3/market/klines" => "/capi/v3/market/klines",
                    "/capi/v3/market/historyKlines" => "/capi/v3/market/historyKlines",
                    "/capi/v3/market/depth" => "/capi/v3/market/depth",
                    "/capi/v3/market/trades" => "/capi/v3/market/trades",
                    "/capi/v3/userTrades" => "/capi/v3/userTrades",
                    _ => panic!("endpoint"),
                };
                let params = if r["params"].is_null() {
                    None
                } else {
                    Some(decode(&r["params"]))
                };
                let timeout = if r["timeout"].is_null() {
                    None
                } else {
                    let V::Number(n) = decode(&r["timeout"]) else {
                        panic!("timeout")
                    };
                    Some(n.to_bits())
                };
                (endpoint, params, timeout)
            })
            .collect::<Vec<_>>();
        assert_eq!(s.requests, requests, "native_requests::{id}");
        assert_eq!(
            s.reports,
            c["reports"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>(),
            "native_reports::{id}"
        );
        let calls = c["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| {
                (
                    j["receiver"].as_str().unwrap().to_owned(),
                    j["method"].as_str().unwrap().to_owned(),
                    decode(&j["argument"]),
                    j["callbackSame"].as_bool().unwrap(),
                    j["argumentSame"].as_bool().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(s.calls, calls, "native_calls::{id}");
        assert_eq!(
            s.tasks.iter().filter(|t| t.done.is_none()).count(),
            0,
            "native_peers_complete::{id}"
        );
        if id == "query_filters" {
            let V::Array(a) = result.as_ref().unwrap() else {
                panic!("query")
            };
            assert_eq!(a.borrow().len(), 2, "native_query_boundary_floor");
        }
        if id == "candles_recent" {
            let V::Array(a) = result.as_ref().unwrap() else {
                panic!("candles")
            };
            assert_eq!(a.borrow().len(), 3, "native_candle_filter_floor");
        }
        traces.push((
            id.to_owned(),
            prefix,
            s.events.clone(),
            c["prefix"].clone(),
            c["events"].clone(),
        ));
    }
    for (id, prefix, events, want_prefix, want_events) in traces {
        assert_eq!(
            prefix,
            serde_json::from_value::<Vec<String>>(want_prefix).unwrap(),
            "source_eager_effects::{id}"
        );
        assert_eq!(
            events,
            serde_json::from_value::<Vec<String>>(want_events).unwrap(),
            "source_effects::{id}"
        );
    }
}
