use safe_cex_num::{
    TableHit, ccxt_precision::precision_to_string, js_number_string, number_from_string,
    parse_float, parse_int,
};
use safe_cex_venue_kraken::rest_account::{Entries, Handle, ReadHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(Vec<Rc<Raw>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
}
#[derive(Debug)]
struct Error {
    message: Rc<Raw>,
    original: bool,
    response: Option<Rc<Raw>>,
    cause: Option<E>,
    name: &'static str,
}
type E = Rc<Error>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> Rc<Raw> {
    Rc::new(Raw::Text(units(s)))
}
fn decode(v: &Value) -> Rc<Raw> {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(b) => Raw::Bool(*b),
        Value::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Value::String(s) => Raw::Text(units(s)),
        Value::Array(a) => Raw::Array(a.iter().map(decode).collect()),
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
fn get(v: &Raw, key: &str) -> Rc<Raw> {
    match v {
        Raw::Object(o) => o
            .borrow()
            .iter()
            .find(|(k, _)| *k == units(key))
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined)),
        _ => Rc::new(Raw::Undefined),
    }
}
fn flag(v: &Raw, key: &str) -> bool {
    matches!(&*get(v, key), Raw::Bool(true))
}
fn to_value(v: &Raw) -> Value {
    match v {
        Raw::Undefined | Raw::Null => Value::Null,
        Raw::Bool(b) => Value::Bool(*b),
        Raw::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Raw::Text(s) => Value::String(String::from_utf16(s).unwrap()),
        Raw::Array(a) => Value::Array(a.iter().map(|v| to_value(v)).collect()),
        Raw::Object(o) => Value::Object(
            o.borrow()
                .iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .map(|(k, v)| (String::from_utf16(k).unwrap(), to_value(v)))
                .collect(),
        ),
    }
}
struct Host {
    cfg: Rc<Raw>,
    store: Rc<Raw>,
    events: RefCell<Vec<Rc<Raw>>>,
    margin: RefCell<Vec<(Vec<u16>, Rc<Raw>)>>,
    leverage: RefCell<Vec<(Vec<u16>, Rc<Raw>)>>,
    calls: Cell<usize>,
    original: E,
    last_data: RefCell<Option<Rc<Raw>>>,
    disposed: Cell<bool>,
}
impl Host {
    fn new(cfg: &Value) -> Self {
        let cfg = decode(cfg);
        let store = get(&cfg, "store");
        let entries = |k| match &*get(&cfg, k) {
            Raw::Object(o) => o.borrow().clone(),
            _ => vec![],
        };
        Self {
            store,
            events: RefCell::new(vec![]),
            margin: RefCell::new(entries("margin")),
            leverage: RefCell::new(entries("leverage")),
            calls: Cell::new(0),
            original: Rc::new(Error {
                message: if matches!(&*get(&cfg, "errorMessage"), Raw::Undefined) {
                    text("read failure")
                } else {
                    get(&cfg, "errorMessage")
                },
                original: true,
                response: if matches!(&*get(&cfg, "errorResponse"), Raw::Undefined) {
                    None
                } else {
                    Some(get(&cfg, "errorResponse"))
                },
                cause: None,
                name: "Error",
            }),
            last_data: RefCell::new(None),
            disposed: Cell::new(false),
            cfg: cfg.clone(),
        }
    }
    fn event(&self, pairs: Vec<(&str, Rc<Raw>)>) {
        self.events.borrow_mut().push(
            self.object(pairs.into_iter().map(|(k, v)| (units(k), v)).collect())
                .unwrap(),
        );
    }
    fn after(&self) {
        if let Raw::Object(o) = &*get(&self.cfg, "after") {
            for (k, v) in o.borrow().iter() {
                self.assign(&self.store, k, v.clone()).unwrap();
            }
        }
        self.disposed.set(flag(&self.cfg, "disposed"));
    }
    fn error_text(&self, s: String) -> E {
        Rc::new(Error {
            message: text(&s),
            original: false,
            response: None,
            cause: None,
            name: "TypeError",
        })
    }
}
impl ReadHost for Host {
    type Value = Raw;
    type Error = E;
    fn field(&self, v: &Raw, key: &[u16], optional: bool) -> Result<Rc<Raw>, E> {
        if matches!(v, Raw::Undefined | Raw::Null) {
            if optional {
                return Ok(Rc::new(Raw::Undefined));
            }
            return Err(self.error_text(format!(
                "Cannot read properties of {} (reading '{}')",
                if matches!(v, Raw::Null) {
                    "null"
                } else {
                    "undefined"
                },
                String::from_utf16(key).unwrap()
            )));
        }
        if key == units("length") {
            if let Raw::Array(a) = v {
                return Ok(self.number(a.len() as f64));
            }
            if let Raw::Text(s) = v {
                return Ok(self.number(s.len() as f64));
            }
        }
        Ok(match v {
            Raw::Object(o) => o
                .borrow()
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(Rc::new(Raw::Undefined)),
            Raw::Array(a) => String::from_utf16(key)
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .and_then(|i| a.get(i))
                .cloned()
                .unwrap_or(Rc::new(Raw::Undefined)),
            _ => Rc::new(Raw::Undefined),
        })
    }
    fn keyed(&self, v: &Raw, k: &str) -> Result<Rc<Raw>, E> {
        if let Raw::Object(o) = v {
            for name in [
                k.to_owned(),
                safe_cex_num::lodash_snake_case(k),
                safe_cex_num::lodash_camel_case(k),
            ] {
                if let Some((_, v)) = o.borrow().iter().find(|(key, _)| *key == units(&name)) {
                    return Ok(v.clone());
                }
            }
        }
        self.field(v, &units(k), true)
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
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined | Raw::Null)
    }
    fn undefined(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined)
    }
    fn strict_eq(&self, a: &Raw, b: &Raw) -> bool {
        match (a, b) {
            (Raw::Undefined, Raw::Undefined) | (Raw::Null, Raw::Null) => true,
            (Raw::Number(a), Raw::Number(b)) => a == b,
            (Raw::Text(a), Raw::Text(b)) => a == b,
            (Raw::Bool(a), Raw::Bool(b)) => a == b,
            _ => std::ptr::eq(a, b),
        }
    }
    fn number(&self, n: f64) -> Rc<Raw> {
        Rc::new(Raw::Number(n))
    }
    fn boolean(&self, b: bool) -> Rc<Raw> {
        Rc::new(Raw::Bool(b))
    }
    fn text(&self, t: &[u16]) -> Rc<Raw> {
        Rc::new(Raw::Text(t.into()))
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, E> {
        Ok(match v {
            Raw::Undefined => units("undefined"),
            Raw::Null => units("null"),
            Raw::Bool(b) => units(if *b { "true" } else { "false" }),
            Raw::Number(n) => units(&js_number_string(*n)),
            Raw::Text(t) => t.clone(),
            Raw::Object(_) => units("[object Object]"),
            Raw::Array(a) => {
                let mut out = vec![];
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(44);
                    }
                    if !self.nullish(v) {
                        out.extend(self.string(v)?);
                    }
                }
                out
            }
        })
    }
    fn lower(&self, s: &[u16]) -> Vec<u16> {
        units(&String::from_utf16(s).unwrap().to_lowercase())
    }
    fn upper(&self, s: &[u16]) -> Vec<u16> {
        units(&String::from_utf16(s).unwrap().to_uppercase())
    }
    fn starts_with(&self, v: &Raw, p: &str) -> Result<bool, E> {
        match v {
            Raw::Text(s) => Ok(s.starts_with(&units(p))),
            _ => Err(self.error_text("symbol.startsWith is not a function".into())),
        }
    }
    fn split_pair(&self, v: &Raw) -> Result<(Rc<Raw>, Rc<Raw>), E> {
        match v {
            Raw::Text(s) => {
                let mut parts = s.split(|c| *c == 47);
                Ok((
                    self.text(parts.next().unwrap()),
                    parts
                        .next()
                        .map(|s| self.text(s))
                        .unwrap_or(Rc::new(Raw::Undefined)),
                ))
            }
            _ => Err(self.error_text("wsname.split is not a function".into())),
        }
    }
    fn normalize_asset(&self, v: &Raw) -> Result<Rc<Raw>, E> {
        if matches!(v, Raw::Undefined) {
            return Ok(Rc::new(Raw::Undefined));
        }
        let s = self.string(v)?;
        match safe_cex_venue_kraken::spot_types::normalize_asset(&s) {
            TableHit::Mapped(s) => Ok(self.text(&s)),
            TableHit::Inherited(_) => {
                Err(self.error_text("fixture outside intrinsic floor".into()))
            }
            TableHit::Missing => unreachable!(),
        }
    }
    fn order_type(&self, v: &Raw) -> Result<Rc<Raw>, E> {
        let s = String::from_utf16(&self.string(v)?).unwrap();
        let t = safe_cex_venue_kraken::spot_types::ORDER_TYPE
            .iter()
            .find(|(k, _)| *k == s)
            .map(|(_, v)| *v)
            .unwrap_or(safe_cex_types::OrderType::Limit);
        Ok(decode(&serde_json::to_value(t).unwrap()))
    }
    fn parse_float(&self, v: &Raw) -> Result<f64, E> {
        Ok(parse_float(&String::from_utf16(&self.string(v)?).unwrap()))
    }
    fn numeric(&self, v: &Raw) -> Result<f64, E> {
        Ok(match v {
            Raw::Undefined => f64::NAN,
            Raw::Null => 0.0,
            Raw::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Raw::Number(n) => *n,
            _ => number_from_string(&String::from_utf16(&self.string(v)?).unwrap()),
        })
    }
    fn amount_precision(&self, v: &Raw) -> Result<f64, E> {
        Ok(parse_float(&precision_to_string(parse_int(
            &String::from_utf16(&self.string(v)?).unwrap(),
            None,
        ))))
    }
    fn includes(&self, v: &Raw, t: &str) -> Result<bool, E> {
        match v {
            Raw::Text(s) => Ok(s.windows(units(t).len()).any(|w| w == units(t))),
            Raw::Array(a) => Ok(a.iter().any(|v| self.strict_eq(v, &text(t)))),
            _ => {
                Err(self
                    .error_text("((intermediate value) || \"\").includes is not a function".into()))
            }
        }
    }
    fn items(&self, v: &Raw) -> Result<Vec<Rc<Raw>>, E> {
        match v {
            Raw::Array(a) => Ok(a.clone()),
            _ => Err(self.error_text("not array".into())),
        }
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn entries(&self, v: &Raw) -> Result<Entries<Self>, E> {
        match v {
            Raw::Object(o) => {
                let rows = o.borrow();
                let keys = safe_cex_num::js_own_keys(
                    rows.iter()
                        .map(|(k, _)| String::from_utf16(k).unwrap())
                        .collect::<Vec<_>>()
                        .iter()
                        .map(String::as_str),
                )
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
                Ok(keys
                    .iter()
                    .map(|k| {
                        rows.iter()
                            .find(|(key, _)| *key == units(k))
                            .unwrap()
                            .clone()
                    })
                    .collect())
            }
            Raw::Array(a) => Ok(a
                .iter()
                .enumerate()
                .map(|(i, v)| (units(&i.to_string()), v.clone()))
                .collect()),
            Raw::Text(s) => Ok(s
                .iter()
                .enumerate()
                .map(|(i, c)| (units(&i.to_string()), self.text(&[*c])))
                .collect()),
            Raw::Null | Raw::Undefined => {
                Err(self.error_text("Cannot convert undefined or null to object".into()))
            }
            _ => Ok(vec![]),
        }
    }
    fn is_object(&self, v: &Raw) -> bool {
        matches!(v, Raw::Object(_) | Raw::Array(_) | Raw::Null)
    }
    fn array(&self, v: Vec<Rc<Raw>>) -> Rc<Raw> {
        Rc::new(Raw::Array(v))
    }
    fn object(&self, v: Entries<Self>) -> Result<Rc<Raw>, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(v))))
    }
    fn spread(&self, v: &Raw, values: Entries<Self>) -> Result<Rc<Raw>, E> {
        let out = self.object(if self.nullish(v) {
            vec![]
        } else {
            self.entries(v)?
        })?;
        for (k, v) in values {
            self.assign(&out, &k, v)?;
        }
        Ok(out)
    }
    fn assign(&self, o: &Raw, key: &[u16], v: Rc<Raw>) -> Result<(), E> {
        match o {
            Raw::Object(o) => {
                let mut o = o.borrow_mut();
                if let Some(row) = o.iter_mut().find(|(k, _)| k == key) {
                    row.1 = v;
                } else {
                    o.push((key.into(), v));
                }
                Ok(())
            }
            _ => Err(self.error_text("not object".into())),
        }
    }
    fn current(&self, s: &str) -> Result<Rc<Raw>, E> {
        match s {
            "disposed" => Ok(self.boolean(self.disposed.get())),
            "publicDataOnly" => Ok(self.boolean(flag(&self.cfg, "public"))),
            _ => Ok(get(&self.store, s)),
        }
    }
    fn preference(&self, c: &str, s: &Raw) -> Result<Rc<Raw>, E> {
        let key = self.string(s)?;
        let rows = if c == "margin" {
            self.margin.borrow()
        } else {
            self.leverage.borrow()
        };
        Ok(rows
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined)))
    }
    fn set_preference(&self, s: Rc<Raw>, m: Rc<Raw>) -> Result<(), E> {
        let key = self.string(&s)?;
        let mut rows = self.margin.borrow_mut();
        if let Some(row) = rows.iter_mut().find(|(k, _)| *k == key) {
            row.1 = m
        } else {
            rows.push((key, m));
        }
        Ok(())
    }
    fn now(&self) -> Result<f64, E> {
        let n = get(&self.cfg, "clock");
        if self.undefined(&n) {
            Ok(1723638896789.0)
        } else {
            self.numeric(&n)
        }
    }
    fn read(
        &self,
        _spot: bool,
        endpoint: &str,
        post: bool,
        body: Handle<Self>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        let options = if post {
            body
        } else if endpoint.ends_with("fills") {
            self.object(vec![(units("params"), body)])?
        } else {
            Rc::new(Raw::Undefined)
        };
        self.event(vec![
            ("kind", text("read")),
            ("endpoint", text(endpoint)),
            ("options", options),
        ]);
        let n = self.calls.get();
        self.calls.set(n + 1);
        if flag(&self.cfg, "syncFail") {
            return Err(self.original.clone());
        }
        let data = if let Raw::Array(pages) = &*get(&self.cfg, "pages") {
            pages[n.min(pages.len() - 1)].clone()
        } else {
            get(&self.cfg, "data")
        };
        Ok(async move {
            if n == 0 {
                yield_once().await;
                self.after();
            }
            if (n == 0 && flag(&self.cfg, "fail"))
                || self.numeric(&get(&self.cfg, "failAt"))? == n as f64
            {
                return Err(self.original.clone());
            }
            *self.last_data.borrow_mut() = Some(data.clone());
            Ok(data)
        })
    }
    fn fetch_balance(&self) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        self.event(vec![("kind", text("fetchBalance"))]);
        Ok(async move {
            yield_once().await;
            self.after();
            if flag(&self.cfg, "fail") {
                Err(self.original.clone())
            } else {
                Ok(get(&self.cfg, "balance"))
            }
        })
    }
    fn update_balance(&self, patch: Rc<Raw>) -> Result<(), E> {
        self.event(vec![("kind", text("update")), ("patch", patch)]);
        if flag(&self.cfg, "updateFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn trace(&self, e: &E) -> Result<(), E> {
        self.event(vec![
            ("kind", text("trace")),
            ("identity", self.boolean(e.original)),
        ]);
        if flag(&self.cfg, "traceFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn error_message(&self, e: &E, fallback: &str) -> Result<Rc<Raw>, E> {
        Ok(if self.truthy(&e.message) {
            e.message.clone()
        } else {
            text(fallback)
        })
    }
    fn error(&self, message: Rc<Raw>) -> E {
        Rc::new(Error {
            message,
            original: false,
            response: None,
            cause: None,
            name: "Error",
        })
    }
    fn emit_error(&self, m: Rc<Raw>) -> Result<(), E> {
        self.event(vec![("kind", text("emit")), ("message", m)]);
        if flag(&self.cfg, "emitFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn map_fills(&self, rows: Rc<Raw>) -> Result<Vec<Rc<Raw>>, E> {
        let input = safe_cex_num::numeric_ingress::js_from_value(to_value(&rows)).unwrap();
        let mut resolver = |s: &str| {
            let markets = get(&self.store, "marketsBySymbol");
            let found = get(&markets, &s.to_uppercase());
            let symbol = get(&found, "symbol");
            Ok(if self.nullish(&symbol) {
                None
            } else {
                Some(String::from_utf16(&self.string(&symbol).unwrap()).unwrap())
            })
        };
        let fills =
            safe_cex_venue_kraken::fills::map_kraken_fills(input, &mut resolver, &mut |n| n)
                .unwrap();
        Ok(fills
            .into_iter()
            .map(|f| {
                self.object(vec![
                    (units("id"), text(f.id.as_deref().unwrap())),
                    (units("timestamp"), self.number(f.timestamp)),
                    (units("symbol"), text(&f.symbol)),
                    (
                        units("orderId"),
                        f.order_id
                            .as_deref()
                            .map(text)
                            .unwrap_or(Rc::new(Raw::Undefined)),
                    ),
                    (units("price"), self.number(f.price)),
                    (units("qty"), self.number(f.qty)),
                    (
                        units("side"),
                        text(if f.side == safe_cex_types::OrderSide::Buy {
                            "buy"
                        } else {
                            "sell"
                        }),
                    ),
                    (units("maker"), self.boolean(f.maker.unwrap())),
                    (
                        units("realizedPnl"),
                        f.realized_pnl
                            .map(|n| self.number(n))
                            .unwrap_or(Rc::new(Raw::Undefined)),
                    ),
                ])
                .unwrap()
            })
            .collect())
    }
    fn iso(&self, ms: f64) -> Result<Vec<u16>, E> {
        if !ms.is_finite() || ms.abs() > 8640000000000000.0 {
            return Err(self.error_text("Invalid time value".into()));
        }
        let ms = ms.trunc() as i64;
        let days = ms.div_euclid(86400000);
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let y = y + if m <= 2 { 1 } else { 0 };
        let time = ms.rem_euclid(86400000);
        Ok(units(&format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
            time / 3600000,
            (time / 60000) % 60,
            (time / 1000) % 60,
            time % 1000
        )))
    }
    fn warn_fills(&self, page: usize, from: Vec<u16>) -> Result<(), E> {
        let msg = format!(
            "fetchHistoricalFills stopped after {page} page(s) without reaching {} — the result is a lower bound, not the whole window.",
            String::from_utf16(&from).unwrap()
        );
        self.event(vec![
            ("kind", text("warn")),
            ("message", text(&msg)),
            ("severity", text("warning")),
        ]);
        if flag(&self.cfg, "warnFail") {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn slice_limit(&self, values: Vec<Rc<Raw>>, limit: &Raw) -> Result<Rc<Raw>, E> {
        let n = self.numeric(limit)?.trunc();
        Ok(self.array(
            values
                .into_iter()
                .take(if n.is_infinite() {
                    usize::MAX
                } else {
                    n as usize
                })
                .collect(),
        ))
    }
}
