use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, OrderBookOrders, Timeframe};
use safe_cex_venue_kraken::stream_adapters::{
    self, BookCallback, CandleCallback, PublicStreams, PublicVenue, Unsubscribe,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/stream-adapters-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["header"]["hashes"].as_object().unwrap().len(), 3);
        result
    })
}
fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
fn candle() -> Candle {
    Candle {
        timestamp: 17.25,
        open: 19.5,
        high: 23.75,
        low: 29.125,
        close: 31.5,
        volume: 37.25,
        last_side: None,
    }
}
fn book() -> OrderBook {
    OrderBook {
        bids: vec![OrderBookOrders {
            price: 17.25,
            amount: 19.5,
            total: 23.75,
        }],
        asks: vec![],
    }
}
fn options() -> OHLCVOptions {
    OHLCVOptions {
        symbol: "BTC/雪".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::H12),
        from: Some(-0.0),
        to: Some(1770900000123.75),
        limit: Some(17.25),
    }
}
fn assert_candle(value: &Candle, expected: &Value) {
    for (name, number) in [
        ("timestamp", value.timestamp),
        ("open", value.open),
        ("high", value.high),
        ("low", value.low),
        ("close", value.close),
        ("volume", value.volume),
    ] {
        assert_eq!(
            bits(number),
            expected[name]["number"],
            "STREAM native candle {name}"
        )
    }
    assert!(value.last_side.is_none())
}
fn assert_book(value: &OrderBook, expected: &Value) {
    for (side, rows) in [("bids", &value.bids), ("asks", &value.asks)] {
        assert_eq!(rows.len(), expected[side].as_array().unwrap().len());
        for (index, row) in rows.iter().enumerate() {
            for (name, number) in [
                ("price", row.price),
                ("amount", row.amount),
                ("total", row.total),
            ] {
                assert_eq!(
                    bits(number),
                    expected[side][index][name]["number"],
                    "STREAM native book {name}"
                )
            }
        }
    }
}
fn assert_options(value: &OHLCVOptions, expected: &Value) {
    assert_eq!(value.symbol, expected["symbol"]);
    assert_eq!(
        value.interval,
        safe_cex_types::AnyTimeframe::Variant1(Timeframe::H12)
    );
    assert_eq!(expected["interval"], "12h");
    for (name, number) in [
        ("from", value.from.unwrap()),
        ("to", value.to.unwrap()),
        ("limit", value.limit.unwrap()),
    ] {
        assert_eq!(
            bits(number),
            expected[name]["number"],
            "STREAM native options {name}"
        )
    }
}

