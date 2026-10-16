use safe_cex_venue_paradex::market_data::{self as api, DataFuture, MarketDataHost as Host};
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
    Error(Error),
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(RefCell<Vec<(String, Raw)>>),
    Array(RefCell<Vec<Raw>>),
    Map(RefCell<Vec<(Vec<u16>, Raw)>>),
}
#[derive(Debug)]
struct Failure {
    primary: bool,
    message: Vec<u16>,
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
        fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
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
        Node::Error(_) => panic!("error escaped native atom"),
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => json!({"n":bits(*n)}),
        Node::Text(s) => json!({"s":s}),
        Node::Object(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Map(_) => panic!("map escaped native"),
    }
}
fn decode(v: &Value) -> Raw {
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
    let fields = v["o"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_owned(), decode(&p[1])))
        .collect();
    Rc::new(Node::Object(RefCell::new(fields)))
}
fn get(v: &Raw, key: &str) -> Raw {
    match v.as_ref() {
        Node::Error(e) => match key {
            "message" => text(e.message.clone()),
            "stack" => raw("STACK"),
            _ => undef(),
        },
        Node::Object(o) => o
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef),
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
    let Node::Object(o) = v.as_ref() else {
        panic!("write non object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, v)) = o.iter_mut().find(|(k, _)| k == key) {
        *v = item;
    } else {
        o.push((key.into(), item));
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
        Node::Object(o) => format!(
            "{{{}}}",
            o.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v.as_ref(), Node::Undefined))
                .map(|(k, v)| format!("{}:{}", quote(&u(k)), stringify(v)))
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
            self.record(&name, args.clone())?;
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
            set(&store, k, v.clone());
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
            "Math.min" => number(numeric(&a(0)).min(numeric(&a(1)))),
            "Math.abs" => number(numeric(&a(0)).abs()),
            "member" => {
                let key = String::from_utf16_lossy(&js(&a(1)));
                let own = get(&a(0), &key);
                if !self.is_undefined(&own) {
                    own
                } else if key == "__proto__" {
                    obj(vec![])
                } else if key == "constructor" {
                    raw("function Object() { [native code] }")
                } else {
                    own
                }
            }
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
                    .map(|(k, _)| k.clone())
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
                if let Some((_, v)) = m.iter_mut().find(|(k, _)| *k == key) {
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
#[test]
fn source_oracle() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/market_data_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source instrument: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 4320);
    for case in cases.as_array().unwrap() {
        let input = decode(&case["input"]);
        let name = case["name"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let h = Harness {
            disposed: Cell::new(matches!(
                get(&get(&input, "states"), "isDisposed").as_ref(),
                Node::Bool(true)
            )),
            input,
            name: name.into(),
            profile: profile.into(),
            failure: failure.into(),
            primary: Rc::new(Failure {
                primary: true,
                message: u("primary-").into_iter().chain([0xd800]).collect(),
            }),
            counts: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            updates: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            ticker_updates: RefCell::new(vec![]),
            broadcasts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            callback_ids: RefCell::new(vec![0, 1]),
            native_books: RefCell::new(vec![]),
            public_connections: Cell::new(0),
            native_tick_delay: Cell::new(None),
        };
        let args = elements(&h.input("args"));
        let result = match name {
            "start" => block_on(api::start(&h)),
            "tick" => block_on(api::tick(&h)),
            "fetchMarkets" => block_on(api::fetch_markets(&h)),
            "fetchTickers" => block_on(api::fetch_tickers(&h)),
            "fetchOHLCV" => block_on(api::fetch_ohlcv(&h, args[0].clone())),
            "fetchPositions" => block_on(api::fetch_positions(&h)),
            "fetchOrderBook" => {
                block_on(api::fetch_order_book(&h, args[0].clone(), args[1].clone()))
            }
            "handleOrderbookUpdate" => api::handle_orderbook_update(&h, args[0].clone()),
            _ => unreachable!(),
        };
        if profile == "basic" && failure == "none" {
            match name {
                "start" => assert_eq!(
                    h.public_connections.get(),
                    1,
                    "start_native_public_connection"
                ),
                "tick" => assert_eq!(
                    h.native_tick_delay.get().unwrap().to_bits(),
                    10000.0f64.to_bits(),
                    "tick_native_schedule"
                ),
                "fetchMarkets" => assert_eq!(
                    numeric(&get(
                        &get(
                            &get(&elements(result.as_ref().unwrap())[0], "limits"),
                            "leverage"
                        ),
                        "max"
                    )),
                    40.0,
                    "markets_native_max_leverage"
                ),
                "fetchTickers" => assert_eq!(
                    numeric(&get(&elements(result.as_ref().unwrap())[0], "timestamp")),
                    555000.0,
                    "tickers_native_timestamp"
                ),
                "fetchOHLCV" => assert_eq!(
                    numeric(&get(&elements(result.as_ref().unwrap())[0], "timestamp")),
                    123.0,
                    "ohlcv_native_timestamp"
                ),
                "fetchPositions" => assert_eq!(
                    numeric(&get(&elements(result.as_ref().unwrap())[0], "contracts")),
                    2.0,
                    "positions_native_actual_precedence"
                ),
                "fetchOrderBook" => assert_eq!(
                    numeric(&get(
                        &elements(&get(result.as_ref().unwrap(), "bids"))[0],
                        "total"
                    )),
                    198.0,
                    "book_native_bid_total"
                ),
                "handleOrderbookUpdate" => {
                    let books = h.native_books.borrow();
                    let levels = elements(&get(&books[0], "bids"));
                    assert_eq!(
                        numeric(&get(&levels[0], "total")),
                        198.0,
                        "book_ingress_native_broadcast"
                    );
                    assert_eq!(
                        numeric(&get(&levels[0], "price")),
                        99.0,
                        "book_ingress_native_best_bid"
                    );
                }
                _ => unreachable!(),
            }
        }
        if failure == "none" {
            if name == "start" && profile.starts_with("dispose-") {
                let expected = match profile {
                    "dispose-fetchMarkets" => 0,
                    "dispose-fetchTickers" => 1,
                    "dispose-fetchOrders" | "dispose-fetchLeverageSettings" | "dispose-tick" => 2,
                    _ => 3,
                };
                assert_eq!(
                    h.updates.borrow().len(),
                    expected,
                    "start_native_disposal_updates"
                );
            }
            if name == "tick" && profile.starts_with("dispose-") {
                let expected = match profile {
                    "dispose-fetchPositions" => 0,
                    "dispose-fetchBalance" => 1,
                    "dispose-fetchOrders" => 2,
                    "dispose-fetchLeverageSettings" => 3,
                    _ => 3,
                };
                assert_eq!(
                    h.updates.borrow().len(),
                    expected,
                    "tick_native_disposal_updates"
                );
                if matches!(
                    profile,
                    "dispose-fetchPositions"
                        | "dispose-fetchBalance"
                        | "dispose-fetchOrders"
                        | "dispose-fetchLeverageSettings"
                ) {
                    assert!(
                        h.native_tick_delay.get().is_none(),
                        "tick_native_disposal_no_timer"
                    );
                }
            }
            if name == "handleOrderbookUpdate" && profile == "callback-growth" {
                let books = h.native_books.borrow();
                assert_eq!(books.len(), 3, "book_ingress_native_live_set_growth");
                assert!(
                    Rc::ptr_eq(&books[0], &books[2]),
                    "book_ingress_native_broadcast_identity"
                );
            }
            if name == "fetchPositions" && profile == "basic" {
                let actual = elements(result.as_ref().unwrap());
                let rows = elements(&get(&h.input("data"), "results"));
                assert!(
                    Rc::ptr_eq(&get(&actual[0], "info"), &rows[0]),
                    "positions_native_info_identity"
                );
            }
        }
        let outcome = match &result {
            Ok(v) => json!({"ok":true,"value":atom(v)}),
            Err(e) => {
                if e.primary {
                    assert!(Rc::ptr_eq(e, &h.primary), "data_native_error_identity");
                }
                json!({"ok":false,"primary":e.primary,"message":e.message})
            }
        };
        for (key, actual) in [
            ("outcome", outcome),
            ("ops", json!(*h.ops.borrow())),
            ("updates", json!(*h.updates.borrow())),
            ("timers", json!(*h.timers.borrow())),
            ("tickerUpdates", json!(*h.ticker_updates.borrow())),
            ("broadcast", json!(*h.broadcasts.borrow())),
            ("logs", json!(*h.logs.borrow())),
            ("disposed", json!(h.disposed.get())),
        ] {
            assert_eq!(
                actual, case[key],
                "data_supplemental_{key} {name}/{profile}/{failure}"
            );
        }
    }
}
