use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_table_lookup, lodash_camel_case, lodash_snake_case, parse_float};
use safe_cex_venue_kraken::{
    book_listeners::{self, Feed, Frame, Key, Listener, ListenerHost},
    orderbooks::{self, Book, BookHost, Level, Side},
    stream_updates::{Lookup, StreamValues},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[derive(Clone, Debug)]
enum Raw {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    A(Vec<Rc<Raw>>),
    O(Vec<(String, Rc<Raw>)>),
}
fn text(s: &str) -> Rc<Raw> {
    Rc::new(Raw::S(s.encode_utf16().collect()))
}
fn raw(v: &Value) -> Rc<Raw> {
    if let Some(tag) = v.get("$number").and_then(Value::as_str) {
        return Rc::new(Raw::N(match tag {
            "nan" => f64::NAN,
            "inf" => f64::INFINITY,
            "negzero" => -0.0,
            _ => panic!("number tag"),
        }));
    }
    if let Some(units) = v.get("$units").and_then(Value::as_array) {
        return Rc::new(Raw::S(
            units.iter().map(|n| n.as_u64().unwrap() as u16).collect(),
        ));
    }
    if v.get("$undefined") == Some(&Value::Bool(true)) {
        return undefined();
    }
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(v) => Raw::B(*v),
        Value::Number(v) => Raw::N(v.as_f64().unwrap()),
        Value::String(v) => Raw::S(v.encode_utf16().collect()),
        Value::Array(v) => Raw::A(v.iter().map(raw).collect()),
        Value::Object(v) => Raw::O(v.iter().map(|(k, v)| (k.clone(), raw(v))).collect()),
    })
}
fn obj(fields: Vec<(&str, Rc<Raw>)>) -> Rc<Raw> {
    Rc::new(Raw::O(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
fn undefined() -> Rc<Raw> {
    Rc::new(Raw::U)
}
fn boolraw(v: bool) -> Rc<Raw> {
    Rc::new(Raw::B(v))
}
fn truth(v: &Raw) -> bool {
    match v {
        Raw::U | Raw::Null => false,
        Raw::B(v) => *v,
        Raw::N(v) => *v != 0.0 && !v.is_nan(),
        Raw::S(v) => !v.is_empty(),
        _ => true,
    }
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::U => "undefined".encode_utf16().collect(),
        Raw::Null => "null".encode_utf16().collect(),
        Raw::S(v) => v.clone(),
        Raw::B(v) => v.to_string().encode_utf16().collect(),
        Raw::N(v) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
        Raw::O(_) => "[object Object]".encode_utf16().collect(),
        Raw::A(v) => v
            .iter()
            .map(|v| {
                if matches!(**v, Raw::U | Raw::Null) {
                    String::new()
                } else {
                    String::from_utf16_lossy(&string(v))
                }
            })
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
    }
}
fn assert_native(actual: &Raw, expected: &Value, label: &str) {
    match actual {
        Raw::U => assert_eq!(
            expected,
            &serde_json::json!({"u":true}),
            "{label} native undefined"
        ),
        Raw::Null => assert!(expected.is_null(), "{label} native null"),
        Raw::B(v) => assert_eq!(Some(*v), expected.as_bool(), "{label} native bool"),
        Raw::N(v) => {
            let bits = expected["n"]
                .as_str()
                .map(|s| u64::from_str_radix(s, 16).unwrap())
                .unwrap_or_else(|| panic!("{label} native number tag {expected}"));
            if f64::from_bits(bits).is_nan() {
                assert!(v.is_nan(), "{label} native NaN")
            } else {
                assert_eq!(v.to_bits(), bits, "{label} native number")
            }
        }
        Raw::S(v) => assert_eq!(
            v,
            &expected["s"]
                .as_array()
                .unwrap_or_else(|| panic!("{label} native UTF16 tag {expected}"))
                .iter()
                .map(|x| x.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
            "{label} native UTF16"
        ),
        Raw::A(v) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(v.len(), expected.len(), "{label} native array length");
            for (a, e) in v.iter().zip(expected) {
                assert_native(a, e, label)
            }
        }
        Raw::O(v) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(v.len(), expected.len(), "{label} native object presence");
            for (k, a) in v {
                assert_native(a, &expected[k], label)
            }
        }
    }
}
struct Event {
    name: String,
    args: Vec<Rc<Raw>>,
}
#[derive(Default)]
struct Cache {
    bids: RefCell<Vec<(f64, f64)>>,
    asks: RefCell<Vec<(f64, f64)>>,
}
impl Cache {
    fn side(&self, side: Side) -> &RefCell<Vec<(f64, f64)>> {
        if side == Side::Bids {
            &self.bids
        } else {
            &self.asks
        }
    }
}
struct MapSelection {
    cache: Rc<Cache>,
    side: Side,
}
struct Host {
    config: Value,
    family: String,
    operation: String,
    events: RefCell<Vec<Event>>,
    cache: RefCell<Option<Rc<Cache>>>,
    handlers: Rc<RefCell<Vec<Option<usize>>>>,
    retained: RefCell<Option<Level>>,
    original: RefCell<Option<ExchangeError>>,
    pointer: usize,
    options: RefCell<HashMap<&'static str, Rc<Raw>>>,
    connected: Cell<bool>,
    options_pointer: Cell<Option<usize>>,
    maps: RefCell<HashMap<(Feed, Vec<u16>), Vec<Rc<usize>>>>,
    polls: RefCell<Vec<Rc<Listener<Host>>>>,
    first_poll: Cell<Option<usize>>,
    first_frame: Cell<Option<usize>>,
    faulted: Cell<bool>,
    same: Rc<usize>,
    other: Rc<usize>,
}
fn number(v: f64) -> Rc<Raw> {
    Rc::new(Raw::N(v))
}
fn side_name(side: Side) -> &'static str {
    if side == Side::Bids { "bids" } else { "asks" }
}
fn feed_name(feed: Feed) -> &'static str {
    if feed == Feed::Book { "book" } else { "ohlc" }
}
fn same_key(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}
fn insert(levels: &mut Vec<(f64, f64)>, price: f64, quantity: f64) {
    let price = if price == 0.0 { 0.0 } else { price };
    if let Some((_, q)) = levels.iter_mut().find(|(p, _)| same_key(*p, price)) {
        *q = quantity
    } else {
        levels.push((price, quantity))
    }
}
fn numeric(v: &Raw) -> f64 {
    match v {
        Raw::N(v) => *v,
        Raw::U => f64::NAN,
        Raw::Null => 0.0,
        Raw::B(v) => f64::from(*v),
        Raw::S(v) => safe_cex_num::number_from_string(&String::from_utf16_lossy(v)),
        _ => f64::NAN,
    }
}
impl Host {
    fn new(config: &Value, family: &str, operation: &str) -> Self {
        let mut e = ExchangeError::new("book failure");
        e.name = "BookError".into();
        let pointer = (&*e.metadata as *const _) as usize;
        let same = Rc::new(0);
        let other = Rc::new(1);
        let symbol = config.get("symbol").map(raw).unwrap_or_else(|| {
            text(if operation == "listener" {
                "BTC/雪"
            } else {
                "PF_雪"
            })
        });
        let interval = config
            .get("interval")
            .map(raw)
            .unwrap_or_else(|| text("1m"));
        let key = if family == "ohlc" {
            let mut k = string(&symbol);
            k.extend([58, 58]);
            k.extend(string(&interval));
            k
        } else {
            string(&symbol)
        };
        let feed = if family == "ohlc" {
            Feed::Ohlc
        } else {
            Feed::Book
        };
        let initial = if config["existing"] == true {
            vec![same.clone(), other.clone(), same.clone()]
        } else {
            vec![]
        };
        let host = Self {
            config: config.clone(),
            family: family.into(),
            operation: operation.into(),
            events: RefCell::new(vec![]),
            cache: RefCell::new(None),
            handlers: Rc::new(RefCell::new(if config["noHandlers"] == true {
                vec![]
            } else {
                vec![Some(0), Some(1)]
            })),
            retained: RefCell::new(None),
            original: RefCell::new(Some(e)),
            pointer,
            options: RefCell::new(HashMap::from([("symbol", symbol), ("interval", interval)])),
            connected: Cell::new(config["connected"] == true),
            options_pointer: Cell::new(None),
            maps: RefCell::new(HashMap::from([((feed, key), initial)])),
            polls: RefCell::new(vec![]),
            first_poll: Cell::new(None),
            first_frame: Cell::new(None),
            faulted: Cell::new(false),
            same,
            other,
        };
        if config["noCache"] != true {
            host.cache.replace(Some(host.make_cache(true)));
        }
        host
    }
    fn flag(&self, key: &str) -> bool {
        self.config[key] == true
    }
    fn error(&self) -> ExchangeError {
        self.original
            .borrow_mut()
            .take()
            .expect("original error is available")
    }
    fn identity(&self, e: &ExchangeError) -> bool {
        (&*e.metadata as *const _) as usize == self.pointer
    }
    fn fresh(&self, name: &str) -> ExchangeError {
        let mut e = ExchangeError::new("bound intrinsic failure");
        e.name = name.into();
        e
    }
    fn event(&self, name: &str, args: Vec<Rc<Raw>>) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        });
        if self.config["fail"].as_str() == Some(name) && !self.faulted.replace(true) {
            Err(self.error())
        } else {
            Ok(())
        }
    }
    fn make_cache(&self, initial: bool) -> Rc<Cache> {
        let cache = Rc::new(Cache::default());
        if initial {
            for (side, key, default) in [
                (
                    Side::Bids,
                    "cacheBids",
                    serde_json::json!([[5.5, 7.25], [17.25, 11.5]]),
                ),
                (
                    Side::Asks,
                    "cacheAsks",
                    serde_json::json!([[41.25, 43.5], [37.75, 47.125]]),
                ),
            ] {
                for row in self.config.get(key).unwrap_or(&default).as_array().unwrap() {
                    insert(
                        &mut cache.side(side).borrow_mut(),
                        numeric(&raw(&row[0])),
                        numeric(&raw(&row[1])),
                    );
                }
            }
        }
        cache
    }
    fn key(&self, key: &Key<Raw>) -> Vec<u16> {
        match key {
            Key::Value(v) => string(v),
            Key::Units(v) => v.clone(),
        }
    }
    fn compare(&self, row: &Value, label: &str) {
        let events = self.events.borrow();
        let expected = row["trace"].as_array().unwrap();
        assert_eq!(
            events.len(),
            expected.len(),
            "{label} native event count {} {}",
            self.config,
            self.family
        );
        for (i, (actual, expected)) in events.iter().zip(expected).enumerate() {
            let expected = expected.as_array().unwrap();
            assert_eq!(
                actual.name,
                expected[0].as_str().unwrap(),
                "{label} native event {i} {}",
                self.config
            );
            assert_eq!(
                actual.args.len(),
                expected.len() - 1,
                "{label} native arity"
            );
            for (a, e) in actual.args.iter().zip(&expected[1..]) {
                assert_native(a, e, label)
            }
        }
    }
    fn final_cache(&self) -> Rc<Raw> {
        match self.cache.borrow().as_ref() {
            None => Rc::new(Raw::Null),
            Some(cache) => {
                let entries = |side| {
                    Rc::new(Raw::A(
                        cache
                            .side(side)
                            .borrow()
                            .iter()
                            .map(|(p, q)| Rc::new(Raw::A(vec![number(*p), number(*q)])))
                            .collect(),
                    ))
                };
                obj(vec![
                    ("bids", entries(Side::Bids)),
                    ("asks", entries(Side::Asks)),
                ])
            }
        }
    }
    fn failure(&self, result: Result<(), ExchangeError>, expected: &Value) {
        match result {
            Ok(()) => assert!(
                expected.is_null(),
                "BOOK native fulfillment {}",
                self.config
            ),
            Err(error) => {
                assert_eq!(
                    error.name,
                    expected["name"].as_str().unwrap(),
                    "BOOK native error name"
                );
                assert_eq!(
                    self.identity(&error),
                    expected["identity"].as_bool().unwrap(),
                    "BOOK native original error identity"
                )
            }
        }
    }
}
impl StreamValues for Host {
    type Value = Raw;
    type Items = std::vec::IntoIter<Result<Rc<Raw>, ExchangeError>>;
    fn field(
        &self,
        value: &Raw,
        key: &'static str,
        lookup: Lookup,
    ) -> Result<Rc<Raw>, ExchangeError> {
        if matches!(value, Raw::U | Raw::Null) && lookup == Lookup::Direct {
            return Err(self.fresh("TypeError"));
        }
        if lookup == Lookup::GetKey || matches!(value, Raw::O(_)) {
            self.event(
                "field",
                vec![
                    text(key),
                    text(if lookup == Lookup::GetKey {
                        "v"
                    } else {
                        "direct"
                    }),
                ],
            )?;
        }
        if self.operation == "listener" {
            assert_eq!(
                Some(value as *const Raw as usize),
                self.options_pointer.get(),
                "LISTEN native original options identity"
            );
            return Ok(self
                .options
                .borrow()
                .get(key)
                .cloned()
                .unwrap_or_else(undefined));
        }
        if key == "bids" && self.flag("recacheOnBids")
            || key == "qty" && lookup == Lookup::Direct && self.flag("recacheOnQty")
        {
            self.cache.replace(Some(self.make_cache(true)));
        }
        let Raw::O(fields) = value else {
            return Ok(undefined());
        };
        let keys = if lookup == Lookup::GetKey {
            vec![key.into(), lodash_snake_case(key), lodash_camel_case(key)]
        } else {
            vec![key.into()]
        };
        for key in keys {
            if let Some((_, value)) = fields.iter().find(|(k, _)| k == &key) {
                return Ok(value.clone());
            }
        }
        Ok(undefined())
    }
    fn nullish(&self, value: &Raw) -> bool {
        matches!(value, Raw::U | Raw::Null)
    }
    fn truthy(&self, value: &Raw) -> bool {
        truth(value)
    }
    fn parse_float(&self, value: &Raw) -> Result<f64, ExchangeError> {
        self.event("parse", vec![])?;
        Ok(parse_float(&String::from_utf16_lossy(&string(value))))
    }
    fn array_data(&self, _message: &Raw) -> Result<Option<Self::Items>, ExchangeError> {
        panic!("unrelated stream intrinsic")
    }
}
impl BookHost for Host {
    type Handlers = RefCell<Vec<Option<usize>>>;
    type Cache = Cache;
    type Map = MapSelection;
    fn handlers(&self, symbol: &Raw) -> Result<Rc<Self::Handlers>, ExchangeError> {
        self.event("handlers", vec![Rc::new(Raw::S(string(symbol)))])?;
        Ok(self.handlers.clone())
    }
    fn handler_length(&self, handlers: &Self::Handlers) -> Result<usize, ExchangeError> {
        Ok(handlers.borrow().len())
    }
    fn stats(&self, feed: &'static str) -> Result<(), ExchangeError> {
        self.event("stats", vec![text(feed)])?;
        if self.flag("clearStats") {
            self.handlers.borrow_mut().clear()
        }
        Ok(())
    }
    fn iterable_or_empty(&self, value: Rc<Raw>) -> Result<Self::Items, ExchangeError> {
        if !truth(&value) {
            return Ok(vec![].into_iter());
        }
        match &*value {
            Raw::A(v) => Ok(v.iter().cloned().map(Ok).collect::<Vec<_>>().into_iter()),
            Raw::S(v) => Ok(v
                .iter()
                .map(|unit| Ok(Rc::new(Raw::S(vec![*unit]))))
                .collect::<Vec<_>>()
                .into_iter()),
            _ => Err(self.fresh("TypeError")),
        }
    }
    fn reset_cache(&self, symbol: &Raw) -> Result<(), ExchangeError> {
        self.event("reset", vec![Rc::new(Raw::S(string(symbol)))])?;
        self.cache.replace(Some(self.make_cache(false)));
        Ok(())
    }
    fn cache(&self, symbol: &Raw) -> Result<Option<Rc<Cache>>, ExchangeError> {
        self.event("cache", vec![Rc::new(Raw::S(string(symbol)))])?;
        Ok(self.cache.borrow().clone())
    }
    fn required_side(&self, symbol: &Raw, side: Side) -> Result<Rc<MapSelection>, ExchangeError> {
        let cache = self.cache(symbol)?.ok_or_else(|| self.fresh("TypeError"))?;
        Ok(Rc::new(MapSelection { cache, side }))
    }
    fn set(&self, map: Rc<MapSelection>, price: f64, quantity: f64) -> Result<(), ExchangeError> {
        self.event(
            "set",
            vec![text(side_name(map.side)), number(price), number(quantity)],
        )?;
        assert!(
            Rc::ptr_eq(&map.cache, self.cache.borrow().as_ref().unwrap()),
            "BOOK native current map identity"
        );
        insert(&mut map.cache.side(map.side).borrow_mut(), price, quantity);
        Ok(())
    }
    fn delete(&self, map: Rc<MapSelection>, price: f64) -> Result<(), ExchangeError> {
        self.event("delete", vec![text(side_name(map.side)), number(price)])?;
        map.cache
            .side(map.side)
            .borrow_mut()
            .retain(|(p, _)| !same_key(*p, price));
        Ok(())
    }
    fn is_buy(&self, value: &Raw) -> bool {
        matches!(value,Raw::S(v) if *v=="buy".encode_utf16().collect::<Vec<_>>())
    }
    fn contract_size_or_one(&self, symbol: &Raw) -> Result<Rc<Raw>, ExchangeError> {
        self.event("market", vec![Rc::new(symbol.clone())])?;
        let value = if self.flag("noMarket") {
            undefined()
        } else {
            self.config
                .get("contractSize")
                .map(raw)
                .unwrap_or_else(|| number(2.75))
        };
        Ok(if truth(&value) { value } else { number(1.0) })
    }
    fn sorted_entries(&self, cache: &Cache, side: Side) -> Result<Vec<(f64, f64)>, ExchangeError> {
        self.event("entries", vec![text(side_name(side))])?;
        let mut entries = cache.side(side).borrow().clone();
        for i in 1..entries.len() {
            let mut j = i;
            while j > 0 {
                let compare = if side == Side::Bids {
                    entries[j - 1].0 - entries[j].0
                } else {
                    entries[j].0 - entries[j - 1].0
                };
                if compare < 0.0 {
                    entries.swap(j, j - 1);
                    j -= 1
                } else {
                    break;
                }
            }
        }
        Ok(entries)
    }
    fn multiply(&self, amount: f64, contract: &Raw) -> Result<f64, ExchangeError> {
        Ok(amount * numeric(contract))
    }
    fn emit(&self, symbol: Rc<Raw>) -> Result<(), ExchangeError> {
        self.event("emit", vec![symbol])
    }
    fn for_each(
        &self,
        handlers: Rc<Self::Handlers>,
        mut build: impl FnMut() -> Rc<RefCell<Book>>,
    ) -> Result<(), ExchangeError> {
        let length = handlers.borrow().len();
        let mut prior: Option<Rc<RefCell<Book>>> = None;
        for index in 0..length {
            if handlers.borrow().get(index).copied().flatten().is_none() {
                continue;
            }
            let book = build();
            if let Some(prior) = prior.as_ref() {
                assert!(
                    !Rc::ptr_eq(prior, &book),
                    "BOOK native fresh outer identity"
                );
                assert!(
                    Rc::ptr_eq(&prior.borrow().asks, &book.borrow().asks),
                    "BOOK native shared array identity"
                )
            }
            let levels = |side: &Rc<RefCell<Vec<Level>>>| {
                Rc::new(Raw::A(
                    side.borrow()
                        .iter()
                        .map(|level| {
                            let level = level.borrow();
                            obj(vec![
                                ("price", number(level.price)),
                                ("amount", number(level.amount)),
                                ("total", number(level.total)),
                            ])
                        })
                        .collect(),
                ))
            };
            let value = {
                let book = book.borrow();
                obj(vec![
                    ("bids", levels(&book.bids)),
                    ("asks", levels(&book.asks)),
                ])
            };
            self.event("callback", vec![number(index as f64), value])?;
            if index == 0 && self.flag("mutateLevels") {
                if let Some(level) = book.borrow().bids.borrow_mut().first_mut() {
                    level.borrow_mut().amount = 59.25
                }
            }
            if index == 0 && self.flag("replaceBids") {
                book.borrow_mut().bids = Rc::new(RefCell::new(vec![]))
            }
            if index == 0 && self.flag("deleteSecond") {
                handlers.borrow_mut()[1] = None
            }
            if index == 0 && self.flag("appendCallback") {
                handlers.borrow_mut().push(Some(2))
            }
            if self.flag("retainLevel") {
                if index == 0 && !book.borrow().bids.borrow().is_empty() {
                    let original = book.borrow().bids.borrow()[0].clone();
                    let retained = book.borrow().bids.borrow_mut().remove(0);
                    assert!(
                        Rc::ptr_eq(&original, &retained),
                        "BOOK native retained level identity"
                    );
                    self.retained.replace(Some(retained));
                }
                if index == 1 {
                    if let Some(retained) = self.retained.borrow().as_ref() {
                        retained.borrow_mut().amount = 91.25;
                    }
                }
                let retained = self.retained.borrow().as_ref().unwrap().clone();
                let present = book
                    .borrow()
                    .bids
                    .borrow()
                    .iter()
                    .any(|level| Rc::ptr_eq(level, &retained));
                let fields = retained.borrow();
                self.event(
                    "retained",
                    vec![
                        obj(vec![
                            ("price", number(fields.price)),
                            ("amount", number(fields.amount)),
                            ("total", number(fields.total)),
                        ]),
                        boolraw(present),
                    ],
                )?;
            }
            prior = Some(book);
        }
        Ok(())
    }
}
impl ListenerHost for Host {
    type Callback = usize;
    fn string_units(&self, value: &Raw) -> Result<Vec<u16>, ExchangeError> {
        Ok(string(value))
    }
    fn interval(&self, value: &Raw) -> Result<TableHit<f64>, ExchangeError> {
        Ok(
            match String::from_utf16(&string(value))
                .ok()
                .map(|key| js_table_lookup(safe_cex_venue_kraken::spot_types::INTERVAL, &key))
            {
                Some(TableHit::Mapped(value)) => TableHit::Mapped(*value),
                Some(TableHit::Inherited(member)) => TableHit::Inherited(member),
                _ => TableHit::Mapped(1.0),
            },
        )
    }
    fn append(&self, feed: Feed, key: &Key<Raw>, callback: Rc<usize>) -> Result<(), ExchangeError> {
        assert!(
            Rc::ptr_eq(&callback, &self.same),
            "LISTEN native original callback identity"
        );
        let key = self.key(key);
        self.event(
            "getHandlers",
            vec![text(feed_name(feed)), Rc::new(Raw::S(key.clone()))],
        )?;
        let mut callbacks = self
            .maps
            .borrow()
            .get(&(feed, key.clone()))
            .cloned()
            .unwrap_or_default();
        callbacks.push(callback);
        self.event(
            "setHandlers",
            vec![
                text(feed_name(feed)),
                Rc::new(Raw::S(key.clone())),
                Rc::new(Raw::A(
                    callbacks
                        .iter()
                        .map(|c| {
                            text(if Rc::ptr_eq(c, &self.same) {
                                "same"
                            } else {
                                "other"
                            })
                        })
                        .collect(),
                )),
            ],
        )?;
        self.maps.borrow_mut().insert((feed, key), callbacks);
        Ok(())
    }
    fn filter(&self, feed: Feed, key: &Key<Raw>, callback: Rc<usize>) -> Result<(), ExchangeError> {
        let key = self.key(key);
        self.event(
            "getHandlers",
            vec![text(feed_name(feed)), Rc::new(Raw::S(key.clone()))],
        )?;
        let callbacks = self
            .maps
            .borrow()
            .get(&(feed, key.clone()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !Rc::ptr_eq(c, &callback))
            .collect::<Vec<_>>();
        self.event(
            "setHandlers",
            vec![
                text(feed_name(feed)),
                Rc::new(Raw::S(key.clone())),
                Rc::new(Raw::A(
                    callbacks
                        .iter()
                        .map(|c| {
                            text(if Rc::ptr_eq(c, &self.same) {
                                "same"
                            } else {
                                "other"
                            })
                        })
                        .collect(),
                )),
            ],
        )?;
        self.maps.borrow_mut().insert((feed, key), callbacks);
        Ok(())
    }
    fn clear_idle(&self, feed: Feed) -> Result<(), ExchangeError> {
        self.event("clearIdle", vec![text(feed_name(feed))])?;
        if self.flag("clearDeletes") {
            self.maps.borrow_mut().retain(|(f, _), _| *f != feed);
        }
        if self.flag("clearAdds") {
            for ((f, _), v) in self.maps.borrow_mut().iter_mut() {
                if *f == feed {
                    *v = vec![self.other.clone()]
                }
            }
        }
        Ok(())
    }
    fn current_length(&self, feed: Feed, key: &Key<Raw>) -> Result<Option<usize>, ExchangeError> {
        let key = self.key(key);
        self.event(
            "getHandlers",
            vec![text(feed_name(feed)), Rc::new(Raw::S(key.clone()))],
        )?;
        Ok(self.maps.borrow().get(&(feed, key)).map(Vec::len))
    }
    fn connected(&self) -> Result<bool, ExchangeError> {
        self.event("connected", vec![])?;
        Ok(self.connected.get())
    }
    fn send(&self, frame: Rc<RefCell<Frame<Raw>>>) -> Result<(), ExchangeError> {
        let value = match &*frame.borrow() {
            Frame::Futures {
                event,
                feed,
                products,
            } => {
                let pointer = Rc::as_ptr(&frame) as usize;
                if let Some(first) = self.first_frame.get() {
                    assert_eq!(pointer, first, "LISTEN native original frame identity")
                }
                self.first_frame.set(Some(pointer));
                obj(vec![
                    ("event", text(event)),
                    ("feed", text(feed)),
                    ("product_ids", Rc::new(Raw::A(products.clone()))),
                ])
            }
            Frame::SpotBook {
                method,
                channel,
                symbols,
                depth,
            } => obj(vec![
                ("method", text(method)),
                (
                    "params",
                    obj(vec![
                        ("channel", text(channel)),
                        ("symbol", Rc::new(Raw::A(symbols.clone()))),
                        ("depth", number(*depth)),
                    ]),
                ),
            ]),
            Frame::SpotOhlc {
                method,
                channel,
                symbols,
                interval,
            } => {
                let interval = match interval {
                    TableHit::Mapped(v) => number(*v),
                    TableHit::Inherited(safe_cex_num::ProtoMember::Constructor) => {
                        obj(vec![("function", text("Object"))])
                    }
                    _ => panic!("independent interval floor"),
                };
                obj(vec![
                    ("method", text(method)),
                    (
                        "params",
                        obj(vec![
                            ("channel", text(channel)),
                            ("symbol", Rc::new(Raw::A(symbols.clone()))),
                            ("interval", interval),
                        ]),
                    ),
                ])
            }
        };
        self.event("frame", vec![value.clone()])?;
        let wire_value = if matches!(
            &*frame.borrow(),
            Frame::SpotOhlc {
                interval: TableHit::Inherited(_),
                ..
            }
        ) {
            let Raw::O(mut fields) = (*value).clone() else {
                unreachable!()
            };
            let (_, params) = fields.iter_mut().find(|(k, _)| k == "params").unwrap();
            let Raw::O(mut p) = (*params).as_ref().clone() else {
                unreachable!()
            };
            p.retain(|(k, _)| k != "interval");
            *params = Rc::new(Raw::O(p));
            Rc::new(Raw::O(fields))
        } else {
            value
        };
        // The wire boundary preserves UTF-16 symbol strings, including unpaired units.
        self.event("send", vec![wire_value])?;
        if self.flag("sendFail") {
            Err(self.error())
        } else {
            Ok(())
        }
    }
    fn send_error(&self, prefix: &'static str, error: ExchangeError) -> Result<(), ExchangeError> {
        let identity = self.identity(&error);
        self.original.replace(Some(error));
        self.event("sendError", vec![text(prefix), boolraw(identity)])
    }
    fn schedule(&self, poll: Rc<Listener<Self>>, delay: u32) -> Result<(), ExchangeError> {
        let pointer = Rc::as_ptr(&poll) as usize;
        if let Some(first) = self.first_poll.get() {
            assert_eq!(first, pointer, "LISTEN native recurring callback identity")
        }
        self.first_poll.set(Some(pointer));
        self.event("timer", vec![number(delay as f64)])?;
        self.polls.borrow_mut().push(poll);
        Ok(())
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/orderbook-oracle.cjs"
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
        value
    })
}
fn book_input(config: &Value) -> Rc<Raw> {
    let levels=config.get("levels").cloned().unwrap_or_else(||serde_json::json!([{"price":"17.25tail","qty":"19.5"},{"price":"23.75","qty":29.125},{"price":"17.25","qty":31.5}]));
    obj(vec![
        (
            if config["alias"] == true {
                "productId"
            } else {
                "product_id"
            },
            config
                .get("symbol")
                .map(raw)
                .unwrap_or_else(|| text("PF_雪")),
        ),
        (
            "bids",
            config.get("bids").map(raw).unwrap_or_else(|| raw(&levels)),
        ),
        (
            "asks",
            config
                .get("asks")
                .map(raw)
                .unwrap_or_else(|| raw(&serde_json::json!([{"price":37.75,"qty":41.125}]))),
        ),
        (
            "side",
            config.get("side").map(raw).unwrap_or_else(|| text("buy")),
        ),
        (
            "price",
            config
                .get("price")
                .map(raw)
                .unwrap_or_else(|| text("17.25tail")),
        ),
        (
            "qty",
            config
                .get("qty")
                .map(raw)
                .unwrap_or_else(|| text("13.125tail")),
        ),
    ])
}
fn books(family: &str, operation: &str) {
    let rows = oracle()["books"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] == family && r["operation"] == operation)
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        match operation {
            "snapshot" => 17,
            "delta" => 25,
            _ => 21,
        },
        "BOOK independent cases"
    );
    let mut floor = (0usize, 0usize);
    for row in rows {
        let config = &row["config"];
        let host = Host::new(config, family, operation);
        let input = book_input(config);
        let symbol = config
            .get("symbol")
            .map(raw)
            .unwrap_or_else(|| text("PF_雪"));
        let result = match (family, operation) {
            ("Futures", "snapshot") => orderbooks::futures_snapshot(&host, &input),
            ("Spot", "snapshot") => orderbooks::spot_snapshot(&host, symbol, &input),
            ("Futures", "delta") => orderbooks::futures_delta(&host, &input),
            ("Spot", "delta") => orderbooks::spot_delta(&host, symbol, &input),
            ("Futures", _) => orderbooks::futures_emit(&host, &symbol),
            _ => orderbooks::spot_emit(&host, &symbol),
        };
        floor.0 += host.events.borrow().len();
        floor.1 += usize::from(result.is_err());
        host.failure(result, &row["failure"]);
        host.compare(row, "BOOK");
        assert_native(&host.final_cache(), &row["cache"], "BOOK cache");
        let retained = match host.retained.borrow().as_ref() {
            Some(level) => {
                let fields = level.borrow();
                obj(vec![
                    ("price", number(fields.price)),
                    ("amount", number(fields.amount)),
                    ("total", number(fields.total)),
                ])
            }
            None => undefined(),
        };
        assert_native(&retained, &row["retained"], "BOOK retained after return");
    }
    let expected = match (family, operation) {
        ("Futures", "snapshot") => (332, 5),
        ("Spot", "snapshot") => (328, 5),
        ("Futures", "delta") => (252, 5),
        ("Spot", "delta") => (530, 6),
        ("Futures", "emit") => (129, 3),
        ("Spot", "emit") => (114, 2),
        _ => panic!("selected body"),
    };
    assert_eq!(floor, expected, "BOOK independent finite floor");
}
fn listeners(family: &str) {
    let rows = oracle()["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 18, "LISTEN independent cases");
    let mut floor = (0usize, 0usize, 0usize);
    for row in rows {
        let host = Rc::new(Host::new(&row["config"], family, "listener"));
        let mut failure = None;
        let remember = |error: ExchangeError| {
            let description = (error.name.clone(), host.identity(&error));
            host.original.replace(Some(error));
            description
        };
        let symbol = host.options.borrow()["symbol"].clone();
        let options = obj(vec![
            ("symbol", symbol.clone()),
            ("interval", host.options.borrow()["interval"].clone()),
        ]);
        host.options_pointer
            .set(Some(Rc::as_ptr(&options) as usize));
        let result = match family {
            "Futures" => {
                book_listeners::futures_listen_book(host.clone(), symbol, host.same.clone())
            }
            "Spot" => book_listeners::spot_listen_book(host.clone(), symbol, host.same.clone()),
            _ => book_listeners::spot_listen_ohlcv(host.clone(), options, host.same.clone()),
        };
        let listener = match result {
            Ok(v) => Some(v),
            Err(e) => {
                failure = Some(remember(e));
                None
            }
        };
        assert_eq!(
            listener.is_some(),
            row["registered"].as_bool().unwrap(),
            "LISTEN native disposer presence"
        );
        let mut checkpoints = vec![host.events.borrow().len()];
        for now in host.config["polls"].as_array().into_iter().flatten() {
            host.connected.set(now.as_bool().unwrap());
            let poll = if host.polls.borrow().is_empty() {
                None
            } else {
                Some(host.polls.borrow_mut().remove(0))
            };
            if let Some(poll) = poll {
                if let Err(e) = poll.poll() {
                    failure = Some(remember(e))
                }
            }
            checkpoints.push(host.events.borrow().len());
        }
        if host.flag("mutateOpts") {
            host.options
                .borrow_mut()
                .insert("symbol", text("CHANGED/雪"));
        }
        if let Some(now) = host.config["disposeConnected"].as_bool() {
            host.connected.set(now)
        }
        for _ in 0..host.config["dispose"].as_u64().unwrap_or(1) {
            if let Some(listener) = listener.as_ref() {
                if let Err(e) = listener.dispose() {
                    failure = Some(remember(e))
                }
            }
            checkpoints.push(host.events.borrow().len());
        }
        assert_eq!(
            checkpoints,
            row["checkpoints"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<Vec<_>>(),
            "LISTEN native checkpoints"
        );
        floor.0 += host.events.borrow().len();
        floor.1 += usize::from(failure.is_some());
        floor.2 += checkpoints.len();
        match failure {
            None => assert!(row["failure"].is_null()),
            Some((name, identity)) => {
                assert_eq!(name, row["failure"]["name"]);
                assert_eq!(identity, row["failure"]["identity"].as_bool().unwrap())
            }
        }
        host.compare(row, "LISTEN");
        let expected = row["handlers"].as_array().unwrap();
        let maps = host.maps.borrow();
        assert_eq!(
            maps.len(),
            expected.len(),
            "LISTEN native handler-key presence"
        );
        for expected in expected {
            let key = expected["key"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u16)
                .collect::<Vec<_>>();
            let callbacks = maps
                .get(&(
                    if family == "ohlc" {
                        Feed::Ohlc
                    } else {
                        Feed::Book
                    },
                    key,
                ))
                .unwrap();
            let actual = callbacks
                .iter()
                .map(|cb| {
                    if Rc::ptr_eq(cb, &host.same) {
                        "same"
                    } else {
                        "other"
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                expected["callbacks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "LISTEN native callback identity filter"
            )
        }
        drop(maps);
        host.polls.borrow_mut().clear();
    }
    let expected = match family {
        "Futures" => (153, 1, 41),
        "Spot" => (199, 3, 41),
        "ohlc" => (260, 3, 41),
        _ => panic!("selected listener"),
    };
    assert_eq!(floor, expected, "LISTEN independent finite floor");
}
#[test]
fn futures_snapshot_live_source() {
    books("Futures", "snapshot")
}
#[test]
fn spot_snapshot_live_source() {
    books("Spot", "snapshot")
}
#[test]
fn futures_delta_live_source() {
    books("Futures", "delta")
}
#[test]
fn spot_delta_live_source() {
    books("Spot", "delta")
}
#[test]
fn futures_emit_live_source() {
    books("Futures", "emit")
}
#[test]
fn spot_emit_live_source() {
    books("Spot", "emit")
}
#[test]
fn futures_listener_live_source() {
    listeners("Futures")
}
#[test]
fn spot_listener_live_source() {
    listeners("Spot")
}
#[test]
fn ohlcv_listener_live_source() {
    listeners("ohlc")
}
