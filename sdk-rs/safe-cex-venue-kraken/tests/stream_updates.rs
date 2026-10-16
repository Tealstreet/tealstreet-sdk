use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_date_parse_utf16, js_number_string, js_time_clip, lodash_camel_case, lodash_snake_case,
    parse_float,
};
use safe_cex_types::Candle;
use safe_cex_venue_kraken::stream_updates::{
    self, CandleUpdateHost, Lookup, NumericChanges, StreamValues, TickerUpdateHost,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .env("TZ", "UTC")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/stream-updates-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 3);
        assert_eq!(value["header"]["tz"], "UTC");
        value
    })
}
fn flag(config: &Value, key: &str) -> bool {
    config[key].as_bool().unwrap_or(false)
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u16)
        .collect()
}
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn number(bits: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
fn assert_number(actual: f64, expected: &Value, label: &str) {
    let bits = expected.as_str().unwrap();
    if bits == "7ff8000000000000" {
        assert!(actual.is_nan(), "{label}: native NaN")
    } else {
        assert_eq!(format!("{:016x}", actual.to_bits()), bits, "{label}")
    }
}
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(Rc<RefCell<Vec<Rc<Raw>>>>),
    Object(RefCell<Vec<(String, Rc<Raw>)>>),
    ToText(Vec<u16>),
    Throw,
    BigInt(String),
}
struct Items {
    array: Rc<RefCell<Vec<Rc<Raw>>>>,
    cursor: usize,
}
impl Iterator for Items {
    type Item = Result<Rc<Raw>, ExchangeError>;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.array.borrow().get(self.cursor)?.clone();
        self.cursor += 1;
        Some(Ok(value))
    }
}
struct Ticker {
    marker: f64,
}
type Handlers = RefCell<Vec<Option<usize>>>;
enum Event {
    Field(&'static str, Lookup),
    Parse,
    Lookup(Vec<u16>),
    Update(bool, f64, NumericChanges),
    Handlers(Vec<u16>),
    Stats(&'static str),
    Candle(usize, bool, Candle),
}
struct Host {
    config: Value,
    message: Rc<Raw>,
    array: Rc<RefCell<Vec<Rc<Raw>>>>,
    input: Vec<(String, Rc<Raw>)>,
    ticker: Rc<Ticker>,
    handlers: Rc<Handlers>,
    events: RefCell<Vec<Event>>,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
    appended: Cell<bool>,
}
fn decode(value: &Value) -> Rc<Raw> {
    Rc::new(match value["kind"].as_str().unwrap() {
        "undefined" => Raw::Undefined,
        "null" => Raw::Null,
        "bool" => Raw::Bool(value["value"].as_bool().unwrap()),
        "number" => Raw::Number(number(value["bits"].as_str().unwrap())),
        "text" => Raw::Text(units(&value["units"])),
        "array" => Raw::Array(Rc::new(RefCell::new(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect(),
        ))),
        "object" => Raw::ToText(units(&value["units"])),
        "throw" => Raw::Throw,
        "bigint" => Raw::BigInt(value["text"].as_str().unwrap().into()),
        _ => panic!("independent raw tag"),
    })
}
impl Host {
    fn new(kind: &str, config: Value) -> Self {
        let text = |s: &str| Rc::new(Raw::Text(utf16(s)));
        let num = |n| Rc::new(Raw::Number(n));
        let mut input: Vec<(String, Rc<Raw>)> = if kind == "Futures" {
            vec![
                ("product_id", text("PF_X")),
                ("bid", text("17.25tail")),
                ("ask", num(19.5)),
                ("last", num(23.75)),
                ("mark", num(29.125)),
                ("index", num(31.5)),
                ("relative_funding_rate", num(37.25)),
                ("next_funding_rate_time", num(41.125)),
                ("open24h", num(5.5)),
            ]
        } else if kind == "Spot" {
            vec![
                ("symbol", text("BTC/USD")),
                ("bid", text("17.25tail")),
                ("ask", num(19.5)),
                ("last", num(23.75)),
                ("volume", num(43.125)),
                ("change_pct", num(47.5)),
            ]
        } else {
            vec![
                ("symbol", text("BTC/USD")),
                ("interval", num(1.0)),
                ("interval_begin", text("2026-08-14T12:34:56.789Z")),
                ("open", text("17.25tail")),
                ("high", num(19.5)),
                ("low", num(23.75)),
                ("close", num(29.125)),
                ("volume", num(31.5)),
            ]
        }
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
        if let Some(fields) = config["fields"].as_object() {
            for (key, value) in fields {
                if let Some(row) = input.iter_mut().find(|row| &row.0 == key) {
                    row.1 = decode(value)
                } else {
                    input.push((key.clone(), decode(value)))
                }
            }
        }
        let item = if flag(&config, "nullItem") {
            Rc::new(Raw::Null)
        } else {
            Rc::new(Raw::Object(RefCell::new(input.clone())))
        };
        let array = Rc::new(RefCell::new(if flag(&config, "empty") {
            vec![]
        } else {
            vec![item.clone()]
        }));
        let message = if kind == "Futures" {
            item
        } else {
            let data = if flag(&config, "notArray") {
                Rc::new(Raw::Object(RefCell::new(vec![])))
            } else {
                Rc::new(Raw::Array(array.clone()))
            };
            Rc::new(Raw::Object(RefCell::new(vec![("data".into(), data)])))
        };
        let mut error = ExchangeError::new("stream failure");
        error.name = "StreamError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        Self {
            handlers: Rc::new(RefCell::new(if flag(&config, "noHandlers") {
                vec![]
            } else {
                vec![Some(0), Some(1)]
            })),
            config,
            message,
            array,
            input,
            ticker: Rc::new(Ticker { marker: 53.125 }),
            events: RefCell::new(vec![]),
            error: RefCell::new(Some(error)),
            pointer,
            appended: Cell::new(false),
        }
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            Err(self.error.borrow_mut().take().unwrap())
        } else {
            Ok(())
        }
    }
    fn text(&self, value: &Raw) -> Result<Vec<u16>, ExchangeError> {
        Ok(match value {
            Raw::Undefined => utf16("undefined"),
            Raw::Null => utf16("null"),
            Raw::Bool(value) => utf16(if *value { "true" } else { "false" }),
            Raw::Number(value) => utf16(&js_number_string(*value)),
            Raw::Text(value) | Raw::ToText(value) => value.clone(),
            Raw::Object(_) => utf16("[object Object]"),
            Raw::BigInt(text) => utf16(text),
            Raw::Throw => return Err(self.error.borrow_mut().take().unwrap()),
            Raw::Array(array) => {
                let mut out = vec![];
                for (i, value) in array.borrow().iter().enumerate() {
                    if i > 0 {
                        out.push(44)
                    }
                    if !self.nullish(value) {
                        out.extend(self.text(value)?);
                    }
                }
                out
            }
        })
    }
    fn append(&self) {
        if flag(&self.config, "append") && !self.appended.replace(true) {
            self.array
                .borrow_mut()
                .push(Rc::new(Raw::Object(RefCell::new(self.input.clone()))))
        }
    }
}
fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
impl StreamValues for Host {
    type Value = Raw;
    type Items = Items;
    fn field(
        &self,
        value: &Raw,
        key: &'static str,
        lookup: Lookup,
    ) -> Result<Rc<Raw>, ExchangeError> {
        if lookup == Lookup::Direct && matches!(value, Raw::Null | Raw::Undefined) {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(value, Raw::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        self.events.borrow_mut().push(Event::Field(key, lookup));
        self.fail(&format!("field-{key}"))?;
        match value {
            Raw::Object(fields) => {
                let fields = fields.borrow();
                let keys = if lookup == Lookup::GetKey {
                    vec![
                        key.to_owned(),
                        lodash_snake_case(key),
                        lodash_camel_case(key),
                    ]
                } else {
                    vec![key.to_owned()]
                };
                for key in keys {
                    if let Some(row) = fields.iter().find(|row| row.0 == key) {
                        return Ok(row.1.clone());
                    }
                }
                Ok(Rc::new(Raw::Undefined))
            }
            _ => Ok(Rc::new(Raw::Undefined)),
        }
    }
    fn nullish(&self, value: &Raw) -> bool {
        matches!(value, Raw::Undefined | Raw::Null)
    }
    fn truthy(&self, value: &Raw) -> bool {
        match value {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(value) => *value,
            Raw::Number(value) => *value != 0.0 && !value.is_nan(),
            Raw::Text(value) => !value.is_empty(),
            _ => true,
        }
    }
    fn parse_float(&self, value: &Raw) -> Result<f64, ExchangeError> {
        self.events.borrow_mut().push(Event::Parse);
        self.fail("parse")?;
        let text = self.text(value)?;
        Ok(parse_float(&String::from_utf16_lossy(&text)))
    }
    fn array_data(&self, message: &Raw) -> Result<Option<Items>, ExchangeError> {
        let value = self.field(message, "data", Lookup::Direct)?;
        Ok(if let Raw::Array(array) = value.as_ref() {
            Some(Items {
                array: array.clone(),
                cursor: 0,
            })
        } else {
            None
        })
    }
}
impl TickerUpdateHost for Host {
    type Ticker = Ticker;
    fn ticker_for(&self, symbol: &Raw) -> Result<Option<Rc<Ticker>>, ExchangeError> {
        let key = self.text(symbol)?;
        self.events.borrow_mut().push(Event::Lookup(key));
        self.fail("lookup")?;
        if flag(&self.config, "changeLookup") {
            let row = self
                .array
                .borrow()
                .first()
                .cloned()
                .unwrap_or_else(|| self.message.clone());
            if let Raw::Object(fields) = row.as_ref() {
                let mut fields = fields.borrow_mut();
                if let Some(bid) = fields.iter_mut().find(|row| row.0 == "bid") {
                    bid.1 = Rc::new(Raw::Number(101.25));
                }
            }
        }
        Ok(if flag(&self.config, "noTicker") {
            None
        } else {
            Some(self.ticker.clone())
        })
    }
    fn update_ticker(
        &self,
        ticker: Rc<Ticker>,
        changes: NumericChanges,
    ) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Update(
            Rc::ptr_eq(&ticker, &self.ticker),
            ticker.marker,
            changes,
        ));
        self.fail("update")?;
        self.append();
        Ok(())
    }
}
impl CandleUpdateHost for Host {
    type Handlers = Handlers;
    fn strict_number_eq(&self, value: &Raw, number: f64) -> bool {
        matches!(value,Raw::Number(value)if *value==number)
    }
    fn string_units(&self, value: &Raw) -> Result<Vec<u16>, ExchangeError> {
        self.text(value)
    }
    fn handlers(&self, key: &[u16]) -> Result<Rc<Handlers>, ExchangeError> {
        self.events.borrow_mut().push(Event::Handlers(key.to_vec()));
        self.fail("handlers")?;
        Ok(
            if flag(&self.config, "onlyAlias") && key.ends_with(&utf16("::1m")) {
                Rc::new(RefCell::new(vec![]))
            } else {
                self.handlers.clone()
            },
        )
    }
    fn handler_length(&self, handlers: &Handlers) -> Result<usize, ExchangeError> {
        Ok(handlers.borrow().len())
    }
    fn record_stats(&self, feed: &'static str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Stats(feed));
        self.fail("stats")?;
        if flag(&self.config, "clearStats") {
            self.handlers.borrow_mut().clear()
        }
        Ok(())
    }
    fn date_ms(&self, value: &Raw) -> Result<f64, ExchangeError> {
        Ok(match value {
            Raw::Undefined => f64::NAN,
            Raw::Null => 0.0,
            Raw::Bool(value) => f64::from(u8::from(*value)),
            Raw::Number(value) => js_time_clip(*value),
            Raw::BigInt(_) => return Err(type_error("Cannot convert a BigInt value to a number")),
            _ => js_date_parse_utf16(&self.text(value)?, |ms| ms),
        })
    }
    fn candle_for_each(
        &self,
        handlers: Rc<Handlers>,
        candle: Rc<RefCell<Candle>>,
    ) -> Result<(), ExchangeError> {
        let first = candle.clone();
        let length = handlers.borrow().len();
        for cursor in 0..length {
            let callback = handlers.borrow().get(cursor).copied().flatten();
            if let Some(index) = callback {
                self.events.borrow_mut().push(Event::Candle(
                    index,
                    Rc::ptr_eq(&first, &candle),
                    candle.borrow().clone(),
                ));
                self.fail("callback")?;
                if index == 0 && flag(&self.config, "mutateCandle") {
                    candle.borrow_mut().close = 59.25
                }
                if index == 0 && flag(&self.config, "removeSecond") {
                    handlers.borrow_mut()[1] = None
                }
                if index == 0 {
                    self.append();
                }
            }
        }
        Ok(())
    }
}
fn run(kind: &str, body: fn(&Host, &Raw) -> Result<(), ExchangeError>) {
    let cases = oracle()[kind].as_array().unwrap();
    assert_eq!(
        cases.len(),
        if kind == "Candle" { 45 } else { 31 },
        "STREAM independent cases"
    );
    let mut total = 0;
    let mut updates = 0;
    let mut candles = 0;
    let mut failures = 0;
    for case in cases {
        let host = Host::new(kind, case["config"].clone());
        match body(&host, &host.message) {
            Ok(()) => assert!(case["failure"].is_null()),
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                if case["failure"]["identity"] == true {
                    assert_eq!(
                        (&*error.metadata as *const _) as usize,
                        host.pointer,
                        "STREAM native original error identity"
                    )
                } else {
                    assert_eq!(error.name, "TypeError")
                }
            }
        }
        let events = host.events.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(events.len(), expected.len(), "STREAM native event count");
        total += events.len();
        for (event, value) in events.iter().zip(expected) {
            match event {
                Event::Field(key, lookup) => {
                    assert_eq!(value["kind"], "field");
                    assert_eq!(*key, value["key"], "STREAM native getter order");
                    assert_eq!(
                        value["lookup"],
                        if *lookup == Lookup::GetKey {
                            "v"
                        } else {
                            "direct"
                        }
                    )
                }
                Event::Parse => assert_eq!(value["kind"], "parse"),
                Event::Lookup(key) => {
                    assert_eq!(value["kind"], "lookup");
                    assert_eq!(*key, units(&value["key"]), "STREAM native lookup UTF16")
                }
                Event::Update(identity, marker, changes) => {
                    updates += 1;
                    assert_eq!(value["kind"], "update");
                    assert!(*identity, "STREAM native original ticker identity");
                    assert_eq!(value["identity"], true);
                    assert_number(
                        *marker,
                        &value["marker"]["number"],
                        "STREAM native ticker marker",
                    );
                    let entries = value["entries"].as_array().unwrap();
                    assert_eq!(changes.len(), entries.len(), "STREAM native patch presence");
                    for ((key, number), expected) in changes.iter().zip(entries) {
                        assert_eq!(*key, expected["key"], "STREAM native patch key");
                        assert_number(
                            *number,
                            &expected["number"],
                            &format!("STREAM native patch {key}"),
                        );
                    }
                }
                Event::Handlers(key) => {
                    assert_eq!(value["kind"], "handlers");
                    assert_eq!(*key, units(&value["key"]), "STREAM native timeframe key")
                }
                Event::Stats(feed) => {
                    assert_eq!(value["kind"], "stats");
                    assert_eq!(*feed, value["feed"])
                }
                Event::Candle(index, identity, candle) => {
                    candles += 1;
                    assert_eq!(value["kind"], "candle");
                    assert_eq!(*index, value["index"].as_u64().unwrap() as usize);
                    assert!(*identity, "STREAM native shared candle identity");
                    assert_eq!(value["identity"], true);
                    assert!(candle.last_side.is_none());
                    assert_eq!(value["lastSide"], false);
                    for (key, number) in [
                        ("timestamp", candle.timestamp),
                        ("open", candle.open),
                        ("high", candle.high),
                        ("low", candle.low),
                        ("close", candle.close),
                        ("volume", candle.volume),
                    ] {
                        assert_number(
                            number,
                            &value["fields"][key]["number"],
                            &format!("STREAM native candle {key}"),
                        );
                    }
                }
            }
        }
    }
    let expected = match kind {
        "Futures" => (480, 23, 0, 6),
        "Spot" => (328, 22, 0, 7),
        "Candle" => (479, 0, 40, 12),
        _ => panic!("case family"),
    };
    assert_eq!(
        (total, updates, candles, failures),
        expected,
        "STREAM independent event/update/candle/failure floors"
    );
}
#[test]
fn futures_handle_ticker_live_source() {
    run("Futures", stream_updates::futures_handle_ticker)
}
#[test]
fn spot_handle_ticker_live_source() {
    run("Spot", stream_updates::spot_handle_ticker)
}
#[test]
fn spot_handle_ohlc_live_source() {
    run("Candle", stream_updates::spot_handle_ohlc)
}
