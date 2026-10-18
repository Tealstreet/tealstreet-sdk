use super::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_venue_binance::exchange_classes::Method;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct Http {
    requests: Mutex<Vec<RestRequest>>,
    routes: Mutex<std::collections::BTreeMap<String, (u16, String)>>,
    held: AtomicBool,
}
impl Http {
    fn response(&self, path: &str, status: u16, data: Value) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), (status, data.to_string()));
    }
    fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.url.split('?').next() == Some(path))
            .count()
    }
}
impl RestAdapter for Http {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let path = request.url.split('?').next().unwrap().to_owned();
        self.requests.lock().unwrap().push(request);
        let (status, body) = self
            .routes
            .lock()
            .unwrap()
            .get(&path)
            .cloned()
            .unwrap_or_else(|| panic!("unscripted actual native request: {path}"));
        Box::pin(std::future::poll_fn(move |_| {
            if self.held.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
            Poll::Ready(Ok(RestResponse {
                status,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body.as_bytes().to_vec()),
            }))
        }))
    }
}

#[derive(Default)]
struct Wire {
    sent: RefCell<Vec<String>>,
    opened: Cell<bool>,
}
impl WsSocket for Wire {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        if !self.opened.replace(true) {
            Box::pin(async { Ok(WsEvent::Open) })
        } else {
            Box::pin(std::future::pending())
        }
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(text.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}

fn owner(http: Arc<Http>, runtime: Rc<Runtime>) -> Rc<ExchangeOwner> {
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime,
        http,
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"alias-key","secret":"alias-secret"})).unwrap(),
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        dependencies,
    )
    .unwrap()
}
fn observe(owner: &ExchangeOwner, event: &str) -> Rc<RefCell<Vec<Vec<String>>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let output = seen.clone();
    let error_event = event == "error";
    owner.base.on_args(
        event,
        Rc::new(move |_, args| {
            output.borrow_mut().push(
                args.iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        if error_event && index == 1 {
                            assert_eq!(arg.type_of(), "object");
                            return "metadata".into();
                        }
                        String::from_utf16(arg.as_str().expect("native string event").as_ref())
                            .unwrap()
                    })
                    .collect(),
            );
            Ok(())
        }),
    );
    seen
}
fn invoke(
    owner: &Rc<ExchangeOwner>,
    runtime: &Runtime,
    method: Method,
    args: Vec<EventValue>,
) -> EventValue {
    run(runtime, owner.invoke(method, args)).unwrap()
}
fn native_number(value: &EventValue, key: &str) -> f64 {
    match value.read(key).unwrap() {
        EventValue::Number(number) => number,
        other => panic!("native {key} must be Number: {other:?}"),
    }
}
fn native_text(value: &EventValue, key: &str) -> String {
    let value = value.read(key).unwrap();
    String::from_utf16(value.as_str().expect("native String field").as_ref()).unwrap()
}
fn rows(value: EventValue) -> Vec<EventValue> {
    match value {
        EventValue::Array(rows) => array_values(&rows),
        other => panic!("expected native rows: {other:?}"),
    }
}

