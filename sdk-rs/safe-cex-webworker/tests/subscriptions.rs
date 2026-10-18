#[path = "support/data_trace.rs"]
mod data_trace;
#[path = "support/error_post.rs"]
mod error_post_support;
#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use indexmap::IndexMap;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ListenerCleanup},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_types::*;
use safe_cex_webworker::{order_book_post::*, payload::*, subscriptions::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
type Listeners<T> = Rc<RefCell<IndexMap<String, Rc<dyn Fn(T)>>>>;
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    books: Listeners<OrderBook>,
    candles: Listeners<Candle>,
    log: Rc<RefCell<Vec<Value>>>,
    setup_error: Cell<bool>,
    cleanup_error: Rc<Cell<bool>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.log.borrow_mut().push(json!(["subscribe", symbol]));
        if self.setup_error.get() {
            return Err(ExchangeError::new("authored listener setup error"));
        }
        self.books.borrow_mut().insert(symbol.clone(), callback);
        let log = self.log.clone();
        let cleanup_error = self.cleanup_error.clone();
        let books = self.books.clone();
        Ok(Box::new(move || {
            log.borrow_mut().push(json!(["unsubscribe", symbol]));
            books.borrow_mut().shift_remove(&symbol);
            if cleanup_error.get() {
                return Err(ExchangeError::new("authored listener cleanup error"));
            }
            Ok(())
        }))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let interval = serde_json::to_value(options.interval).unwrap();
        let key = format!("{}:{}", options.symbol, interval.as_str().unwrap());
        self.log.borrow_mut().push(json!(["subscribe", key]));
        if self.setup_error.get() {
            return Err(ExchangeError::new("authored listener setup error"));
        }
        self.candles.borrow_mut().insert(key.clone(), callback);
        let log = self.log.clone();
        let cleanup_error = self.cleanup_error.clone();
        let candles = self.candles.clone();
        Ok(Box::new(move || {
            log.borrow_mut().push(json!(["unsubscribe", key]));
            candles.borrow_mut().shift_remove(&key);
            if cleanup_error.get() {
                return Err(ExchangeError::new("authored listener cleanup error"));
            }
            Ok(())
        }))
    }
}
fn numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        v => v,
    }
}

