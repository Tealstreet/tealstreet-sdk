use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch},
    journal::{HistoricalPositionOptions, JournalFolder, fold_closed_position_records},
    runtime::{
        Cleanup, ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeSigners, ExchangeTask,
        RemoteSigner,
    },
    signing::SigningHttpClient,
    worker::CallableMethodsInWorker,
};
use safe_cex_transport::{
    TransportError, TransportFuture,
    signing::{HttpSigningRequest, RemoteSignerRequest, RemoteSignerResponse, SigningConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

#[path = "support/store.rs"]
mod test_store;
use test_store::TestStore;

fn poll<T>(future: Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match poll(future.as_mut()) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("expected ready future"),
    }
}
#[derive(Default)]
struct Runtime {
    next: Cell<usize>,
    exceptions: RefCell<Vec<ExchangeError>>,
    timeout: Rc<Cell<bool>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        123_000.0
    }
    fn now_ms(&self) -> f64 {
        123_000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        let timeout = self.timeout.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if timeout.get() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no task expected")
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        self.next.set(self.next.get() + 1);
        self.next.get().to_string()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.exceptions.borrow_mut().push(error.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { unreachable!() })
    }
}
fn base(runtime: Rc<Runtime>) -> BaseExchange {
    BaseExchange::new(
        ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        Rc::new(TestStore::new()),
        runtime,
        ExchangeSettings::default(),
    )
}
type AvailabilityCallback = Rc<dyn Fn(bool)>;
struct Signer {
    available: Cell<bool>,
    callbacks: Rc<RefCell<Vec<AvailabilityCallback>>>,
    unsubscribed: Rc<Cell<usize>>,
}
impl Signer {
    fn new(available: bool) -> Rc<Self> {
        Rc::new(Self {
            available: Cell::new(available),
            callbacks: Rc::default(),
            unsubscribed: Rc::default(),
        })
    }
    fn notify(&self, available: bool) {
        self.available.set(available);
        let callbacks = self.callbacks.borrow().clone();
        for callback in callbacks {
            callback(available);
        }
    }
}
impl safe_cex_transport::signing::RemoteSigner for Signer {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        let RemoteSignerRequest::Http(request) = request else {
            return Box::pin(async { Err(TransportError::Host("WS auth unsupported".into())) });
        };
        Box::pin(async move {
            Ok(RemoteSignerResponse::Http(
                safe_cex_transport::signing::HttpSigningResponse {
                    id: request.id,
                    ..Default::default()
                },
            ))
        })
    }
}
impl RemoteSigner for Signer {
    fn is_available(&self) -> bool {
        self.available.get()
    }
    fn on_availability_change(&self, callback: AvailabilityCallback) -> Cleanup {
        self.callbacks.borrow_mut().push(callback.clone());
        let callbacks = self.callbacks.clone();
        let count = self.unsubscribed.clone();
        Box::new(move || {
            callbacks
                .borrow_mut()
                .retain(|row| !Rc::ptr_eq(row, &callback));
            count.set(count.get() + 1);
        })
    }
}

#[test]
fn signer_wait_absent_or_ready_returns_without_registering_listener() {
    let mut base = base(Rc::new(Runtime::default()));
    assert!(ready(base.wait_for_remote_signer()).unwrap());
    let signer = Signer::new(true);
    base.signers = ExchangeSigners {
        remote_signer: Some(signer.clone()),
        payload_signer: None,
    };
    assert!(ready(base.wait_for_remote_signer()).unwrap());
    assert!(signer.callbacks.borrow().is_empty());
    assert_eq!(signer.unsubscribed.get(), 0);
}

