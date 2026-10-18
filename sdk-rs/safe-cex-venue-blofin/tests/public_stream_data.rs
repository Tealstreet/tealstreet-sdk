use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::{js_from_value, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, StoreDataLoaded, Ticker};
use safe_cex_venue_blofin::{
    public_stream_core::PublicStreamHost, public_stream_data::*, utility::UtilityInput,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_stream_data_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            93,
            "independent93floor"
        );
        assert_eq!(v["stoppedAfter"], 93);
        v
    })
}
fn number(n: f64, e: &Value) {
    assert_eq!(
        n.to_bits(),
        u64::from_str_radix(e["number"].as_str().unwrap(), 16).unwrap(),
        "native numeric bits"
    );
}
fn plain(n: &Value, e: &Value) {
    if e.get("number").is_some() {
        number(js_value_as_f64(n).unwrap(), e)
    } else {
        match (n, e) {
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.len(), b.len());
                for (k, v) in a {
                    plain(v, &b[k])
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for (n, e) in a.iter().zip(b) {
                    plain(n, e)
                }
            }
            _ => assert_eq!(n, e),
        }
    }
}
fn candle(n: &Candle, e: &Value) {
    for (v, k) in [
        (n.timestamp, "timestamp"),
        (n.open, "open"),
        (n.high, "high"),
        (n.low, "low"),
        (n.close, "close"),
        (n.volume, "volume"),
    ] {
        number(v, &e[k]);
    }
    assert_eq!(n.last_side, None);
}
fn book(n: &OrderBook, e: &Value) {
    for (rows, k) in [(&n.bids, "bids"), (&n.asks, "asks")] {
        let expected = e[k].as_array().unwrap();
        assert_eq!(rows.len(), expected.len());
        for (n, e) in rows.iter().zip(expected) {
            number(n.price, &e["price"]);
            number(n.amount, &e["amount"]);
            number(n.total, &e["total"]);
        }
    }
}
fn ticker(n: &Ticker, e: &Value) {
    assert_eq!(n.id, e["id"]);
    assert_eq!(n.symbol, e["symbol"]);
    for (v, k) in [
        (n.bid, "bid"),
        (n.ask, "ask"),
        (n.last, "last"),
        (n.mark, "mark"),
        (n.index, "index"),
        (n.percentage, "percentage"),
        (n.open_interest, "openInterest"),
        (n.funding_rate, "fundingRate"),
        (n.funding_time, "fundingTime"),
        (n.volume, "volume"),
        (n.quote_volume, "quoteVolume"),
    ] {
        number(v, &e[k]);
    }
    assert_eq!(n.timestamp, None);
    assert_eq!(n.mid_price, None);
    assert_eq!(n.info, None);
}
type TimerCallback = Rc<dyn Fn() -> Result<(), String>>;
enum Call {
    Custom(String, Value),
    Candle(Candle),
    Book(OrderBook),
}
struct Host {
    shared_payload: RefCell<Option<Rc<RefCell<UtilityInput>>>>,
    timer_events: RefCell<Vec<(u32, f64)>>,
    reentered: Cell<bool>,
    stream: RefCell<std::rc::Weak<BlofinPublicWebsocket<Host>>>,
    cleanups: Rc<RefCell<Vec<PublicCleanup<String>>>>,
    retained_books: RefCell<Vec<Rc<RefCell<OrderBook>>>>,
    retained_candles: RefCell<Vec<Rc<RefCell<Candle>>>>,
    initial: Value,
    store: DefaultStore,
    disposed: Cell<bool>,
    connected: Cell<bool>,
    socket: Cell<bool>,
    any_pong: Cell<bool>,
    next: Cell<u32>,
    sends: Cell<u32>,
    timers: RefCell<BTreeMap<u32, TimerCallback>>,
    trace: Rc<RefCell<Vec<Value>>>,
    sent: RefCell<Vec<String>>,
    calls: RefCell<Vec<Call>>,
}
impl Host {
    fn new(i: &Value) -> Rc<Self> {
        let store = DefaultStore::default();
        let mut initial_ticker = corpus()["ticker"].clone();
        if let Some(last) = i.get("last") {
            initial_ticker["last"] = last.clone()
        }
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(serde_json::from_value(json!([corpus()["market"]])).unwrap()),
                tickers: Some(serde_json::from_value(Value::Array(vec![initial_ticker])).unwrap()),
                loaded: Some(StoreDataLoaded {
                    markets: i["loaded"] != false,
                    tickers: true,
                    balance: false,
                    positions: false,
                    orders: false,
                }),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        Rc::new(Self {
            shared_payload: RefCell::default(),
            timer_events: RefCell::default(),
            reentered: Cell::new(false),
            stream: RefCell::default(),
            cleanups: Rc::default(),
            retained_books: RefCell::default(),
            retained_candles: RefCell::default(),
            initial: i.clone(),
            store,
            disposed: Cell::new(i["disposed"] == true),
            connected: Cell::new(i["connected"] != false),
            socket: Cell::new(false),
            any_pong: Cell::new(false),
            next: Cell::new(i["firstTimer"].as_u64().unwrap_or(1) as u32),
            sends: Cell::new(0),
            timers: RefCell::new(BTreeMap::new()),
            trace: Rc::default(),
            sent: RefCell::default(),
            calls: RefCell::default(),
        })
    }
    fn event(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
    fn call(&self, key: &str) -> Result<(), String> {
        if self.initial["fail"] == key {
            Err(format!("{key} failed"))
        } else {
            Ok(())
        }
    }
}
impl PublicStreamHost for Host {
    type Error = String;
    type Timer = u32;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_testnet(&self) -> bool {
        false
    }
    fn market_ids(&self) -> Result<Vec<std::string::String>, std::string::String> {
        Ok(Store::markets(&self.store)
            .unwrap()
            .iter()
            .map(|m| m.id.clone())
            .collect())
    }
    fn has_socket(&self) -> bool {
        self.socket.get()
    }
    fn create_and_assign_socket(&self, url: &str) -> Result<(), String> {
        self.socket.set(true);
        self.event(json!(["create", url]));
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), String> {
        self.event(json!(["listener", event]));
        Ok(())
    }
    fn send(&self, frame: &str) -> Result<(), String> {
        let n = self.sends.get() + 1;
        self.sends.set(n);
        self.call(&format!("send:{n}"))?;
        self.sent.borrow_mut().push(frame.into());
        self.event(json!(["send", frame]));
        if self.initial["reenterSend"] == true
            && !self.reentered.get()
            && serde_json::from_str::<Value>(frame).unwrap()["op"] == "unsubscribe"
        {
            self.reentered.set(true);
            let stream = self.stream.borrow().upgrade().unwrap();
            stream.on_message(r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"data":[{"bids":[["99","2"],["98","3"]],"asks":[["101","4"],["102","5"]],"seqId":"10"}],"action":"snapshot"}"#).map_err(|e|e.to_string())?;
        }
        Ok(())
    }
    fn performance_now(&self) -> f64 {
        123.5
    }
    fn markets_changed_off(&self) -> Result<(), String> {
        self.call("off")?;
        self.event(json!(["off"]));
        Ok(())
    }
    fn markets_changed_on(&self) -> Result<(), String> {
        self.event(json!(["on"]));
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        true
    }
    fn emit_ws_reconnected(&self) -> Result<(), String> {
        self.event(json!(["reconnected"]));
        Ok(())
    }
    fn set_timeout(&self, cb: TimerCallback, delay: f64) -> Result<u32, String> {
        self.call("timer")?;
        let n = self.next.get();
        self.next.set(n + 1);
        self.timers.borrow_mut().insert(n, cb);
        self.timer_events.borrow_mut().push((n, delay));
        self.event(json!(["timer", n, delay as u64]));
        Ok(n)
    }
    fn timer_is_truthy(&self, t: &u32) -> bool {
        *t != 0
    }
    fn clear_timeout(&self, t: &u32) -> Result<(), String> {
        self.call("clear")?;
        self.timers.borrow_mut().remove(t);
        self.event(json!(["clear", t]));
        Ok(())
    }
    fn log_warning(&self, m: &str) -> Result<(), String> {
        self.call("log")?;
        self.event(json!(["log", m, "warning"]));
        Ok(())
    }
    fn emit_warning(&self, m: &str, f: bool) -> Result<(), String> {
        self.event(json!(["warning", m, f]));
        Ok(())
    }
}
impl PublicDataHost for Host {
    fn store(&self) -> &dyn Store {
        let handle = self.shared_payload.borrow().clone();
        if let Some(handle) = handle {
            let mut value = handle.borrow().json().clone();
            value["nativeMutated"] = json!(77);
            value["data"][0]["bidPrice"] = json!("88");
            value["data"][0]["fundingRate"] = json!("0.0002");
            *handle.borrow_mut() = js_from_value::<UtilityInput>(value).unwrap();
        }
        &self.store
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn super_construct(&self) -> Result<(), String> {
        self.call("super-construct")?;
        self.event(json!(["super-construct"]));
        Ok(())
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.any_pong.set(enabled)
    }
    fn base_close_handler(&self) -> CloseCallback<String> {
        let trace = self.trace.clone();
        let fail = self.initial["fail"] == "base-close";
        Rc::new(move |event| {
            if fail {
                return Err("base-close failed".into());
            }
            trace.borrow_mut().push(json!(["base-close", event]));
            Ok(())
        })
    }
    fn super_dispose(&self) -> Result<(), String> {
        self.call("super-dispose")?;
        self.event(json!(["super-dispose"]));
        self.disposed.set(true);
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), String> {
        self.call("track")?;
        self.event(json!(["track"]));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), String> {
        self.call("pong")?;
        self.event(json!(["pong"]));
        Ok(())
    }
    fn cleanup_message_handler_stats(&self, key: &str) -> Result<(), String> {
        self.call("cleanup")?;
        self.event(json!(["cleanup", key]));
        Ok(())
    }
    fn scheduled_error(&self, e: PublicDataError<String>) -> String {
        e.to_string()
    }
}
type TopicRows = Vec<(String, Vec<(String, String)>)>;
struct Snapshot {
    handlers: Vec<(String, usize)>,
    topics: TopicRows,
    timeout: Option<u32>,
    notified: bool,
    disposed: bool,
    connected: bool,
    ticker: Ticker,
    loaded: bool,
}
struct Run {
    host: Rc<Host>,
    snapshots: Vec<Snapshot>,
    results: Vec<Result<(), PublicDataError<String>>>,
}
fn run(c: &Value) -> Run {
    let host = Host::new(&c["initial"]);
    let stream = BlofinPublicWebsocket::new(host.clone()).unwrap();
    *host.stream.borrow_mut() = Rc::downgrade(&stream);
    assert!(
        host.any_pong.get(),
        "native inherited any-message keepalive field"
    );
    *stream.core.connection_failure_notice_timeout.borrow_mut() =
        c["initial"]["timeout"].as_u64().map(|n| n as u32);
    let cleanups = host.cleanups.clone();
    let mut snapshots = vec![];
    let mut results = vec![];
    for step in c["actions"].as_array().unwrap() {
        let arg = &step[1];
        let result = match step[0].as_str().unwrap() {
            "custom-default" => {
                let key = arg.as_str().unwrap_or("tickers");
                let original = stream.message_handlers.borrow()[key].borrow()[0].clone();
                let h = host.clone();
                let first: PublicDataCallback<String> = Rc::new(move |data| {
                    *h.shared_payload.borrow_mut() = Some(data.clone());
                    h.calls
                        .borrow_mut()
                        .push(Call::Custom("capture".into(), data.borrow().json().clone()));
                    h.event(json!(["custom", "capture"]));
                    Ok(())
                });
                let h = host.clone();
                let third: PublicDataCallback<String> = Rc::new(move |data| {
                    h.calls.borrow_mut().push(Call::Custom(
                        "after-store".into(),
                        data.borrow().json().clone(),
                    ));
                    h.event(json!(["custom", "after-store"]));
                    Ok(())
                });
                stream.message_handlers.borrow_mut().insert(
                    key.into(),
                    Rc::new(RefCell::new(vec![first, original, third])),
                );
                Ok(())
            }
            "custom" => {
                for (key, names) in [
                    ("books", vec!["first", "second"]),
                    ("books.BTC-USDT", vec!["exact"]),
                ] {
                    let mut callbacks = vec![];
                    for name in names {
                        let h = host.clone();
                        let cb: PublicDataCallback<String> = Rc::new(move |data| {
                            h.call(&format!("custom:{name}"))
                                .map_err(PublicDataError::Host)?;
                            if h.initial["mutatePayload"] == true && name == "first" {
                                let mut v = data.borrow().json().clone();
                                v["nativeMutated"] = json!(77);
                                *data.borrow_mut() = js_from_value::<UtilityInput>(v).unwrap();
                            }
                            h.calls
                                .borrow_mut()
                                .push(Call::Custom(name.into(), data.borrow().json().clone()));
                            if h.initial["appendDuringDispatch"] == true && name == "first" {
                                let host = h.clone();
                                let append: PublicDataCallback<String> = Rc::new(move |data| {
                                    host.calls.borrow_mut().push(Call::Custom(
                                        "appended".into(),
                                        data.borrow().json().clone(),
                                    ));
                                    host.event(json!(["custom", "appended"]));
                                    Ok(())
                                });
                                let stream = h.stream.borrow().upgrade().unwrap();
                                stream.message_handlers.borrow()["books"]
                                    .borrow_mut()
                                    .push(append);
                            }
                            h.event(json!(["custom", name]));
                            Ok(())
                        });
                        callbacks.push(cb)
                    }
                    stream
                        .message_handlers
                        .borrow_mut()
                        .insert(key.into(), Rc::new(RefCell::new(callbacks)));
                }
                Ok(())
            }
            "message" => stream.on_message(arg.as_str().unwrap()),
            "ticker" => {
                stream.handle_ticker_events(&js_from_value::<UtilityInput>(arg.clone()).unwrap())
            }
            "funding" => stream
                .handle_funding_rate_events(&js_from_value::<UtilityInput>(arg.clone()).unwrap()),
            "candle" => {
                let h = host.clone();
                stream
                    .listen_ohlcv(
                        serde_json::from_value::<OHLCVOptions>(arg.clone()).unwrap(),
                        Rc::new(move |c| {
                            h.call("candle-callback")?;
                            h.calls.borrow_mut().push(Call::Candle(c.borrow().clone()));
                            h.retained_candles.borrow_mut().push(c.clone());
                            if h.initial["mutateCandle"] == true {
                                c.borrow_mut().close += 1.0
                            }
                            if h.initial["reenterCandle"]==true&&!h.reentered.get(){h.reentered.set(true);h.stream.borrow().upgrade().unwrap().on_message(r#"{"arg":{"channel":"candle1m","instId":"BTC-USDT"},"data":[[1710000000000,"100","110","90","106","7",null,"8"]]}"#).map_err(|e|e.to_string())?;}
                            Ok(())
                        }),
                    )
                    .map(|f| cleanups.borrow_mut().push(f))
            }
            "book" => {
                let h = host.clone();
                stream
                    .listen_order_book(
                        arg.as_str().unwrap().into(),
                        Rc::new(move |b| {
                            h.call("book-callback")?;
                            h.calls.borrow_mut().push(Call::Book(b.borrow().clone()));
                            h.retained_books.borrow_mut().push(b.clone());
                            if h.initial["mutateBook"] == true
                                && let Some(l) = b.borrow_mut().bids.first_mut()
                            {
                                l.amount = 77.0;
                            }
                            if h.initial["cleanupOnBook"] == true {
                                let cleanup = h.cleanups.borrow()[0].clone();
                                cleanup().map_err(|e| e.to_string())?;
                            }
                            Ok(())
                        }),
                    )
                    .map(|f| cleanups.borrow_mut().push(f))
            }
            "cleanup" => {
                let cleanup = cleanups.borrow()[arg.as_u64().unwrap() as usize].clone();
                cleanup()
            }
            "close" => stream.on_close(Some(arg)),
            "dispose" => stream.dispose(),
            "connect" => stream.connect_and_subscribe(),
            "open" => stream.on_open(),
            "fire" => {
                let cb = host
                    .timers
                    .borrow_mut()
                    .remove(&(arg.as_u64().unwrap() as u32));
                if let Some(cb) = cb {
                    cb().map_err(PublicDataError::Host)
                } else {
                    Ok(())
                }
            }
            "opts-symbol" => Ok(()),
            "loaded" => {
                let mut loaded = Store::loaded(&host.store).unwrap().clone();
                loaded.markets = arg.as_bool().unwrap();
                Store::update(
                    &host.store,
                    StoreUpdate {
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
                Ok(())
            }
            "connected" => {
                host.connected.set(arg.as_bool().unwrap());
                Ok(())
            }
            "disposed" => {
                host.disposed.set(arg.as_bool().unwrap());
                Ok(())
            }
            s => panic!("unknown action{s}"),
        };
        results.push(result);
        snapshots.push(Snapshot {
            handlers: stream
                .message_handlers
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), v.borrow().len()))
                .collect(),
            topics: stream
                .core
                .topics
                .borrow()
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.iter()
                            .map(|t| {
                                (
                                    t.fields["channel"].as_str().unwrap().into(),
                                    t.fields["instId"].as_str().unwrap().into(),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
            timeout: *stream.core.connection_failure_notice_timeout.borrow(),
            notified: stream.core.connection_failure_notified.get(),
            disposed: host.disposed.get(),
            connected: host.connected.get(),
            ticker: Store::tickers(&host.store).unwrap()[0].clone(),
            loaded: Store::loaded(&host.store).unwrap().markets,
        });
    }
    Run {
        host,
        snapshots,
        results,
    }
}
fn native_assert(c: &Value, r: &Run) {
    let expected = c["snapshots"].as_array().unwrap();
    assert_eq!(r.snapshots.len(), expected.len());
    for (idx, (n, e)) in r.snapshots.iter().zip(expected).enumerate() {
        if c["group"] == "options-alias" && idx >= 3 {
            assert_eq!(n.handlers.len(), 4);
            assert_eq!(n.handlers[2], ("candle1m.BTC-USDT".into(), 1));
            assert_eq!(n.topics.len(), 2);
            assert_eq!(e["handlers"].as_array().unwrap().len(), 2);
            assert!(e["topics"].as_array().unwrap().is_empty());
            assert_eq!(
                n.topics,
                vec![
                    (
                        "candle1m.BTC-USDT".into(),
                        vec![("candle1m".into(), "BTC-USDT".into())]
                    ),
                    (
                        "books5.BTC-USDT".into(),
                        vec![("books5".into(), "BTC-USDT".into())]
                    )
                ]
            );
        } else {
            assert_eq!(
                n.handlers,
                serde_json::from_value::<Vec<(String, usize)>>(e["handlers"].clone()).unwrap()
            );
            let topics = e["topics"].as_array().unwrap();
            assert_eq!(n.topics.len(), topics.len());
            for ((key, rows), e) in n.topics.iter().zip(topics) {
                assert_eq!(key, e[0].as_str().unwrap());
                let erows = e[1].as_array().unwrap();
                assert_eq!(rows.len(), erows.len());
                for ((channel, id), e) in rows.iter().zip(erows) {
                    assert_eq!(channel, e["channel"].as_str().unwrap());
                    assert_eq!(id, e["instId"].as_str().unwrap());
                }
            }
        }
        assert_eq!(n.timeout, e["timeout"].as_u64().map(|n| n as u32));
        assert_eq!(n.notified, e["notified"].as_bool().unwrap());
        assert_eq!(n.disposed, e["disposed"].as_bool().unwrap());
        assert_eq!(n.connected, e["connected"].as_bool().unwrap());
        assert_eq!(n.loaded, e["loaded"].as_bool().unwrap());
        if c["group"] == "default-reentry" && idx == 1 {
            let mut expected = e["ticker"].clone();
            if c["actions"][0][1] == "funding-rate" {
                number(0.0002, &expected["fundingRate"]);
                assert_eq!(n.ticker.funding_rate.to_bits(), 0.0001_f64.to_bits());
                expected["fundingRate"] = json!({"number":format!("{:016x}",0.0001_f64.to_bits())});
            } else {
                number(88.0, &expected["bid"]);
                assert_eq!(n.ticker.bid, 99.0);
                expected["bid"] = json!({"number":format!("{:016x}",99.0_f64.to_bits())});
            }
            ticker(&n.ticker, &expected);
        } else {
            ticker(&n.ticker, &e["ticker"]);
        }
    }
    for (n, e) in r.results.iter().zip(c["results"].as_array().unwrap()) {
        match (n, e) {
            (Ok(()), Value::Null) => {}
            (Err(PublicDataError::Host(msg)), e) => {
                assert_eq!(e["name"], "Error");
                assert_eq!(msg, e["message"].as_str().unwrap());
            }
            (Err(PublicDataError::Coercion(err)), e) => {
                assert_eq!(err.name, e["name"]);
                assert_eq!(err.message, e["message"]);
            }
            (Err(PublicDataError::Shape(msg)), e) => {
                assert_eq!(e["name"], "TypeError");
                assert_eq!(*msg, e["message"]);
            }
            (n, e) => panic!("native error mismatch {n:?} expected{e}"),
        }
    }
    let calls = r.host.calls.borrow();
    let expected = c["calls"].as_array().unwrap();
    if c["initial"]["boundary"] == "surrogate" || c["initial"]["boundary"] == "depth" {
        assert!(calls.is_empty(), "declared native parse refusal");
        assert_eq!(
            expected.len(),
            2,
            "two source broadcast callbacks at parser boundary"
        );
        for e in expected {
            if c["initial"]["boundary"] == "surrogate" {
                assert_eq!(e["units"], json!([0xd800]))
            } else {
                assert_eq!(e["depth"], 130);
                assert_eq!(e["leaf"], 42)
            }
        }
    } else {
        assert_eq!(calls.len(), expected.len(), "native callback count");
        for (n, e) in calls.iter().zip(expected) {
            match n {
                Call::Custom(kind, data) => {
                    assert_eq!(kind, e["kind"].as_str().unwrap());
                    plain(data, &e["payload"])
                }
                Call::Candle(n) => {
                    assert_eq!(e["kind"], "candle");
                    candle(n, &e["payload"])
                }
                Call::Book(n) => {
                    assert_eq!(e["kind"], "book");
                    book(n, &e["payload"])
                }
            }
        }
    }
    let expected_timers: Vec<_> = c["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s[0] == "timer")
        .collect();
    let timers = r.host.timer_events.borrow();
    assert_eq!(timers.len(), expected_timers.len());
    for ((id, delay), e) in timers.iter().zip(expected_timers) {
        assert_eq!(*id, e[1].as_u64().unwrap() as u32);
        assert_eq!(
            delay.to_bits(),
            e[2].as_f64().unwrap().to_bits(),
            "native timer delay"
        );
    }
    let retained_books = r.host.retained_books.borrow();
    let retained_candles = r.host.retained_candles.borrow();
    let expected = c["retained"].as_array().unwrap();
    assert_eq!(
        retained_books.len() + retained_candles.len(),
        expected.len()
    );
    for (n, e) in retained_books.iter().zip(expected) {
        book(&n.borrow(), e)
    }
    for (n, e) in retained_candles.iter().zip(expected) {
        candle(&n.borrow(), e)
    }
}
fn wires(c: &Value, r: &Run) {
    let sent = r.host.sent.borrow();
    let expected = c["sent"].as_array().unwrap();
    if c["group"] == "options-alias" {
        assert!(expected.is_empty());
        assert_eq!(sent.len(), 2);
        for (frame, channel) in sent.iter().zip(["candle1m", "books5"]) {
            let v: Value = serde_json::from_str(frame).unwrap();
            assert_eq!(v["op"], "subscribe");
            assert_eq!(v["args"], json!([{"channel":channel,"instId":"BTC-USDT"}]));
        }
        let ntrace: Vec<_> = r
            .host
            .trace
            .borrow()
            .iter()
            .filter(|s| s[0] != "send")
            .cloned()
            .collect();
        assert_eq!(
            ntrace,
            c["trace"].as_array().unwrap().clone(),
            "declared options boundary retains shared effects"
        );
        return;
    }
    assert_eq!(sent.len(), expected.len());
    for (s, e) in sent.iter().zip(expected) {
        let decoded: Value = if s == "ping" {
            json!({"ping":true})
        } else {
            serde_json::from_str(s).unwrap()
        };
        assert_eq!(&decoded, e, "independent wire frame decoder");
    }
    let mut trace = c["trace"].as_array().unwrap().clone();
    if c["initial"]["boundary"] == "surrogate" || c["initial"]["boundary"] == "depth" {
        trace.retain(|s| s[0] != "custom");
    }
    assert_eq!(*r.host.trace.borrow(), trace, "effect ordering");
}
fn group(groups: &[&str], floor: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| groups.contains(&c["group"].as_str().unwrap()))
        .collect();
    assert_eq!(cases.len(), floor, "independent group floor");
    let runs: Vec<_> = cases.iter().map(|c| run(c)).collect();
    for (c, r) in cases.iter().zip(&runs) {
        native_assert(c, r);
    }
    for (c, r) in cases.iter().zip(&runs) {
        wires(c, r)
    }
}
#[test]
fn native_constructor_close_capture() {
    group(&["constructor"], 3)
}
#[test]
fn native_dispose_timer_order() {
    group(&["dispose"], 6)
}
#[test]
fn native_dispatch_and_parser_boundaries() {
    group(&["message", "dispatch-live", "default-reentry"], 21)
}
#[test]
fn native_ticker_patch() {
    group(&["ticker", "ticker-shape"], 9)
}
#[test]
fn native_funding_patch() {
    group(&["funding"], 4)
}
#[test]
fn native_candle_listener() {
    group(
        &[
            "candle",
            "candle-reentry",
            "candle-string-book",
            "candle-coercion-order",
        ],
        18,
    )
}
#[test]
fn native_book_listener() {
    group(&["book", "book-reentry", "book-boundary", "book-shape"], 22)
}
#[test]
fn native_listener_retry_cleanup() {
    group(&["retry", "options-alias"], 9)
}
#[test]
fn native_public_facade_composition() {
    group(&["facade"], 1)
}
#[test]
fn native_callback_ownership() {
    // Independent retained native snapshots stay owned while source aliases are live.
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["group"] == "book"
                && c["actions"].as_array().unwrap().last().unwrap()[0] == "cleanup"
                && c["retained"].as_array().unwrap().len() == 2
        })
        .unwrap();
    let r = run(c);
    native_assert(c, &r);
    let calls = r.host.calls.borrow();
    let Call::Book(first) = &calls[0] else {
        panic!()
    };
    assert_eq!(first.bids.len(), 2);
    assert_eq!(first.bids[0].price, 99.0);
    assert_eq!(first.bids[0].amount, 2.0);
    assert_eq!(first.bids[0].total, 2.0);
    for refbook in c["retained"].as_array().unwrap() {
        assert!(
            refbook["bids"].as_array().unwrap().is_empty(),
            "source retained alias emptied at cleanup"
        );
        assert!(refbook["asks"].as_array().unwrap().is_empty());
    }
    let alias = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["group"] == "book"
                && c["actions"].as_array().unwrap().get(2).is_some_and(|a| {
                    a[1].as_str()
                        .is_some_and(|s| s.contains("97") && s.contains("books5"))
                })
                && c["actions"].as_array().unwrap().len() == 3
        })
        .unwrap();
    let ar = run(alias);
    native_assert(alias, &ar);
    let calls = ar.host.calls.borrow();
    let Call::Book(old) = &calls[0] else { panic!() };
    assert_eq!(old.bids[0].price, 99.0);
    number(97.0, &alias["retainedArrays"][0][0]["price"]);
    assert_eq!(
        ar.host.retained_books.borrow()[0].borrow().bids[0].price,
        97.0,
        "book root alias retained while Vec clone owned"
    );
    assert_eq!(
        corpus()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["group"] == "facade")
            .unwrap()["defaultCallbackLast"],
        999,
        "source retained default callback keeps facade"
    );
    let host = Host::new(&json!({}));
    let stream = BlofinPublicWebsocket::new(host.clone()).unwrap();
    let retained = stream.message_handlers.borrow()["tickers"].borrow()[0].clone();
    drop(stream);
    retained(Rc::new(RefCell::new(
        js_from_value::<UtilityInput>(
            json!({"data":[{"instId":"BTC-USDT","last":"999","open24h":"1"}]}),
        )
        .unwrap(),
    )))
    .unwrap();
    assert_eq!(
        Store::tickers(&host.store).unwrap()[0].last,
        100.0,
        "weak default callback releases facade"
    );
}

#[test]
fn native_listener_has_no_facade_self_cycle() {
    let host = Host::new(&json!({}));
    let stream = BlofinPublicWebsocket::new(host.clone()).unwrap();
    let weak = Rc::downgrade(&stream);
    let cleanup = stream
        .listen_order_book("BTC/USDT".into(), Rc::new(|_| Ok(())))
        .unwrap();
    stream.dispose().unwrap();
    drop(cleanup);
    drop(stream);
    assert!(
        weak.upgrade().is_none(),
        "connected listener must not own its facade"
    );
}