// Trace-only adapter: native book assertions run in the dedicated leaf witness.
// Production hosts must bind structured clone, never this JSON comparison sink.
fn trace_post(post: Rc<dyn Fn(StoreWorkerPayload)>) -> OrderBookPost {
    OrderBookPost::new(Rc::new(move |p| {
        post(StoreWorkerPayload {
            op: p.op,
            instance_id: None,
            data: Some(serde_json::to_value(p.data).unwrap()),
        });
        Ok(())
    }))
}
#[test]
fn bbo_reports_subscriber_identity_after_post_and_committed_write() {
    for name in ["Error", "TypeError"] {
        let runtime = Rc::new(Runtime::default());
        runtime.advance(10_000);
        let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
        store.update(serde_json::from_value(json!({
            "markets":[{"id":"m","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":2,"price":0.01},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":3,"max":50}}}],
            "tickers":[{"id":"t","symbol":"BTC","bid":100,"ask":102,"last":101,"mark":103,"index":104,"percentage":5,"openInterest":6,"fundingRate":0.01,"fundingTime":8,"volume":9,"quoteVolume":10}]
        })).unwrap(), true).unwrap();
        let remove_warmup = store.subscribe(Box::new(|_, _| Ok(())));
        remove_warmup();
        let venue = Rc::new(Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"bbo-key","secret":"bbo-secret"})).unwrap(),
                store.clone(),
                runtime.clone(),
                Default::default(),
            ),
            capabilities: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            log: Default::default(),
            setup_error: Cell::new(false),
            cleanup_error: Rc::new(Cell::new(false)),
        });
        let posts = Rc::new(RefCell::new(Vec::new()));
        let sink = posts.clone();
        let observed_store = store.clone();
        let handler = OrderBookSubscriptionsHandler::new(
            venue.clone(),
            trace_post(Rc::new(move |payload| {
                assert_eq!(
                    observed_store.tickers()[0].bid,
                    100.0,
                    "BBO-IDENTITY post precedes mutation"
                );
                sink.borrow_mut().push(payload);
            })),
            100.0,
            error_post_support::unexpected(),
        );
        handler
            .handle(StoreWorkerPayload {
                op: StoreWorkerOp::SubscribeOrderBook,
                instance_id: None,
                data: Some(json!({"subId":"identity","symbol":"BTC"})),
            })
            .unwrap();
        let mut original = safe_cex_exchange::runtime::ExchangeError::new("bbo subscriber failed");
        original.name = name.into();
        original.metadata = Box::new(safe_cex_exchange::runtime::ExchangeErrorMetadata {
            origin: None,
            store_subscriber_ids: vec![401],
            store_subscriber_cause: None,
            is_axios_error: true,
            request: Some(json!({"request":"bbo-request"})),
            stack: Some("bbo-stack".into()),
            response: Some(json!({"response":"bbo-response"})),
            context: Some(json!({"origin":"subscriber"})),
            config: Some(json!({"config":"bbo-config"})),
            enhanced_message: Some("bbo-enhanced".into()),
            url: Some("https://example.invalid/bbo".into()),
            method: Some("POST".into()),
            original_error: Some(json!({"original":"bbo-original"})),
            code: Some(safe_cex_exchange::js_runtime::value::from_json(&json!(197))),
        });
        let original_cause: safe_cex_types::StoreSubscriberError = original.clone().into();
        let thrown = original_cause.clone();
        let calls = Rc::new(Cell::new(0));
        let first_calls = calls.clone();
        let sent = posts.clone();
        let report = runtime.clone();
        let unsubscribe_first = store.subscribe(Box::new(move |data, _| {
            first_calls.set(first_calls.get() + 1);
            assert_eq!(
                sent.borrow().len(),
                1,
                "BBO-IDENTITY notification follows post"
            );
            assert_eq!(
                data.tickers[0].bid, 101.0,
                "BBO-IDENTITY mutation precedes failure"
            );
            assert_eq!(
                data.tickers[0].ask, 103.0,
                "BBO-IDENTITY both fields committed"
            );
            assert!(
                report.exceptions.borrow().is_empty(),
                "BBO-IDENTITY failure precedes host report"
            );
            Err(thrown.clone())
        }));
        let later = Rc::new(Cell::new(0));
        let second_calls = later.clone();
        let unsubscribe_second = store.subscribe(Box::new(move |_, _| {
            second_calls.set(second_calls.get() + 1);
            Ok(())
        }));
        let callback = venue.books.borrow()["BTC"].clone();
        callback(OrderBook {
            bids: vec![OrderBookOrders {
                price: 101.0,
                amount: 2.0,
                total: 7.0,
            }],
            asks: vec![OrderBookOrders {
                price: 103.0,
                amount: 3.0,
                total: 11.0,
            }],
        });
        assert_eq!(calls.get(), 1, "BBO-IDENTITY first subscriber count");
        assert_eq!(later.get(), 0, "BBO-IDENTITY later subscriber stopped");
        let reports = runtime.exceptions.borrow();
        assert_eq!(reports.len(), 1, "BBO-IDENTITY one visible host failure");
        let (error, context) = &reports[0];
        // Warmup and BaseExchange's Store subscription occupy IDs 0 and 1.
        assert_eq!(
            error.store_subscriber_ids,
            vec![401, 2],
            "BBO-IDENTITY inner-to-outer nonzero subscriber"
        );
        assert!(
            std::ptr::eq(
                std::error::Error::source(error).expect("BBO-IDENTITY retained source"),
                std::error::Error::source(&original_cause).unwrap(),
            ),
            "BBO-IDENTITY exact original subscriber cause pointer"
        );
        let mut expected = original;
        expected.store_subscriber_ids.push(2);
        expected.store_subscriber_cause = Some(original_cause);
        assert_eq!(
            error, &expected,
            "BBO-IDENTITY complete retained error metadata"
        );
        assert_eq!(
            context,
            &json!({"method":"sendOrderBookUpdate"}).into(),
            "BBO-IDENTITY host boundary context"
        );
        unsubscribe_first();
        unsubscribe_second();
    }
}
#[test]
fn subscriptions_match_live_source_dedup_fanout_flush_and_first_envelope() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/subscriptions_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scenarios: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(scenarios.len(), 100);
    for (i, s) in scenarios.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        let mut eth = s["market"].clone();
        eth["id"] = json!("ETH");
        eth["symbol"] = json!("ETH");
        Store::update(
            store.as_ref(),
            serde_json::from_value::<StoreUpdate>(
                json!({"markets":[s["market"],eth],"tickers":[s["ticker"]]}),
            )
            .unwrap(),
            true,
        )
        .unwrap();
        let log = Rc::new(RefCell::new(Vec::new()));
        let venue=Rc::new(Venue{base:BaseExchange::new(ExchangeName::Bybit,serde_json::from_value(json!({"key":"","secret":"","tuning":{"dataUpdateThrottleMultiplier":s["multiplier"]}})).unwrap(),store.clone(),runtime.clone(),Default::default()),capabilities:Default::default(),books:Default::default(),candles:Default::default(),log:log.clone(),setup_error:Cell::new(false),cleanup_error:Rc::new(Cell::new(false))});
        let reentrant = Rc::new(Cell::new(false));
        let reenter = reentrant.clone();
        let target: Rc<RefCell<Option<std::rc::Weak<OHLCVSubscriptionsHandler>>>> =
            Rc::new(RefCell::new(None));
        let callback_target = target.clone();
        let sink = log.clone();
        let clock = runtime.now.clone();
        let snapshot = store.clone();
        let post: Rc<dyn Fn(StoreWorkerPayload)> =
            Rc::new(move |p| {
                let ticker = Store::tickers(snapshot.as_ref());
                sink.borrow_mut().push(json!([
                    "post",
                    clock.get(),
                    p,
                    ticker[0].bid,
                    ticker[0].ask
                ]));
                drop(ticker);
                if reenter.replace(false) {
                    let handler = callback_target
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .upgrade()
                        .unwrap();
                    handler
                        .handle(StoreWorkerPayload {
                            op: StoreWorkerOp::UnsubscribeOHLCV,
                            instance_id: None,
                            data: Some(json!({"subId":"b"})),
                        })
                        .unwrap();
                    handler.handle(StoreWorkerPayload {
                    op: StoreWorkerOp::SubscribeOHLCV,
                    instance_id: None,
                    data: Some(
                        json!({"subId":"c","symbol":"BTC","interval":"1m","tag":"during-post"}),
                    ),
                }).unwrap();
                }
            });
        let books = OrderBookSubscriptionsHandler::new(
            venue.clone(),
            trace_post(post.clone()),
            100.0,
            error_post_support::unexpected(),
        );
        let candles = OHLCVSubscriptionsHandler::new(
            venue.clone(),
            data_trace::post(post),
            error_post_support::unexpected(),
        );
        *target.borrow_mut() = Some(Rc::downgrade(&candles));
        let ob = s["kind"] == "orderbook";
        for a in s["actions"].as_array().unwrap() {
            match a["op"].as_str().unwrap() {
                "sub" | "unsub" => {
                    let op = match (ob, a["op"] == "sub") {
                        (true, true) => StoreWorkerOp::SubscribeOrderBook,
                        (true, false) => StoreWorkerOp::UnsubscribeOrderBook,
                        (false, true) => StoreWorkerOp::SubscribeOHLCV,
                        (false, false) => StoreWorkerOp::UnsubscribeOHLCV,
                    };
                    let payload = StoreWorkerPayload {
                        op,
                        instance_id: None,
                        data: Some(a["data"].clone()),
                    };
                    if ob {
                        books.handle(payload).unwrap()
                    } else {
                        candles.handle(payload).unwrap()
                    }
                }
                "reentrant" => reentrant.set(!ob),
                "markets" => {
                    let rows = if a["loaded"] == true {
                        vec![s["market"].clone()]
                    } else {
                        vec![]
                    };
                    Store::update(
                        store.as_ref(),
                        serde_json::from_value::<StoreUpdate>(json!({"markets":rows})).unwrap(),
                        true,
                    )
                    .unwrap();
                }
                "advance" => runtime.advance(a["t"].as_u64().unwrap()),
                "dispose" => {
                    if ob {
                        books.base.dispose()
                    } else {
                        candles.base.dispose()
                    }
                }
                "emit" => {
                    let n = a["n"].as_f64().unwrap();
                    if ob {
                        let callbacks: Vec<_> = venue.books.borrow().values().cloned().collect();
                        for callback in callbacks {
                            callback(OrderBook {
                                bids: vec![OrderBookOrders {
                                    price: 100.0 + n,
                                    amount: 2.0,
                                    total: 7.0,
                                }],
                                asks: vec![OrderBookOrders {
                                    price: 102.0 + n,
                                    amount: 3.0,
                                    total: 11.0,
                                }],
                            });
                        }
                    } else {
                        let callbacks: Vec<_> = venue.candles.borrow().values().cloned().collect();
                        for callback in callbacks {
                            callback(Candle {
                                timestamp: a["timestamp"].as_f64().unwrap(),
                                open: n,
                                high: n + 3.0,
                                low: n - 2.0,
                                close: n + 1.0,
                                volume: n + 17.0,
                                last_side: None,
                            });
                        }
                    }
                }
                other => panic!("unknown action {other}"),
            }
        }
        assert_eq!(
            numbers(json!(*log.borrow())),
            numbers(s["expected"].clone()),
            "SUBSCRIPTION-IDENTITY source callback fields scenario {i}: {}",
            s["kind"]
        );
    }
}