enum Event {
    Create {
        venue: PublicVenue,
        parent_identity: bool,
    },
    Connect {
        id: usize,
    },
    ListenBook {
        id: usize,
        symbol: String,
        callback_identity: bool,
    },
    ListenCandle {
        id: usize,
        options: OHLCVOptions,
        options_identity: bool,
        callback_identity: bool,
    },
    Book(OrderBook),
    Candle(Candle),
    Unsubscribe {
        id: usize,
    },
}
struct Host {
    config: Value,
    first: Cell<bool>,
    slot: Weak<RefCell<Option<Rc<Socket>>>>,
    trace: RefCell<Vec<Event>>,
    pointer: Cell<usize>,
    book_callback: RefCell<Option<BookCallback>>,
    candle_callback: RefCell<Option<CandleCallback>>,
    options_pointer: Cell<usize>,
}
impl Host {
    fn failure(&self) -> ExchangeError {
        let mut error = ExchangeError::new("stream failure");
        error.name = "StreamError".into();
        error.metadata.code = Some(safe_cex_exchange::js_runtime::value::Js::str("original"));
        self.pointer.set((&*error.metadata as *const _) as usize);
        error
    }
    fn fails(&self, name: &str) -> bool {
        self.first.get() && flag(&self.config, name)
    }
}
struct Socket {
    id: usize,
    host: Rc<Host>,
    disposer: Unsubscribe,
}
impl Socket {
    fn new(id: usize, host: Rc<Host>) -> Rc<Self> {
        let weak = Rc::downgrade(&host);
        let disposer: Unsubscribe = Rc::new(move || {
            let host = weak.upgrade().unwrap();
            host.trace.borrow_mut().push(Event::Unsubscribe { id });
            if host.fails("unsubscribeFail") {
                Err(host.failure())
            } else {
                Ok(())
            }
        });
        Rc::new(Self { id, host, disposer })
    }
}
impl PublicStreams for Socket {
    fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        self.host
            .trace
            .borrow_mut()
            .push(Event::Connect { id: self.id });
        if self.host.fails("replaceConnect") {
            *self.host.slot.upgrade().unwrap().borrow_mut() = Some(Self::new(13, self.host.clone()))
        }
        if self.host.fails("removeConnect") {
            *self.host.slot.upgrade().unwrap().borrow_mut() = None
        }
        if self.host.fails("connectFail") {
            Err(self.host.failure())
        } else {
            Ok(())
        }
    }
    fn listen_order_book(
        &self,
        symbol: &str,
        callback: BookCallback,
    ) -> Result<Unsubscribe, ExchangeError> {
        self.host.trace.borrow_mut().push(Event::ListenBook {
            id: self.id,
            symbol: symbol.into(),
            callback_identity: Rc::ptr_eq(
                &callback,
                self.host.book_callback.borrow().as_ref().unwrap(),
            ),
        });
        if self.host.fails("listenFail") {
            return Err(self.host.failure());
        }
        callback(&book())?;
        Ok(self.disposer.clone())
    }
    fn listen_ohlcv(
        &self,
        options: &OHLCVOptions,
        callback: CandleCallback,
    ) -> Result<Unsubscribe, ExchangeError> {
        self.host.trace.borrow_mut().push(Event::ListenCandle {
            id: self.id,
            options: options.clone(),
            options_identity: (options as *const _) as usize == self.host.options_pointer.get(),
            callback_identity: Rc::ptr_eq(
                &callback,
                self.host.candle_callback.borrow().as_ref().unwrap(),
            ),
        });
        if self.host.fails("listenFail") {
            return Err(self.host.failure());
        }
        callback(&candle())?;
        Ok(self.disposer.clone())
    }
}
fn compare(family: &str) {
    let cases = oracle()["streams"][family].as_array().unwrap();
    assert_eq!(cases.len(), 13, "STREAM independent family cases");
    let mut events = 0;
    let mut failures = 0;
    for case in cases {
        let slot = Rc::new(RefCell::new(None));
        let host = Rc::new(Host {
            config: case["config"].clone(),
            first: Cell::new(true),
            slot: Rc::downgrade(&slot),
            trace: RefCell::new(vec![]),
            pointer: Cell::new(0),
            book_callback: RefCell::new(None),
            candle_callback: RefCell::new(None),
            options_pointer: Cell::new(0),
        });
        if flag(&host.config, "existing") {
            *slot.borrow_mut() = Some(Socket::new(7, host.clone()))
        }
        let weak = Rc::downgrade(&host);
        let book_callback: BookCallback = Rc::new(move |value| {
            let host = weak.upgrade().unwrap();
            host.trace.borrow_mut().push(Event::Book(value.clone()));
            if host.fails("callbackFail") {
                Err(host.failure())
            } else {
                Ok(())
            }
        });
        let weak = Rc::downgrade(&host);
        let candle_callback: CandleCallback = Rc::new(move |value| {
            let host = weak.upgrade().unwrap();
            host.trace.borrow_mut().push(Event::Candle(value.clone()));
            if host.fails("callbackFail") {
                Err(host.failure())
            } else {
                Ok(())
            }
        });
        *host.book_callback.borrow_mut() = Some(book_callback.clone());
        *host.candle_callback.borrow_mut() = Some(candle_callback.clone());
        let parent = 73_u64;
        let options = options();
        host.options_pointer.set((&options as *const _) as usize);
        for attempt in 0..2 {
            let create = |venue, supplied_parent: &u64| {
                host.trace.borrow_mut().push(Event::Create {
                    venue,
                    parent_identity: std::ptr::eq(supplied_parent, &parent),
                });
                if host.fails("replaceCreate") {
                    *slot.borrow_mut() = Some(Socket::new(13, host.clone()))
                }
                if host.fails("createFail") {
                    Err(host.failure())
                } else {
                    Ok(Socket::new(11, host.clone()))
                }
            };
            let result = match family {
                "futures-book" => stream_adapters::futures_listen_order_book(
                    &parent,
                    &slot,
                    "BTC/雪",
                    book_callback.clone(),
                    create,
                ),
                "spot-book" => stream_adapters::spot_listen_order_book(
                    &parent,
                    &slot,
                    "BTC/雪",
                    book_callback.clone(),
                    create,
                ),
                "spot-ohlcv" => stream_adapters::spot_listen_ohlcv(
                    &parent,
                    &slot,
                    &options,
                    candle_callback.clone(),
                    create,
                ),
                _ => panic!("unknown family"),
            };
            let outcome = match result {
                Err(error) => Err(error),
                Ok(disposer) => {
                    assert!(
                        Rc::ptr_eq(&disposer, &slot.borrow().as_ref().unwrap().disposer),
                        "STREAM native disposer identity"
                    );
                    disposer().and_then(|_| disposer())
                }
            };
            let expected = &case["outcomes"][attempt];
            match outcome {
                Ok(()) => {
                    assert!(expected.get("failure").is_none(), "STREAM native success");
                    assert_eq!(expected["identity"], true)
                }
                Err(error) => {
                    failures += 1;
                    assert_eq!(
                        error.name, expected["failure"]["name"],
                        "STREAM native error name"
                    );
                    assert_eq!(
                        error.message, expected["failure"]["message"],
                        "STREAM native error message"
                    );
                    let original = (&*error.metadata as *const _) as usize == host.pointer.get();
                    assert_eq!(
                        original,
                        expected["failure"]["identity"].as_bool().unwrap(),
                        "STREAM native original error identity"
                    );
                    if original {
                        assert!(error.metadata.code.as_ref().unwrap().strict_equals(
                            &safe_cex_exchange::js_runtime::value::Js::str("original")
                        ))
                    }
                }
            }
            let id = slot.borrow().as_ref().map(|socket| socket.id as u64);
            assert_eq!(id, expected["slot"].as_u64(), "STREAM native socket slot");
            host.first.set(false);
        }
        let trace = host.trace.borrow();
        assert_eq!(
            trace.len(),
            case["trace"].as_array().unwrap().len(),
            "STREAM native trace length"
        );
        events += trace.len();
        for (native, expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            match native {
                Event::Create {
                    venue,
                    parent_identity,
                } => {
                    assert_eq!(expected["kind"], "create");
                    assert_eq!(
                        match venue {
                            PublicVenue::Futures => "futures",
                            PublicVenue::Spot => "spot",
                        },
                        expected["venue"],
                        "STREAM native constructor venue"
                    );
                    assert!(*parent_identity, "STREAM native constructor parent");
                    assert_eq!(expected["parentIdentity"], true)
                }
                Event::Connect { id } => {
                    assert_eq!(expected["kind"], "connect");
                    assert_eq!(*id as u64, expected["id"].as_u64().unwrap())
                }
                Event::ListenBook {
                    id,
                    symbol,
                    callback_identity,
                } => {
                    assert_eq!(expected["kind"], "listen");
                    assert_eq!(*id as u64, expected["id"].as_u64().unwrap());
                    assert_eq!(symbol, &expected["symbol"]);
                    assert!(*callback_identity, "STREAM native book callback identity");
                    assert_eq!(expected["callbackIdentity"], true)
                }
                Event::ListenCandle {
                    id,
                    options,
                    options_identity,
                    callback_identity,
                } => {
                    assert_eq!(expected["kind"], "listen");
                    assert_eq!(*id as u64, expected["id"].as_u64().unwrap());
                    assert_options(options, &expected["options"]);
                    assert!(
                        *options_identity && *callback_identity,
                        "STREAM native candle arguments identity"
                    );
                    assert_eq!(expected["optionsIdentity"], true);
                    assert_eq!(expected["callbackIdentity"], true)
                }
                Event::Book(value) => {
                    assert_eq!(expected["kind"], "callback");
                    assert_book(value, &expected["value"])
                }
                Event::Candle(value) => {
                    assert_eq!(expected["kind"], "callback");
                    assert_candle(value, &expected["value"])
                }
                Event::Unsubscribe { id } => {
                    assert_eq!(expected["kind"], "unsubscribe");
                    assert_eq!(*id as u64, expected["id"].as_u64().unwrap())
                }
            }
        }
    }
    assert_eq!(events, 99, "STREAM independent family events");
    assert_eq!(failures, 9, "STREAM independent family failures");
}
#[test]
fn futures_book_live_source() {
    compare("futures-book")
}
#[test]
fn spot_book_live_source() {
    compare("spot-book")
}
#[test]
fn spot_ohlcv_live_source() {
    compare("spot-ohlcv")
}
#[test]
fn futures_ohlcv_noop_live_source() {
    let cases = oracle()["noops"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "STREAM independent no-op cases");
    for case in cases {
        let calls = Cell::new(0);
        let callback = || calls.set(calls.get() + 1);
        let first = stream_adapters::futures_listen_ohlcv(&case["input"], &callback);
        let second = stream_adapters::futures_listen_ohlcv(&case["input"], &callback);
        assert!(
            !Rc::ptr_eq(&first, &second),
            "STREAM native distinct no-op disposers"
        );
        assert_eq!(case["distinct"], true);
        for disposer in [&first, &first, &second] {
            assert!(
                disposer().is_ok(),
                "STREAM native no-op disposer completion"
            )
        }
        assert_eq!(calls.get(), 0, "STREAM native no-op callback count");
        assert_eq!(case["callbackCalls"], 0);
        for result in case["returns"].as_array().unwrap().iter().take(3) {
            assert_eq!(result["undefined"], true)
        }
    }
}
#[test]
fn futures_ohlc_noop_live_source() {
    let cases = oracle()["noops"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "STREAM independent no-op handler cases");
    for case in cases {
        let input = case["input"].clone();
        let before = input.clone();
        assert!(
            std::panic::catch_unwind(|| stream_adapters::futures_handle_ohlc(&input)).is_ok(),
            "STREAM native OHLC handler completion"
        );
        assert_eq!(input, before, "STREAM native untouched OHLC input");
        assert_eq!(case["returns"][3]["undefined"], true)
    }
}