#[test]
fn native_history_aliases_keep_pending_http_pages_and_error_effects() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response(
        "/fapi/v1/userTrades",
        200,
        json!([{"id":17,"orderId":23,"symbol":"BTCUSDT",
        "time":1700000000123_u64,"price":"12.5","qty":"-0","side":"BUY",
        "commission":"0.125","commissionAsset":"USDT","realizedPnl":"-2.5","maker":true}]),
    );
    http.response(
        "/fapi/v1/income",
        200,
        json!([{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE",
        "income":"-0","asset":"USDT","time":101,"tranId":31,"tradeId":"41"}]),
    );
    let owner = owner(http.clone(), runtime.clone());
    let logs = observe(&owner, "log");
    let errors = observe(&owner, "error");
    http.held.store(true, Ordering::SeqCst);
    let mut pending = Box::pin(owner.invoke(
        Method::HistoricalFills,
        vec![record([
            ("symbol", string("BTCUSDT")),
            ("limit", number(7.0)),
        ])],
    ));
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    runtime.pump();
    assert_eq!(http.count("/fapi/v1/userTrades"), 1);
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "HistoryFuture stays held"
    );
    http.held.store(false, Ordering::SeqCst);
    let fills = rows(run(&runtime, pending).unwrap());
    assert_eq!(fills.len(), 1, "HistoryGet publishes result");
    assert_eq!(native_text(&fills[0], "id"), "17");
    assert_eq!(
        native_number(&fills[0], "qty").to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        native_number(&fills[0], "price").to_bits(),
        12.5_f64.to_bits()
    );
    runtime.monotonic_clock.set(Some(23000.0));
    let income = rows(invoke(
        &owner,
        &runtime,
        Method::IncomeRange,
        vec![number(100.0), number(102.0), string("FUNDING_FEE")],
    ));
    assert_eq!(
        income.len(),
        1,
        "PageResolver/PageCall reach native paginator"
    );
    assert_eq!(native_text(&income[0], "tranId"), "31");
    assert_eq!(
        native_number(&income[0], "income").to_bits(),
        (-0.0_f64).to_bits()
    );
    let requests = http.requests.lock().unwrap();
    let query = &requests
        .iter()
        .find(|request| request.url.split('?').next() == Some("/fapi/v1/income"))
        .unwrap()
        .url;
    for part in [
        "startTime=100",
        "endTime=102",
        "incomeType=FUNDING_FEE",
        "signature=",
    ] {
        assert!(query.contains(part), "native page request {part}: {query}");
    }
    drop(requests);
    runtime.monotonic_clock.set(Some(24000.0));
    runtime.now.set(2000.0);
    runtime.sleep_clock.set(2000.0);
    http.response("/fapi/v1/userTrades", 200, json!([null]));
    assert!(
        rows(invoke(
            &owner,
            &runtime,
            Method::HistoricalFills,
            vec![record([("symbol", string("BTCUSDT"))])]
        ))
        .is_empty()
    );
    assert!(
        logs.borrow()
            .iter()
            .any(|row| row[0].contains("Error fetching executions:") && row[1] == "error"),
        "HistoryLog effect"
    );
    runtime.monotonic_clock.set(Some(25000.0));
    runtime.now.set(4000.0);
    runtime.sleep_clock.set(4000.0);
    http.response("/fapi/v1/allOrders", 200, json!([null]));
    assert!(
        rows(invoke(
            &owner,
            &runtime,
            Method::HistoricalOrders,
            vec![record([("symbol", string("BTCUSDT"))])]
        ))
        .is_empty()
    );
    assert_eq!(
        errors.borrow().len(),
        1,
        "HistoryEmit forwards mapping refusal once"
    );
    assert!(errors.borrow()[0][0].contains("null"));
    owner.dispose().unwrap();
}

#[test]
fn native_income_range_retains_source_number_profiles() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/lifecycle-alias-source.mjs"
        ))
        .output()
        .expect("maintained TypeScript income source");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let profiles: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(profiles.len(), 4);
    for profile in profiles {
        let runtime = Rc::new(Runtime::default());
        let http = Arc::new(Http::default());
        http.response(
            "/fapi/v1/income",
            200,
            json!([{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE",
            "income":profile["income"],"asset":"USDT","time":101,"tranId":31,"tradeId":"41"}]),
        );
        let owner = owner(http.clone(), runtime.clone());
        let result = rows(invoke(
            &owner,
            &runtime,
            Method::IncomeRange,
            vec![number(100.0), number(102.0), string("FUNDING_FEE")],
        ));
        assert_eq!(result.len(), 1, "source retains {}", profile["income"]);
        let income = native_number(&result[0], "income");
        let bits = if income.is_nan() {
            "NaN".into()
        } else {
            format!("{:016x}", income.to_bits())
        };
        assert_eq!(
            bits,
            profile["incomeBits"].as_str().unwrap(),
            "source income {}",
            profile["income"]
        );
        assert_eq!(native_text(&result[0], "tranId"), profile["transaction"]);
        let requests = http.requests.lock().unwrap();
        let expected = profile["requests"].as_array().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, expected) in requests.iter().zip(expected) {
            assert_eq!(request.url.split('?').next().unwrap(), expected["path"]);
            for key in ["startTime", "endTime", "limit", "incomeType"] {
                let value = expected[key]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| expected[key].to_string());
                assert!(
                    request.url.contains(&format!("{key}={value}")),
                    "source page {key}"
                );
            }
        }
        drop(requests);
        owner.dispose().unwrap();
    }
}