#[test]
fn bbo_native_patch_preserves_existing_and_new_nonfinite_prices() {
    let output = std::process::Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/subscriptions_oracle.mjs"
            ),
            "--nonfinite-witness",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scenarios: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let source = scenarios
        .iter()
        .find(|s| {
            s["kind"] == "orderbook"
                && s["multiplier"] == 1
                && s["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["op"] == "poisonTicker")
        })
        .unwrap();
    assert_eq!(source["finalTickerNumbers"]["last"], "NaN");
    assert_eq!(source["finalTicker"]["bid"], 101);
    let runtime = Rc::new(Runtime::default());
    let store = Rc::new(DefaultStore::new());
    let mut ticker: Ticker = serde_json::from_value(source["ticker"].clone()).unwrap();
    ticker.last = f64::NAN;
    Store::update(
        store.as_ref(),
        StoreUpdate {
            tickers: Some(vec![ticker]),
            markets: Some(vec![
                serde_json::from_value(source["market"].clone()).unwrap(),
            ]),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    let venue = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store.clone(),
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
        books: Default::default(),
        candles: Default::default(),
        log: Default::default(),
        setup_error: Cell::new(false),
        cleanup_error: Rc::new(Cell::new(false)),
    });
    let handler = OrderBookSubscriptionsHandler::new(
        venue.clone(),
        trace_post(Rc::new(|_| {})),
        100.0,
        error_post_support::unexpected(),
    );
    handler
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOrderBook,
            instance_id: None,
            data: Some(json!({"subId":"a","symbol":"BTC"})),
        })
        .unwrap();
    let callback = venue.books.borrow()["BTC"].clone();
    callback(serde_json::from_value(json!({"bids":[{"price":101,"amount":1,"total":1}],"asks":[{"price":103,"amount":1,"total":1}]})).unwrap());
    let tickers = Store::tickers(store.as_ref());
    assert!(tickers[0].last.is_nan());
    assert_eq!(tickers[0].bid, 101.0);
    assert_eq!(tickers[0].ask, 103.0);
    drop(tickers);
    let source = scenarios
        .iter()
        .find(|s| {
            s["kind"] == "orderbook"
                && s["multiplier"] == 1
                && s["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["op"] == "poisonBook")
        })
        .unwrap();
    assert_eq!(source["finalTickerNumbers"]["bid"], "Infinity");
    assert_eq!(source["finalTickerNumbers"]["ask"], "NaN");
    assert_eq!(
        source["sentBookNumbers"][0],
        json!({"bid":"Infinity","ask":"NaN"})
    );
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let fresh = OrderBookSubscriptionsHandler::new(
        venue.clone(),
        OrderBookPost::new(Rc::new(move |p| {
            let book = p.data.book().unwrap();
            assert_eq!(
                book.bids[0].price,
                f64::INFINITY,
                "BBO native post retains Infinity"
            );
            assert!(book.asks[0].price.is_nan(), "BBO native post retains NaN");
            drop(book);
            sink.borrow_mut().push(p.structured_clone());
            Ok(())
        })),
        100.0,
        error_post_support::unexpected(),
    );
    fresh
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOrderBook,
            instance_id: None,
            data: Some(json!({"subId":"fresh","symbol":"BTC"})),
        })
        .unwrap();
    let callback = venue.books.borrow()["BTC"].clone();
    callback(OrderBook {
        bids: vec![OrderBookOrders {
            price: f64::INFINITY,
            amount: 1.0,
            total: 1.0,
        }],
        asks: vec![OrderBookOrders {
            price: f64::NAN,
            amount: 1.0,
            total: 1.0,
        }],
    });
    let tickers = Store::tickers(store.as_ref());
    assert_eq!(
        tickers[0].bid,
        f64::INFINITY,
        "NATIVE-BBO must not retain stale bid"
    );
    assert!(
        tickers[0].ask.is_nan(),
        "NATIVE-BBO must not retain stale ask"
    );
    assert!(
        tickers[0].last.is_nan(),
        "NATIVE-BBO unrelated field remains native"
    );
    let messages = messages.borrow();
    let book = messages[0].data.book().unwrap();
    assert_eq!(
        book.bids[0].price,
        f64::INFINITY,
        "BBO received clone retains Infinity"
    );
    assert!(
        book.asks[0].price.is_nan(),
        "BBO received clone retains NaN"
    );
}

