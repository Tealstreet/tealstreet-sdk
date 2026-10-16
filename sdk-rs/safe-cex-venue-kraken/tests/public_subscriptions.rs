use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_array_index};
use safe_cex_venue_kraken::public_subscriptions::{
    self, PublicSubscription, PublicSubscriptionHost, Topic,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    sync::OnceLock,
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-subscriptions-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 2);
        value
    })
}
fn flag(config: &Value, key: &str) -> bool {
    config[key].as_bool().unwrap_or(false)
}
fn units(value: &Value) -> Topic {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_u64().unwrap() as u16)
        .collect()
}
fn topics(value: &Value) -> Vec<Topic> {
    value.as_array().unwrap().iter().map(units).collect()
}
fn utf16(text: &str) -> Topic {
    text.encode_utf16().collect()
}
#[derive(Debug)]
enum Event {
    Products,
    Keys(&'static str),
    Length(&'static str, Topic),
    Frame(PublicSubscription),
    Send,
    Error(&'static str, Topic, bool),
}
struct Host {
    config: Value,
    disposed: Cell<bool>,
    products: RefCell<Vec<Topic>>,
    books: RefCell<Vec<(Topic, Option<usize>)>>,
    ohlc: RefCell<Vec<(Topic, Option<usize>)>>,
    events: RefCell<Vec<Event>>,
    sends: Cell<usize>,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
}
impl Host {
    fn new(config: Value) -> Self {
        let count = config["count"].as_u64().unwrap_or(1) as usize;
        let products = (0..count)
            .map(|i| {
                if i == 0 {
                    vec![38634, 47, 0xd800]
                } else {
                    utf16(&format!("P{i}"))
                }
            })
            .collect();
        let rows = |name: &str, default: Vec<(Topic, Option<usize>)>| {
            config[name]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|r| (units(&r[0]), r[1].as_u64().map(|n| n as usize)))
                        .collect()
                })
                .unwrap_or(default)
        };
        let books = rows(
            "books",
            vec![
                (utf16("B"), Some(1)),
                (utf16("idle"), Some(0)),
                (utf16("absent"), None),
            ],
        );
        let ohlc = rows(
            "ohlc",
            vec![
                (utf16("BTC/USD::1h"), Some(1)),
                (utf16("idle::5m"), Some(0)),
            ],
        );
        let mut error = ExchangeError::new("subscription failure");
        error.name = "SubscriptionError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        Self {
            disposed: Cell::new(flag(&config, "disposed")),
            config,
            products: RefCell::new(products),
            books: RefCell::new(books),
            ohlc: RefCell::new(ohlc),
            events: RefCell::new(vec![]),
            sends: Cell::new(0),
            error: RefCell::new(Some(error)),
            pointer,
        }
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            Err(self.error.borrow_mut().take().unwrap())
        } else {
            Ok(())
        }
    }
    fn insert(&self, map: &RefCell<Vec<(Topic, Option<usize>)>>, key: &str, len: usize) {
        let key = utf16(key);
        let mut rows = map.borrow_mut();
        if let Some(row) = rows.iter_mut().find(|r| r.0 == key) {
            row.1 = Some(len)
        } else {
            rows.push((key, Some(len)))
        }
    }
    fn keys(
        &self,
        map: &RefCell<Vec<(Topic, Option<usize>)>>,
        name: &'static str,
    ) -> Result<Vec<Topic>, ExchangeError> {
        self.events.borrow_mut().push(Event::Keys(name));
        self.fail(&format!("keys-{name}"))?;
        let mut keys: Vec<_> = map.borrow().iter().map(|r| r.0.clone()).collect();
        let index = |key: &Topic| {
            String::from_utf16(key)
                .ok()
                .and_then(|s| js_array_index(&s))
        };
        keys.sort_by(|a, b| match (index(a), index(b)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });
        Ok(keys)
    }
    fn length(
        &self,
        map: &RefCell<Vec<(Topic, Option<usize>)>>,
        name: &'static str,
        key: &[u16],
    ) -> Result<Option<usize>, ExchangeError> {
        let value = map.borrow().iter().find(|r| r.0 == key).map(|r| r.1);
        if let Some(value) = value {
            self.events
                .borrow_mut()
                .push(Event::Length(name, key.to_vec()));
            self.fail(&format!("length-{}", String::from_utf16_lossy(key)))?;
            Ok(value)
        } else {
            Ok(None)
        }
    }
}
impl PublicSubscriptionHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn ticker_product_ids(&self) -> Result<Vec<Topic>, ExchangeError> {
        self.events.borrow_mut().push(Event::Products);
        self.fail("products")?;
        Ok(self.products.borrow().clone())
    }
    fn market_symbols(&self) -> Result<Vec<Topic>, ExchangeError> {
        Ok(self.products.borrow().clone())
    }
    fn book_keys(&self) -> Result<Vec<Topic>, ExchangeError> {
        self.keys(&self.books, "book")
    }
    fn book_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError> {
        self.length(&self.books, "book", key)
    }
    fn ohlcv_keys(&self) -> Result<Vec<Topic>, ExchangeError> {
        self.keys(&self.ohlc, "ohlc")
    }
    fn ohlcv_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError> {
        self.length(&self.ohlc, "ohlc", key)
    }
    fn stringify_and_send(&self, frame: PublicSubscription) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Frame(frame));
        self.fail("stringify")?;
        self.events.borrow_mut().push(Event::Send);
        let sends = self.sends.get() + 1;
        self.sends.set(sends);
        if flag(&self.config, "disposeSend") {
            self.disposed.set(true);
        }
        if flag(&self.config, "mutateTicker") && sends == 1 {
            self.products.borrow_mut().push(utf16("LATE"));
            self.insert(&self.books, "LATE", 1);
        }
        let batches = (self.config["count"].as_u64().unwrap_or(1) as usize)
            .div_ceil(50)
            .max(1);
        if flag(&self.config, "mutateBook") && sends == batches + 1 {
            self.books.borrow_mut().retain(|r| r.0 != utf16("idle"));
            self.insert(&self.books, "ADDED", 1);
            self.insert(&self.ohlc, "ADDED::3m", 1);
        }
        if self.config["fail"] == "console" && sends == 2 {
            return Err(self.error.borrow_mut().take().unwrap());
        }
        self.fail(&format!("send-{sends}"))
    }
    fn book_resubscribe_error(
        &self,
        label: &'static str,
        symbol: &[u16],
        error: ExchangeError,
    ) -> Result<(), ExchangeError> {
        let identity = (&*error.metadata as *const _) as usize == self.pointer;
        self.events
            .borrow_mut()
            .push(Event::Error(label, symbol.to_vec(), identity));
        if self.config["fail"] == "console" {
            Err(error)
        } else {
            Ok(())
        }
    }
}
fn assert_frame(frame: &PublicSubscription, value: &Value) {
    let (variant, symbols) = match frame {
        PublicSubscription::Futures {
            event,
            feed,
            product_ids,
        } => {
            assert_eq!(*event, value["event"].as_str().unwrap());
            assert_eq!(*feed, value["feed"].as_str().unwrap(), "OPEN native feed");
            ("futures", product_ids)
        }
        PublicSubscription::SpotTicker {
            method,
            channel,
            symbols,
            event_trigger,
        } => {
            assert_eq!(*method, value["method"].as_str().unwrap());
            assert_eq!(*channel, value["channel"].as_str().unwrap());
            assert_eq!(*event_trigger, value["eventTrigger"].as_str().unwrap());
            ("ticker", symbols)
        }
        PublicSubscription::SpotBook {
            method,
            channel,
            symbols,
            depth,
        } => {
            assert_eq!(*method, value["method"].as_str().unwrap());
            assert_eq!(*channel, value["channel"].as_str().unwrap());
            assert_eq!(
                format!("{:016x}", depth.to_bits()),
                value["depth"]["number"],
                "OPEN native book depth"
            );
            ("book", symbols)
        }
        PublicSubscription::SpotOhlc {
            method,
            channel,
            symbols,
            interval,
        } => {
            assert_eq!(*method, value["method"].as_str().unwrap());
            assert_eq!(*channel, value["channel"].as_str().unwrap());
            match interval {
                TableHit::Mapped(number) => assert_eq!(
                    format!("{:016x}", number.to_bits()),
                    value["interval"]["number"],
                    "OPEN native interval"
                ),
                TableHit::Inherited(member) => {
                    assert_eq!(
                        member.js_string(),
                        value["interval"]["inherited"],
                        "OPEN native inherited interval"
                    );
                    assert_eq!(value["interval"]["identity"], true)
                }
                TableHit::Missing => panic!("OPEN native interval missing"),
            }
            ("ohlc", symbols)
        }
    };
    assert_eq!(variant, value["variant"]);
    assert_eq!(
        *symbols,
        topics(&value["symbols"]),
        "OPEN native batch symbols"
    );
}
fn run(family: &str, body: fn(&Host) -> Result<(), ExchangeError>) {
    let cases = oracle()[family].as_array().unwrap();
    assert_eq!(cases.len(), 43, "OPEN independent cases");
    let mut count = 0;
    let mut frames = 0;
    let mut failures = 0;
    for case in cases {
        let host = Host::new(case["config"].clone());
        match body(&host) {
            Ok(()) => assert!(case["failure"].is_null()),
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    host.pointer,
                    "OPEN native original error identity"
                );
                assert_eq!(case["failure"]["identity"], true)
            }
        }
        assert_eq!(host.disposed.get(), case["disposed"].as_bool().unwrap());
        let events = host.events.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(events.len(), expected.len(), "OPEN native event count");
        count += events.len();
        for (event, value) in events.iter().zip(expected) {
            match event {
                Event::Products => assert_eq!(value["kind"], "products"),
                Event::Keys(map) => {
                    assert_eq!(value["kind"], "keys");
                    assert_eq!(*map, value["map"])
                }
                Event::Length(map, key) => {
                    assert_eq!(value["kind"], "length");
                    assert_eq!(*map, value["map"]);
                    assert_eq!(
                        *key,
                        units(&value["key"]),
                        "OPEN native current handler key"
                    )
                }
                Event::Frame(frame) => {
                    frames += 1;
                    assert_eq!(value["kind"], "frame");
                    assert_frame(frame, value)
                }
                Event::Send => assert_eq!(value["kind"], "send"),
                Event::Error(label, key, identity) => {
                    assert_eq!(value["kind"], "error");
                    assert_eq!(*label, value["label"]);
                    assert_eq!(*key, units(&value["key"]));
                    assert!(*identity);
                    assert_eq!(value["identity"], true)
                }
            }
        }
    }
    let expected = if family == "Futures" {
        (446, 85, 8)
    } else {
        (477, 122, 9)
    };
    assert_eq!(
        (count, frames, failures),
        expected,
        "OPEN independent event/frame/failure floors"
    );
}
#[test]
fn futures_on_open_live_source() {
    run("Futures", public_subscriptions::futures_on_open)
}
#[test]
fn spot_on_open_live_source() {
    run("Spot", public_subscriptions::spot_on_open)
}
