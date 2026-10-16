use safe_cex_venue_paradex::public_ingress::{self as api, PublicIngressHost as Host};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Undefined,
    Null,
    Number(f64),
    Text(Vec<u16>),
    Object(&'static str, RefCell<BTreeMap<String, Raw>>),
    Array(RefCell<Vec<Raw>>),
    Function(usize),
}
#[derive(Debug)]
struct Failure;
type Error = Rc<Failure>;
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn units(prefix: &str) -> Vec<u16> {
    u(prefix).into_iter().chain([0xd800]).collect()
}
fn raw(s: &str) -> Raw {
    Rc::new(Node::Text(u(s)))
}
fn surrogate(s: &str) -> Raw {
    Rc::new(Node::Text(units(s)))
}
fn n(v: f64) -> Raw {
    Rc::new(Node::Number(v))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn obj(label: &'static str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object(
        label,
        RefCell::new(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    ))
}
fn arr(v: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn put(v: &Raw, k: &str, x: Raw) {
    if let Node::Object(_, m) = v.as_ref() {
        m.borrow_mut().insert(k.into(), x);
    } else {
        panic!("not object")
    }
}
fn field(v: &Raw, k: &str) -> Raw {
    match v.as_ref() {
        Node::Object(_, m) => m.borrow().get(k).cloned().unwrap_or_else(undef),
        Node::Array(a) => {
            if k == "length" {
                n(a.borrow().len() as f64)
            } else {
                a.borrow()
                    .get(k.parse::<usize>().unwrap_or(usize::MAX))
                    .cloned()
                    .unwrap_or_else(undef)
            }
        }
        _ => undef(),
    }
}
fn bits(v: f64) -> String {
    format!(
        "{:016x}",
        if v.is_nan() {
            0x7ff8000000000000
        } else {
            v.to_bits()
        }
    )
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn decode(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Undefined => json!({"undefined":true}),
        Node::Null => Value::Null,
        Node::Number(x) => json!({"number":bits(*x)}),
        Node::Text(s) => json!({"string":s}),
        Node::Object(_, m) => Value::Object(
            m.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), atom(v)))
                .collect(),
        ),
        Node::Array(a) => Value::Array(a.borrow().iter().map(atom).collect()),
        _ => panic!("not serializable"),
    }
}
fn text(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Number(v) => u(&safe_cex_num::js_number_string(*v)),
        Node::Text(s) => s.clone(),
        Node::Object(_, _) => u("[object Object]"),
        Node::Array(a) => a
            .borrow()
            .iter()
            .enumerate()
            .flat_map(|(i, v)| {
                if i == 0 {
                    text(v)
                } else {
                    u(",").into_iter().chain(text(v)).collect()
                }
            })
            .collect(),
        _ => panic!("no function text"),
    }
}
fn native_number(v: &Raw) -> f64 {
    match v.as_ref() {
        Node::Number(v) => *v,
        Node::Null => 0.0,
        Node::Undefined => f64::NAN,
        _ => safe_cex_num::number_from_string(&String::from_utf16(&text(v)).unwrap_or_default()),
    }
}
fn f(v: &Raw) -> f64 {
    let s = String::from_utf16(&text(v)).unwrap_or_default();
    s.parse().unwrap_or(f64::NAN)
}
struct Fixture {
    failure: String,
    error: Error,
    trace: RefCell<Vec<Vec<u16>>>,
    counts: RefCell<BTreeMap<String, usize>>,
    tickers: RefCell<Vec<api::FeedTicker>>,
    trades: RefCell<Vec<(Vec<u16>, f64, f64, f64)>>,
    logs: RefCell<Vec<Value>>,
    profile: String,
    event: RefCell<Raw>,
    handlers: Raw,
    tracked: Cell<usize>,
    stats: RefCell<Vec<Raw>>,
    calls: RefCell<Vec<String>>,
    deleted: RefCell<Vec<f64>>,
    retries: RefCell<Vec<(Raw, Raw, Raw)>>,
    pending: RefCell<Raw>,
    entries: RefCell<Vec<(Vec<u16>, Raw)>>,
    intervals: RefCell<BTreeMap<Vec<u16>, f64>>,
    cache: RefCell<Vec<(Vec<u16>, Raw)>>,
    times: RefCell<Vec<(Vec<u16>, f64)>>,
    now: Cell<f64>,
    candle_callbacks: RefCell<Vec<Raw>>,
    made: RefCell<Vec<Raw>>,
}
impl Fixture {
    fn new(profile: &str, failure: &str) -> Self {
        Self {
            failure: failure.into(),
            error: Rc::new(Failure),
            trace: RefCell::new(vec![]),
            counts: RefCell::new(BTreeMap::new()),
            tickers: RefCell::new(vec![]),
            trades: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            profile: profile.into(),
            event: RefCell::new(undef()),
            handlers: arr(vec![Rc::new(Node::Function(0)), Rc::new(Node::Function(1))]),
            tracked: Cell::new(0),
            stats: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            deleted: RefCell::new(vec![]),
            retries: RefCell::new(vec![]),
            pending: RefCell::new(undef()),
            entries: RefCell::new(vec![]),
            intervals: RefCell::new(BTreeMap::new()),
            cache: RefCell::new(vec![]),
            times: RefCell::new(vec![]),
            now: Cell::new(60000.0),
            candle_callbacks: RefCell::new(vec![]),
            made: RefCell::new(vec![]),
        }
    }
    fn touch(&self, key: &str) -> Result<(), Error> {
        let mut counts = self.counts.borrow_mut();
        let count = counts.entry(key.into()).or_default();
        let label = format!("{key}#{count}");
        *count += 1;
        self.trace.borrow_mut().push(u(&label));
        if self.failure == label {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn success(&self, r: Result<(), Error>, expected: bool) {
        match r {
            Ok(()) => assert!(expected, "public_native_success"),
            Err(e) => {
                assert!(!expected, "public_native_failure");
                assert!(Rc::ptr_eq(&e, &self.error), "public_native_error_identity");
            }
        }
    }
}
impl Host for Fixture {
    type Value = Raw;
    type Error = Error;
    type Log = ();
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if let Node::Object(label, _) = v.as_ref() {
            if *label == "raw" || *label == "data" {
                self.touch(&format!("{label}.{k}"))?;
            }
        }
        Ok(field(v, k))
    }
    fn optional_get(&self, v: &Raw, k: &str) -> Result<Raw, Error> {
        if self.is_null(v) || self.is_undefined(v) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn is_object(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Object(..) | Node::Array(_) | Node::Null)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Array(_))
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Undefined)
    }
    fn is_null(&self, v: &Raw) -> bool {
        matches!(v.as_ref(), Node::Null)
    }
    fn text_value(&self, v: Vec<u16>) -> Raw {
        Rc::new(Node::Text(v))
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn number(&self, x: f64) -> Raw {
        n(x)
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        match (a.as_ref(), b.as_ref()) {
            (Node::Undefined, Node::Undefined) | (Node::Null, Node::Null) => true,
            (Node::Number(a), Node::Number(b)) => a == b,
            (Node::Text(a), Node::Text(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        self.touch("string")?;
        Ok(text(v))
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        Ok(text(v))
    }
    fn parse_float(&self, v: &Raw) -> Result<f64, Error> {
        self.touch("parse")?;
        Ok(f(v))
    }
    fn to_number(&self, v: &Raw) -> Result<f64, Error> {
        self.touch("number")?;
        Ok(native_number(v))
    }
    fn coerce_number(&self, v: &Raw) -> Result<f64, Error> {
        Ok(native_number(v))
    }
    fn add(&self, v: &Raw, x: f64) -> Result<Raw, Error> {
        if let Node::Text(s) = v.as_ref() {
            Ok(self.text_value(
                s.iter()
                    .copied()
                    .chain(u(&safe_cex_num::js_number_string(x)))
                    .collect(),
            ))
        } else {
            Ok(n(native_number(v) + x))
        }
    }
    fn stringify(&self, v: &Raw) -> Result<Vec<u16>, Error> {
        fn quoted(v: &[u16]) -> Vec<u16> {
            let mut out = u("\"");
            for (i, c) in v.iter().copied().enumerate() {
                match c {
                    34 => out.extend(u("\\\"")),
                    92 => out.extend(u("\\\\")),
                    8 => out.extend(u("\\b")),
                    9 => out.extend(u("\\t")),
                    10 => out.extend(u("\\n")),
                    12 => out.extend(u("\\f")),
                    13 => out.extend(u("\\r")),
                    c if c < 32
                        || ((0xd800..=0xdbff).contains(&c)
                            && !v.get(i + 1).is_some_and(|c| (0xdc00..=0xdfff).contains(c)))
                        || ((0xdc00..=0xdfff).contains(&c)
                            && (i == 0 || !(0xd800..=0xdbff).contains(&v[i - 1]))) =>
                    {
                        out.extend(u(&format!("\\u{c:04x}")))
                    }
                    _ => out.push(c),
                }
            }
            out.push(34);
            out
        }
        fn serialize(h: &Fixture, v: &Raw) -> Result<Vec<u16>, Error> {
            match v.as_ref() {
                Node::Undefined | Node::Null => Ok(u("null")),
                Node::Number(x) => Ok(u(&safe_cex_num::js_number_string(*x))),
                Node::Text(v) => Ok(quoted(v)),
                Node::Array(a) => {
                    let mut out = u("[");
                    for (i, v) in a.borrow().iter().enumerate() {
                        if i > 0 {
                            out.push(44);
                        }
                        out.extend(serialize(h, v)?);
                    }
                    out.push(93);
                    Ok(out)
                }
                Node::Object(label, m) => {
                    h.get(v, "toJSON")?;
                    let order: &[&str] = if *label == "raw" {
                        &[
                            "market",
                            "bid_price",
                            "ask_price",
                            "index_price",
                            "symbol",
                            "bid",
                            "ask",
                            "last_traded_price",
                            "mark_price",
                            "oracle_price",
                            "price_change_rate_24h",
                            "volume_24h",
                            "open_interest",
                            "funding_rate",
                            "price",
                            "size",
                            "created_at",
                            "last_price",
                            "volume",
                            "timestamp",
                        ]
                    } else {
                        &["channel", "data"]
                    };
                    let keys: Vec<_> = order
                        .iter()
                        .filter(|k| m.borrow().contains_key(**k))
                        .map(|k| (*k).to_owned())
                        .collect();
                    let mut out = u("{");
                    let mut count = 0;
                    for key in keys {
                        let value = h.get(v, &key)?;
                        if h.is_undefined(&value) {
                            continue;
                        }
                        if count > 0 {
                            out.push(44);
                        }
                        count += 1;
                        out.extend(quoted(&u(&key)));
                        out.push(58);
                        out.extend(serialize(h, &value)?);
                    }
                    out.push(125);
                    Ok(out)
                }
                _ => panic!("not JSON data"),
            }
        }
        serialize(self, v)
    }
    fn split_optional(&self, v: &Raw, c: char) -> Result<Raw, Error> {
        if self.is_null(v) || self.is_undefined(v) {
            return Ok(arr(vec![]));
        }
        Ok(arr(text(v)
            .split(|v| *v == c as u16)
            .map(|v| self.text_value(v.to_vec()))
            .collect()))
    }
    fn object_keys_empty(&self, v: &Raw) -> Result<bool, Error> {
        Ok(match v.as_ref() {
            Node::Object(_, m) => m.borrow().is_empty(),
            Node::Array(a) => a.borrow().is_empty(),
            _ => true,
        })
    }
    fn empty_object(&self) -> Raw {
        obj("empty", vec![])
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        arr(v)
    }
    fn each(&self, v: &Raw, cb: &mut dyn FnMut(Raw) -> Result<(), Error>) -> Result<(), Error> {
        let Node::Array(a) = v.as_ref() else {
            panic!("not array")
        };
        let len = a.borrow().len();
        for i in 0..len {
            let value = a.borrow().get(i).cloned();
            if let Some(v) = value {
                cb(v)?;
            }
        }
        Ok(())
    }
    fn iterate(&self, v: &Raw, cb: &mut dyn FnMut(Raw) -> Result<(), Error>) -> Result<(), Error> {
        let Node::Array(a) = v.as_ref() else {
            panic!("not array")
        };
        let mut i = 0;
        loop {
            let v = a.borrow().get(i).cloned();
            let Some(v) = v else { break };
            i += 1;
            cb(v)?;
        }
        Ok(())
    }
    fn select_log(&self) -> Result<(), Error> {
        Ok(())
    }
    fn log(&self, _: (), s: Vec<u16>, severity: &'static str) -> Result<(), Error> {
        self.touch("log")?;
        self.logs
            .borrow_mut()
            .push(json!({"text":s,"severity":severity}));
        Ok(())
    }
    fn caught(&self, p: &str, e: Error) -> Result<(), Error> {
        assert!(Rc::ptr_eq(&e, &self.error));
        self.log(
            (),
            u(p).into_iter().chain(units("fixture-")).collect(),
            "error",
        )
    }
    fn update_ticker(&self, t: api::FeedTicker) -> Result<(), Error> {
        self.touch("ticker")?;
        self.tickers.borrow_mut().push(t);
        Ok(())
    }
    fn now(&self) -> Result<f64, Error> {
        if self.profile.starts_with("candle:") {
            let x = self.now.get();
            self.now.set(x + 1.0);
            Ok(x)
        } else {
            self.touch("now")?;
            Ok(60000.0)
        }
    }
    fn update_candle_from_trade(&self, s: Vec<u16>, p: f64, z: f64, t: f64) -> Result<(), Error> {
        self.touch("trade-candle")?;
        self.trades.borrow_mut().push((s, p, z, t));
        Ok(())
    }
    fn disposed(&self) -> Result<bool, Error> {
        Ok(self.profile == "disposed")
    }
    fn track(&self) -> Result<(), Error> {
        self.tracked.set(self.tracked.get() + 1);
        Ok(())
    }
    fn parse_event(&self, _: &Raw) -> Result<Raw, Error> {
        Ok(self.event.borrow().clone())
    }
    fn handlers(&self, t: &Raw) -> Result<Raw, Error> {
        Ok(if text(t) == u("order_book") || text(t) == u("ping") {
            self.handlers.clone()
        } else {
            undef()
        })
    }
    fn stats(&self, v: Raw) -> Result<(), Error> {
        self.stats.borrow_mut().push(v);
        Ok(())
    }
    fn call_handler(&self, h: Raw, d: Raw) -> Result<(), Error> {
        let Node::Function(i) = h.as_ref() else {
            panic!("not callback")
        };
        if self.profile.starts_with("candle:") {
            self.candle_callbacks.borrow_mut().push(d);
            if self.profile == "candle:grow" && *i == 0 {
                let key = u("BTC:2m");
                self.entries
                    .borrow_mut()
                    .push((key.clone(), arr(vec![Rc::new(Node::Function(1))])));
                self.intervals.borrow_mut().insert(key, 120000.0);
            }
            return Ok(());
        }
        let data = self.event.borrow().clone();
        let label = if Rc::ptr_eq(&data, &d) {
            "data"
        } else if let Node::Object("replacement", _) = d.as_ref() {
            "replacement"
        } else {
            "params"
        };
        self.calls.borrow_mut().push(label.into());
        if *i == 0 {
            if self.profile == "grow" {
                if let Node::Array(a) = self.handlers.as_ref() {
                    a.borrow_mut().push(Rc::new(Node::Function(2)));
                }
            }
            if self.profile == "replace" {
                put(
                    &data,
                    "params",
                    obj("replacement", vec![("channel", raw("replacement"))]),
                );
            }
        }
        Ok(())
    }
    fn pending(&self, _: Raw) -> Result<Raw, Error> {
        Ok(self.pending.borrow().clone())
    }
    fn delete_pending(&self, id: Raw) -> Result<(), Error> {
        self.deleted.borrow_mut().push(native_number(&id));
        *self.pending.borrow_mut() = undef();
        Ok(())
    }
    fn retry(&self, c: Raw, a: Raw, r: Raw) -> Result<(), Error> {
        self.retries.borrow_mut().push((c, a, r));
        Ok(())
    }
    fn visit_candles(
        &self,
        cb: &mut dyn FnMut(Vec<u16>, Raw) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut i = 0;
        loop {
            let item = self.entries.borrow().get(i).cloned();
            let Some((k, v)) = item else { break };
            i += 1;
            cb(k, v)?;
        }
        Ok(())
    }
    fn interval(&self, k: &[u16]) -> Result<Option<f64>, Error> {
        Ok(self.intervals.borrow().get(k).copied())
    }
    fn write_trade_time(&self, k: &[u16], v: f64) -> Result<(), Error> {
        self.times.borrow_mut().push((k.to_vec(), v));
        Ok(())
    }
    fn candle(&self, k: &[u16]) -> Result<Raw, Error> {
        Ok(self
            .cache
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef))
    }
    fn spread(&self, v: &Raw) -> Result<Raw, Error> {
        if let Node::Object(_, m) = v.as_ref() {
            Ok(Rc::new(Node::Object(
                "spread",
                RefCell::new(m.borrow().clone()),
            )))
        } else {
            panic!("no spread")
        }
    }
    fn make_candle(&self, c: api::FeedCandle<Raw>) -> Result<Raw, Error> {
        let value = c.spread.unwrap_or_else(|| obj("candle", vec![]));
        if let Some(timestamp) = c.timestamp {
            put(&value, "timestamp", timestamp);
        }
        if let Some(open) = c.open {
            put(&value, "open", open);
        }
        for (k, v) in [
            ("high", c.high),
            ("low", c.low),
            ("close", n(c.close)),
            ("volume", c.volume),
        ] {
            put(&value, k, v);
        }
        self.made.borrow_mut().push(value.clone());
        Ok(value)
    }
    fn write_candle(&self, k: &[u16], v: Raw) -> Result<(), Error> {
        let mut cache = self.cache.borrow_mut();
        if let Some((_, old)) = cache.iter_mut().find(|(key, _)| key == k) {
            *old = v;
        } else {
            cache.push((k.to_vec(), v));
        }
        Ok(())
    }
}
fn feed(row: &Value) {
    let kind = row["kind"].as_str().unwrap();
    let profile = row["profile"].as_str().unwrap();
    let h = Fixture::new(profile, row["failure"].as_str().unwrap());
    let body = obj(
        "raw",
        vec![
            ("market", surrogate("BTC-")),
            ("bid_price", raw("101.5")),
            ("ask_price", raw("102.5")),
            ("index_price", raw("103.5")),
            ("symbol", surrogate("BTC-")),
            ("bid", raw("101.5")),
            ("ask", raw("102.5")),
            ("last_traded_price", raw("100.25")),
            ("mark_price", raw("104.5")),
            ("oracle_price", raw("105.5")),
            ("price_change_rate_24h", raw("0.0125")),
            ("volume_24h", raw("9.5")),
            ("open_interest", raw("8.5")),
            ("funding_rate", raw("-0.125")),
            ("price", raw("100.25")),
            ("size", raw("3.5")),
            ("created_at", n(12345.0)),
        ],
    );
    if profile == "fallback" {
        for k in [
            "market",
            "symbol",
            "last_traded_price",
            "oracle_price",
            "volume_24h",
        ] {
            put(&body, k, raw(""));
        }
        for (k, v) in [
            ("last_price", "99.5"),
            ("index_price", "106.5"),
            ("volume", "7.25"),
        ] {
            put(&body, k, raw(v));
        }
        put(&body, "created_at", Rc::new(Node::Null));
        put(&body, "timestamp", n(54321.0));
    }
    if ["invalid", "zero", "negative"].contains(&profile) {
        let value = if profile == "invalid" {
            "NaN"
        } else if profile == "zero" {
            "-0"
        } else {
            "-2.5"
        };
        let keys = if let Node::Object(_, m) = body.as_ref() {
            m.borrow().keys().cloned().collect::<Vec<_>>()
        } else {
            unreachable!()
        };
        for k in keys {
            if k != "market" && k != "symbol" && (profile != "invalid" || k != "created_at") {
                put(&body, &k, raw(value));
            }
        }
        if profile == "invalid" {
            put(&body, "created_at", raw("bad"));
        }
    }
    if profile == "missing" {
        put(&body, "market", raw(""));
        put(&body, "symbol", raw(""));
    }
    let data = obj(
        "data",
        vec![
            (
                "channel",
                if profile == "missing" {
                    raw("")
                } else {
                    surrogate("trades.CHANNEL-")
                },
            ),
            (
                "data",
                if profile == "empty" {
                    arr(vec![])
                } else if profile == "array" {
                    arr(vec![Rc::new(Node::Null), n(0.0), body.clone(), body])
                } else if profile == "absent" {
                    undef()
                } else {
                    body
                },
            ),
        ],
    );
    let result = match kind {
        "bbo" => api::handle_bbo_update(&h, &data),
        "summary" => api::handle_summary_update(&h, &data),
        _ => api::handle_trades_update(&h, &data),
    };
    h.success(result, row["ok"].as_bool().unwrap());
    if profile == "basic" && h.failure == "none" {
        let t = h.tickers.borrow();
        assert_eq!(t.len(), 1, "feed_native_count");
        assert_eq!(t[0].symbol, units("BTC-"), "feed_native_symbol");
        match kind {
            "bbo" => assert_eq!(
                t[0].fields,
                vec![("bid", 101.5), ("ask", 102.5), ("index", 103.5)],
                "bbo_native_fields"
            ),
            "summary" => assert_eq!(
                t[0].fields,
                vec![
                    ("bid", 101.5),
                    ("ask", 102.5),
                    ("last", 100.25),
                    ("mark", 104.5),
                    ("index", 105.5),
                    ("percentage", 1.25),
                    ("volume", 9.5),
                    ("openInterest", 8.5),
                    ("fundingRate", -0.125)
                ],
                "summary_native_fields"
            ),
            _ => {
                assert_eq!(
                    t[0].fields,
                    vec![("last", 100.25), ("mark", 100.25)],
                    "trades_native_fields"
                );
                assert_eq!(
                    *h.trades.borrow(),
                    vec![(units("BTC-"), 100.25, 3.5, 12345.0)],
                    "trades_native_candle"
                );
            }
        }
    }
    let tickers: Vec<Value> = h
        .tickers
        .borrow()
        .iter()
        .map(|t| {
            let mut map = serde_json::Map::new();
            map.insert("symbol".into(), json!({"string":t.symbol}));
            for (k, v) in &t.fields {
                map.insert((*k).into(), json!({"number":bits(*v)}));
            }
            Value::Object(map)
        })
        .collect();
    let candles:Vec<Value>=h.trades.borrow().iter().map(|(s,p,z,t)|json!({"symbol":s,"price":bits(*p),"size":bits(*z),"timestamp":bits(*t)})).collect();
    assert_eq!(
        json!(tickers),
        row["tickers"],
        "feed_supplemental_tickers {kind}/{profile}/{}",
        h.failure
    );
    assert_eq!(json!(candles), row["candles"], "feed_supplemental_candles");
    assert_eq!(
        json!(*h.logs.borrow()),
        row["logs"],
        "feed_supplemental_logs {kind}/{profile}/{}",
        h.failure
    );
    assert_eq!(
        json!(*h.trace.borrow()),
        row["trace"],
        "feed_supplemental_trace {kind}/{profile}/{}",
        h.failure
    );
    for feed in [api::ThrottledFeed::Bbo, api::ThrottledFeed::Summary] {
        let v =
            api::install_feed_throttle(feed, |f, w, l, t| Ok::<_, Error>((f, w, l, t))).unwrap();
        assert_eq!(v, (feed, 100.0, true, true), "feed_native_throttle");
    }
}
fn constructor(row: &Value) {
    let h = Fixture::new("constructor", row["failure"].as_str().unwrap());
    let parent = Rc::new(());
    let base = Cell::new(false);
    let writes = RefCell::new(vec![]);
    let testnet = row["testnet"].as_bool().unwrap();
    let result = api::initialize_public_socket(
        &parent,
        |p| {
            h.touch("base")?;
            assert!(Rc::ptr_eq(p, &parent));
            base.set(true);
            Ok(())
        },
        || {
            h.touch("urls")?;
            Ok(())
        },
        |p| {
            assert!(Rc::ptr_eq(p, &parent));
            h.touch("options")?;
            h.touch("testnet")?;
            Ok(testnet)
        },
        |_, network| {
            h.touch("lookup")?;
            Ok(u(if network {
                "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
            } else {
                "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
            }))
        },
        |v| {
            h.touch("write")?;
            writes.borrow_mut().push(v);
            Ok(())
        },
    );
    h.success(result, row["ok"].as_bool().unwrap());
    if h.failure == "none" {
        assert_eq!(
            writes.borrow()[0],
            u(if testnet {
                "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
            } else {
                "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
            }),
            "constructor_native_url"
        );
    }
    assert_eq!(base.get(), row["base"]);
    assert_eq!(json!(*writes.borrow()), row["writes"]);
    assert_eq!(json!(*h.trace.borrow()), row["trace"]);
}
fn retry(row: &Value) {
    let h = Fixture::new("retry", row["failure"].as_str().unwrap());
    let channel = decode(&row["channel"]);
    let reason = if row["reason"].is_null() {
        None
    } else {
        Some(decode(&row["reason"]))
    };
    let attempt = number(&row["attempt"]);
    let removed = RefCell::new(vec![]);
    let delays = RefCell::new(vec![]);
    let next = Cell::new(0.0);
    let scheduled = Cell::new(false);
    let added = Cell::new(0);
    let deleted = Cell::new(0);
    let sends = RefCell::new(vec![]);
    let result = api::retry_subscribe_channel(
        &channel,
        attempt,
        reason.as_deref(),
        || {
            h.touch("active")?;
            Ok(row["active"].as_bool().unwrap())
        },
        |s| {
            h.touch("remove-sent")?;
            removed.borrow_mut().push(s.to_vec());
            Ok(())
        },
        |t| h.log((), t, "warning"),
        |delay, a| {
            h.touch("schedule")?;
            delays.borrow_mut().push(delay);
            next.set(a);
            scheduled.set(true);
            Ok(77)
        },
        |id| {
            assert_eq!(id, 77, "retry_native_timer_identity");
            h.touch("add")?;
            added.set(added.get() + 1);
            Ok(())
        },
    );
    h.success(result, row["ok"].as_bool().unwrap());
    if h.failure == "none" {
        let expected = attempt < 2.0 || attempt.is_nan();
        let expected = expected && row["active"] == true;
        assert_eq!(scheduled.get(), expected, "retry_native_schedule");
        if expected {
            assert_eq!(
                bits(delays.borrow()[0]),
                bits(1000.0 * (attempt + 1.0)),
                "retry_native_delay"
            );
            assert_eq!(
                bits(next.get()),
                bits(attempt + 1.0),
                "retry_native_next_attempt"
            );
            assert_eq!(added.get(), 1, "retry_native_registration");
        }
    }
    let before = json!({"added":added.get(),"removed":*removed.borrow(),"logs":*h.logs.borrow(),"delays":delays.borrow().iter().map(|x|bits(*x)).collect::<Vec<_>>()});
    let mut callback_ok = None;
    if scheduled.get() {
        let r = api::execute_subscription_retry(
            || {
                h.touch("delete")?;
                deleted.set(deleted.get() + 1);
                Ok(())
            },
            || {
                h.touch("disposed")?;
                Ok(row["disposed"].as_bool().unwrap())
            },
            || {
                h.touch("active")?;
                Ok(row["afterActive"].as_bool().unwrap())
            },
            || {
                h.touch("send")?;
                sends.borrow_mut().push(
                    json!({"method":"subscribe","channel":channel,"attempt":bits(next.get())}),
                );
                Ok(())
            },
        );
        callback_ok = Some(r.is_ok());
        h.success(r, row["callbackOk"].as_bool().unwrap());
    }
    if h.failure == "none" && scheduled.get() {
        assert_eq!(deleted.get(), 1, "retry_native_deleted");
        assert_eq!(
            sends.borrow().len(),
            usize::from(row["disposed"] == false && row["afterActive"] == true),
            "retry_native_deferred_send"
        );
    }
    assert_eq!(before, row["before"], "retry_supplemental_before");
    assert_eq!(json!(callback_ok), row["callbackOk"]);
    assert_eq!(deleted.get(), row["deleted"]);
    assert_eq!(json!(*sends.borrow()), row["sends"]);
    assert_eq!(json!(*h.trace.borrow()), row["trace"]);
}
fn recovery(row: &Value) {
    let h = Fixture::new("recovery", row["failure"].as_str().unwrap());
    let now = number(&row["now"]);
    let last = number(&row["last"]);
    let recovered = number(&row["recovered"]);
    let written = Cell::new(None);
    let sends = RefCell::new(vec![]);
    let result = api::maybe_recover_ohlcv_trade_stream(
        || {
            h.touch("now")?;
            Ok(now)
        },
        || {
            h.touch("trade-time")?;
            Ok(Some(last))
        },
        || {
            h.touch("recovery-time")?;
            Ok(Some(recovered))
        },
        |t| {
            h.touch("write-recovery")?;
            written.set(Some(t));
            Ok(())
        },
        || Ok((units("BTC-"), u("1m"))),
        |t| h.log((), t, "warning"),
        |force| {
            h.touch("subscribe")?;
            sends.borrow_mut().push(force);
            Ok(())
        },
    );
    h.success(result, row["ok"].as_bool().unwrap());
    if h.failure == "none" {
        let normal_last = if last.is_nan() || last == 0.0 {
            0.0
        } else {
            last
        };
        let normal_recovered = if recovered.is_nan() || recovered == 0.0 {
            0.0
        } else {
            recovered
        };
        // Negations kept deliberately: these mirror the source's recovery
        // windows, where a NaN timestamp must satisfy the guard. `partial_cmp`
        // or inverted operators would flip that case.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let expected = !(normal_last > 0.0 && now - normal_last <= 20000.0)
            && !(now - normal_recovered < 30000.0);
        assert_eq!(written.get().is_some(), expected, "recovery_native_written");
        assert_eq!(
            *sends.borrow(),
            if expected { vec![true] } else { vec![] },
            "recovery_native_force"
        );
        if expected {
            assert_eq!(
                bits(written.get().unwrap()),
                bits(now),
                "recovery_native_time"
            );
            assert_eq!(
                h.logs.borrow()[0],
                json!({"text":u("No Paradex trade updates for ").into_iter().chain(units("BTC-")).chain(u(" 1m; re-subscribing trade channel")).collect::<Vec<_>>(),"severity":"warning"}),
                "recovery_native_warning"
            );
        }
    }
    assert_eq!(json!(written.get().map(bits)), row["written"]);
    let sends: Vec<Value> = sends
        .borrow()
        .iter()
        .map(|force| json!({"symbol":units("BTC-"),"force":force}))
        .collect();
    assert_eq!(json!(sends), row["sends"]);
    assert_eq!(json!(*h.logs.borrow()), row["logs"]);
    assert_eq!(json!(*h.trace.borrow()), row["trace"]);
}
fn candle(row: &Value) {
    let profile = row["profile"].as_str().unwrap();
    let h = Fixture::new(&format!("candle:{profile}"), "none");
    let price = if profile == "unchanged" {
        104.0
    } else if profile == "nan" {
        f64::NAN
    } else if profile == "zero" {
        -0.0
    } else {
        105.0
    };
    let size = if profile == "unchanged" {
        0.0
    } else if profile == "zero" {
        -0.0
    } else {
        2.0
    };
    let timestamp = if profile == "next" {
        180000.0
    } else {
        120500.0
    };
    let previous = obj(
        "candle",
        vec![
            ("timestamp", n(120000.0)),
            ("open", n(100.0)),
            ("high", n(if profile == "zero" { -0.0 } else { 110.0 })),
            ("low", n(if profile == "zero" { 0.0 } else { 90.0 })),
            ("close", n(104.0)),
            (
                "volume",
                if profile == "string-volume" {
                    raw("3")
                } else {
                    n(3.0)
                },
            ),
            ("extra", surrogate("kept-")),
        ],
    );
    if profile != "new" {
        h.cache.borrow_mut().push((u("BTC:1m"), previous.clone()));
    }
    h.entries
        .borrow_mut()
        .push((u("OTHER:1m"), arr(vec![Rc::new(Node::Function(99))])));
    h.entries
        .borrow_mut()
        .push((u("BTC:1m"), arr(vec![Rc::new(Node::Function(0))])));
    h.intervals.borrow_mut().insert(u("OTHER:1m"), 60000.0);
    h.intervals.borrow_mut().insert(
        u("BTC:1m"),
        if profile == "no-interval" {
            0.0
        } else {
            60000.0
        },
    );
    api::update_ohlcv_from_trade(&h, &u("BTC"), price, size, timestamp).unwrap();
    if profile == "basic" {
        let cache = h.cache.borrow();
        let c = &cache[0].1;
        assert_eq!(
            native_number(&field(c, "timestamp")),
            120000.0,
            "candle_native_timestamp"
        );
        assert_eq!(
            native_number(&field(c, "open")),
            100.0,
            "candle_native_open"
        );
        assert_eq!(
            native_number(&field(c, "high")),
            110.0,
            "candle_native_high"
        );
        assert_eq!(native_number(&field(c, "low")), 90.0, "candle_native_low");
        assert_eq!(
            native_number(&field(c, "close")),
            105.0,
            "candle_native_close"
        );
        assert_eq!(
            native_number(&field(c, "volume")),
            5.0,
            "candle_native_volume"
        );
        assert_eq!(
            text(&field(c, "extra")),
            units("kept-"),
            "candle_native_spread"
        );
        assert_eq!(
            h.candle_callbacks.borrow().len(),
            1,
            "candle_native_callback_count"
        );
        assert!(
            Rc::ptr_eq(&h.candle_callbacks.borrow()[0], c),
            "candle_native_callback_identity"
        );
        assert!(!Rc::ptr_eq(c, &previous), "candle_native_new_object");
    }
    if profile == "unchanged" {
        assert_eq!(
            h.candle_callbacks.borrow().len(),
            0,
            "candle_native_unchanged"
        );
        assert!(Rc::ptr_eq(&h.cache.borrow()[0].1, &previous));
    }
    if profile == "grow" {
        assert_eq!(
            h.candle_callbacks.borrow().len(),
            2,
            "candle_native_live_map"
        );
    }
    if profile == "zero" {
        assert_eq!(
            native_number(&field(&h.cache.borrow()[0].1, "high")).to_bits(),
            (-0.0f64).to_bits(),
            "candle_native_negative_zero_high"
        );
        assert_eq!(
            native_number(&field(&h.cache.borrow()[0].1, "low")).to_bits(),
            (-0.0f64).to_bits(),
            "candle_native_negative_zero_low"
        );
    }
    let cache: Vec<_> = h
        .cache
        .borrow()
        .iter()
        .map(|(k, v)| json!({"key":k,"value":atom(v)}))
        .collect();
    let times: Vec<_> = h
        .times
        .borrow()
        .iter()
        .map(|(k, v)| json!({"key":k,"value":bits(*v)}))
        .collect();
    let callbacks: Vec<_> = h.candle_callbacks.borrow().iter().map(atom).collect();
    assert_eq!(
        json!(cache),
        row["cache"],
        "candle_supplemental_cache {profile}"
    );
    assert_eq!(json!(times), row["times"]);
    assert_eq!(json!(callbacks), row["callbacks"]);
}
fn message(row: &Value) {
    let profile = row["profile"].as_str().unwrap();
    let h = Fixture::new(profile, "none");
    let params = obj(
        "params",
        vec![(
            "channel",
            if profile == "unknown" {
                surrogate("unknown-")
            } else {
                Rc::new(Node::Text(
                    u("order_book.BTC-")
                        .into_iter()
                        .chain([0xd800])
                        .chain(u(".snapshot"))
                        .collect(),
                ))
            },
        )],
    );
    let data = if profile == "null" {
        Rc::new(Node::Null)
    } else if profile == "legacy" {
        obj("message", vec![("type", raw("ping"))])
    } else if profile == "error" || profile == "success" {
        let v = obj("message", vec![("jsonrpc", raw("2.0")), ("id", n(7.0))]);
        *h.pending.borrow_mut() = obj(
            "pending",
            vec![("channel", surrogate("trades.BTC-")), ("attempt", n(1.0))],
        );
        if profile == "error" {
            put(
                &v,
                "error",
                obj("error", vec![("message", surrogate("bad-"))]),
            );
        }
        v
    } else {
        obj(
            "message",
            vec![
                ("jsonrpc", raw("2.0")),
                ("method", raw("subscription")),
                ("params", params),
            ],
        )
    };
    *h.event.borrow_mut() = data;
    api::on_public_message(&h, &raw("wire")).unwrap();
    assert_eq!(
        h.tracked.get(),
        usize::from(profile != "disposed"),
        "message_native_tracked"
    );
    if profile == "basic" {
        assert_eq!(
            text(&h.stats.borrow()[0]),
            units("order_book.BTC-"),
            "message_native_stats"
        );
        assert_eq!(
            *h.calls.borrow(),
            vec!["params", "params"],
            "message_native_calls"
        );
    }
    if profile == "replace" {
        assert_eq!(
            *h.calls.borrow(),
            vec!["params", "replacement"],
            "message_native_fresh_params"
        );
    }
    if profile == "grow" {
        assert_eq!(
            *h.calls.borrow(),
            vec!["params", "params"],
            "message_native_captured_length"
        );
    }
    if profile == "error" {
        assert_eq!(
            *h.deleted.borrow(),
            vec![7.0],
            "message_native_pending_delete"
        );
        assert_eq!(h.retries.borrow().len(), 1, "message_native_retry");
    }
    let stats: Vec<_> = h.stats.borrow().iter().map(atom).collect();
    let retries:Vec<_>=h.retries.borrow().iter().map(|(c,a,r)|json!({"channel":text(c),"attempt":bits(native_number(a)),"reason":text(r)})).collect();
    let len = if let Node::Array(a) = h.handlers.as_ref() {
        a.borrow().len()
    } else {
        unreachable!()
    };
    assert_eq!(json!(stats), row["stats"]);
    assert_eq!(json!(*h.calls.borrow()), row["calls"]);
    assert_eq!(json!(*h.logs.borrow()), row["logs"]);
    assert_eq!(json!(retries), row["retries"]);
    assert_eq!(
        json!(
            h.deleted
                .borrow()
                .iter()
                .map(|v| bits(*v))
                .collect::<Vec<_>>()
        ),
        row["deleted"]
    );
    assert_eq!(len, row["handlerCount"]);
}
#[test]
fn actual_typescript_public_ingress() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_ingress_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source_oracle_exit: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let all: Value = serde_json::from_slice(&output.stdout).unwrap();
    for row in all["constructors"].as_array().unwrap() {
        constructor(row);
    }
    for row in all["feeds"].as_array().unwrap() {
        feed(row);
    }
    for row in all["retries"].as_array().unwrap() {
        retry(row);
    }
    for row in all["recoveries"].as_array().unwrap() {
        recovery(row);
    }
    for row in all["candles"].as_array().unwrap() {
        candle(row);
    }
    for row in all["messages"].as_array().unwrap() {
        message(row);
    }
    let count: usize = all
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_array().unwrap().len())
        .sum();
    assert_eq!(count, 1950, "source_case_count");
    println!("public ingress source cases: {count}");
}