fn leaf_venue() -> (Rc<Venue>, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
    store.update(serde_json::from_value(json!({"tickers":[{"id":"t","symbol":"BTC","bid":99,"ask":101,"last":100,"mark":100,"index":100,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0}]})).unwrap(),true).unwrap();
    (
        Rc::new(Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                store,
                runtime.clone(),
                Default::default(),
            ),
            capabilities: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            log: Default::default(),
            setup_error: Cell::new(false),
            cleanup_error: Rc::new(Cell::new(false)),
        }),
        runtime,
    )
}
fn native_book() -> OrderBook {
    OrderBook {
        bids: vec![OrderBookOrders {
            price: f64::INFINITY,
            amount: -0.0,
            total: f64::NAN,
        }],
        asks: vec![OrderBookOrders {
            price: f64::NEG_INFINITY,
            amount: f64::NAN,
            total: -0.0,
        }],
    }
}
fn assert_book_numbers(book: &OrderBook) {
    assert_eq!(book.bids.len(), 1, "BBO native bid count");
    assert_eq!(book.asks.len(), 1, "BBO native ask count");
    assert_eq!(
        book.bids[0].price,
        f64::INFINITY,
        "BBO native leaf Infinity"
    );
    assert_eq!(
        book.bids[0].amount.to_bits(),
        (-0.0f64).to_bits(),
        "BBO native leaf negative zero"
    );
    assert!(book.bids[0].total.is_nan(), "BBO native leaf NaN");
    assert_eq!(
        book.asks[0].price,
        f64::NEG_INFINITY,
        "BBO native leaf negative Infinity"
    );
    assert!(book.asks[0].amount.is_nan(), "BBO native leaf ask NaN");
    assert_eq!(
        book.asks[0].total.to_bits(),
        (-0.0f64).to_bits(),
        "BBO native leaf ask negative zero"
    );
}
#[test]
fn bbo_leaf_posts_native_clone_and_observes_reentrant_book_changes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/store_notification_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["bboMetadata"]["units"],
        json!([0xd800]),
        "BBO source materialized metadata unit"
    );
    assert_eq!(
        source["bboMetadata"]["postCalls"], 1,
        "BBO source metadata post count"
    );
    let metadata_input = source["bboMetadata"]["inputJson"].as_str().unwrap();
    assert!(
        serde_json::from_str::<serde_json::Map<String, Value>>(metadata_input).is_err(),
        "BBO native whole metadata input refuses lone unit"
    );
    let rows = source["bboLeaf"].as_array().unwrap();
    assert_eq!(rows.len(), 4, "BBO independent source leaf floor");
    for (index, scenario) in ["numbers", "post-error", "book-mutation", "metadata-alias"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(rows[index]["scenario"], scenario);
        assert_eq!(rows[index]["postCalls"], 1);
        let (venue, runtime) = leaf_venue();
        let observed = venue.base.store.clone();
        let calls = Rc::new(Cell::new(0));
        let counted = calls.clone();
        let received = Rc::new(RefCell::new(None));
        let receipt = received.clone();
        let mut failure = safe_cex_exchange::runtime::ExchangeError::new("post failed");
        failure.name = "TypeError".into();
        let thrown = failure.clone();
        let post = OrderBookPost::new(Rc::new(move |mut post| {
            counted.set(counted.get() + 1);
            assert_eq!(
                observed.tickers()[0].bid,
                99.0,
                "BBO native post precedes BBO"
            );
            assert_book_numbers(&post.data.book().unwrap());
            assert_eq!(
                post.data.fields().map(|(key, _)| key).collect::<Vec<_>>(),
                ["2", "42", "symbol", "orderbook", "extra"],
                "BBO native own-key order"
            );
            *receipt.borrow_mut() = Some(post.structured_clone());
            if scenario == "post-error" {
                return Err(thrown.clone());
            }
            if scenario == "book-mutation" {
                {
                    let mut book = post.data.book_mut().unwrap();
                    book.bids[0].price = 101.0;
                    book.asks[0].price = 103.0;
                }
                // Reassigning the posted field must not replace the sender's original o.
                *post.data.get_mut("orderbook").unwrap() =
                    OrderBookPostField::Metadata(json!({"bids":[],"asks":[]}).into());
            }
            if scenario == "metadata-alias" {
                let OrderBookPostField::Metadata(extra) = post.data.get_mut("extra").unwrap()
                else {
                    panic!("metadata field")
                };
                let safe_cex_num::JsJsonValue::Object(fields) = extra else {
                    panic!("metadata object")
                };
                fields.iter_mut().find(|(key, _)| key == "x").unwrap().1 =
                    safe_cex_num::JsJsonValue::Number(9.0);
            }
            Ok(())
        }));
        let handler = OrderBookSubscriptionsHandler::new(
            venue.clone(),
            post,
            100.0,
            error_post_support::unexpected(),
        );
        let data = serde_json::from_str::<serde_json::Map<String, Value>>(
            r#"{"42":"b","2":"a","symbol":"BTC","orderbook":null,"extra":{"x":1}}"#,
        )
        .unwrap();
        let data: JsObjectMap<safe_cex_num::JsJsonValue> = data
            .into_iter()
            .map(|(key, value)| (key, value.into()))
            .collect();
        let caller_book = Rc::new(RefCell::new(native_book()));
        let result = handler.send_order_book_update(&data, caller_book.clone());
        if scenario == "post-error" {
            assert_eq!(
                result,
                Err(OrderBookUpdateError::Post(failure)),
                "BBO native leaf post error"
            )
        } else {
            result.unwrap()
        }
        assert_eq!(calls.get(), 1, "BBO native one post");
        assert!(
            runtime.exceptions.borrow().is_empty(),
            "BBO native leaf does not flatten errors into reports"
        );
        assert_book_numbers(&received.borrow().as_ref().unwrap().data.book().unwrap());
        let ticker = venue.base.store.tickers()[0].clone();
        let (bid, ask) = if scenario == "post-error" {
            (99.0, 101.0)
        } else if scenario == "book-mutation" {
            (101.0, 103.0)
        } else {
            (f64::INFINITY, f64::NEG_INFINITY)
        };
        assert_eq!(ticker.bid, bid, "BBO native post result drives BBO");
        assert_eq!(ticker.ask, ask);
        assert_eq!(
            caller_book.borrow().bids[0].price,
            if scenario == "book-mutation" {
                101.0
            } else {
                f64::INFINITY
            },
            "BBO caller book alias remains live"
        );
        let safe_cex_num::JsJsonValue::Object(extra) = &data["extra"] else {
            panic!("native extra object")
        };
        assert_eq!(
            extra.iter().find(|(key, _)| key == "x").unwrap().1,
            safe_cex_num::JsJsonValue::Number(1.0),
            "BBO retained owned metadata boundary"
        );
        assert_eq!(
            rows[index]["extra"],
            if scenario == "metadata-alias" { 9 } else { 1 },
            "BBO witnessed source metadata alias"
        );
        // Secondary wire observation follows all native field/clone/error assertions.
        let encoded = serde_json::to_value(received.borrow().as_ref().unwrap()).unwrap();
        assert_eq!(
            encoded["data"]["orderbook"]["bids"][0]["price"],
            Value::Null,
            "BBO explicit JSON trace is lossy"
        );
    }
}
#[test]
fn bbo_leaf_returns_original_store_notification_after_committed_write() {
    use std::error::Error;
    #[derive(Debug)]
    struct Cause;
    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("arbitrary subscriber cause")
        }
    }
    impl Error for Cause {}
    let (venue, runtime) = leaf_venue();
    let cause = Rc::new(Cause);
    let original = StoreSubscriberError::new("subscriber failed").with_cause(cause.clone());
    let failure = original.clone();
    let seen = Rc::new(Cell::new(0));
    let counted = seen.clone();
    let unsubscribe = venue.base.store.subscribe(Box::new(move |data, _| {
        counted.set(counted.get() + 1);
        assert_eq!(
            data.tickers[0].bid,
            f64::INFINITY,
            "BBO leaf Store commit precedes failure"
        );
        Err(failure.clone())
    }));
    let later = Rc::new(Cell::new(0));
    let called = later.clone();
    let unsubscribe_later = venue.base.store.subscribe(Box::new(move |_, _| {
        called.set(called.get() + 1);
        Ok(())
    }));
    let posted = Rc::new(Cell::new(0));
    let posts = posted.clone();
    let handler = OrderBookSubscriptionsHandler::new(
        venue.clone(),
        OrderBookPost::new(Rc::new(move |post| {
            assert_book_numbers(&post.data.book().unwrap());
            posts.set(posts.get() + 1);
            Ok(())
        })),
        100.0,
        error_post_support::unexpected(),
    );
    let data = JsObjectMap::from_iter([(
        "symbol".into(),
        safe_cex_num::JsJsonValue::String("BTC".into()),
    )]);
    let result = handler.send_order_book_update(&data, Rc::new(RefCell::new(native_book())));
    assert!(result.is_err(), "BBO leaf returns Store failure");
    let error = result.unwrap_err();
    let OrderBookUpdateError::Store(safe_cex_store::StoreMutationError::Notification(notification)) =
        error
    else {
        panic!("BBO leaf original Store notification variant")
    };
    assert_eq!(
        notification.subscriber_id(),
        1,
        "BBO leaf actual subscriber ID"
    );
    assert_eq!(
        notification.cause(),
        &original,
        "BBO leaf original subscriber error"
    );
    assert!(
        std::ptr::eq(
            notification
                .cause()
                .source()
                .unwrap()
                .downcast_ref::<Cause>()
                .unwrap(),
            cause.as_ref()
        ),
        "BBO leaf cause identity"
    );
    assert_eq!(posted.get(), 1);
    assert_eq!(seen.get(), 1);
    assert_eq!(later.get(), 0, "BBO leaf stops fanout");
    assert!(
        runtime.exceptions.borrow().is_empty(),
        "BBO leaf returns rather than reports"
    );
    unsubscribe();
    unsubscribe_later();
}