#[test]
fn signer_wait_starts_before_first_poll_and_unpolled_drop_cleans_up() {
    let signer = Signer::new(false);
    let mut base = base(Rc::new(Runtime::default()));
    base.signers.remote_signer = Some(signer.clone());
    let statuses = Rc::new(RefCell::new(vec![]));
    let recorded = statuses.clone();
    base.on(
        "remoteSignerStatus",
        Rc::new(move |_, value| {
            recorded.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let waiter = base.wait_for_remote_signer();
    assert_eq!(
        *statuses.borrow(),
        [json!("disconnected")],
        "SIGNER native invocation emits before first poll"
    );
    assert_eq!(
        signer.callbacks.borrow().len(),
        1,
        "SIGNER native invocation registers before first poll"
    );
    drop(waiter);
    assert!(
        signer.callbacks.borrow().is_empty(),
        "SIGNER native unpolled drop releases subscription"
    );
    assert_eq!(signer.unsubscribed.get(), 1);
    signer.notify(true);
    assert_eq!(*statuses.borrow(), [json!("disconnected")]);
}

#[test]
fn signer_wait_disconnect_callback_settles_once_and_unsubscribes() {
    let signer = Signer::new(false);
    let mut base = base(Rc::new(Runtime::default()));
    base.signers.remote_signer = Some(signer.clone());
    let statuses = Rc::new(RefCell::new(vec![]));
    let recorded = statuses.clone();
    base.on(
        "remoteSignerStatus",
        Rc::new(move |_, value| {
            recorded.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let mut waiter = Box::pin(base.wait_for_remote_signer());
    assert!(poll(waiter.as_mut()).is_pending());
    assert_eq!(*statuses.borrow(), vec![json!("disconnected")]);
    assert_eq!(signer.callbacks.borrow().len(), 1);
    signer.notify(false);
    assert!(poll(waiter.as_mut()).is_pending());
    signer.notify(true);
    assert_eq!(
        *statuses.borrow(),
        vec![json!("disconnected"), json!("connected")]
    );
    base.dispose().unwrap();
    assert!(matches!(poll(waiter.as_mut()), Poll::Ready(Ok(true))));
    assert_eq!(signer.unsubscribed.get(), 1);
    assert!(signer.callbacks.borrow().is_empty());
    assert_eq!(
        *statuses.borrow(),
        vec![json!("disconnected"), json!("connected")]
    );
    signer.notify(true);
    assert_eq!(signer.unsubscribed.get(), 1);
}

#[test]
fn disposal_settles_signer_wait_false_and_unsubscribes_without_connected_event() {
    let signer = Signer::new(false);
    let mut base = base(Rc::new(Runtime::default()));
    base.signers.remote_signer = Some(signer.clone());
    let mut waiter = Box::pin(base.wait_for_remote_signer());
    assert!(poll(waiter.as_mut()).is_pending());
    base.dispose().unwrap();
    assert!(matches!(poll(waiter.as_mut()), Poll::Ready(Ok(false))));
    assert_eq!(signer.unsubscribed.get(), 1);
    assert!(signer.callbacks.borrow().is_empty());
    signer.notify(true);
    assert_eq!(signer.unsubscribed.get(), 1);
}

type HookRecord = (Value, Value, Vec<Value>);
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    hooks: RefCell<Vec<HookRecord>>,
    hook_error: RefCell<Option<ExchangeError>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn sign_ws_auth<'a>(
        &'a self,
        request: &'a safe_cex_crypto::WsAuthRequest,
    ) -> ExchangeFuture<'a, safe_cex_crypto::WsAuthResponse> {
        Box::pin(async move {
            Ok(safe_cex_crypto::WsAuthResponse {
                id: request.id.clone(),
                ws_auth_signature: Some("venue-specific".into()),
                error: None,
            })
        })
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.hooks
            .borrow_mut()
            .push((json!(symbol), json!("listenOrderBook"), vec![]));
        callback(serde_json::from_value(json!({"bids":[],"asks":[]})).unwrap());
        Ok(Box::new(|| Ok(())))
    }
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn place_orders(&self, _: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async { unreachable!() })
    }
    fn after_method_called_in_worker(
        &self,
        result: &Value,
        method: CallableMethodsInWorker,
        args: &[Value],
    ) -> Result<(), ExchangeError> {
        self.hooks.borrow_mut().push((
            result.clone(),
            serde_json::to_value(method).unwrap(),
            args.to_vec(),
        ));
        if let Some(error) = self.hook_error.borrow().clone() {
            return Err(error);
        }
        Ok(())
    }
}
fn venue(runtime: Rc<Runtime>) -> Venue {
    Venue {
        base: base(runtime),
        capabilities: ExchangeCapabilities::default(),
        hooks: RefCell::new(vec![]),
        hook_error: RefCell::new(None),
    }
}
fn capture_request(base: &BaseExchange, event: &str) -> Rc<RefCell<Vec<Value>>> {
    let requests = Rc::new(RefCell::new(vec![]));
    let recorded = requests.clone();
    base.on(
        event,
        Rc::new(move |_, value| {
            recorded.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    requests
}

#[test]
fn worker_correlation_ignores_wrong_request_and_dispatches_override_without_socket() {
    let venue = venue(Rc::new(Runtime::default()));
    let requests = capture_request(&venue.base, "callMethod");
    let mut request = Box::pin(venue.base.call_method_in_worker(
        &venue,
        CallableMethodsInWorker::FetchOrders,
        vec![json!("BTC")],
    ));
    assert!(poll(request.as_mut()).is_pending());
    let id = requests.borrow()[0]["requestId"].clone();
    venue
        .base
        .emit(
            "callMethod:done",
            json!({"requestId":"wrong","success":true,"result":99}),
        )
        .unwrap();
    assert!(poll(request.as_mut()).is_pending());
    assert!(venue.hooks.borrow().is_empty());
    venue
        .base
        .emit(
            "callMethod:done",
            json!({"requestId":id,"success":true,"result":[1,2]}),
        )
        .unwrap();
    assert!(matches!(poll(request.as_mut()), Poll::Ready(Ok(value)) if value == json!([1,2])));
    assert_eq!(
        *venue.hooks.borrow(),
        vec![(json!([1, 2]), json!("fetchOrders"), vec![json!("BTC")])]
    );
}

struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn worker_timeout_removes_listener_and_releases_pending_waker() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime.clone());
    let requests = capture_request(&venue.base, "callMethod");
    let wake = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(wake.clone());
    let mut context = Context::from_waker(&waker);
    let mut request = Box::pin(venue.base.call_method_in_worker(
        &venue,
        CallableMethodsInWorker::FetchOrders,
        vec![],
    ));
    assert!(request.as_mut().poll(&mut context).is_pending());
    assert_eq!(Arc::strong_count(&wake), 3);
    runtime.timeout.set(true);
    assert!(
        matches!(request.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error.message == "Timeout callMethodInWorker")
    );
    assert_eq!(
        Arc::strong_count(&wake),
        2,
        "timeout must unregister captured listener/waker"
    );
    let id = requests.borrow()[0]["requestId"].clone();
    venue
        .base
        .emit(
            "callMethod:done",
            json!({"requestId":id,"success":true,"result":42}),
        )
        .unwrap();
    assert_eq!(
        wake.0.load(Ordering::SeqCst),
        0,
        "late reply must not wake timed-out request"
    );
    assert!(venue.hooks.borrow().is_empty());
}

#[test]
fn update_positions_worker_correlates_and_preserves_reason() {
    let base = base(Rc::new(Runtime::default()));
    let requests = capture_request(&base, "updatePositions");
    let mut request = Box::pin(base.update_positions_in_webworker(Some(vec![])));
    assert!(poll(request.as_mut()).is_pending());
    assert_eq!(requests.borrow()[0]["symbols"], json!([]));
    let id = requests.borrow()[0]["requestId"].clone();
    base.emit(
        "updatePositions:done",
        json!({"requestId":"wrong","success":true}),
    )
    .unwrap();
    assert!(poll(request.as_mut()).is_pending());
    base.emit(
        "updatePositions:done",
        json!({"requestId":id,"success":false,"reason":"retry_too_soon"}),
    )
    .unwrap();
    assert!(
        matches!(poll(request.as_mut()), Poll::Ready(Err(error)) if error.message == "retry_too_soon")
    );
}

#[derive(Default)]
struct Capture {
    requests: RefCell<Vec<SigningConfig>>,
}
impl SigningHttpClient for Capture {
    fn capture(&self, mut config: SigningConfig) -> ExchangeFuture<'_, SigningConfig> {
        self.requests.borrow_mut().push(config.clone());
        config.headers.insert("signed".into(), config.url.clone());
        Box::pin(async move { Ok(config) })
    }
}

#[test]
fn http_signing_capture_parses_json_per_request_and_missing_client_refuses() {
    let mut base = base(Rc::new(Runtime::default()));
    let request = |id: &str, url: &str| HttpSigningRequest {
        id: id.into(),
        exchange_name: ExchangeName::Bybit,
        account_name: None,
        config: SigningConfig {
            method: "POST".into(),
            url: url.into(),
            base_url: "https://venue".into(),
            params: Some(serde_json::json!("{\"x\":1}")),
            data: Some(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(json!("[1,2]"))
                    .unwrap(),
            ),
            ..Default::default()
        },
    };
    let absent = ready(base.sign_http_request(request("absent", "/absent"))).unwrap();
    assert_eq!(
        absent.error.as_deref(),
        Some("Exchange does not have xhr for signing")
    );
    let client = Rc::new(Capture::default());
    base.signing_http_client = Some(client.clone());
    for (id, url) in [("first", "/one"), ("second", "/two")] {
        let response = ready(base.sign_http_request(request(id, url))).unwrap();
        assert_eq!(response.id, id);
        let config = response.config.unwrap();
        assert_eq!(config.params, Some(serde_json::json!({"x":1})));
        assert_eq!(
            config.data,
            Some(safe_cex_types::retained_json::RetainedJsonInput::Array(
                vec![
                    safe_cex_types::retained_json::RetainedJsonInput::Float(1.0),
                    safe_cex_types::retained_json::RetainedJsonInput::Float(2.0)
                ]
            ))
        );
        assert_eq!(config.headers.get("signed").map(String::as_str), Some(url));
    }
    assert_eq!(client.requests.borrow().len(), 2);
    assert!(!client.requests.borrow()[0].headers.contains_key("signed"));
    assert!(!client.requests.borrow()[1].headers.contains_key("signed"));
}

struct Folder(Cell<usize>);
impl JournalFolder for Folder {
    fn build_folded_position_records<'a>(
        &'a self,
        _: &'a dyn Exchange,
        _: &'a str,
        _: f64,
        _: f64,
        _: bool,
        _: Box<dyn Fn(&str) -> String + 'a>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        self.0.set(self.0.get() + 1);
        Box::pin(async { Ok(vec![]) })
    }
}

#[test]
fn journal_default_refuses_and_both_inert_fill_brands_block_fold() {
    let mut venue = venue(Rc::new(Runtime::default()));
    let options = || HistoricalPositionOptions {
        account: "account".into(),
        from: None,
        to: None,
    };
    let error = ready(venue.fetch_historical_positions(options())).unwrap_err();
    assert!(error.message.contains("no journal records path declared"));
    let folder = Folder(Cell::new(0));
    assert!(
        ready(fold_closed_position_records(
            &venue,
            &folder,
            options(),
            false
        ))
        .is_err()
    );
    venue.capabilities.fetch_historical_fills =
        CapabilityBinding::empty_override(|_, _| Box::pin(async { Ok(vec![]) }));
    assert!(
        ready(fold_closed_position_records(
            &venue,
            &folder,
            options(),
            false
        ))
        .is_err()
    );
    assert_eq!(folder.0.get(), 0);
    venue.capabilities.fetch_historical_fills =
        CapabilityBinding::implemented(|_, _| Box::pin(async { Ok(vec![]) }));
    assert!(
        ready(fold_closed_position_records(
            &venue,
            &folder,
            options(),
            false
        ))
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        folder.0.get(),
        1,
        "positive control proves the folder recorder works"
    );
}

#[test]
fn venue_signer_and_subscription_overrides_dispatch_through_trait_object() {
    let venue = venue(Rc::new(Runtime::default()));
    let exchange: &dyn Exchange = &venue;
    let request = serde_json::from_value(json!({"id":"correlation", "wsAuth":{"toSign":"challenge", "outputFormat":"kraken-challenge"}})).unwrap();
    let response = ready(exchange.sign_ws_auth(&request)).unwrap();
    assert_eq!(response.id, Some(json!("correlation")));
    assert_eq!(
        response.ws_auth_signature.as_deref(),
        Some("venue-specific")
    );
    let seen = Rc::new(Cell::new(false));
    let callback_seen = seen.clone();
    let cleanup = exchange
        .listen_order_book("BTC".into(), Rc::new(move |_| callback_seen.set(true)))
        .unwrap();
    assert!(seen.get());
    assert_eq!(venue.hooks.borrow()[0].0, json!("BTC"));
    cleanup().unwrap();
    let expected = json!({"symbol":"BTC","interval":"1m","timestamp":101.0,"open":2.0,"high":5.0,"low":1.0,"close":3.0,"volume":7.0});
    let candle: safe_cex_exchange::exchange::SubscribedCandle =
        serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(
        candle.candle.open, 2.0,
        "subscribed candle native open identity"
    );
    assert_eq!(
        candle.candle.high, 5.0,
        "subscribed candle native high identity"
    );
    assert_eq!(
        candle.candle.low, 1.0,
        "subscribed candle native low identity"
    );
    assert_eq!(
        candle.candle.close, 3.0,
        "subscribed candle native close identity"
    );
    assert_eq!(
        candle.candle.volume, 7.0,
        "subscribed candle native volume identity"
    );
    assert_eq!(
        candle.candle.timestamp, 101.0,
        "subscribed candle native timestamp identity"
    );
    assert_eq!(
        serde_json::to_value(candle).unwrap(),
        expected,
        "subscribed candle preserves each OHLCV field identity"
    );
}

#[test]
fn dropping_worker_waiter_releases_its_listener_and_waker() {
    let runtime = Rc::new(Runtime::default());
    let venue = venue(runtime);
    let requests = capture_request(&venue.base, "callMethod");
    let wake = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(wake.clone());
    let mut context = Context::from_waker(&waker);
    let mut request = Box::pin(venue.base.call_method_in_worker(
        &venue,
        CallableMethodsInWorker::FetchOrders,
        vec![],
    ));
    assert!(request.as_mut().poll(&mut context).is_pending());
    assert_eq!(Arc::strong_count(&wake), 3);
    drop(request);
    assert_eq!(Arc::strong_count(&wake), 2);
    venue
        .base
        .emit(
            "callMethod:done",
            json!({"requestId":requests.borrow()[0]["requestId"],"success":true,"result":42}),
        )
        .unwrap();
    assert_eq!(wake.0.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelled_signer_waits_release_subscription_and_ignore_late_callbacks() {
    let signer = Signer::new(false);
    let mut base = base(Rc::new(Runtime::default()));
    base.signers.remote_signer = Some(signer.clone());
    let statuses = Rc::new(RefCell::new(vec![]));
    let recorded = statuses.clone();
    base.on(
        "remoteSignerStatus",
        Rc::new(move |_, value| {
            recorded.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    let mut stale_callbacks = vec![];
    for _ in 0..5 {
        let mut task = Box::pin(base.wait_for_remote_signer());
        assert!(poll(task.as_mut()).is_pending());
        stale_callbacks.extend(signer.callbacks.borrow().iter().cloned());
        drop(task); // Here the caller future owns the subscription; no detached task exists.
        assert!(
            signer.callbacks.borrow().is_empty(),
            "cancelled signer subscription leaked"
        );
    }
    assert_eq!(signer.unsubscribed.get(), 5);
    for callback in stale_callbacks {
        callback(true);
    }
    assert_eq!(
        *statuses.borrow(),
        vec![json!("disconnected"); 5],
        "cancelled waits emitted connected"
    );
    base.dispose().unwrap();
    assert_eq!(signer.unsubscribed.get(), 5, "cleanup must be idempotent");
}

#[test]
fn unified_remote_signer_dispatches_ws_to_the_venue_override() {
    let venue = venue(Rc::default());
    let exchange: &dyn Exchange = &venue;
    let request: RemoteSignerRequest=serde_json::from_str(r#"{"id":"ws-1","type":"ws-auth","exchangeName":"bybit","wsAuth":{"toSign":"GET/realtime1234","outputFormat":"hex","extraData":{"expires":1234}}}"#).unwrap();
    let response = ready(exchange.sign_remote_request(request)).unwrap();
    let RemoteSignerResponse::WsAuth(response) = response else {
        panic!("remote WS must dispatch the concrete venue WS hook")
    };
    assert_eq!(
        response.ws_auth_signature.as_deref(),
        Some("venue-specific"),
        "remote WS must dispatch the concrete venue WS hook"
    );
    assert_eq!(response.id, Some(json!("ws-1")));
    let host = Signer::new(false);
    let request: RemoteSignerRequest = serde_json::from_str(
        r#"{"type":"ws-auth","exchangeName":"bybit","wsAuth":{"toSign":"x"}}"#,
    )
    .unwrap();
    let result = ready(safe_cex_transport::signing::RemoteSigner::sign(
        host.as_ref(),
        request,
    ));
    assert!(
        matches!(result,Err(TransportError::Host(message)) if message=="WS auth unsupported"),
        "HTTP-only host must explicitly refuse WS auth"
    );
}

#[test]
fn signer_listener_throw_keeps_source_settled_but_unresolved_state() {
    let runtime = Rc::new(Runtime::default());
    let signer = Signer::new(false);
    let mut base = base(runtime.clone());
    base.signers.remote_signer = Some(signer.clone());
    let mut expected = ExchangeError::new("connected listener failed");
    expected.name = "TypeError".into();
    let error = expected.clone();
    base.on(
        "remoteSignerStatus",
        Rc::new(move |_, value| {
            if value == "connected" {
                Err(error.clone())
            } else {
                Ok(())
            }
        }),
    );
    let mut waiter = Box::pin(base.wait_for_remote_signer());
    assert!(poll(waiter.as_mut()).is_pending());
    signer.notify(true);
    assert_eq!(
        *runtime.exceptions.borrow(),
        [expected],
        "unawaited listener throw reaches host with original identity"
    );
    assert_eq!(signer.unsubscribed.get(), 1);
    assert!(
        poll(waiter.as_mut()).is_pending(),
        "source resolves only after successful connected emit"
    );
    base.dispose().unwrap();
    assert!(
        poll(waiter.as_mut()).is_pending(),
        "source already-settled callback cannot be resolved by disposal"
    );
}

#[test]
fn initial_signer_status_failure_returns_before_registration() {
    let runtime = Rc::new(Runtime::default());
    let signer = Signer::new(false);
    let mut base = base(runtime);
    base.signers.remote_signer = Some(signer.clone());
    base.on(
        "remoteSignerStatus",
        Rc::new(|_, _| Err(ExchangeError::new("disconnected listener failed"))),
    );
    assert_eq!(
        ready(base.wait_for_remote_signer()).unwrap_err().message,
        "disconnected listener failed"
    );
    assert!(
        signer.callbacks.borrow().is_empty(),
        "initial emit failure must precede availability registration"
    );
}

#[test]
#[ignore = "mandatory core CI executes the actual source with isolated TinyEmitter"]
fn signer_listener_throw_order_matches_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/listener-oracle.cjs"
        ))
        .arg("--signer")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "signer listener oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows,
        json!([
            {"fail":"disconnected","settled":true,"registered":false,"removals":0,"name":"TypeError"},
            {"fail":"connected","settled":false,"registered":true,"removals":1,"name":"TypeError"}
        ]),
        "source signer latch and promise settlement are distinct"
    );
    signer_listener_throw_keeps_source_settled_but_unresolved_state();
    initial_signer_status_failure_returns_before_registration();
}

#[test]
fn successful_worker_reply_rejects_when_concrete_after_hook_fails() {
    let venue = venue(Rc::new(Runtime::default()));
    let mut original = ExchangeError::new("authored worker confirmation error");
    original.code = Some(json!(731));
    original.store_subscriber_ids = vec![17];
    *venue.hook_error.borrow_mut() = Some(original.clone());
    let requests = capture_request(&venue.base, "callMethod");
    let mut operation = Box::pin(venue.base.call_method_in_worker(
        &venue,
        CallableMethodsInWorker::ChangePositionMode,
        vec![json!(true)],
    ));
    assert!(
        poll(operation.as_mut()).is_pending(),
        "native worker request awaits correlated reply"
    );
    let id = requests.borrow()[0]["requestId"].clone();
    venue
        .base
        .emit(
            "callMethod:done",
            json!({"requestId":id,"success":true,"result":73}),
        )
        .unwrap();
    let outcome = poll(operation.as_mut());
    assert!(
        matches!(outcome, Poll::Ready(Err(error)) if error == original),
        "native successful worker reply preserves concrete after-hook rejection metadata"
    );
    assert_eq!(
        venue.hooks.borrow().len(),
        1,
        "native after-hook called exactly once before rejection"
    );
}

#[test]
#[ignore = "requires the live TypeScript compiler; mandatory core CI includes ignored tests"]
fn live_divergence_prefix_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/divergence-prefix-source-gate.cjs"
        ))
        .output()
        .expect("live prefix source process");
    assert!(
        output.status.success(),
        "PREFIX actual-source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "EXCHANGE_PREFIX_SOURCE signer=1 pager=6 resync=1\n",
        "PREFIX independent source completion marker"
    );
}
