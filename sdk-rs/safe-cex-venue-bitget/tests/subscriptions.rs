use safe_cex_exchange::{
    external::ExternalPublicData,
    runtime::{ExchangeError, ListenerCleanup},
};
use safe_cex_types::{AnyTimeframe, Candle, OHLCVOptions, OrderBook, OrderSide, Timeframe};
use safe_cex_venue_bitget::subscriptions::{listen_ohlcv, listen_order_book};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};

struct Socket {
    candle_callback: Rc<dyn Fn(Candle)>,
    book_callback: Rc<dyn Fn(OrderBook)>,
    candle_cleanup: RefCell<Option<ListenerCleanup>>,
    book_cleanup: RefCell<Option<ListenerCleanup>>,
    calls: RefCell<Vec<&'static str>>,
}
impl ExternalPublicData for Socket {
    fn connect(&self) {
        panic!("SUBSCRIPTION must not connect")
    }
    fn dispose(&self) {
        panic!("SUBSCRIPTION must not dispose")
    }
    fn is_initialized(&self) -> bool {
        panic!("SUBSCRIPTION must not inspect readiness")
    }
    fn is_healthy(&self) -> bool {
        panic!("SUBSCRIPTION must not inspect health")
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        panic!("SUBSCRIPTION must not inspect clock")
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.calls.borrow_mut().push("listenOHLCV");
        assert_eq!(
            options.symbol, "BTCUSDT",
            "SUBSCRIPTION native options symbol"
        );
        assert_eq!(
            options.interval,
            AnyTimeframe::Variant1(Timeframe::M1),
            "SUBSCRIPTION native interval"
        );
        assert_eq!(
            options.from.unwrap().to_bits(),
            (-0.0_f64).to_bits(),
            "SUBSCRIPTION native from bits"
        );
        assert_eq!(options.to, Some(937.0), "SUBSCRIPTION native to");
        assert_eq!(options.limit, Some(23.0), "SUBSCRIPTION native limit");
        assert!(
            Rc::ptr_eq(&callback, &self.candle_callback),
            "SUBSCRIPTION native candle callback identity"
        );
        let candle: Candle = serde_json::from_str(r#"{"timestamp":101,"open":11,"high":17,"low":7,"close":13,"volume":29,"lastSide":"buy"}"#).unwrap();
        callback(candle);
        Ok(self.candle_cleanup.borrow_mut().take().unwrap())
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.calls.borrow_mut().push("listenOrderBook");
        assert_eq!(symbol, "ETHUSDT", "SUBSCRIPTION native order book symbol");
        assert!(
            Rc::ptr_eq(&callback, &self.book_callback),
            "SUBSCRIPTION native book callback identity"
        );
        let book: OrderBook = serde_json::from_str(r#"{"bids":[{"price":103,"amount":3,"total":309}],"asks":[{"price":107,"amount":5,"total":535}]}"#).unwrap();
        callback(book);
        Ok(self.book_cleanup.borrow_mut().take().unwrap())
    }
}

fn native_delegation() {
    let candles = Rc::new(RefCell::new(Vec::new()));
    let books = Rc::new(RefCell::new(Vec::new()));
    let candle_callback: Rc<dyn Fn(Candle)> = {
        let candles = candles.clone();
        Rc::new(move |candle| candles.borrow_mut().push(candle))
    };
    let book_callback: Rc<dyn Fn(OrderBook)> = {
        let books = books.clone();
        Rc::new(move |book| books.borrow_mut().push(book))
    };
    let cleaned = Rc::new(Cell::new(0));
    let candle_cleanup: ListenerCleanup = {
        let cleaned = cleaned.clone();
        Box::new(move || {
            cleaned.set(cleaned.get() + 1);
            Ok(())
        })
    };
    let book_cleanup: ListenerCleanup = {
        let cleaned = cleaned.clone();
        Box::new(move || {
            cleaned.set(cleaned.get() + 10);
            Ok(())
        })
    };
    let candle_pointer = std::ptr::from_ref(candle_cleanup.as_ref()) as *const ();
    let book_pointer = std::ptr::from_ref(book_cleanup.as_ref()) as *const ();
    let socket = Socket {
        candle_callback: candle_callback.clone(),
        book_callback: book_callback.clone(),
        candle_cleanup: RefCell::new(Some(candle_cleanup)),
        book_cleanup: RefCell::new(Some(book_cleanup)),
        calls: RefCell::new(Vec::new()),
    };
    let options: OHLCVOptions = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","interval":"1m","from":-0.0,"to":937,"limit":23}"#,
    )
    .unwrap();
    let returned_candle = listen_ohlcv(&socket, options, candle_callback).unwrap();
    let returned_book = listen_order_book(&socket, "ETHUSDT".into(), book_callback).unwrap();
    assert_eq!(
        socket.calls.borrow().as_slice(),
        ["listenOHLCV", "listenOrderBook"],
        "SUBSCRIPTION native call order/count"
    );
    assert_eq!(
        std::ptr::from_ref(returned_candle.as_ref()) as *const (),
        candle_pointer,
        "SUBSCRIPTION native candle cleanup identity"
    );
    assert_eq!(
        std::ptr::from_ref(returned_book.as_ref()) as *const (),
        book_pointer,
        "SUBSCRIPTION native book cleanup identity"
    );
    assert_eq!(cleaned.get(), 0, "SUBSCRIPTION native cleanup deferred");
    {
        let candles = candles.borrow();
        assert_eq!(
            candles.len(),
            1,
            "SUBSCRIPTION native candle callback count"
        );
        let candle = &candles[0];
        assert_eq!(
            candle.timestamp, 101.0,
            "SUBSCRIPTION native candle timestamp"
        );
        assert_eq!(candle.open, 11.0, "SUBSCRIPTION native candle open");
        assert_eq!(candle.high, 17.0, "SUBSCRIPTION native candle high");
        assert_eq!(candle.low, 7.0, "SUBSCRIPTION native candle low");
        assert_eq!(candle.close, 13.0, "SUBSCRIPTION native candle close");
        assert_eq!(candle.volume, 29.0, "SUBSCRIPTION native candle volume");
        assert_eq!(
            candle.last_side,
            Some(OrderSide::Buy),
            "SUBSCRIPTION native candle last side"
        );
        let books = books.borrow();
        assert_eq!(books.len(), 1, "SUBSCRIPTION native book callback count");
        assert_eq!(books[0].bids.len(), 1, "SUBSCRIPTION native bid count");
        assert_eq!(books[0].asks.len(), 1, "SUBSCRIPTION native ask count");
        for (row, price, amount, total) in [
            (&books[0].bids[0], 103.0, 3.0, 309.0),
            (&books[0].asks[0], 107.0, 5.0, 535.0),
        ] {
            assert_eq!(row.price, price, "SUBSCRIPTION native book price");
            assert_eq!(row.amount, amount, "SUBSCRIPTION native book amount");
            assert_eq!(row.total, total, "SUBSCRIPTION native book total");
        }
    }
    returned_candle().unwrap();
    assert_eq!(cleaned.get(), 1, "SUBSCRIPTION native candle cleanup call");
    returned_book().unwrap();
    assert_eq!(cleaned.get(), 11, "SUBSCRIPTION native book cleanup call");
}

#[test]
fn subscriptions_forward_native_arguments_callbacks_and_cleanup() {
    native_delegation();
}

#[test]
#[ignore = "requires Node and TypeScript; included in venue oracle"]
fn subscriptions_match_actual_source() {
    // Finish the native assertions before launching source output capture.
    native_delegation();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/subscriptions-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SUBSCRIPTION source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capture,
        serde_json::json!({"calls":["listenOHLCV","listenOrderBook"],"events":["candle","book"],"cleanups":["candle","book"]}),
        "SUBSCRIPTION independent source effects"
    );
}
