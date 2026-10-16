use safe_cex_venue_orderly::{
    account_order_reads::AccountReadHost,
    data_mappers::{self as data, DataHost, Op},
    ws_controls::WsHost,
};
use serde_json::Value;
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
    Function(&'static str),
    Promise(Promise),
}
type V = Rc<Raw>;
type E = Rc<&'static str>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    Rc::new(Raw::Text(units(s)))
}
fn array(a: Vec<V>) -> V {
    Rc::new(Raw::Array(RefCell::new(a)))
}
fn object(a: Vec<(&str, V)>) -> V {
    Rc::new(Raw::Object(RefCell::new(
        a.into_iter().map(|(k, v)| (units(k), v)).collect(),
    )))
}
fn decode(v: &Value) -> V {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(b) => Raw::Bool(*b),
        Value::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Value::String(s) => Raw::Text(units(s)),
        Value::Array(a) => Raw::Array(RefCell::new(a.iter().map(decode).collect())),
        Value::Object(o) => {
            if o.get("undefined") == Some(&Value::Bool(true)) {
                Raw::Undefined
            } else if let Some(n) = o.get("number") {
                Raw::Number(f64::from_bits(
                    u64::from_str_radix(n.as_str().unwrap(), 16).unwrap(),
                ))
            } else {
                Raw::Object(RefCell::new(
                    o.iter().map(|(k, v)| (units(k), decode(v))).collect(),
                ))
            }
        }
    })
}
fn get(v: &Raw, key: &str) -> V {
    if let Raw::Object(o) = v {
        o.borrow()
            .iter()
            .find(|(k, _)| *k == units(key))
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined))
    } else {
        Rc::new(Raw::Undefined)
    }
}
fn set(v: &V, key: &str, next: V) {
    let Raw::Object(o) = &**v else {
        panic!("ordinary fixture object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, value)) = o.iter_mut().find(|(k, _)| *k == units(key)) {
        *value = next
    } else {
        o.push((units(key), next))
    }
}
fn boolean(b: bool) -> V {
    Rc::new(Raw::Bool(b))
}
fn flag(v: &Raw, key: &str) -> bool {
    matches!(&*get(v, key), Raw::Bool(true))
}
fn items(v: &Raw) -> Vec<V> {
    let Raw::Array(a) = v else {
        panic!("fixture array")
    };
    a.borrow().clone()
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::Undefined => units("undefined"),
        Raw::Null => units("null"),
        Raw::Bool(b) => units(if *b { "true" } else { "false" }),
        Raw::Number(n) => units(&safe_cex_num::js_number_string(*n)),
        Raw::Text(s) => s.clone(),
        Raw::Object(_) => units("[object Object]"),
        Raw::Array(a) => a
            .borrow()
            .iter()
            .map(|v| String::from_utf16(&string(v)).unwrap())
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
        Raw::Function(_) | Raw::Promise(_) => panic!("outside fixture coercion floor"),
    }
}
fn ordered(v: &Raw) -> Vec<(Vec<u16>, V)> {
    let Raw::Object(o) = v else {
        panic!("fixture own entries")
    };
    let mut a = o.borrow().clone();
    a.sort_by_key(|(k, _)| {
        let s = String::from_utf16(k).unwrap();
        let n = s
            .parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == s);
        (n.is_none(), n.unwrap_or(0))
    });
    a
}
fn native(v: &Raw, expected: &Value, path: &str) {
    match v {
        Raw::Undefined => assert_eq!(
            expected.get("undefined"),
            Some(&Value::Bool(true)),
            "native undefined {path}"
        ),
        Raw::Null => assert!(expected.is_null(), "native null {path}"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "native boolean {path}"),
        Raw::Number(n) => {
            let bits = u64::from_str_radix(expected["number"].as_str().unwrap(), 16).unwrap();
            if n.is_nan() {
                assert!(f64::from_bits(bits).is_nan(), "native NaN {path}")
            } else {
                assert_eq!(n.to_bits(), bits, "native bits {path}")
            }
        }
        Raw::Text(s) => {
            assert!(expected.is_string(), "native text kind {path}");
            assert_eq!(
                s,
                &units(expected.as_str().unwrap()),
                "native UTF-16 {path}"
            );
        }
        Raw::Array(a) => {
            assert!(expected.is_array(), "native array kind {path}");
            let a = a.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "native array cardinality {path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{path}[{i}]"))
            }
        }
        Raw::Object(o) => {
            assert!(expected.is_object(), "native object kind {path}");
            let o = o.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(o.len(), e.len(), "native object cardinality {path}");
            for (k, v) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                assert!(e.contains_key(&k), "native object key {path}.{k}");
                native(v, &e[&k], &format!("{path}.{k}"))
            }
        }
        Raw::Function(_) | Raw::Promise(_) => panic!("no callback encoding assertion"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Function(f) => Raw::Function(f),
        Raw::Promise(_) => panic!("no promise trace snapshot"),
        Raw::Array(a) => Raw::Array(RefCell::new(a.borrow().iter().map(snapshot).collect())),
        Raw::Object(o) => Raw::Object(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        )),
    })
}
#[derive(Clone, Copy, Debug)]
enum Promise {
    Request,
    Mode,
}
struct Host {
    cfg: V,
    receiver: V,
    store: V,
    xhr: V,
    socket: V,
    callback: V,
    cleanup: V,
    original: E,
    events: RefCell<Vec<V>>,
}
impl Host {
    fn effect(&self, kind: &str, fields: Vec<(&str, V)>) -> Result<(), E> {
        self.events.borrow_mut().push(object(
            vec![("kind", text(kind))]
                .into_iter()
                .chain(fields.into_iter().map(|(k, v)| (k, snapshot(&v))))
                .collect(),
        ));
        if string(&get(&self.cfg, "fail")) == units(kind) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn num(&self, v: &Raw) -> f64 {
        match v {
            Raw::Number(n) => *n,
            Raw::Null => 0.0,
            Raw::Bool(b) => u8::from(*b) as f64,
            Raw::Text(t) => safe_cex_num::number_from_string(&String::from_utf16(t).unwrap()),
            _ => f64::NAN,
        }
    }
    fn run<'a>(&'a self, name: &str) -> Pin<Box<dyn Future<Output = Result<V, E>> + 'a>> {
        let r = self.receiver.clone();
        match name {
            "fetchMarkets" => Box::pin(data::markets(self, r)),
            "fetchTickers" => Box::pin(data::tickers(self, r)),
            "fetchPositions" => Box::pin(data::positions(self, r)),
            "fetchOHLCV" => Box::pin(data::candles(self, r, get(&self.cfg, "options"))),
            "mapPosition" => Box::pin(async move {
                data::map_position(self, r, get(&self.cfg, "position"), get(&self.cfg, "mode"))
            }),
            "mapLimitOrder" => {
                Box::pin(async move { data::map_limit(self, r, get(&self.cfg, "limit")) })
            }
            "mapAlgoOrder" => {
                Box::pin(async move { data::map_algo(self, r, get(&self.cfg, "algo")) })
            }
            "listenOHLCV" => Box::pin(async move {
                data::listen_candles(self, r, get(&self.cfg, "options"), self.callback.clone())
            }),
            "listenOrderBook" => Box::pin(async move {
                data::listen_book(self, r, get(&self.cfg, "symbol"), self.callback.clone())
            }),
            _ => panic!(),
        }
    }
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, k: &[u16]) -> Result<V, E> {
        Ok(get(v, &String::from_utf16(k).unwrap()))
    }
    fn set(&self, v: &V, k: &[u16], next: V) -> Result<(), E> {
        set(v, &String::from_utf16(k).unwrap(), next);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, E> {
        Ok(string(v))
    }
    fn text(&self, s: &[u16]) -> V {
        Rc::new(Raw::Text(s.to_vec()))
    }
    fn object(&self, e: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(e))
    }
    fn stringify(&self, _: V) -> Result<V, E> {
        panic!("outside data floor")
    }
    fn parse(&self, _: V) -> Result<V, E> {
        panic!("outside data floor")
    }
    fn includes(&self, _: &V, _: &[u16]) -> Result<bool, E> {
        panic!("outside data floor")
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside data floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside data floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let a = items(&i.0);
        let v = a.get(i.1).cloned();
        i.1 += 1;
        assert!(i.1 <= 16, "finite iterator floor");
        Ok(v)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("callable floor")
        };
        let is = |v: &V| receiver.as_ref().is_some_and(|r| Rc::ptr_eq(r, v));
        match *name {
            "get" => {
                self.effect(
                    "request",
                    vec![
                        ("receiver", boolean(is(&self.xhr))),
                        ("url", args[0].clone()),
                        ("config", args.get(1).cloned().unwrap_or(self.undefined())),
                    ],
                )?;
                if flag(&self.cfg, "syncFail") {
                    return Err(self.original.clone());
                }
                return Ok(Rc::new(Raw::Promise(Promise::Request)));
            }
            "mapMarginType" => {
                self.effect("margin", vec![("raw", args[0].clone())])?;
                return Ok(text(
                    if matches!(&*args[0],Raw::Text(t) if String::from_utf16(t).unwrap().to_uppercase()=="ISOLATED")
                    {
                        "isolated"
                    } else {
                        "cross"
                    },
                ));
            }
            "getCurrentMarginType" => {
                self.effect("currentMargin", vec![("symbol", args[0].clone())])?;
                return Ok(get(&self.cfg, "margin"));
            }
            "createOrderlyPositionId" => {
                self.effect(
                    "id",
                    vec![
                        ("symbol", args[0].clone()),
                        ("side", args[1].clone()),
                        ("mode", args[2].clone()),
                        ("margin", args[3].clone()),
                    ],
                )?;
                return Ok(text(
                    &args
                        .iter()
                        .map(|v| String::from_utf16(&string(v)).unwrap())
                        .collect::<Vec<_>>()
                        .join(":"),
                ));
            }
            "getPositionMode" => {
                self.effect("mode", vec![])?;
                return Ok(Rc::new(Raw::Promise(Promise::Mode)));
            }
            "mapPosition" => {
                self.effect(
                    "mapPosition",
                    vec![
                        (
                            "rowIdentity",
                            boolean(
                                items(&get(&self.cfg, "rows"))
                                    .iter()
                                    .any(|p| Rc::ptr_eq(p, &args[0])),
                            ),
                        ),
                        (
                            "modeIdentity",
                            boolean(Rc::ptr_eq(&args[1], &get(&self.cfg, "mode"))),
                        ),
                    ],
                )?;
                return if flag(&self.cfg, "integrate") {
                    data::map_position(
                        self,
                        self.receiver.clone(),
                        args[0].clone(),
                        args[1].clone(),
                    )
                } else if self.is_null(&get(&self.cfg, "mapped")) {
                    Ok(self.null())
                } else {
                    Ok(object(vec![
                        (
                            "id",
                            text(&format!(
                                "BTCUSDC:long:{}:cross",
                                String::from_utf16(&string(&args[1])).unwrap()
                            )),
                        ),
                        ("opaque", text("position")),
                    ]))
                };
            }
            "emitAndLogError" => self.effect("log", vec![("message", args[0].clone())])?,
            "update" => {
                self.effect(
                    "update",
                    vec![
                        ("receiver", boolean(is(&self.store))),
                        ("patch", args[0].clone()),
                    ],
                )?;
                for (k, v) in ordered(&args[0]) {
                    self.set(&self.store, &k, v)?;
                }
            }
            "listenOHLCV" | "listenOrderBook" => {
                let original = get(
                    &self.cfg,
                    if *name == "listenOHLCV" {
                        "options"
                    } else {
                        "symbol"
                    },
                );
                self.effect(
                    "delegate",
                    vec![
                        ("receiver", boolean(is(&self.socket))),
                        ("argumentIdentity", boolean(Rc::ptr_eq(&args[0], &original))),
                        (
                            "callbackIdentity",
                            boolean(Rc::ptr_eq(&args[1], &self.callback)),
                        ),
                    ],
                )?;
                return Ok(self.cleanup.clone());
            }
            _ => panic!("data callable {name}"),
        };
        Ok(self.undefined())
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside data floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        Ok(self.number(1723638896125.5))
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside data floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
#[allow(clippy::manual_async_fn)]
impl AccountReadHost for Host {
    fn await_value(&self, v: V) -> impl Future<Output = Result<V, E>> {
        async move {
            let Raw::Promise(kind) = &*v else { panic!() };
            match kind {
                Promise::Mode => Ok(get(&self.cfg, "mode")),
                Promise::Request => {
                    yield_once().await;
                    self.effect("await", vec![])?;
                    let change = get(&self.cfg, "afterAwait");
                    for name in ["marketsBySymbol", "balance"] {
                        let value = get(&change, name);
                        if self.truthy(&value) {
                            set(&self.store, name, value)
                        }
                    }
                    let leverage = get(&change, "leverage");
                    if self.truthy(&leverage) {
                        set(&self.receiver, "accountLeverage", leverage)
                    }
                    if flag(&self.cfg, "reject") {
                        return Err(self.original.clone());
                    }
                    Ok(object(vec![(
                        "data",
                        object(vec![(
                            "data",
                            object(vec![
                                ("rows", get(&self.cfg, "rows")),
                                ("total_collateral_value", get(&self.cfg, "total")),
                                ("free_collateral", get(&self.cfg, "free")),
                            ]),
                        )]),
                    )]))
                }
            }
        }
    }
    fn error_value(&self, e: &E) -> V {
        assert!(
            Rc::ptr_eq(e, &self.original),
            "native original error on catch"
        );
        get(&self.cfg, "error")
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined | Raw::Null)
    }
    fn is_null(&self, v: &Raw) -> bool {
        matches!(v, Raw::Null)
    }
    fn is_string(&self, v: &Raw) -> bool {
        matches!(v, Raw::Text(_))
    }
    fn text_eq(&self, v: &Raw, s: &str) -> bool {
        matches!(v,Raw::Text(t) if *t==units(s))
    }
    fn undefined(&self) -> V {
        Rc::new(Raw::Undefined)
    }
    fn null(&self) -> V {
        Rc::new(Raw::Null)
    }
    fn number(&self, n: f64) -> V {
        Rc::new(Raw::Number(n))
    }
    fn array(&self, a: Vec<V>) -> V {
        array(a)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn reduce(&self, rows: V, mut acc: V, mut f: impl FnMut(V, V) -> Result<V, E>) -> Result<V, E> {
        for row in items(&rows) {
            acc = f(acc, row)?
        }
        Ok(acc)
    }
    fn normalize_symbol(&self, v: V) -> Result<V, E> {
        let parts = self.split(v)?;
        let base = parts.get(1).cloned().unwrap_or(self.undefined());
        let quote = parts.get(2).cloned().unwrap_or(self.undefined());
        Ok(self.text(&[string(&base), string(&quote)].concat()))
    }
    fn get_key(&self, v: V, k: &str) -> Result<V, E> {
        if self.nullish(&v) {
            return Ok(self.undefined());
        }
        let entries = ordered(&v);
        for name in [k.to_string(), snake(k), camel(k)] {
            if let Some((_, v)) = entries.iter().find(|(key, _)| *key == units(&name)) {
                return Ok(v.clone());
            }
        }
        Ok(self.undefined())
    }
}
fn snake(key: &str) -> String {
    let mut s = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            s.push('_');
            s.push(c.to_ascii_lowercase())
        } else {
            s.push(c)
        }
    }
    s
}
fn camel(key: &str) -> String {
    key.split('_')
        .enumerate()
        .map(|(i, s)| {
            if i == 0 {
                s.to_string()
            } else {
                let mut c = s.chars();
                c.next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()
                    + c.as_str()
            }
        })
        .collect()
}
impl DataHost for Host {
    fn numeric(&self, v: V) -> Result<f64, E> {
        Ok(self.num(&v))
    }
    fn arithmetic(&self, op: Op, a: V, b: V) -> Result<V, E> {
        let (a, b) = (self.num(&a), self.num(&b));
        Ok(self.number(match op {
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => a / b,
            Op::Min => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }
            }
            Op::Max => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }
            }
        }))
    }
    fn compare(&self, a: V, b: V, less: bool) -> Result<bool, E> {
        Ok(if less {
            self.num(&a) < self.num(&b)
        } else {
            self.num(&a) > self.num(&b)
        })
    }
    fn round(&self, v: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::math_round(self.num(&v))))
    }
    fn split(&self, v: V) -> Result<Vec<V>, E> {
        let Raw::Text(t) = &*v else {
            panic!("string split floor")
        };
        Ok(t.split(|c| *c == b'_' as u16)
            .map(|s| self.text(s))
            .collect())
    }
    fn starts_with(&self, v: V, p: &str) -> Result<bool, E> {
        let Raw::Text(t) = &*v else { panic!() };
        Ok(t.starts_with(&units(p)))
    }
    fn filter(&self, rows: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        let mut a = vec![];
        for row in items(&rows) {
            if f(row.clone())? {
                a.push(row)
            }
        }
        Ok(array(a))
    }
    fn map(&self, rows: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        let mut a = vec![];
        for row in items(&rows) {
            a.push(f(row)?)
        }
        Ok(array(a))
    }
    fn clone_deep(&self, v: V) -> Result<V, E> {
        self.effect("clone", vec![])?;
        let cloned = snapshot(&v);
        assert!(!Rc::ptr_eq(&v, &cloned), "native clone distinct root");
        Ok(cloned)
    }
    fn merge(&self, v: V, fields: Vec<(&'static str, V)>) -> Result<V, E> {
        let out = Rc::new(Raw::Object(RefCell::new(ordered(&v))));
        for (k, v) in fields {
            set(&out, k, v)
        }
        Ok(out)
    }
    fn uniq_id(&self, v: V) -> Result<V, E> {
        let mut ids = vec![];
        let mut out = vec![];
        for row in items(&v) {
            let id = string(&get(&row, "id"));
            if !ids.contains(&id) {
                ids.push(id);
                out.push(row)
            }
        }
        Ok(array(out))
    }
    fn reverse(&self, v: V) -> Result<V, E> {
        let Raw::Array(a) = &*v else { panic!() };
        a.borrow_mut().reverse();
        Ok(v)
    }
    fn push(&self, v: V, values: Vec<V>) -> Result<(), E> {
        let Raw::Array(a) = &*v else { panic!() };
        a.borrow_mut().extend(values);
        Ok(())
    }
    fn reverse_symbol(&self, v: V) -> Result<V, E> {
        Ok(text(&format!(
            "PERP_{}_USDC",
            String::from_utf16(&string(&v))
                .unwrap()
                .replacen("USDC", "", 1)
        )))
    }
    fn order_type(&self, v: V) -> Result<V, E> {
        Ok(match String::from_utf16(&string(&v)).unwrap().as_str() {
            "LIMIT" => text("limit"),
            "MARKET" => text("market"),
            "STOP_LOSS" => text("stop_market"),
            "TAKE_PROFIT" => text("take_profit_market"),
            "TRAILING_STOP" => text("trailing_stop_market"),
            _ => self.undefined(),
        })
    }
    fn order_side(&self, v: V) -> Result<V, E> {
        Ok(if self.text_eq(&v, "BUY") {
            text("buy")
        } else if self.text_eq(&v, "SELL") {
            text("sell")
        } else {
            self.undefined()
        })
    }
    fn subtract(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::subtract(self.num(&a), self.num(&b))))
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined)
    }
    fn boolean(&self, b: bool) -> V {
        boolean(b)
    }
}
async fn yield_once() {
    let mut first = true;
    std::future::poll_fn(move |_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let store = object(vec![]);
    for (key, name) in [
        ("markets", "markets"),
        ("marketsBySymbol", "marketsBySymbol"),
        ("tickersBySymbol", "tickersBySymbol"),
        ("tickers", "cachedTickers"),
        ("positions", "cachedPositions"),
        ("balance", "balance"),
    ] {
        set(&store, key, get(&cfg, name));
    }
    set(&store, "update", Rc::new(Raw::Function("update")));
    let xhr = object(vec![("get", Rc::new(Raw::Function("get")))]);
    let socket = object(vec![
        ("listenOHLCV", Rc::new(Raw::Function("listenOHLCV"))),
        ("listenOrderBook", Rc::new(Raw::Function("listenOrderBook"))),
    ]);
    let receiver = object(vec![
        ("store", store.clone()),
        ("xhr", xhr.clone()),
        ("publicWebsocket", socket.clone()),
        ("accountLeverage", get(&cfg, "leverage")),
    ]);
    for name in [
        "mapMarginType",
        "getCurrentMarginType",
        "createOrderlyPositionId",
        "getPositionMode",
        "mapPosition",
        "emitAndLogError",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    Host {
        cfg,
        receiver,
        store,
        xhr,
        socket,
        callback: object(vec![("token", text("callback"))]),
        cleanup: object(vec![("token", text("cleanup"))]),
        original: Rc::new("original data error"),
        events: RefCell::new(vec![]),
    }
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/data-mappers-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["hashes"].as_object().unwrap().len(),
            6,
            "independent source file floor"
        );
        v
    })
}
fn cloned_info(info: &V, original: &V) -> bool {
    !Rc::ptr_eq(info, original)
        && ordered(original).iter().all(|(k, v)| {
            !matches!(&**v, Raw::Object(_) | Raw::Array(_))
                || !Rc::ptr_eq(&get(info, &String::from_utf16(k).unwrap()), v)
        })
}
fn check(name: &str, floor: usize) {
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent data case floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        let mut future = h.run(name);
        let mut cx = Context::from_waker(Waker::noop());
        let mut result = None;
        for _ in 0..24 {
            if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
                result = Some(v);
                break;
            }
        }
        match result.expect("finite continuation floor") {
            Ok(v) => {
                assert!(row["error"].is_null(), "source error {row}");
                let identity = if name.starts_with("listen") {
                    Rc::ptr_eq(&v, &h.cleanup)
                } else if name == "mapLimitOrder" && !h.is_null(&v) {
                    Rc::ptr_eq(&get(&v, "info"), &get(&h.cfg, "limit"))
                } else if name == "fetchTickers" {
                    Rc::ptr_eq(&v, &get(&h.store, "tickers"))
                        || items(&v).iter().all(|t| {
                            items(&get(&h.cfg, "rows"))
                                .iter()
                                .any(|r| Rc::ptr_eq(r, &get(t, "info")))
                        })
                } else if name == "fetchPositions"
                    && (flag(&h.cfg, "reject")
                        || flag(&h.cfg, "syncFail")
                        || matches!(&*get(&h.cfg, "fail"), Raw::Text(_)))
                {
                    Rc::ptr_eq(&v, &get(&h.store, "positions"))
                } else if name == "fetchMarkets" {
                    let originals = items(&get(&h.cfg, "rows"));
                    items(&v)
                        .iter()
                        .all(|m| originals.iter().all(|r| cloned_info(&get(m, "info"), r)))
                } else if name == "mapPosition" && !h.is_null(&v) {
                    cloned_info(&get(&v, "info"), &get(&h.cfg, "position"))
                } else if name == "mapAlgoOrder" {
                    let o = get(&h.cfg, "algo");
                    let child = get(&o, "childOrders");
                    let child = if h.truthy(&child) {
                        child
                    } else {
                        get(&o, "child_orders")
                    };
                    let originals = if h.truthy(&child) {
                        items(&child)
                    } else {
                        vec![o]
                    };
                    items(&v)
                        .iter()
                        .all(|m| originals.iter().all(|r| cloned_info(&get(m, "info"), r)))
                } else {
                    true
                };
                assert_eq!(
                    identity,
                    row["outputIdentity"].as_bool().unwrap(),
                    "native original output handle"
                );
                native(&v, &row["result"], &format!("{name}[{i}] result"));
            }
            Err(e) => {
                assert!(
                    Rc::ptr_eq(&e, &h.original),
                    "native original rejection allocation"
                );
                assert_eq!(row["error"]["identity"], true);
                native(
                    &get(&h.error_value(&e), "message"),
                    &row["error"]["message"],
                    "native error metadata",
                );
            }
        }
        native(
            &Raw::Array(RefCell::new(h.events.borrow().clone())),
            &row["events"],
            &format!("{name}[{i}] effects"),
        );
        native(
            &object(vec![("balance", get(&h.store, "balance"))]),
            &row["state"],
            "native data state",
        );
    }
}
#[test]
fn markets_native() {
    check("fetchMarkets", 7)
}
#[test]
fn tickers_native() {
    check("fetchTickers", 8)
}
#[test]
fn positions_native() {
    check("fetchPositions", 11)
}
#[test]
fn position_mapper_native() {
    check("mapPosition", 11)
}
#[test]
fn limit_mapper_native() {
    check("mapLimitOrder", 7)
}
#[test]
fn algo_mapper_native() {
    check("mapAlgoOrder", 10)
}
#[test]
fn candles_native() {
    check("fetchOHLCV", 6)
}
#[test]
fn listen_candles_native() {
    check("listenOHLCV", 3)
}
#[test]
fn listen_book_native() {
    check("listenOrderBook", 3)
}
