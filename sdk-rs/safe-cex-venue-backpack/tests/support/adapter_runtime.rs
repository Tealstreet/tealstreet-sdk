use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    http::{HttpClientOptions, HttpFuture, HttpHost, safe_cex_axios_create_no_proxy},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportProvider, socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_backpack::adapter::BackpackExchange;
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
    pub jobs: RefCell<Vec<Job>>,
    pub errors: RefCell<Vec<String>>,
    pub dial_sockets: RefCell<Vec<Rc<dyn WsSocket>>>,
    pub provider: RefCell<Option<Arc<dyn TransportProvider>>>,
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
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.375
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
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
        self.provider
            .borrow()
            .clone()
            .unwrap_or_else(|| Arc::new(safe_cex_transport::DirectTransport))
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("fixture refuses tunnel transport")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        if self.dial_sockets.borrow().is_empty() {
            Box::pin(async { Err(ExchangeError::new("No dial in class construction fixture")) })
        } else {
            let socket = self.dial_sockets.borrow_mut().remove(0);
            Box::pin(std::future::ready(Ok(socket)))
        }
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
    pub bodies: Mutex<BTreeMap<String, RestResponseBody>>,
}
impl RestAdapter for Wire {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(r.clone());
        let body = self.bodies.lock().unwrap().get(&r.url).cloned().unwrap_or(
            RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    json!({"id":"native-id"}),
                )
                .unwrap(),
            ),
        );
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body,
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
pub fn fixture() -> (
    Rc<BackpackExchange>,
    Rc<Runtime>,
    Rc<DefaultStore>,
    Arc<Wire>,
) {
    fixture_options(
        serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap(),
    )
}
pub fn fixture_options(
    opts: ExchangeOptions,
) -> (
    Rc<BackpackExchange>,
    Rc<Runtime>,
    Rc<DefaultStore>,
    Arc<Wire>,
) {
    let runtime = Rc::new(Runtime::default());
    let store = Rc::new(DefaultStore::new());
    let base = BaseExchange::new(
        ExchangeName::Backpack,
        opts,
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let wire = Arc::new(Wire::default());
    let adapter = wire.clone();
    let rt = runtime.clone();
    let owner = BackpackExchange::new(
        base,
        Box::new(move |c, _| {
            assert_eq!(
                c.timeout.to_bits(),
                30000.0f64.to_bits(),
                "CLASS native API timeout"
            );
            Ok(safe_cex_axios_create_no_proxy(HttpClientOptions {
                response_transform: None,
                params_serializer: None,
                exchange: ExchangeName::Backpack,
                options: serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter: adapter.clone(),
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
        Rc::new(|_, _| panic!("Tunnel installation disabled")),
        runtime.clone(),
        Rc::new(|_| Ok(1735689600000.0)),
    )
    .unwrap();
    let market: safe_cex_types::Market=serde_json::from_value(json!({"id":"BTC_USDC","symbol":"BTC_USDC","base":"BTC","quote":"USDC","active":true,"precision":{"amount":0.25,"price":0.125},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":10}}})).unwrap();
    let ticker:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"BTC_USDC","symbol":"BTC_USDC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    (owner, runtime, store, wire)
}