#[test]
fn bbo_leaf_metadata_keeps_native_numbers_with_parent_json_finding_explicit() {
    use safe_cex_num::JsJsonValue;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/store_notification_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = [
        ("NaN", f64::NAN),
        ("Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
        ("-0", -0.0),
    ];
    assert_eq!(source["bboNumericMetadata"].as_array().unwrap().len(), 4);
    for (index, (kind, metric)) in cases.into_iter().enumerate() {
        assert_eq!(source["bboNumericMetadata"][index]["kind"], kind);
        assert_eq!(source["bboNumericMetadata"][index]["calls"], 1);
        let (venue, _) = leaf_venue();
        let calls = Rc::new(Cell::new(0));
        let counted = calls.clone();
        let handler = OrderBookSubscriptionsHandler::new(
            venue,
            OrderBookPost::new(Rc::new(move |post| {
                counted.set(counted.get() + 1);
                for data in [&post.data, &post.structured_clone().data] {
                    let Some(OrderBookPostField::Metadata(JsJsonValue::Number(value))) =
                        data.get("metric")
                    else {
                        panic!("BBO native metadata number variant")
                    };
                    if metric.is_nan() {
                        assert!(value.is_nan(), "BBO native metadata NaN")
                    } else {
                        assert_eq!(
                            value.to_bits(),
                            metric.to_bits(),
                            "BBO native metadata binary64"
                        )
                    }
                }
                Ok(())
            })),
            100.0,
            error_post_support::unexpected(),
        );
        let data = JsObjectMap::from_iter([
            ("symbol".into(), JsJsonValue::String("BTC".into())),
            ("metric".into(), JsJsonValue::Number(metric)),
        ]);
        handler
            .send_order_book_update(
                &data,
                Rc::new(RefCell::new(OrderBook {
                    bids: vec![],
                    asks: vec![],
                })),
            )
            .unwrap();
        assert_eq!(calls.get(), 1, "BBO native metadata one post");
    }
    // Separate parent finding: its existing JSON payload has already lost NaN.
    assert_eq!(source["bboParentMetadata"]["kind"], "NaN");
    assert_eq!(source["bboParentMetadata"]["posts"], 1);
    let (venue, _) = leaf_venue();
    venue.base.store.update(serde_json::from_value(json!({"markets":[{"id":"m","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":2,"price":0.01},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":3,"max":50}}}]})).unwrap(),true).unwrap();
    let posts = Rc::new(Cell::new(0));
    let counted = posts.clone();
    let handler = OrderBookSubscriptionsHandler::new(
        venue.clone(),
        OrderBookPost::new(Rc::new(move |post| {
            assert!(
                matches!(
                    post.data.get("metric"),
                    Some(OrderBookPostField::Metadata(JsJsonValue::Null))
                ),
                "FINDING parent JSON metadata projected NaN to null before native leaf"
            );
            counted.set(counted.get() + 1);
            Ok(())
        })),
        100.0,
        error_post_support::unexpected(),
    );
    handler
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOrderBook,
            instance_id: None,
            data: Some(json!({"symbol":"BTC","subId":"metadata","metric":f64::NAN})),
        })
        .unwrap();
    let callback = venue.books.borrow()["BTC"].clone();
    callback(OrderBook {
        bids: vec![],
        asks: vec![],
    });
    assert_eq!(posts.get(), 1, "FINDING native parent metadata post count");
}

#[test]
fn listener_setup_failure_propagates_but_cleanup_failure_keeps_source_catch_order() {
    for book in [true, false] {
        let (venue, runtime) = leaf_venue();
        runtime.advance(10_000);
        let mut loaded = venue.base.store.loaded().clone();
        loaded.markets = true;
        venue.base.store.update(StoreUpdate { markets:Some(vec![serde_json::from_value(json!({"id":"m","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":20}}})).unwrap()]), loaded:Some(loaded), ..Default::default() }, false).unwrap();
        let posts = Rc::new(Cell::new(0));
        let count = posts.clone();
        let post: Rc<dyn Fn(StoreWorkerPayload)> = Rc::new(move |_| count.set(count.get() + 1));
        let books = OrderBookSubscriptionsHandler::new(
            venue.clone(),
            trace_post(post.clone()),
            100.0,
            error_post_support::unexpected(),
        );
        let candles = OHLCVSubscriptionsHandler::new(
            venue.clone(),
            data_trace::post(post),
            error_post_support::unexpected(),
        );
        let payload = |subscribe, id: &str| StoreWorkerPayload {
            op: match (book, subscribe) {
                (true, true) => StoreWorkerOp::SubscribeOrderBook,
                (true, false) => StoreWorkerOp::UnsubscribeOrderBook,
                (false, true) => StoreWorkerOp::SubscribeOHLCV,
                (false, false) => StoreWorkerOp::UnsubscribeOHLCV,
            },
            instance_id: None,
            data: Some(if subscribe {
                json!({"symbol":"BTC","interval":"1m","subId":id})
            } else {
                json!({"subId":id})
            }),
        };
        let handle = |p| {
            if book {
                books.handle(p)
            } else {
                candles.handle(p)
            }
        };
        venue.setup_error.set(true);
        let error = handle(payload(true, "a")).unwrap_err();
        assert_eq!(
            error.message, "authored listener setup error",
            "native subscription setup failure propagates before insertion: book={book}"
        );
        assert_eq!(
            venue.books.borrow().len() + venue.candles.borrow().len(),
            0,
            "native failed setup installs zero listener callbacks"
        );
        venue.setup_error.set(false);
        handle(payload(true, "a")).unwrap();
        let emit: Box<dyn Fn()> = if book {
            let cb = venue.books.borrow()["BTC"].clone();
            Box::new(move || {
                cb(serde_json::from_value(json!({"bids":[{"price":99,"amount":1,"total":99}],"asks":[{"price":101,"amount":1,"total":101}]})).unwrap())
            })
        } else {
            let cb = venue.candles.borrow()["BTC:1m"].clone();
            Box::new(move || {
                cb(serde_json::from_value(
                    json!({"timestamp":7,"open":1,"high":3,"low":0.5,"close":2,"volume":4}),
                )
                .unwrap())
            })
        };
        emit();
        emit();
        assert_eq!(
            posts.get(),
            1,
            "native listener leading update before cleanup failure"
        );
        assert_eq!(
            runtime.tasks.borrow().len(),
            1,
            "native authored listener has one pending timer before cleanup"
        );
        venue.cleanup_error.set(true);
        handle(payload(false, "a")).expect("native worker catches listener cleanup failure");
        assert_eq!(
            runtime.tasks.borrow().len(),
            1,
            "native throwing cleanup retains one pending timer"
        );
        assert!(
            !runtime.tasks.borrow()[0].cancelled.get(),
            "native cleanup failure skips throttle cancellation: book={book}"
        );
        runtime.advance(10_100);
        assert_eq!(
            posts.get(),
            if book { 2 } else { 1 },
            "native queued delivery sees current subscriber payloads after removal: book={book}"
        );
        handle(payload(true, "b")).unwrap();
        let subscribes = venue
            .log
            .borrow()
            .iter()
            .filter(|v| v[0] == "subscribe")
            .count();
        assert_eq!(
            subscribes, 3,
            "native cleanup failure still removes worker map before resubscribe: book={book}"
        );
    }
}

#[test]
fn ohlcv_worker_subscribes_every_source_timeframe_and_delivers_candles() {
    let (venue, _) = leaf_venue();
    venue.base.store.update(serde_json::from_value(json!({"markets":[{"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":20}}}]})).unwrap(), false).unwrap();
    let posts = Rc::new(RefCell::new(Vec::new()));
    let capture = posts.clone();
    let handler = OHLCVSubscriptionsHandler::new(
        venue.clone(),
        data_trace::post(Rc::new(move |post| capture.borrow_mut().push(post))),
        error_post_support::unexpected(),
    );
    let intervals = [
        "1M", "1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h",
        "12h", "1d", "1w", "3h", "5h", "7h", "8h", "9h", "10h", "11h", "24h", "2d", "3d",
    ];
    for (i, interval) in intervals.iter().enumerate() {
        handler
            .handle(StoreWorkerPayload {
                op: StoreWorkerOp::SubscribeOHLCV,
                instance_id: None,
                data: Some(json!({"symbol":"BTC","interval":interval,"subId":format!("s{i}")})),
            })
            .unwrap();
        let callback = venue
            .candles
            .borrow()
            .get(&format!("BTC:{interval}"))
            .cloned()
            .unwrap_or_else(|| panic!("OHLCV-WORKER subscription for {interval} must reach venue"));
        callback(
            serde_json::from_value(
                json!({"timestamp":123,"open":1,"high":3,"low":0.5,"close":2,"volume":4}),
            )
            .unwrap(),
        );
        let posts = posts.borrow();
        let post = posts.last().unwrap();
        assert_eq!(post.op, StoreWorkerOp::ListenOHLCV);
        assert_eq!(post.data.as_ref().unwrap()["interval"], *interval);
        assert_eq!(post.data.as_ref().unwrap()["subId"], format!("s{i}"));
        assert_eq!(post.data.as_ref().unwrap()["candle"]["volume"], 4.0);
    }
    assert_eq!(venue.candles.borrow().len(), 27);
    assert_eq!(posts.borrow().len(), 27);
}
