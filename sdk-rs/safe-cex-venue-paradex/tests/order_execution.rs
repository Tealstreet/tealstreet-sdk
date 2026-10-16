use safe_cex_venue_paradex::account_rest::AccountRestHost;
use safe_cex_venue_paradex::market_data::{DataFuture, MarketDataHost as Host};
use safe_cex_venue_paradex::order_execution::{self as api, OrderExecutionHost};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    ClassObject {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Raw,
    },
    Ref(&'static str),
    Function(usize),
    Error(Error),
    BigInt(String),
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(RefCell<Vec<(Vec<u16>, Raw)>>),
    Array(RefCell<Vec<Raw>>),
    Map(RefCell<Vec<(Vec<u16>, Raw)>>),
}
#[derive(Debug)]
struct Failure {
    primary: bool,
    message: Vec<u16>,
    history: bool,
}
type Error = Rc<Failure>;
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn raw(s: &str) -> Raw {
    Rc::new(Node::Text(u(s)))
}
fn text(s: Vec<u16>) -> Raw {
    Rc::new(Node::Text(s))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn obj(fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object(RefCell::new(
        fields.into_iter().map(|(k, v)| (u(k), v)).collect(),
    )))
}
fn arr(v: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Ref(r) => json!({"ref":r}),
        Node::Function(id) => json!({"f":id}),
        Node::Error(e) => json!({"error":true,"message":e.message}),
        Node::BigInt(v) => json!({"b":v}),
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => json!({"n":bits(*n)}),
        Node::Text(s) => json!({"s":s}),
        Node::Object(o) | Node::ClassObject { fields: o, .. } => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Map(_) => panic!("map escaped native"),
    }
}
fn decode(v: &Value) -> Raw {
    if v["ref"] == "this" {
        return Rc::new(Node::Ref("this"));
    }
    if let Some(id) = v["f"].as_u64() {
        return Rc::new(Node::Function(id as usize));
    }
    if v.is_null() {
        return Rc::new(Node::Null);
    }
    if let Some(b) = v.as_bool() {
        return Rc::new(Node::Bool(b));
    }
    if let Some(a) = v.as_array() {
        return arr(a.iter().map(decode).collect());
    }
    if v["u"] == true {
        return undef();
    }
    if let Some(s) = v["s"].as_array() {
        return text(s.iter().map(|v| v.as_u64().unwrap() as u16).collect());
    }
    if let Some(n) = v["n"].as_str() {
        return number(f64::from_bits(u64::from_str_radix(n, 16).unwrap()));
    }
    if let Some(b) = v["b"].as_str() {
        return Rc::new(Node::BigInt(b.into()));
    }
    let fields = v["o"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                serde_json::from_value::<Vec<u16>>(p[0].clone()).unwrap(),
                decode(&p[1]),
            )
        })
        .collect();
    Rc::new(Node::Object(RefCell::new(fields)))
}
fn get(v: &Raw, key: &str) -> Raw {
    match v.as_ref() {
        Node::Map(m) if key == "size" => number(m.borrow().len() as f64),
        Node::Error(e) => match key {
            "message" => text(e.message.clone()),
            "stack" => raw("STACK"),
            _ => undef(),
        },
        Node::Object(o) | Node::ClassObject { fields: o, .. } => o
            .borrow()
            .iter()
            .find(|(k, _)| *k == u(key))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef),
        Node::Text(s) if key == "length" => number(s.len() as f64),
        Node::Array(a) if key == "length" => number(a.borrow().len() as f64),
        Node::Array(a) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| a.borrow().get(i).cloned())
            .unwrap_or_else(undef),
        _ => undef(),
    }
}
fn set(v: &Raw, key: &str, item: Raw) {
    let o = match v.as_ref() {
        Node::Object(o) | Node::ClassObject { fields: o, .. } => o,
        _ => panic!("write non object"),
    };
    let mut o = o.borrow_mut();
    if let Some((_, v)) = o.iter_mut().find(|(k, _)| *k == u(key)) {
        *v = item;
    } else {
        o.push((u(key), item));
    }
}
fn elements(v: &Raw) -> Vec<Raw> {
    let Node::Array(a) = v.as_ref() else {
        panic!("array capability")
    };
    a.borrow().clone()
}
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Ref(r) => u(r),
        Node::Function(id) => u(&format!("function:{id}")),
        Node::BigInt(v) => u(v),
        Node::Error(e) => u("Error: ").into_iter().chain(e.message.clone()).collect(),
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Text(s) => s.clone(),
        Node::Number(n) => u(&if n.is_nan() {
            "NaN".into()
        } else if *n == 0.0 {
            "0".into()
        } else if *n == f64::INFINITY {
            "Infinity".into()
        } else if *n == f64::NEG_INFINITY {
            "-Infinity".into()
        } else {
            n.to_string()
        }),
        Node::Array(a) => a
            .borrow()
            .iter()
            .enumerate()
            .flat_map(|(i, v)| {
                let mut s = if i > 0 { u(",") } else { vec![] };
                if !matches!(v.as_ref(), Node::Undefined | Node::Null) {
                    s.extend(js(v));
                }
                s
            })
            .collect(),
        _ => u("[object Object]"),
    }
}
fn numeric(v: &Raw) -> f64 {
    match v.as_ref() {
        Node::Number(n) => *n,
        Node::Null => 0.0,
        Node::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Node::Text(s) => {
            let s = String::from_utf16_lossy(s);
            let s = s.trim();
            if s.is_empty() {
                0.0
            } else {
                s.parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}
fn pf(v: &Raw) -> f64 {
    let s = String::from_utf16_lossy(&js(v));
    let s = s.trim_start();
    if s.starts_with("Infinity") {
        return f64::INFINITY;
    }
    if s.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    for end in (1..=s.len()).rev() {
        if !s.is_char_boundary(end) {
            continue;
        }
        if let Ok(n) = s[..end].parse() {
            return n;
        }
    }
    f64::NAN
}
fn quote(s: &[u16]) -> String {
    let mut out = String::from("\"");
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        match c {
            34 => out.push_str("\\\""),
            92 => out.push_str("\\\\"),
            8 => out.push_str("\\b"),
            9 => out.push_str("\\t"),
            10 => out.push_str("\\n"),
            12 => out.push_str("\\f"),
            13 => out.push_str("\\r"),
            0..=31 => out.push_str(&format!("\\u{c:04x}")),
            0xd800..=0xdbff if s.get(i + 1).is_some_and(|c| (0xdc00..=0xdfff).contains(c)) => {
                out.push_str(&String::from_utf16(&s[i..i + 2]).unwrap());
                i += 1;
            }
            0xd800..=0xdfff => out.push_str(&format!("\\u{c:04x}")),
            _ => out.push(char::from_u32(c as u32).unwrap()),
        }
        i += 1;
    }
    out.push('"');
    out
}
fn stringify(v: &Raw) -> String {
    match v.as_ref() {
        Node::Ref(_) | Node::Function(_) => panic!("reference stringify"),
        Node::BigInt(_) => panic!("JSON.stringify bigint"),
        Node::Undefined | Node::Null => "null".into(),
        Node::Bool(b) => b.to_string(),
        Node::Number(n) => {
            if n.is_finite() {
                String::from_utf16_lossy(&js(v))
            } else {
                "null".into()
            }
        }
        Node::Text(s) => quote(s),
        Node::Array(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(stringify)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Node::Object(o) | Node::ClassObject { fields: o, .. } => format!(
            "{{{}}}",
            o.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v.as_ref(), Node::Undefined))
                .map(|(k, v)| format!("{}:{}", quote(k), stringify(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Node::Map(_) | Node::Error(_) => "{}".into(),
    }
}
struct Harness {
    input: Raw,
    name: String,
    profile: String,
    failure: String,
    primary: Error,
    counts: RefCell<Vec<(String, usize)>>,
    ops: RefCell<Vec<Value>>,
    updates: RefCell<Vec<Value>>,
    timers: RefCell<Vec<Value>>,
    ticker_updates: RefCell<Vec<Value>>,
    broadcasts: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Value>>,
    disposed: Cell<bool>,
    callback_ids: RefCell<Vec<usize>>,
    rest_calls: RefCell<Vec<(String, Vec<Raw>)>>,
    ctor_writes: RefCell<Vec<Value>>,
    callback_map: RefCell<Vec<(Vec<u16>, Raw)>>,
    rejection_symbols: RefCell<Vec<Raw>>,
    consoles: RefCell<Vec<Value>>,
    emits: RefCell<Vec<Value>>,
    leverage_writes: RefCell<Vec<Raw>>,
    position_writes: RefCell<Vec<Raw>>,
    native_books: RefCell<Vec<Raw>>,
    public_connections: Cell<usize>,
    native_tick_delay: Cell<Option<f64>>,
}
impl Harness {
    fn touch(&self, k: &str) -> Result<(), Error> {
        let mut counts = self.counts.borrow_mut();
        let n = if let Some((_, n)) = counts.iter_mut().find(|(name, _)| name == k) {
            let old = *n;
            *n += 1;
            old
        } else {
            counts.push((k.into(), 1));
            0
        };
        if self.failure == format!("{k}#{n}") {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn input(&self, key: &str) -> Raw {
        get(&self.input, key)
    }
    fn states(&self, key: &str) -> Raw {
        get(&self.input("states"), key)
    }
    fn record(&self, name: &str, args: Vec<Raw>) -> Result<(), Error> {
        self.touch(name)?;
        self.ops
            .borrow_mut()
            .push(json!({"name":name,"args":atom(&arr(args))}));
        if self.profile == format!("dispose-{name}") {
            self.disposed.set(true);
        }
        Ok(())
    }
    fn failure(&self, message: &str) -> Error {
        Rc::new(Failure {
            primary: false,
            message: u(message),
            history: false,
        })
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Error;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if self.nullish(v) {
            return Err(self.failure(&format!(
                "Cannot read properties of {} (reading '{k}')",
                String::from_utf16_lossy(&js(v))
            )));
        }
        Ok(get(v, k))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if self.nullish(v) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn state(&self, k: &str) -> Result<Raw, Error> {
        Ok(if k == "isDisposed" {
            Rc::new(Node::Bool(self.disposed.get()))
        } else {
            self.states(k)
        })
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(b) => *b,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Null | Node::Undefined)
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        match (a.as_ref(), b.as_ref()) {
            (Node::Number(a), Node::Number(b)) => a == b,
            (Node::Text(a), Node::Text(b)) => a == b,
            (Node::Bool(a), Node::Bool(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }
    }
    fn is_object(&self, v: &Raw) -> bool {
        matches!(
            v.as_ref(),
            Node::Null | Node::Object(_) | Node::Array(_) | Node::Map(_)
        )
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Array(_))
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Undefined)
    }
    fn reduce(
        &self,
        v: Raw,
        mut init: Raw,
        cb: &mut dyn FnMut(Raw, Raw) -> Result<Raw, Error>,
    ) -> Result<Raw, Error> {
        for item in elements(&v) {
            init = cb(init, item)?;
        }
        Ok(init)
    }
    fn for_each(&self, v: Raw, cb: &mut dyn FnMut(Raw) -> Result<(), Error>) -> Result<(), Error> {
        let mut i = 0;
        loop {
            let values = elements(&v);
            if i >= values.len() {
                break;
            }
            cb(values[i].clone())?;
            i += 1;
        }
        Ok(())
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn number(&self, n: f64) -> Raw {
        number(n)
    }
    fn boolean(&self, b: bool) -> Raw {
        Rc::new(Node::Bool(b))
    }
    fn undefined(&self) -> Raw {
        undef()
    }
    fn object(&self, f: Vec<(&'static str, Raw)>) -> Raw {
        obj(f)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        arr(v)
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(js(v))
    }
    fn numeric(&self, v: &Raw) -> Result<f64, Error> {
        Ok(numeric(v))
    }
    fn map(&self, v: Raw, cb: &mut dyn FnMut(Raw) -> Result<Raw, Error>) -> Result<Raw, Error> {
        if !self.is_array(&v) {
            return Err(self.failure("data.results.map is not a function"));
        }
        let length = elements(&v).len();
        let mut out = vec![];
        for i in 0..length {
            out.push(cb(elements(&v)[i].clone())?);
        }
        Ok(arr(out))
    }
    fn filter(&self, v: Raw, cb: &mut dyn FnMut(Raw) -> Result<bool, Error>) -> Result<Raw, Error> {
        if !self.is_array(&v) {
            return Err(self.failure("data.results.filter is not a function"));
        }
        let length = elements(&v).len();
        let mut out = vec![];
        for i in 0..length {
            let item = elements(&v)[i].clone();
            if cb(item.clone())? {
                out.push(item);
            }
        }
        Ok(arr(out))
    }
    fn sort(
        &self,
        v: Raw,
        cb: &mut dyn FnMut(Raw, Raw) -> Result<f64, Error>,
    ) -> Result<Raw, Error> {
        let mut a = elements(&v);
        for i in 1..a.len() {
            let mut j = i;
            while j > 0 && cb(a[j].clone(), a[j - 1].clone())? < 0.0 {
                a.swap(j, j - 1);
                j -= 1;
            }
        }
        let Node::Array(target) = v.as_ref() else {
            unreachable!()
        };
        *target.borrow_mut() = a;
        Ok(v)
    }
    fn select_log(&self) -> Result<Raw, Error> {
        Ok(raw("logger"))
    }
    fn log(&self, _logger: Raw, text: Vec<u16>, severity: &str) -> Result<(), Error> {
        self.touch("log")?;
        self.logs
            .borrow_mut()
            .push(json!({"text":text,"severity":severity}));
        Ok(())
    }
    fn operation(&self, name: &str, args: Vec<Raw>) -> Result<DataFuture<'_, Raw, Error>, Error> {
        let name = name.to_owned();
        Ok(Box::pin(async move {
            if name == "awaitSDK" {
                return Ok(undef());
            }
            self.record(
                &name,
                if matches!(name.as_str(), "textResponse" | "jsonResponse") {
                    vec![]
                } else {
                    args.clone()
                },
            )?;
            Ok(match name.as_str() {
                "get" => {
                    let legacy =
                        self.name == "fetchOHLCV" && js(&args[0]) == u("/tradingview/history");
                    obj(vec![(
                        "data",
                        self.input(if legacy { "legacy" } else { "data" }),
                    )])
                }
                "initExternalPublicData" => self.input("external"),
                "fetchMarkets" => get(&self.states("store"), "markets"),
                "fetchTickers" => arr(vec![obj(vec![("symbol", self.input("sym"))])]),
                "fetchPositions" => self.input("dependencyPositions"),
                "fetchBalance" => self.input("dependencyBalance"),
                "fetchOrders" => self.input("dependencyOrders"),
                "fetchLeverageSettings" | "tick" => undef(),
                _ => panic!("unknown op {name}"),
            })
        }))
    }
    fn update(&self, mut fields: Vec<(&'static str, Raw)>, loaded: &[&str]) -> Result<(), Error> {
        self.touch("update")?;
        let store = self.states("store");
        let old = get(&store, "loaded");
        let Node::Object(o) = old.as_ref() else {
            panic!()
        };
        let next = Rc::new(Node::Object(RefCell::new(o.borrow().clone())));
        for key in loaded {
            set(&next, key, self.boolean(true));
        }
        fields.push(("loaded", next));
        let update = obj(fields);
        self.updates.borrow_mut().push(atom(&update));
        let Node::Object(o) = update.as_ref() else {
            panic!()
        };
        for (k, v) in o.borrow().iter() {
            set_units(&store, k.clone(), v.clone());
        }
        Ok(())
    }
    fn write(&self, v: &Raw, key: &str, item: Raw) -> Result<(), Error> {
        set(v, key, item);
        Ok(())
    }
    fn error_value(&self, e: &Error) -> Raw {
        Rc::new(Node::Error(e.clone()))
    }
    fn new_error(&self, message: &str) -> Result<Error, Error> {
        Ok(self.failure(message))
    }
    fn schedule_tick(&self, delay: f64) -> Result<(), Error> {
        self.touch("schedule")?;
        self.native_tick_delay.set(Some(delay));
        self.timers.borrow_mut().push(json!(bits(delay)));
        Ok(())
    }
    fn callbacks(&self, _symbol: Raw) -> Result<Raw, Error> {
        Ok(obj(vec![(
            "size",
            number(self.callback_ids.borrow().len() as f64),
        )]))
    }
    fn broadcast(&self, _callbacks: Raw, book: Raw) -> Result<(), Error> {
        let mut i = 0;
        while i < self.callback_ids.borrow().len() {
            let id = self.callback_ids.borrow()[i];
            self.touch("callback")?;
            self.native_books.borrow_mut().push(book.clone());
            self.broadcasts
                .borrow_mut()
                .push(json!({"id":id,"book":atom(&book)}));
            if self.truthy(&self.input("grow")) && id == 0 {
                self.callback_ids.borrow_mut().push(2);
            }
            i += 1;
        }
        Ok(())
    }
    fn update_ticker(&self, symbol: Raw, fields: Vec<(&'static str, Raw)>) -> Result<(), Error> {
        self.touch("updateTicker")?;
        self.ticker_updates.borrow_mut().push(atom(&obj(vec![
            ("id", obj(vec![("id", symbol)])),
            ("value", obj(fields)),
        ])));
        Ok(())
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Error> {
        if let Some(result) = self.execution_intrinsic(name, &args) {
            return result;
        }
        if let Some(result) = self.rest_intrinsic(name, &args) {
            return result;
        }
        let a = |i: usize| args[i].clone();
        Ok(match name {
            "parseFloat" => number(pf(&a(0))),
            "Number" => number(numeric(&a(0))),
            "Number.isFinite" => {
                self.boolean(matches!(a(0).as_ref(),Node::Number(n)if n.is_finite()))
            }
            "String" => text(js(&a(0))),
            "String.upper" => raw(&String::from_utf16_lossy(&js(&a(0))).to_uppercase()),
            "includes" => self.boolean(
                String::from_utf16_lossy(&js(&a(0)))
                    .contains(&String::from_utf16_lossy(&js(&a(1)))),
            ),
            "endsWith" | "optional.endsWith" => {
                if name == "optional.endsWith" && self.nullish(&a(0)) {
                    undef()
                } else {
                    self.boolean(js(&a(0)).ends_with(&js(&a(1))))
                }
            }
            "Math.min" => number(if numeric(&a(0)).is_nan() || numeric(&a(1)).is_nan() {
                f64::NAN
            } else {
                numeric(&a(0)).min(numeric(&a(1)))
            }),
            "Math.abs" => number(numeric(&a(0)).abs()),
            "member" => get_units(&a(0), &js(&a(1))),
            "assignMember" => {
                let key = String::from_utf16_lossy(&js(&a(1)));
                if key != "__proto__" {
                    set(&a(0), &key, a(2));
                }
                a(2)
            }
            "add" => {
                if matches!(a(0).as_ref(), Node::Text(_)) {
                    let mut s = js(&a(0));
                    s.extend(js(&a(1)));
                    text(s)
                } else {
                    number(numeric(&a(0)) + numeric(&a(1)))
                }
            }
            "JSON.stringify" => {
                if matches!(a(0).as_ref(), Node::Undefined) {
                    undef()
                } else {
                    raw(&stringify(&a(0)))
                }
            }
            "keys.join" => {
                let value = a(0);
                let Node::Object(o) = value.as_ref() else {
                    panic!()
                };
                raw(&o
                    .borrow()
                    .iter()
                    .map(|(k, _)| String::from_utf16_lossy(k))
                    .collect::<Vec<_>>()
                    .join(&String::from_utf16_lossy(&js(&a(1)))))
            }
            "virtualClock" => {
                self.touch("clock")?;
                number(555000.0)
            }
            "Date.now" => {
                self.touch("date")?;
                let n = self
                    .counts
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == "date")
                    .unwrap()
                    .1;
                number(555000.0 + n as f64)
            }
            "timeframe" => number(if js(&a(0)) == u("1m") {
                60.0
            } else {
                panic!("unrepresented timeframe")
            }),
            "tradingViewResolution" => raw(if js(&a(0)) == u("1d") { "1D" } else { "1" }),
            "omitUndefined" => {
                let value = a(0);
                let Node::Object(o) = value.as_ref() else {
                    panic!()
                };
                Rc::new(Node::Object(RefCell::new(
                    o.borrow()
                        .iter()
                        .filter(|(_, v)| !self.is_undefined(v))
                        .cloned()
                        .collect(),
                )))
            }
            "flat" => arr(elements(&a(0)).iter().flat_map(elements).collect()),
            "leverage.get" => {
                let symbol = js(&a(0));
                elements(&self.input("leverage"))
                    .iter()
                    .find_map(|entry| {
                        let items = elements(entry);
                        (js(&items[0]) == symbol).then(|| items[1].clone())
                    })
                    .unwrap_or_else(undef)
            }
            "createPositionId" => a(0),
            "new Map" => Rc::new(Node::Map(RefCell::new(vec![]))),
            "Map.set" => {
                let map = a(0);
                let Node::Map(m) = map.as_ref() else { panic!() };
                let mut m = m.borrow_mut();
                let key = js(&a(1));
                if let Some((_, v)) = m.iter_mut().find(|(k, _)| k.as_slice() == key.as_slice()) {
                    *v = a(2);
                } else {
                    m.push((key, a(2)));
                }
                map.clone()
            }
            "Array.from.values" => {
                let map = a(0);
                let Node::Map(m) = map.as_ref() else { panic!() };
                arr(m.borrow().iter().map(|(_, v)| v.clone()).collect())
            }
            "sumBy" => number(
                elements(&a(0))
                    .iter()
                    .map(|v| numeric(&get(v, "unrealizedPnl")))
                    .sum(),
            ),
            "destructurePair" => arr(elements(&a(0)).into_iter().take(2).collect()),
            "replace" => {
                let input = js(&a(0));
                let needle = js(&a(1));
                let replacement = js(&a(2));
                let at = input
                    .windows(needle.len())
                    .position(|w| w == needle)
                    .unwrap();
                text(
                    input[..at]
                        .iter()
                        .chain(replacement.iter())
                        .chain(input[at + needle.len()..].iter())
                        .copied()
                        .collect(),
                )
            }
            "toFixed" => {
                if numeric(&a(0)).is_finite() {
                    raw(&format!("{:.*}", numeric(&a(1)) as usize, numeric(&a(0))))
                } else {
                    text(js(&a(0)))
                }
            }
            "split.member" => {
                let s = js(&a(0));
                let delimiter = js(&a(1));
                text(
                    s.split(|c| *c == delimiter[0])
                        .nth(numeric(&a(2)) as usize)
                        .unwrap_or(&[])
                        .to_vec(),
                )
            }
            "spread.concat" => arr(elements(&a(0)).into_iter().chain(elements(&a(1))).collect()),
            "connectPublic" | "connectPrivate" => {
                self.record(name, vec![])?;
                if name == "connectPublic" {
                    self.public_connections
                        .set(self.public_connections.get() + 1);
                }
                undef()
            }
            "describeError" => raw("{}"),
            _ => panic!("missing intrinsic {name}"),
        })
    }
}
fn block_on<F: Future>(f: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut f = pin!(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
fn get_units(v: &Raw, key: &[u16]) -> Raw {
    if let Node::Object(o) = v.as_ref() {
        o.borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef)
    } else {
        get(v, &String::from_utf16_lossy(key))
    }
}
fn set_units(v: &Raw, key: Vec<u16>, item: Raw) {
    let Node::Object(o) = v.as_ref() else {
        panic!("write non object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, v)) = o.iter_mut().find(|(k, _)| *k == key) {
        *v = item;
    } else {
        o.push((key, item));
    }
}
fn decimal_hex(input: &str) -> String {
    let negative = input.starts_with('-');
    let mut digits: Vec<u32> = input
        .trim_start_matches('-')
        .chars()
        .map(|c| c.to_digit(10).unwrap())
        .collect();
    let mut out = vec![];
    while digits.iter().any(|d| *d != 0) {
        let mut rem = 0;
        for digit in &mut digits {
            let n = rem * 10 + *digit;
            *digit = n / 16;
            rem = n % 16;
        }
        out.push(char::from_digit(rem, 16).unwrap());
    }
    if out.is_empty() {
        out.push('0');
    }
    let mut result: String = out.into_iter().rev().collect();
    if negative && result != "0" {
        result.insert(0, '-');
    }
    result
}
fn hex_decimal(input: &str) -> String {
    let mut digits = vec![0u32];
    for c in input.chars() {
        let mut carry = c.to_digit(16).unwrap();
        for d in &mut digits {
            let n = *d * 16 + carry;
            *d = n % 10;
            carry = n / 10;
        }
        while carry > 0 {
            digits.push(carry % 10);
            carry /= 10;
        }
    }
    digits
        .into_iter()
        .rev()
        .map(|d| char::from_digit(d, 10).unwrap())
        .collect()
}
impl Harness {
    fn rest_intrinsic(&self, name: &str, args: &[Raw]) -> Option<Result<Raw, Error>> {
        let a = |i: usize| args[i].clone();
        let mut names = std::collections::HashSet::new();
        for n in [
            "Date.now",
            "text",
            "starknetKeccak",
            "radix16",
            "bigint.toString",
            "BigInt.hex",
            "BigInt.zero",
            "BigInt.shift8.or",
            "trim",
            "regex.hex",
            "regex.decimal",
            "TextEncoder.encode",
            "number.toString",
            "spread",
            "JSON.parse",
            "new Set",
            "Set.has",
            "Set.add",
            "push",
            "push.spread",
            "slice",
            "parseInt10",
            "ORDER_SIDE",
            "Math.max",
            "Math.floor",
            "leverage.set",
            "addOrUpdatePositions",
            "JSON.stringify.ownNames",
            "destructureExecution",
        ] {
            names.insert(n);
        }
        if !names.contains(name) {
            return None;
        }
        Some((|| {
            Ok(match name {
                "Date.now" => {
                    self.touch("date")?;
                    number(1700000000123.0)
                }
                "text" => text(elements(&a(0)).iter().map(|v| numeric(v) as u16).collect()),
                "starknetKeccak" => {
                    self.record("starknetKeccak", vec![a(1)])?;
                    Rc::new(Node::BigInt("123".into()))
                }
                "radix16" => raw(&decimal_hex(&String::from_utf16_lossy(&js(&a(0))))),
                "bigint.toString" => text(js(&a(0))),
                "BigInt.hex" => {
                    let value = a(0);
                    let dec = match value.as_ref() {
                        Node::Number(n) => {
                            if !n.is_finite() || n.fract() != 0.0 {
                                return Err(self.failure(&format!("The number {} cannot be converted to a BigInt because it is not an integer",String::from_utf16_lossy(&js(&value)))));
                            }
                            if *n == 0.0 { "0".into() } else { n.to_string() }
                        }
                        _ => String::from_utf16_lossy(&js(&value)),
                    };
                    raw(&decimal_hex(&dec))
                }
                "BigInt.zero" => Rc::new(Node::BigInt("0".into())),
                "BigInt.shift8.or" => {
                    let v = String::from_utf16_lossy(&js(&a(0)));
                    let mut hex = decimal_hex(&v);
                    if hex == "0" {
                        hex.clear();
                    }
                    hex.push_str(&format!("{:02x}", numeric(&a(1)) as u8));
                    Rc::new(Node::BigInt(hex_decimal(&hex)))
                }
                "trim" => raw(String::from_utf16_lossy(&js(&a(0))).trim()),
                "regex.hex" => {
                    let s = js(&a(0));
                    self.boolean(
                        s.starts_with(&u("0x"))
                            && s.len() > 2
                            && s[2..]
                                .iter()
                                .all(|c| (*c as u8).is_ascii_hexdigit() && *c < 128),
                    )
                }
                "regex.decimal" => self.boolean(
                    !js(&a(0)).is_empty() && js(&a(0)).iter().all(|c| (48..=57).contains(c)),
                ),
                "TextEncoder.encode" => arr(String::from_utf16_lossy(&js(&a(0)))
                    .as_bytes()
                    .iter()
                    .map(|b| number(*b as f64))
                    .collect()),
                "number.toString" => text(js(&a(0))),
                "spread" => {
                    let value = a(0);
                    if self.nullish(&value) {
                        obj(vec![])
                    } else {
                        let Node::Object(o) = value.as_ref() else {
                            panic!("spread fixture")
                        };
                        Rc::new(Node::Object(RefCell::new(o.borrow().clone())))
                    }
                }
                "JSON.parse" => {
                    let s = String::from_utf16_lossy(&js(&a(0)));
                    let parsed: Value =
                        serde_json::from_str(&s).map_err(|_| self.failure("parse fixture"))?;
                    obj(vec![("error", raw(parsed["error"].as_str().unwrap()))])
                }
                "new Set" => Rc::new(Node::Map(RefCell::new(vec![]))),
                "Set.has" => {
                    let set = a(0);
                    let Node::Map(m) = set.as_ref() else { panic!() };
                    self.boolean(m.borrow().iter().any(|(k, _)| *k == js(&a(1))))
                }
                "Set.add" => {
                    let set = a(0);
                    let Node::Map(m) = set.as_ref() else { panic!() };
                    let key = js(&a(1));
                    if !m.borrow().iter().any(|(k, _)| *k == key) {
                        m.borrow_mut().push((key, a(1)));
                    }
                    set.clone()
                }
                "push" | "push.spread" => {
                    let target = a(0);
                    let Node::Array(v) = target.as_ref() else {
                        panic!()
                    };
                    if name == "push" {
                        v.borrow_mut().push(a(1));
                    } else {
                        v.borrow_mut().extend(elements(&a(1)));
                    }
                    number(v.borrow().len() as f64)
                }
                "slice" => {
                    let values = elements(&a(0));
                    let n = numeric(&a(2));
                    let end = if n.is_nan() {
                        0
                    } else if n <= 0.0 {
                        (values.len() as f64 + n.trunc()).max(0.0) as usize
                    } else {
                        n.trunc() as usize
                    };
                    arr(values.into_iter().take(end).collect())
                }
                "parseInt10" => {
                    let s = String::from_utf16_lossy(&js(&a(0)));
                    let s = s.trim_start();
                    let signed = s.strip_prefix('-');
                    let n = (signed.unwrap_or(s))
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<f64>()
                        .unwrap_or(f64::NAN);
                    number(if signed.is_some() { -n } else { n })
                }
                "ORDER_SIDE" => {
                    let s = js(&a(0));
                    if s == u("BUY") {
                        raw("buy")
                    } else if s == u("SELL") {
                        raw("sell")
                    } else {
                        undef()
                    }
                }
                "Math.floor" => number(numeric(&a(0)).floor()),
                "Math.max" => number(if numeric(&a(0)).is_nan() || numeric(&a(1)).is_nan() {
                    f64::NAN
                } else {
                    numeric(&a(0)).max(numeric(&a(1)))
                }),
                "leverage.set" => {
                    self.touch("leverageSet")?;
                    self.leverage_writes.borrow_mut().push(arr(args.to_vec()));
                    undef()
                }
                "addOrUpdatePositions" => {
                    self.touch("addPositions")?;
                    self.position_writes.borrow_mut().push(a(0));
                    undef()
                }
                "JSON.stringify.ownNames" => {
                    let v = a(0);
                    let Node::Error(e) = v.as_ref() else { panic!() };
                    let o = obj(vec![
                        ("stack", raw("STACK")),
                        ("message", text(e.message.clone())),
                    ]);
                    if e.primary {
                        set(&o, "history", self.boolean(e.history));
                    }
                    raw(&stringify(&o))
                }
                "destructureExecution" => {
                    if self.nullish(&a(0)) {
                        return Err(self.failure(
                            "Cannot destructure property 'symbol' of 'opts' as it is undefined.",
                        ));
                    }
                    a(0)
                }
                _ => unreachable!(),
            })
        })())
    }
}
impl AccountRestHost for Harness {
    fn make_error(&self, message: Vec<u16>) -> Result<Error, Error> {
        Ok(Rc::new(Failure {
            primary: false,
            message,
            history: false,
        }))
    }
    fn kind(&self, v: &Raw) -> &'static str {
        match v.as_ref() {
            Node::Text(_) => "string",
            Node::Number(_) => "number",
            Node::BigInt(_) => "bigint",
            Node::Bool(_) => "boolean",
            Node::Undefined => "undefined",
            _ => "object",
        }
    }
    fn select(&self, name: &str) -> Result<Raw, Error> {
        Ok(raw(name))
    }
    fn invoke(&self, selected: Raw, args: Vec<Raw>) -> Result<DataFuture<'_, Raw, Error>, Error> {
        if let Some(result) = self.execution_invoke(&selected, &args) {
            return Ok(Box::pin(async move { result }));
        }
        let name = String::from_utf16_lossy(&js(&selected));
        Ok(Box::pin(async move {
            if name == "awaitSDK" {
                return Ok(undef());
            }
            if name == "ensureAuth" && self.profile == "auth-not-ready" {
                return Err(self.primary.clone());
            }
            if name == "get" {
                self.touch("get")?;
                self.ops
                    .borrow_mut()
                    .push(json!({"name":"get","args":atom(&arr(args.clone()))}));
            } else {
                self.record(
                    &name,
                    if matches!(name.as_str(), "textResponse" | "jsonResponse") {
                        vec![]
                    } else {
                        args.clone()
                    },
                )?;
            }
            self.rest_calls
                .borrow_mut()
                .push((name.clone(), args.clone()));
            Ok(match name.as_str() {
                "loadStarknet" => obj(vec![]),
                "signMessage" => self.input("signature"),
                "signTypedData" => self.input("signed"),
                "fetch" => self.input("auth"),
                "jsonResponse" => get(&self.input("auth"), "data"),
                "textResponse" => get(&self.input("auth"), "text"),
                "ensureAuth" => undef(),
                "get" => {
                    let count = self
                        .counts
                        .borrow()
                        .iter()
                        .find(|(k, _)| k == "get")
                        .unwrap()
                        .1
                        - 1;
                    let mut data = if self.name == "fetchHistoricalFills"
                        && js(&args[0]) == u("/orders-history")
                    {
                        self.input("fallback")
                    } else {
                        let pages = elements(&self.input("pages"));
                        pages[count.min(pages.len() - 1)].clone()
                    };
                    if self.truthy(&self.input("cap")) {
                        let Node::Object(o) = data.as_ref() else {
                            panic!()
                        };
                        let fields = o.borrow().clone();
                        data = Rc::new(Node::Object(RefCell::new(fields)));
                        set(&data, "next", raw(&format!("cursor-{count}")));
                    }
                    obj(vec![("data", data)])
                }
                "post" => obj(vec![
                    ("status", number(200.0)),
                    ("data", obj(vec![("ok", self.boolean(true))])),
                ]),
                _ => panic!("unknown rest invoke {name}"),
            })
        }))
    }
    fn console(&self, args: Vec<Raw>) -> Result<(), Error> {
        self.touch("console")?;
        self.consoles.borrow_mut().push(atom(&arr(args)));
        Ok(())
    }
    fn emit(&self, v: Raw) -> Result<(), Error> {
        self.touch("emit")?;
        self.emits.borrow_mut().push(atom(&v));
        Ok(())
    }
    fn income(
        &self,
        row: Raw,
        resolve: &mut dyn FnMut(Raw) -> Result<Raw, Error>,
    ) -> Result<Raw, Error> {
        self.touch("income")?;
        let symbol = resolve(get(&row, "market"))?;
        self.map(get(&row, "mapped"), &mut |v| {
            let Node::Object(o) = v.as_ref() else {
                panic!()
            };
            let next = Rc::new(Node::Object(RefCell::new(o.borrow().clone())));
            set(&next, "symbol", symbol.clone());
            Ok(next)
        })
    }
    fn historical(&self, row: Raw) -> Result<Raw, Error> {
        self.touch("historical")?;
        Ok(get(&row, "historical"))
    }
    fn history_error(&self, e: &Error) -> Result<bool, Error> {
        Ok(e.history)
    }
}
impl Harness {
    fn execution_intrinsic(&self, name: &str, args: &[Raw]) -> Option<Result<Raw, Error>> {
        let a = |i: usize| args[i].clone();
        let names = [
            "roundPriceToStep",
            "method.toString",
            "null",
            "has",
            "array.includes",
            "isTrigger",
            "signerHeaders",
            "entries",
            "Map.has",
            "Map.get",
            "Set.from",
            "filter.join",
            "join",
            "literal.includes.instructions",
            "literal.includes.legacy",
            "adjust",
            "destructureUpdate",
            "callbacks.has",
            "callbacks.setNew",
            "callbacks.get",
            "callbacks.delete",
            "Set.delete",
            "observeInit",
        ];
        if !names.contains(&name) {
            return None;
        }
        Some((|| {
            Ok(match name {
                "roundPriceToStep" => number(
                    safe_cex_venue_paradex::exchange_helpers::round_price_to_step(
                        numeric(&a(0)),
                        numeric(&a(1)),
                        &String::from_utf16_lossy(&js(&a(2))),
                    ),
                ),
                "method.toString" => {
                    if self.nullish(&a(0)) {
                        return Err(self.failure(&format!(
                            "Cannot read properties of {} (reading 'toString')",
                            String::from_utf16_lossy(&js(&a(0)))
                        )));
                    }
                    text(js(&a(0)))
                }
                "null" => Rc::new(Node::Null),
                "has" => {
                    let value = a(0);
                    let Node::Object(fields) = value.as_ref() else {
                        panic!("presence requires object")
                    };
                    self.boolean(fields.borrow().iter().any(|(key, _)| *key == js(&a(1))))
                }
                "array.includes" => {
                    self.boolean(elements(&a(0)).iter().any(|v| self.equal(v, &a(1))))
                }
                "isTrigger" => {
                    let value = js(&a(0));
                    self.boolean(
                        [
                            "STOP_LIMIT",
                            "STOP_MARKET",
                            "STOP_LOSS_LIMIT",
                            "STOP_LOSS_MARKET",
                            "TAKE_PROFIT_LIMIT",
                            "TAKE_PROFIT_MARKET",
                        ]
                        .iter()
                        .any(|v| value == u(v)),
                    )
                }
                "signerHeaders" => {
                    if self.truthy(&a(0)) {
                        arr(vec![obj(vec![("headers", a(0))])])
                    } else {
                        arr(vec![])
                    }
                }
                "entries" => arr(elements(&a(0))
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| arr(vec![number(i as f64), v]))
                    .collect()),
                "Map.has" => {
                    let map = a(0);
                    let Node::Map(m) = map.as_ref() else { panic!() };
                    self.boolean(m.borrow().iter().any(|(k, _)| *k == js(&a(1))))
                }
                "Map.get" => {
                    let map = a(0);
                    let Node::Map(m) = map.as_ref() else { panic!() };
                    m.borrow()
                        .iter()
                        .find(|(k, _)| *k == js(&a(1)))
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(undef)
                }
                "Set.from" => Rc::new(Node::Map(RefCell::new(
                    elements(&a(0)).iter().map(|v| (js(v), v.clone())).collect(),
                ))),
                "filter.join" | "join" => {
                    let items = elements(&a(0));
                    let joined = items
                        .into_iter()
                        .filter(|v| name == "join" || self.truthy(v))
                        .enumerate()
                        .flat_map(|(i, v)| {
                            let mut out = if i > 0 { js(&a(1)) } else { vec![] };
                            out.extend(js(&v));
                            out
                        })
                        .collect();
                    text(joined)
                }
                "literal.includes.instructions" => self.boolean(
                    ["GTC", "POST_ONLY", "IOC", "RPI"]
                        .iter()
                        .any(|v| js(&a(0)) == u(v)),
                ),
                "literal.includes.legacy" => {
                    self.boolean(["GTT", "FOK", "IOC"].iter().any(|v| js(&a(0)) == u(v)))
                }
                "adjust" => number(safe_cex_num::adjust(numeric(&a(0)), numeric(&a(1)))),
                "destructureUpdate" => a(0),
                "callbacks.has" => self.boolean(
                    self.callback_map
                        .borrow()
                        .iter()
                        .any(|(k, _)| *k == js(&a(0))),
                ),
                "callbacks.setNew" => {
                    self.callback_map
                        .borrow_mut()
                        .push((js(&a(0)), Rc::new(Node::Map(RefCell::new(vec![])))));
                    undef()
                }
                "callbacks.get" => self
                    .callback_map
                    .borrow()
                    .iter()
                    .find(|(k, _)| *k == js(&a(0)))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(undef),
                "callbacks.delete" => {
                    self.callback_map
                        .borrow_mut()
                        .retain(|(k, _)| *k != js(&a(0)));
                    self.boolean(true)
                }
                "Set.delete" => {
                    let set = a(0);
                    let Node::Map(m) = set.as_ref() else { panic!() };
                    m.borrow_mut().retain(|(k, _)| *k != js(&a(1)));
                    self.boolean(true)
                }
                "observeInit" => {
                    self.record("observeInit", vec![])?;
                    undef()
                }
                _ => unreachable!(),
            })
        })())
    }
    fn execution_invoke(&self, selected: &Raw, args: &[Raw]) -> Option<Result<Raw, Error>> {
        let name = String::from_utf16_lossy(&js(selected));
        if ![
            "fetchOrderBook",
            "resolveIocPrice",
            "buildOrder",
            "signOrder",
            "signModifyOrder",
            "post",
            "put",
        ]
        .contains(&name.as_str())
        {
            return None;
        }
        Some((|| {
            self.record(&name, args.to_vec())?;
            self.rest_calls
                .borrow_mut()
                .push((name.clone(), args.to_vec()));
            Ok(match name.as_str() {
                "fetchOrderBook" => self.input("book"),
                "resolveIocPrice" => self.input("iocPrice"),
                "signOrder" | "signModifyOrder" => self.input("signed"),
                "buildOrder" => {
                    let counts = self.counts.borrow();
                    let count =
                        |n: &str| counts.iter().find(|(k, _)| k == n).map_or(0, |(_, v)| *v);
                    let part = count("generateClientId") + count("buildOrder") - 1;
                    obj(vec![
                        ("payload", obj(vec![("client_id", args[1].clone())])),
                        ("headers", obj(vec![("part", number(part as f64))])),
                    ])
                }
                "post" => {
                    let data = if self.name == "placeNativeBatchChunk" {
                        self.input("batch")
                    } else {
                        let n = self
                            .counts
                            .borrow()
                            .iter()
                            .find(|(k, _)| k == "post")
                            .unwrap()
                            .1
                            - 1;
                        obj(vec![("id", raw(&format!("child-{n}")))])
                    };
                    obj(vec![("data", data)])
                }
                "put" => obj(vec![("data", obj(vec![("id", self.input("putId"))]))]),
                _ => unreachable!(),
            })
        })())
    }
}
impl OrderExecutionHost for Harness {
    fn map_indexed(
        &self,
        value: Raw,
        cb: &mut dyn FnMut(Raw, f64) -> Result<Raw, Error>,
    ) -> Result<Raw, Error> {
        let mut i = 0;
        self.map(value, &mut |v| {
            let index = i;
            i += 1;
            cb(v, index as f64)
        })
    }
    fn find(
        &self,
        value: Raw,
        cb: &mut dyn FnMut(Raw) -> Result<bool, Error>,
    ) -> Result<Raw, Error> {
        let length = elements(&value).len();
        for i in 0..length {
            let item = elements(&value)[i].clone();
            if cb(item.clone())? {
                return Ok(item);
            }
        }
        Ok(undef())
    }
    fn arguments(&self, value: Raw) -> Result<Vec<Raw>, Error> {
        Ok(elements(&value))
    }
    fn sync_call(&self, selected: Raw, args: Vec<Raw>) -> Result<Raw, Error> {
        let name = String::from_utf16_lossy(&js(&selected));
        let a = |i: usize| args[i].clone();
        Ok(match name.as_str() {
            "mapOrderType" => {
                let v = js(&a(0));
                raw(if v == u("limit") {
                    "LIMIT"
                } else if v == u("market") {
                    "MARKET"
                } else if v == u("conditional") {
                    "STOP_LIMIT"
                } else if v == u("stop_market") {
                    "STOP_MARKET"
                } else if v == u("take_profit_market") {
                    "TAKE_PROFIT_MARKET"
                } else {
                    "LIMIT"
                })
            }
            "mapTimeInForce" => raw(if js(&a(0)) == u("ImmediateOrCancel") {
                "IOC"
            } else if js(&a(0)) == u("PostOnly") {
                "POST_ONLY"
            } else {
                "GTC"
            }),
            "reverseMapOrderType" => {
                let v = js(&a(0));
                raw(if v == u("LIMIT") {
                    "limit"
                } else if v == u("STOP_MARKET") {
                    if self.truthy(&a(1)) {
                        "stop_market"
                    } else {
                        "conditional"
                    }
                } else {
                    "limit"
                })
            }
            "validateTpSl" => {
                let o = a(0);
                let price = get(&o, "price");
                let tp = get(&o, "takeProfit");
                let sl = get(&o, "stopLoss");
                let buy = js(&get(&o, "side")) == u("buy");
                let invalid = self.truthy(&price)
                    && ((self.truthy(&tp)
                        && if buy {
                            numeric(&tp) <= numeric(&price)
                        } else {
                            numeric(&tp) >= numeric(&price)
                        })
                        || (self.truthy(&sl)
                            && if buy {
                                numeric(&sl) >= numeric(&price)
                            } else {
                                numeric(&sl) <= numeric(&price)
                            }));
                if invalid {
                    let (kind, v, direction, side) = if self.truthy(&tp)
                        && if buy {
                            numeric(&tp) <= numeric(&price)
                        } else {
                            numeric(&tp) >= numeric(&price)
                        } {
                        (
                            "TP",
                            tp,
                            if buy { ">" } else { "<" },
                            if buy { "long" } else { "short" },
                        )
                    } else {
                        (
                            "SL",
                            sl,
                            if buy { "<" } else { ">" },
                            if buy { "long" } else { "short" },
                        )
                    };
                    let mut units = u(&format!("Invalid {kind} for {side}: "));
                    units.extend(js(&v));
                    units.extend(u(&format!(" must be {direction} entry ")));
                    units.extend(js(&price));
                    self.log(raw("logger"), units, "warning")?;
                }
                self.boolean(!invalid)
            }
            "generateClientId" => {
                self.record(&name, vec![])?;
                let n = self
                    .counts
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == "generateClientId")
                    .unwrap()
                    .1
                    - 1;
                raw(&format!("ts_{n}"))
            }
            "super"
            | "createAPI"
            | "rateLimit"
            | "newPublicWebSocket"
            | "newPrivateWebSocket"
            | "initializeSDKPromise"
            | "subscribeBook"
            | "unsubscribeBook"
            | "cleanupStats"
            | "setSlippagePromise" => {
                self.record(&name, args.clone())?;
                match name.as_str() {
                    "super" => undef(),
                    "createAPI" => obj(vec![("tag", raw("api"))]),
                    "rateLimit" => obj(vec![("tag", raw("xhr"))]),
                    "newPublicWebSocket" => obj(vec![("tag", raw("public"))]),
                    "newPrivateWebSocket" => obj(vec![("tag", raw("private"))]),
                    "initializeSDKPromise" | "setSlippagePromise" => Rc::new(Node::Ref("promise")),
                    _ => undef(),
                }
            }
            _ => panic!("unknown sync {name}"),
        })
    }
    fn write_state(&self, name: &str, value: Raw) -> Result<(), Error> {
        self.touch(&format!("write:{name}"))?;
        self.ctor_writes
            .borrow_mut()
            .push(json!({"key":name,"value":atom(&value)}));
        set(&self.input("states"), name, value);
        Ok(())
    }
    fn observe_rejection(&self, _promise: Raw, symbol: Raw) -> Result<(), Error> {
        self.rejection_symbols.borrow_mut().push(symbol);
        Ok(())
    }
}
fn harness(input: Raw, name: &str, profile: &str, failure: &str) -> Harness {
    let states = get(&input, "states");
    if !matches!(states.as_ref(), Node::Undefined) {
        set(&states, "this", Rc::new(Node::Ref("this")));
        if name == "listenOrderBook" {
            set(
                &states,
                "parent",
                obj(vec![
                    (
                        "isPublicDataOnly",
                        Rc::new(Node::Bool(profile == "public-only")),
                    ),
                    (
                        "jwtManager",
                        if profile == "no-jwt" {
                            Rc::new(Node::Null)
                        } else {
                            obj(vec![])
                        },
                    ),
                ]),
            );
        }
    }
    Harness {
        input,
        name: name.into(),
        profile: profile.into(),
        failure: failure.into(),
        primary: Rc::new(Failure {
            primary: true,
            message: u("primary-").into_iter().chain([0xd800]).collect(),
            history: false,
        }),
        counts: RefCell::new(vec![]),
        ops: RefCell::new(vec![]),
        updates: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        ticker_updates: RefCell::new(vec![]),
        broadcasts: RefCell::new(vec![]),
        logs: RefCell::new(vec![]),
        disposed: Cell::new(false),
        callback_ids: RefCell::new(vec![]),
        native_books: RefCell::new(vec![]),
        public_connections: Cell::new(0),
        native_tick_delay: Cell::new(None),
        rest_calls: RefCell::new(vec![]),
        consoles: RefCell::new(vec![]),
        emits: RefCell::new(vec![]),
        leverage_writes: RefCell::new(vec![]),
        position_writes: RefCell::new(vec![]),
        ctor_writes: RefCell::new(vec![]),
        callback_map: RefCell::new(vec![]),
        rejection_symbols: RefCell::new(vec![]),
    }
}
#[test]
fn source_oracle() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/order_execution_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source instrument: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["runs"].as_array().unwrap().len(), 726);
    for case in corpus["runs"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let h = harness(decode(&case["input"]), name, profile, failure);
        let args = elements(&h.input("args"));
        let result = match name {
            "resolveMarketIocExecutionPrice" => {
                block_on(api::resolve_market_ioc_price(&h, args[0].clone()))
            }
            "buildMainOrderPayload" => block_on(api::build_main_order_payload(
                &h,
                args[0].clone(),
                args[1].clone(),
            )),
            "placeNativeBatchChunk" => block_on(api::place_native_batch_chunk(&h, args[0].clone())),
            "placeTakeProfitStopLoss" => {
                block_on(api::place_take_profit_stop_loss(&h, args[0].clone()))
            }
            "updateOrder" => block_on(api::update_order(&h, args[0].clone())),
            "constructor" => {
                api::initialize_exchange(&h, args[0].clone(), args[1].clone(), args[2].clone())
            }
            _ => unreachable!(),
        };
        if profile == "basic" && failure == "none" {
            match name {
                "resolveMarketIocExecutionPrice" => assert_eq!(
                    numeric(result.as_ref().unwrap()),
                    102.0,
                    "ioc_native_execution_price"
                ),
                "buildMainOrderPayload" => assert_eq!(
                    js(&get(&get(result.as_ref().unwrap(), "payload"), "stp")),
                    u("EXPIRE_TAKER"),
                    "payload_native_stp"
                ),
                "placeNativeBatchChunk" => assert_eq!(
                    elements(result.as_ref().unwrap())
                        .iter()
                        .map(js)
                        .collect::<Vec<_>>(),
                    vec![u("native-0"), u("native-1")],
                    "batch_native_client_correlation"
                ),
                "placeTakeProfitStopLoss" => assert_eq!(
                    elements(result.as_ref().unwrap())
                        .iter()
                        .map(js)
                        .collect::<Vec<_>>(),
                    vec![u("child-0"), u("child-1")],
                    "brackets_native_child_ids"
                ),
                "updateOrder" => {
                    let calls = h.rest_calls.borrow();
                    let args = &calls.iter().find(|(n, _)| n == "put").unwrap().1;
                    assert_eq!(
                        js(&get(&args[1], "id")),
                        u("order"),
                        "amend_native_stable_id"
                    );
                }
                "constructor" => {
                    let calls = h.ops.borrow();
                    let cfg = &calls.iter().find(|v| v["name"] == "rateLimit").unwrap()["args"][1];
                    assert_eq!(
                        numeric(&get(&decode(cfg), "maxRPS")),
                        10.0,
                        "constructor_native_rate_limit"
                    );
                }
                _ => unreachable!(),
            }
        }
        if failure == "none" && profile == "batch-partial" && name == "placeNativeBatchChunk" {
            assert!(result.is_err(), "batch_native_partial_rejected");
        }
        if name == "placeTakeProfitStopLoss" && profile == "basic" && failure == "post#1" {
            assert_eq!(
                elements(result.as_ref().unwrap())
                    .iter()
                    .map(js)
                    .collect::<Vec<_>>(),
                vec![u("child-0")],
                "brackets_native_partial_retained"
            );
        }
        if name == "placeNativeBatchChunk" && profile == "basic" && failure == "none" {
            let calls = h.rest_calls.borrow();
            let args = &calls.iter().find(|(name, _)| name == "post").unwrap().1;
            assert_eq!(
                numeric(&get(&get(&args[2], "headers"), "part")),
                1.0,
                "batch_native_last_headers"
            );
        }
        if name == "updateOrder"
            && failure == "none"
            && matches!(
                profile,
                "amend-undefined"
                    | "bad-identity"
                    | "bad-size"
                    | "unsupported-status"
                    | "routing"
                    | "filled-size"
            )
        {
            assert!(result.is_err(), "amend_native_preflight_rejected");
            assert!(
                h.rest_calls.borrow().iter().all(|(name, _)| name != "put"),
                "amend_native_no_transport_after_preflight"
            );
        }
        if name == "updateOrder" && profile == "amend-no-update" && failure == "none" {
            let calls = h.rest_calls.borrow();
            let args = &calls.iter().find(|(name, _)| name == "put").unwrap().1;
            assert_eq!(
                js(&get(&args[1], "size")),
                u("2"),
                "amend_native_absent_size_retained"
            );
        }
        let outcome = match result {
            Ok(v) => json!({"ok":true,"value":atom(&v)}),
            Err(e) => {
                if e.primary {
                    assert!(
                        Rc::ptr_eq(&e, &h.primary),
                        "execution_native_error_identity"
                    );
                }
                json!({"ok":false,"primary":e.primary,"message":e.message})
            }
        };
        for (key, actual) in [
            ("outcome", outcome),
            ("ops", json!(*h.ops.borrow())),
            ("logs", json!(*h.logs.borrow())),
            ("emitted", json!(*h.emits.borrow())),
            ("writes", json!(*h.ctor_writes.borrow())),
        ] {
            assert_eq!(
                actual, case[key],
                "execution_supplemental_{key} {name}/{profile}/{failure}"
            );
        }
    }
    for case in corpus["books"].as_array().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let h = harness(
            obj(vec![("states", obj(vec![]))]),
            "listenOrderBook",
            profile,
            "none",
        );
        let symbol = text(u("BOOK-").into_iter().chain([0xd800]).collect());
        let cb0 = Rc::new(Node::Function(0));
        let cb1 = Rc::new(Node::Function(1));
        if profile == "existing" {
            h.callback_map.borrow_mut().push((
                js(&symbol),
                Rc::new(Node::Map(RefCell::new(vec![(js(&cb1), cb1.clone())]))),
            ));
        }
        let mut cleanup = api::listen_order_book(&h, symbol.clone(), cb0.clone()).unwrap();
        let captured = h.intrinsic("callbacks.get", vec![symbol.clone()]).unwrap();
        let after = json!({"size":numeric(&get(&captured,"size"))as usize,"keys":h.callback_map.borrow().iter().map(|(k,_)|k.clone()).collect::<Vec<_>>()});
        if profile == "basic" {
            assert_eq!(
                numeric(&get(&captured, "size")),
                1.0,
                "book_listener_native_callback_set"
            );
        }
        if profile == "duplicate" {
            let _additional = api::listen_order_book(&h, symbol.clone(), cb0).unwrap();
        }
        if profile == "two" {
            let _additional = api::listen_order_book(&h, symbol.clone(), cb1.clone()).unwrap();
        }
        if profile == "reject" {
            let symbol = h.rejection_symbols.borrow()[0].clone();
            let e = Rc::new(Failure {
                primary: false,
                message: u("slippage-").into_iter().chain([0xd800]).collect(),
                history: false,
            });
            api::book_slippage_rejected(&h, symbol, e).unwrap();
        }
        if profile == "replace" {
            h.callback_map.borrow_mut()[0].1 =
                Rc::new(Node::Map(RefCell::new(vec![(js(&cb1), cb1)])));
        }
        cleanup().unwrap();
        if profile == "repeat" {
            cleanup().unwrap();
        }
        let current = h
            .callback_map
            .borrow()
            .first()
            .map(|(_, v)| numeric(&get(v, "size")) as usize);
        let after_cleanup = json!({"capturedSize":numeric(&get(&captured,"size"))as usize,"currentSize":current,"keys":h.callback_map.borrow().iter().map(|(k,_)|k.clone()).collect::<Vec<_>>()});
        if profile == "replace" {
            assert!(
                h.callback_map.borrow().is_empty(),
                "book_native_captured_cleanup_deletes_replacement"
            );
        }
        if profile == "duplicate" {
            assert_eq!(
                h.ops
                    .borrow()
                    .iter()
                    .filter(|v| v["name"] == "subscribeBook")
                    .count(),
                2,
                "book_native_duplicate_resubscribes"
            );
        }
        if profile == "repeat" {
            assert_eq!(
                h.ops
                    .borrow()
                    .iter()
                    .filter(|v| v["name"] == "unsubscribeBook")
                    .count(),
                2,
                "book_native_repeat_cleanup"
            );
        }
        assert_eq!(
            after, case["afterListen"],
            "book_listener_supplemental_listen"
        );
        assert_eq!(
            after_cleanup, case["afterCleanup"],
            "book_listener_supplemental_cleanup"
        );
        assert_eq!(
            json!(*h.ops.borrow()),
            case["ops"],
            "book_listener_supplemental_ops"
        );
        assert_eq!(
            json!(*h.logs.borrow()),
            case["logs"],
            "book_listener_supplemental_logs"
        );
    }
    for case in corpus["classes"].as_array().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let allocation = RefCell::new(vec![]);
        let fields = RefCell::new(vec![]);
        let writes = RefCell::new(vec![]);
        let expected_prototype = obj(vec![(
            "class",
            raw("ParadexPrivateHistoryAuthNotReadyError"),
        )]);
        let primary = Rc::new(Failure {
            primary: true,
            message: u("primary"),
            history: false,
        });
        let original = if profile == "original" {
            text(u("cause-").into_iter().chain([0xd800]).collect())
        } else {
            undef()
        };
        let result = safe_cex_venue_paradex::auth_initialization::initialize_history_auth_error(
            &u("history-")
                .into_iter()
                .chain([0xd800])
                .collect::<Vec<_>>(),
            &original,
            |v| Ok(!matches!(v.as_ref(), Node::Undefined)),
            |v| Ok(v.clone()),
            |v| !matches!(v.as_ref(), Node::Undefined),
            |v| Ok(js(&v)),
            |_message| {
                api::allocate_history_auth_error(
                    |name| {
                        if profile == "allocation-error" {
                            return Err(primary.clone());
                        }
                        let prototype = if name == "ParadexPrivateHistoryAuthNotReadyError" {
                            expected_prototype.clone()
                        } else {
                            obj(vec![])
                        };
                        let allocated = Rc::new(Node::ClassObject {
                            fields: RefCell::new(vec![]),
                            prototype,
                        });
                        let Node::ClassObject { prototype, .. } = allocated.as_ref() else {
                            unreachable!()
                        };
                        assert!(
                            Rc::ptr_eq(prototype, &expected_prototype),
                            "history_class_native_prototype"
                        );
                        allocation.borrow_mut().push(json!({"name":name,"derived":Rc::ptr_eq(prototype, &expected_prototype)}));
                        Ok(allocated)
                    },
                    undef,
                    |parent, key, value, writable, enumerable, configurable| {
                        if profile == "field-error" {
                            return Err(primary.clone());
                        }
                        assert!(
                            writable && enumerable && configurable,
                            "history_class_native_field_descriptor"
                        );
                        assert_eq!(key, "originalError", "history_class_native_field_name");
                        fields.borrow_mut().push(json!({"key":key,"value":atom(&value),"writable":writable,"enumerable":enumerable,"configurable":configurable}));
                        set(parent, key, value);
                        Ok(())
                    },
                )
            },
            |parent, name| {
                writes
                    .borrow_mut()
                    .push(json!({"key":"name","value":atom(&raw(name))}));
                set(parent, "name", raw(name));
                Ok(())
            },
            |parent, original| {
                writes
                    .borrow_mut()
                    .push(json!({"key":"originalError","value":true}));
                set(parent, "originalError", original.clone());
                Ok(())
            },
        );
        let outcome = match result {
            Ok(parent) => {
                assert!(
                    Rc::ptr_eq(&get(&parent, "originalError"), &original),
                    "history_class_native_original_identity"
                );
                let Node::ClassObject { prototype, .. } = parent.as_ref() else {
                    panic!("history_class_native_allocation")
                };
                assert!(
                    Rc::ptr_eq(prototype, &expected_prototype),
                    "history_class_native_prototype"
                );
                json!({"ok":true,"derived":Rc::ptr_eq(prototype, &expected_prototype),"original":Rc::ptr_eq(&get(&parent, "originalError"), &original)})
            }
            Err(e) => {
                assert!(
                    Rc::ptr_eq(&e, &primary),
                    "history_class_native_failure_identity"
                );
                json!({"ok":false,"primary":true})
            }
        };
        assert_eq!(
            json!(*allocation.borrow()),
            case["allocation"],
            "history_class_supplemental_allocation"
        );
        assert_eq!(
            json!(*fields.borrow()),
            case["fields"],
            "history_class_supplemental_fields"
        );
        assert_eq!(
            json!(*writes.borrow()),
            case["writes"],
            "history_class_supplemental_writes"
        );
        assert_eq!(
            outcome, case["outcome"],
            "history_class_supplemental_outcome"
        );
    }
}