#[test]
fn native_candle_aliases_seed_stream_and_cleanup_without_late_publication() {
    use safe_cex_transport::socket::SocketFrame;
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response(
        "/fapi/v1/klines",
        200,
        json!([[1000, "1.25", "3.5", "-0", "2.75", "9.125"]]),
    );
    let wire = Rc::new(Wire::default());
    *runtime.dial_socket.borrow_mut() = Some(wire.clone());
    let owner = owner(http.clone(), runtime.clone());
    let logs = observe(&owner, "log");
    owner.public_socket().connect_and_subscribe().unwrap();
    runtime.pump();
    runtime.pump();
    let opts = || record([("symbol", string("BTCUSDT")), ("interval", string("1m"))]);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let output = seen.clone();
    http.held.store(true, Ordering::SeqCst);
    let cleanup = owner
        .listen_ohlcv(
            opts(),
            Rc::new(move |candle| {
                output.borrow_mut().push(candle);
                Ok(())
            }),
        )
        .unwrap();
    runtime.pump();
    assert_eq!(http.count("/fapi/v1/klines"), 1);
    assert!(
        http.requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .url
            .contains("limit=1"),
        "CandleFetch uses source seed limit"
    );
    assert!(seen.borrow().is_empty());
    http.held.store(false, Ordering::SeqCst);
    runtime.pump();
    runtime.pump();
    assert_eq!(
        seen.borrow().len(),
        1,
        "SeedFuture/CandleFetch settle latest candle"
    );
    assert_eq!(
        native_number(&seen.borrow()[0], "close").to_bits(),
        2.75_f64.to_bits()
    );
    let frame = || {
        SocketFrame::Text(
            json!({"e":"kline","s":"BTCUSDT","k":{"s":"BTCUSDT","i":"1m","t":2000,
        "o":"4","h":"6","l":"3","c":"5.5","v":"12"}})
            .to_string(),
        )
    };
    owner
        .public_socket()
        .market_websocket
        .get()
        .unwrap()
        .base()
        .on_message(&frame())
        .unwrap();
    assert_eq!(
        seen.borrow().len(),
        2,
        "CandleListen/CandleCallback retain live path"
    );
    assert_eq!(native_number(&seen.borrow()[1], "close"), 5.5);
    cleanup().unwrap();
    owner
        .public_socket()
        .market_websocket
        .get()
        .unwrap()
        .base()
        .on_message(&frame())
        .unwrap();
    assert_eq!(
        seen.borrow().len(),
        2,
        "CandleCleanup detaches native listener"
    );
    http.held.store(true, Ordering::SeqCst);
    let output = seen.clone();
    let cleanup = owner
        .listen_ohlcv(
            opts(),
            Rc::new(move |candle| {
                output.borrow_mut().push(candle);
                Ok(())
            }),
        )
        .unwrap();
    runtime.pump();
    cleanup().unwrap();
    http.held.store(false, Ordering::SeqCst);
    runtime.pump();
    runtime.pump();
    assert_eq!(
        seen.borrow().len(),
        2,
        "cleanup suppresses already-running seed result"
    );
    http.response("/fapi/v1/klines", 200, json!([null]));
    let cleanup = owner
        .listen_ohlcv(opts(), Rc::new(|_| panic!("refused seed cannot publish")))
        .unwrap();
    runtime.pump();
    runtime.pump();
    assert!(
        logs.borrow()
            .iter()
            .any(|row| row[0].contains("Unable to seed Binance live candle:") && row[1] == "debug")
    );
    cleanup().unwrap();
    owner.dispose().unwrap();
}

#[test]
fn native_lifecycle_aliases_publish_store_then_resolve_tick_again_at_timer_fire() {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1000.0);
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/symbolConfig", 200, json!([]));
    http.response("/fapi/v3/positionRisk", 200, json!([]));
    http.response(
        "/fapi/v3/account",
        200,
        json!({"totalWalletBalance":"17.25","availableBalance":"12.5",
        "totalUnrealizedProfit":"-0","totalInitialMargin":"4.75","assets":[{"asset":"USDT","walletBalance":"17.25","availableBalance":"12.5","unrealizedProfit":"-0","initialMargin":"4.75"}],"positions":[]}),
    );
    let owner = owner(http.clone(), runtime.clone());
    invoke(&owner, &runtime, Method::Tick, vec![]);
    assert_eq!(
        owner.base.store.balance().total.to_bits(),
        17.25_f64.to_bits(),
        "LifecycleCall Store update"
    );
    assert!(
        owner.base.store.loaded().balance,
        "LifecycleAsyncCall settles account"
    );
    assert_eq!(http.count("/fapi/v3/account"), 1);
    assert_eq!(http.count("/fapi/v3/positionRisk"), 1);
    runtime.pump();
    assert!(
        runtime.sleeps.borrow().contains(&3000),
        "TickTimer owns source delay"
    );
    runtime.monotonic_clock.set(Some(26100.0));
    runtime.now.set(4100.0);
    runtime.sleep_clock.set(4100.0);
    for _ in 0..12 {
        runtime.pump();
    }
    assert_eq!(
        http.count("/fapi/v3/account"),
        2,
        "TickResolver resolves current tick after delay"
    );
    assert_eq!(
        http.count("/fapi/v3/positionRisk"),
        1,
        "risk interval remains independent"
    );
    assert_eq!(
        owner.times.account.get(),
        4100.0,
        "LifecycleFuture settles second update"
    );
    owner.dispose().unwrap();
}

