use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_bingx::{
    data_reads::{BingxDataReads, ReadTransport},
    public_stream::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_stream_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "BINGX-PUBLIC source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("fixture must resolve in 1000 local polls")
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(v) => {
            let mut keys = v.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&v[k])))
                    .collect(),
            )
        }
        Value::Array(v) => Value::Array(v.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}
fn ticker(v: &Ticker) -> Value {
    json!({"id":v.id,"symbol":v.symbol,"bid":number(v.bid),"ask":number(v.ask),"last":number(v.last),"mark":number(v.mark),"index":v.index.map(number).unwrap_or(Value::Null),"percentage":number(v.percentage),"fundingRate":number(v.funding_rate),"fundingTime":number(v.funding_time),"volume":number(v.volume),"quoteVolume":number(v.quote_volume),"openInterest":number(v.open_interest)})
}
struct Host {
    input: Value,
    disposed: Cell<bool>,
    calls: RefCell<Vec<Value>>,
    exceptions: RefCell<Vec<String>>,
    listeners: RefCell<Vec<SocketListener>>,
}
impl Host {
    fn call(&self, v: Value) {
        self.calls.borrow_mut().push(v);
    }
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1700000000000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("no timers")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no tasks")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        e: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exceptions.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker")
    }
}
impl PublicSocketHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn nanoid(&self) -> String {
        self.input["id"].as_str().unwrap().into()
    }
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn iso_time(&self) -> Result<String, ExchangeError> {
        Ok("2023-11-14T22:13:20.000Z".into())
    }
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError> {
        self.call(json!(["create", url]));
        Ok(())
    }
    fn add_listener(&self, l: SocketListener) -> Result<(), ExchangeError> {
        self.call(json!([
            "listener",
            match &l {
                SocketListener::Open(_) => "open",
                SocketListener::Message(_) => "message",
                SocketListener::Close => "close",
            }
        ]));
        self.listeners.borrow_mut().push(l);
        Ok(())
    }
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError> {
        self.call(json!(["send", frame]));
        if self.input["failSend"] == frame {
            Err(ExchangeError::new("send cause"))
        } else {
            Ok(())
        }
    }
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError> {
        self.call(json!(["pingAt", at]));
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.call(json!(["track"]));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), ExchangeError> {
        self.call(json!(["pong"]));
        Ok(())
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.call(json!(["stats", topic]));
        Ok(())
    }
    fn diagnostic_receive(
        &self,
        raw: &str,
        allow: &dyn Fn() -> bool,
        format: &dyn Fn() -> Result<Option<String>, ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.call(json!(["diagnostic", raw]));
        if self.input["diagnostics"] == true && allow() {
            self.call(json!(["formatted", format()?]));
        }
        Ok(())
    }
    fn read_blob(&self, _: u64) -> ExchangeFuture<'_, String> {
        Box::pin(async move {
            if self.input["disposeAfterBlob"] == true {
                self.disposed.set(true);
            }
            Ok(self.input["raw"].as_str().unwrap().into())
        })
    }
    fn report_exception(&self, e: &ExchangeError) {
        self.exceptions.borrow_mut().push(e.message.clone());
    }
}
struct Transport;
impl ReadTransport for Transport {
    fn send(&self, _: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        panic!("public stream never uses REST")
    }
}
fn fixture(
    input: &Value,
) -> (
    Rc<BingxPublicStream>,
    Rc<DefaultStore>,
    Rc<Host>,
    Rc<RefCell<Vec<Value>>>,
) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![
                serde_json::from_value(input["market"].clone()).unwrap(),
            ]),
            tickers: Some(vec![
                serde_json::from_value(input["ticker"].clone()).unwrap(),
            ]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host {
        input: input.clone(),
        disposed: Cell::new(input["disposed"] == true),
        calls: RefCell::default(),
        exceptions: RefCell::default(),
        listeners: RefCell::default(),
    });
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret","testnet":input["testnet"]}))
            .unwrap(),
        store.clone(),
        host.clone(),
        ExchangeSettings::default(),
    ));
    let venue = BingxPublicStream::new(
        Rc::new(BingxDataReads::new(base, Rc::new(Transport))),
        host.clone(),
    );
    *venue.topics.borrow_mut() = input["topics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v[0].as_str().unwrap().into(), v[1].as_f64().unwrap()))
        .collect();
    *venue.message_handlers.borrow_mut() = input["handlers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v.as_str().unwrap().into(),
                Rc::new(RefCell::new(Vec::new())),
            )
        })
        .collect();
    let callbacks = Rc::new(RefCell::new(Vec::new()));
    if let Some(topic) = input["callbackTopic"].as_str() {
        let topic = topic.to_owned();
        let list: HandlerList = Rc::default();
        let log = callbacks.clone();
        let second: MessageCallback = Rc::new(move |rows| {
            log.borrow_mut()
                .push(json!({"name":"second","rows":observe(&json!(rows))}));
            Ok(())
        });
        let weak_list = Rc::downgrade(&list);
        let weak_venue = Rc::downgrade(&venue);
        let log = callbacks.clone();
        let mode = input["callbackMode"].as_str().unwrap().to_owned();
        let key = topic.clone();
        list.borrow_mut().push(Rc::new(move |rows| {
            log.borrow_mut()
                .push(json!({"name":"first","rows":observe(&json!(rows))}));
            if mode == "append" {
                weak_list
                    .upgrade()
                    .unwrap()
                    .borrow_mut()
                    .push(second.clone());
            }
            if mode == "replace" {
                let owner = weak_venue.upgrade().unwrap();
                owner
                    .message_handlers
                    .borrow_mut()
                    .iter_mut()
                    .find(|(k, _)| k == &key)
                    .unwrap()
                    .1 = Rc::new(RefCell::new(vec![second.clone()]));
            }
            Ok(())
        }));
        venue
            .message_handlers
            .borrow_mut()
            .retain(|(k, _)| k != &topic);
        venue.message_handlers.borrow_mut().push((topic, list));
    }
    (venue, store, host, callbacks)
}
#[test]
fn public_stream_matches_actual_typescript() {
    let names = [
        "getDiagnosticSymbols",
        "formatMarketContractsDiagnostic",
        "connectAndSubscribe",
        "onOpen",
        "ping",
        "subscribe",
        "onMessage",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-PUBLIC source census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 38, "BINGX-PUBLIC case floor");
    let inputs = json!(
        cases
            .iter()
            .map(|c| json!([c["method"], c["variant"], c["input"]]))
            .collect::<Vec<_>>()
    );
    let encoded = serde_json::to_string(&sorted(&inputs)).unwrap();
    let fingerprint = encoded.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(
        fingerprint, 0x217dc1f0d03a4bb0,
        "BINGX-PUBLIC independent input fingerprint"
    );
    let mut counts = [0; 7];
    for (index, c) in cases.iter().enumerate() {
        let method = c["method"].as_str().unwrap();
        let family = names.iter().position(|n| *n == method).unwrap();
        let variant = c["variant"].as_u64().unwrap();
        assert_eq!(
            family,
            if index < 28 { index / 4 } else { 6 },
            "BINGX-PUBLIC family order"
        );
        assert_eq!(
            variant,
            if index < 28 { index % 4 } else { index - 24 } as u64,
            "BINGX-PUBLIC variant order"
        );
        counts[family] += 1;
        let (venue, store, host, callbacks) = fixture(&c["input"]);
        let result: Result<Value, ExchangeError> = match method {
            "getDiagnosticSymbols" => Ok(json!(venue.get_diagnostic_symbols())),
            "formatMarketContractsDiagnostic" => venue
                .format_market_contracts_diagnostic(
                    &c["input"]["json"],
                    &serde_json::from_value::<Vec<String>>(c["input"]["symbols"].clone()).unwrap(),
                )
                .map(Value::String),
            "connectAndSubscribe" => venue
                .connect_and_subscribe()
                .map(|_| json!({"undefined":true})),
            "onOpen" => venue.on_open().map(|_| json!({"undefined":true})),
            "ping" => venue.ping().map(|_| json!({"undefined":true})),
            "subscribe" => venue.subscribe().map(|_| json!({"undefined":true})),
            "onMessage" => ready(venue.on_message(if c["input"]["blob"] == true {
                PublicFrame::Blob(7)
            } else {
                PublicFrame::Text(c["input"]["raw"].as_str().unwrap().into())
            }))
            .map(|_| json!({"undefined":true})),
            _ => unreachable!(),
        };
        let output = match result {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        assert_eq!(
            output, c["output"],
            "BINGX-PUBLIC native {method} variant {variant}"
        );
        assert_eq!(
            observe(&json!(*host.calls.borrow())),
            c["calls"],
            "BINGX-PUBLIC operation order {method} variant {variant}"
        );
        assert_eq!(
            json!(*callbacks.borrow()),
            c["callbacks"],
            "BINGX-PUBLIC callbacks {variant}"
        );
        assert_eq!(
            json!(store.tickers().iter().map(ticker).collect::<Vec<_>>()),
            c["tickers"],
            "BINGX-PUBLIC native ticker bits {variant}"
        );
        assert_eq!(
            *host.exceptions.borrow(),
            if method == "ping" && variant == 3 {
                vec!["send cause".to_string()]
            } else {
                vec![]
            },
            "BINGX-PUBLIC CHOSEN visible ping failure"
        );
    }
    assert_eq!(counts, [4, 4, 4, 4, 4, 4, 14], "BINGX-PUBLIC family floor");
    println!("BINGX-PUBLIC source runtime {}", corpus()["runtime"]);
}
#[test]
fn positive_guard_preserves_number_and_error_identity() {
    let inputs = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.0,
        -1.0,
        1.0,
        0.1,
        f64::from_bits(1),
        f64::MAX,
        1e-200,
        1e200,
        13.5,
        -0.1,
        9007199254740992.0,
        f64::MIN_POSITIVE,
    ];
    let cases = corpus()["guards"].as_array().unwrap();
    assert_eq!(cases.len(), 16, "BINGX-GUARD case floor");
    for (i, (&input, c)) in inputs.iter().zip(cases).enumerate() {
        assert_eq!(
            number(input),
            c["input"],
            "BINGX-GUARD independent input {i}"
        );
        let calls = Cell::new(0);
        let out = parse_positive_finite_number::<()>(|| {
            calls.set(calls.get() + 1);
            Ok(input)
        })
        .unwrap()
        .map_or(json!({"undefined":true}), number);
        assert_eq!(out, c["output"], "BINGX-GUARD positive finite {i}");
        assert_eq!(calls.get(), 1, "BINGX-GUARD one Number operation");
    }
    let error = Rc::new("opaque");
    let calls = Cell::new(0);
    let out = parse_positive_finite_number(|| {
        calls.set(calls.get() + 1);
        Err(error.clone())
    });
    assert!(
        Rc::ptr_eq(&error, &out.unwrap_err()),
        "BINGX-GUARD opaque error identity"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(corpus()["guardError"], json!({"calls":1,"same":true}));
}
#[test]
fn public_stream_prices_utf8_and_iterable_boundaries() {
    let input = &corpus()["cases"][0]["input"];
    let (v, _, _, _) = fixture(input);
    let actual = v
        .format_market_contracts_diagnostic(&json!({"data":"🦀"}), &["BTC-USDT".into()])
        .unwrap();
    assert!(
        actual.contains("\"0\":\"🦀\""),
        "BINGX-PUBLIC CHOSEN scalar spread"
    );
    let source = corpus()["boundary"]["astral"]["output"]["ok"]
        .as_str()
        .unwrap();
    assert!(
        source.contains("\\ud83e") && source.contains("\\udd80"),
        "BINGX-PUBLIC source UTF16 spread"
    );
    assert_ne!(actual, source);
    let e = ready(v.on_message(PublicFrame::Text(
        "{\"dataType\":\"custom\",\"value\":\"\\ud800\"}".into(),
    )))
    .unwrap_err();
    assert!(
        e.message.contains("lone surrogate"),
        "BINGX-PUBLIC CHOSEN surrogate refusal {}",
        e.message
    );
    assert_eq!(
        corpus()["boundary"]["surrogate"]["output"]["ok"],
        json!({"undefined":true})
    );
    assert_eq!(
        corpus()["boundary"]["surrogate"]["callbacks"][0]["rows"][0]["value"],
        json!({"utf16":[55296]})
    );
    let e = ready(v.on_message(PublicFrame::Text(
        "{\"dataType\":\"market.contracts\",\"data\":{\"contracts\":\"abc\"}}".into(),
    )))
    .unwrap_err();
    assert_eq!(
        e.message, "contracts is not iterable",
        "BINGX-PUBLIC CHOSEN array boundary"
    );
    assert_eq!(
        corpus()["boundary"]["iterable"]["output"]["ok"],
        json!({"undefined":true})
    );
}
#[test]
fn registered_listeners_execute_native_bodies() {
    let (v, _, host, _) = fixture(&corpus()["cases"][0]["input"]);
    v.connect_and_subscribe().unwrap();
    let listeners = host.listeners.borrow();
    assert_eq!(listeners.len(), 3);
    match &listeners[0] {
        SocketListener::Open(f) => f().unwrap(),
        _ => panic!("open registration"),
    };
    match &listeners[1] {
        SocketListener::Message(f) => ready(f(PublicFrame::Text("Ping".into()))).unwrap(),
        _ => panic!("message registration"),
    };
    assert!(matches!(listeners[2], SocketListener::Close));
    assert_eq!(
        host.calls.borrow().last().unwrap(),
        &json!(["send", "Pong"])
    );
}
#[test]
fn weak_listener_ownership_reports_release() {
    let (v, _, host, _) = fixture(&corpus()["cases"][0]["input"]);
    v.connect_and_subscribe().unwrap();
    drop(v);
    let listeners = host.listeners.borrow();
    let error = match &listeners[0] {
        SocketListener::Open(f) => f().unwrap_err(),
        _ => panic!("open"),
    };
    assert_eq!(
        error.message, "BingX public stream owner released",
        "BINGX-PUBLIC CHOSEN weak owner"
    );
    let error = match &listeners[1] {
        SocketListener::Message(f) => ready(f(PublicFrame::Text("Ping".into()))).unwrap_err(),
        _ => panic!("message"),
    };
    assert_eq!(error.message, "BingX public stream owner released");
}
#[test]
fn ticker_write_failure_preserves_original_cause() {
    let (v, store, _, _) = fixture(&corpus()["cases"][0]["input"]);
    let cleanup = Store::subscribe(
        store.as_ref(),
        Box::new(|_, _| {
            let mut cause = ExchangeError::new("subscriber cause");
            cause.name = "ListenerError".into();
            cause.store_subscriber_ids = vec![71, 83];
            cause =
                cause.with_retained_response((json!({"data":{"msg":"subscriber denied"}})).into());
            Err(StoreSubscriberError::new("subscriber cause").with_cause(Rc::new(cause)))
        }),
    );
    let e = ready(
        v.on_message(PublicFrame::Text(
            corpus()["cases"][0]["input"]["raw"]
                .as_str()
                .unwrap()
                .into(),
        )),
    )
    .unwrap_err();
    assert_eq!(e.name, "ListenerError");
    assert_eq!(e.message, "subscriber cause");
    assert_eq!(&e.store_subscriber_ids[..2], &[71, 83]);
    assert_eq!(
        e.store_subscriber_ids.len(),
        3,
        "BINGX-PUBLIC subscriber outer boundary"
    );
    assert_eq!(
        e.response_json_view(),
        Some(json!({"data":{"msg":"subscriber denied"}}))
    );
    assert_eq!(store.tickers()[0].last, 13.5);
    assert_eq!(
        store.tickers()[0].mark,
        13.25,
        "BINGX-PUBLIC partial first write before rejection"
    );
    cleanup();
}
