// Virtual-clock runtime and scripted REST transport for the concrete Bitunix venue.
// Kept include-compatible for native accepted-set probes of concrete host adapters.
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    http::{HttpClientOptions, HttpFuture, HttpHost, safe_cex_axios_create_no_proxy},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportProvider, socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bitunix::adapter::BitunixVenue;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
#[derive(Default)]
pub struct Runtime {
    pub now: Rc<Cell<f64>>,
    pub id: RefCell<Option<String>>,
    pub jobs: RefCell<Vec<Job>>,
    pub errors: RefCell<Vec<String>>,
    pub console: RefCell<Vec<(String, String)>>,
    /// When set, dials open a scripted socket instead of failing.
    pub dial: Cell<bool>,
    pub dialed: RefCell<Vec<(String, Rc<FakeSocket>)>>,
}
/// Opens on its first poll and records every frame the venue sends.
#[derive(Default)]
pub struct FakeSocket {
    pub events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    pub sent: RefCell<Vec<String>>,
}
impl WsSocket for FakeSocket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
        }))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    pub fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        let mut pending = vec![];
        for (mut f, c) in jobs {
            if !c.get()
                && f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            {
                pending.push((f, c));
            }
        }
        self.jobs.borrow_mut().extend(pending);
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        let at = self.now.get() + d as f64;
        let now = self.now.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let c = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((f, c.clone()));
        Box::new(Task(c))
    }
    fn console(&self, severity: &str, message: &str) {
        self.console
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        self.id
            .borrow()
            .clone()
            .unwrap_or_else(|| "fixture-nonce".into())
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.375
    }
    fn report_raw_exception(&self, _: &Value, _: &Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unused in native fixture")) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("fixture refuses tunnel transport")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        if !self.dial.get() {
            return Box::pin(async {
                Err(ExchangeError::new("No dial in the scripted REST fixture"))
            });
        }
        let socket = Rc::new(FakeSocket::default());
        socket.events.borrow_mut().push_back(WsEvent::Open);
        self.dialed.borrow_mut().push((url, socket.clone()));
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        3000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
#[derive(Default)]
pub struct Wire {
    pub requests: Mutex<Vec<RestRequest>>,
    pub bodies: Mutex<BTreeMap<String, Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(r.clone());
        let body = self
            .bodies
            .lock()
            .unwrap()
            .get(&r.url)
            .cloned()
            .unwrap_or(json!({"code": 0, "msg": "Success", "data": {}}));
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(body).unwrap(),
                ),
            })
        })
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, a: Arc<dyn RestAdapter>, r: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { a.execute_with_receipt(r).await })
    }
    fn enhance_error(&self, e: RestFailure, _: &ExchangeName) -> RestFailure {
        e
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &RestFailure) {}
    fn mark_start(&self, _: &mut RestRequest) {}
    fn signing_capture(&self, _: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}
pub const MARKET: &str = r#"{"id":"BTC/USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0,"max":500},"leverage":{"min":1,"max":100}}}"#;
pub fn fixture() -> (Rc<BitunixVenue>, Rc<Runtime>, Rc<DefaultStore>, Arc<Wire>) {
    fixture_with(true)
}
/// `public_only = false` is a signed account: transport installation runs and private reads proceed.
pub fn fixture_with(
    public_only: bool,
) -> (Rc<BitunixVenue>, Rc<Runtime>, Rc<DefaultStore>, Arc<Wire>) {
    let runtime = Rc::new(Runtime::default());
    let store = Rc::new(DefaultStore::new());
    let opts: ExchangeOptions = serde_json::from_value(
        json!({"key":"fixture-key","secret":"fixture-secret","publicDataOnly":public_only}),
    )
    .unwrap();
    let installs = Rc::new(Cell::new(0));
    let counted = installs.clone();
    let base = BaseExchange::new(
        ExchangeName::Bitunix,
        opts,
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let wire = Arc::new(Wire::default());
    let adapter = wire.clone();
    let rt = runtime.clone();
    let venue = BitunixVenue::new(
        base,
        Rc::new(move |options: &ExchangeOptions| {
            let adapter: Arc<dyn RestAdapter> = adapter.clone();
            Ok(safe_cex_axios_create_no_proxy(HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Bitunix,
                options: options.clone(),
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter,
                host: Rc::new(Host),
                runtime: rt.clone(),
                store: None,
                stats: None,
                logs: None,
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(|| "id".into()),
                signing_now: Arc::new(|| 0),
            }))
        }),
        Rc::new(move |_| {
            assert!(!public_only, "publicDataOnly skips transport installation");
            counted.set(counted.get() + 1);
            Ok(())
        }),
        runtime.clone(),
        Rc::new(|_, _| panic!("fixture has no external public data")),
        None,
    )
    .unwrap();
    assert_eq!(
        installs.get(),
        if public_only { 0 } else { 2 },
        "one transport per createAPI lane"
    );
    let market: safe_cex_types::Market = serde_json::from_str(MARKET).unwrap();
    let ticker: safe_cex_types::Ticker = serde_json::from_value(json!({"id":"BTC/USDT","symbol":"BTCUSDT","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    let positions: Vec<safe_cex_types::Position> = serde_json::from_value(json!([
        {"id":"BTCUSDT:long","symbol":"BTCUSDT","side":"long","entryPrice":0,"notional":0,"leverage":10,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0,"marginType":"cross","positionMode":"hedged"},
        {"id":"BTCUSDT:short","symbol":"BTCUSDT","side":"short","entryPrice":0,"notional":0,"leverage":10,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0,"marginType":"cross","positionMode":"hedged"}
    ]))
    .unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![ticker]),
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    (venue, runtime, store, wire)
}