#[test]
fn native_api_aliases_sign_and_preserve_success_and_rejection() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response(
        "/fapi/v1/income",
        200,
        json!([{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE",
        "income":"1.25","asset":"USDT","time":101,"tranId":17,"tradeId":"41"}]),
    );
    let owner = owner(http.clone(), runtime.clone());
    let errors = observe(&owner, "error");
    let send = || {
        owner.invoke(
            Method::IncomePage,
            vec![record([("symbol", string("BTCUSDT"))])],
        )
    };
    let response = run(&runtime, send()).unwrap();
    assert_eq!(
        native_number(&rows(response)[0], "tranId"),
        17.0,
        "ApiFuture/ResponseInterceptor result"
    );
    let request = http.requests.lock().unwrap()[0].clone();
    let (path, query) = request.url.split_once('?').unwrap();
    assert_eq!(path, "/fapi/v1/income");
    let (unsigned, signature) = query.rsplit_once("&signature=").unwrap();
    assert_eq!(
        signature,
        safe_cex_crypto::hmac_sha256(
            b"alias-secret",
            unsigned.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        ),
        "RequestInterceptor native HMAC"
    );
    assert!(unsigned.contains("symbol=BTCUSDT"));
    http.response(
        "/fapi/v1/income",
        200,
        json!({"code":-2015,"msg":"alias refusal"}),
    );
    let error = run(&runtime, send()).unwrap_err();
    assert_eq!(error.message, "alias refusal");
    assert_eq!(error.response.as_ref().unwrap()["data"]["code"], -2015);
    assert_eq!(
        errors.borrow().as_slice(),
        &[vec![
            "Exchange Error: alias refusal".to_owned(),
            "metadata".to_owned()
        ]],
        "ResponseInterceptor emits once"
    );
    owner.dispose().unwrap();
}

#[test]
fn native_remote_sign_alias_carries_request_and_waits_for_host_result() {
    use safe_cex_transport::signing::{RemoteSignerRequest, RemoteSignerResponse};
    struct Signer {
        seen: RefCell<Vec<Value>>,
        ready: Cell<bool>,
    }
    impl safe_cex_transport::signing::RemoteSigner for Signer {
        fn sign(
            &self,
            request: RemoteSignerRequest,
        ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
            self.seen
                .borrow_mut()
                .push(serde_json::to_value(&request).unwrap());
            Box::pin(std::future::poll_fn(move |_| {
                if !self.ready.get() {
                    return Poll::Pending;
                }
                Poll::Ready(
                    request.decode_response(
                        r#"{"id":"alias-auth","wsAuthSignature":"native-signature"}"#,
                    ),
                )
            }))
        }
    }
    impl RemoteSigner for Signer {
        fn is_available(&self) -> bool {
            true
        }
        fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
            Box::new(|| {})
        }
    }
    let signer = Rc::new(Signer {
        seen: RefCell::default(),
        ready: Cell::new(false),
    });
    let runtime = Rc::new(Runtime::default());
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(Http::default()),
        Rc::new(|_, _| panic!("no external provider")),
        Some((signer.clone(), Some("alias-account".into()))),
        Default::default(),
    );
    let sign = dependencies.imports.remote_sign(record([])).unwrap();
    let mut pending = sign(EventValue::from_json(json!({"id":"alias-auth","type":"ws-auth",
        "exchangeName":"binancespot","accountName":"alias-account","wsAuth":{"toSign":"apiKey=alias-key&timestamp=17",
        "outputFormat":"hex","extraData":{"timestamp":17}}}))).unwrap();
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(
        signer.seen.borrow()[0]["wsAuth"]["toSign"],
        "apiKey=alias-key&timestamp=17"
    );
    assert_eq!(signer.seen.borrow()[0]["accountName"], "alias-account");
    signer.ready.set(true);
    let result = run(&runtime, pending).unwrap();
    assert_eq!(
        native_text(&result, "wsAuthSignature"),
        "native-signature",
        "RemoteSignCall retains native result"
    );
}
