use safe_cex_crypto::{
    Encoding, PayloadScheme, PayloadSigner, PayloadSignerRequest, PayloadSignerResult,
};
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    http::{HttpClientOptions, HttpFailure, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{store::DefaultStore, virtual_clock::VirtualClock};
use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture, TransportRegistry,
    rate_limit::RateLimitClock,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bybit::api_factory::{BybitApi, BybitApiOptions, create_api};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
pub type Work = Pin<Box<dyn Future<Output = ()>>>;
pub type Job = Rc<RefCell<Option<Work>>>;
#[derive(Default)]
pub struct WakeCount(pub AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
pub struct Task(pub Job);
impl ExchangeTask for Task {
    fn cancel(&self) {
        let future = self.0.borrow_mut().take();
        drop(future);
    }
}
pub struct Runtime {
    pub now: Arc<AtomicU64>,
    pub jobs: RefCell<Vec<Job>>,
    pub console: RefCell<Vec<(String, String)>>,
    pub inline: Cell<bool>,
    pub waker: Waker,
}
impl Runtime {
    pub fn pump(&self) {
        let jobs = self.jobs.borrow().clone();
        for job in jobs {
            let future = job.borrow_mut().take();
            if let Some(mut future) = future
                && future
                    .as_mut()
                    .poll(&mut Context::from_waker(&self.waker))
                    .is_pending()
            {
                *job.borrow_mut() = Some(future);
            }
        }
    }
    pub fn active_jobs(&self) -> usize {
        self.jobs
            .borrow()
            .iter()
            .filter(|job| job.borrow().is_some())
            .count()
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.load(Ordering::SeqCst) as f64
    }
    fn now_ms(&self) -> f64 {
        self.now.load(Ordering::SeqCst) as f64
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.now.fetch_add(delay, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Work) -> Box<dyn ExchangeTask> {
        let job = Rc::new(RefCell::new(Some(future)));
        self.jobs.borrow_mut().push(job.clone());
        if self.inline.get() {
            self.pump();
        }
        Box::new(Task(job))
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((level.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "factory-boundary-listener".into()
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
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected factory exception {error}")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker request")
    }
}
pub struct RetryClock(pub Arc<AtomicU64>);
impl RateLimitClock for RetryClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn sleep_ms(&self, delay: u64) -> TransportFuture<'_, ()> {
        self.0.fetch_add(delay, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}
pub struct Owner {
    pub base: BaseExchange,
    pub capabilities: ExchangeCapabilities,
    pub accesses: RefCell<Vec<u8>>,
}
impl Exchange for Owner {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.accesses
            .borrow_mut()
            .push(self.base.auth_failure_streak());
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
#[derive(Default)]
pub struct Gate {
    pub released: Cell<bool>,
    pub waker: RefCell<Option<Waker>>,
    pub active: Cell<usize>,
}
impl Gate {
    pub fn release(&self) {
        self.released.set(true);
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    pub async fn wait(&self) {
        poll_fn(|cx| {
            if self.released.get() {
                Poll::Ready(())
            } else {
                *self.waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}
pub struct ActiveProbe(pub Rc<Gate>);
impl Drop for ActiveProbe {
    fn drop(&mut self) {
        self.0.active.set(self.0.active.get() - 1);
        let waker = self.0.waker.borrow_mut().take();
        drop(waker);
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ProbeKind {
    Mismatch,
    NoMismatch,
    Failed,
}
pub enum Mode {
    Probe { kind: ProbeKind, gate: Rc<Gate> },
    Timeout { physical_ms: u64 },
}
pub struct Wire {
    pub mode: Mode,
    pub now: Arc<AtomicU64>,
    pub requests: RefCell<Vec<RestRequest>>,
    pub starts: RefCell<Vec<u64>>,
    pub probe_count: Cell<usize>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(request.clone());
        self.starts
            .borrow_mut()
            .push(self.now.load(Ordering::SeqCst));
        let call = self.requests.borrow().len();
        Box::pin(async move {
            let data = match &self.mode {
                Mode::Timeout { physical_ms } => {
                    if call == 1 {
                        self.now.fetch_add(*physical_ms, Ordering::SeqCst);
                        return Err(RestFailure::from_transport(
                            TransportError::Network(NetworkFailure::from_code(
                                "scripted network".into(),
                                Some("ECONNRESET".into()),
                            )),
                            request,
                        ));
                    }
                    json!({"retCode":0,"retMsg":"OK"})
                }
                Mode::Probe { kind, gate } => {
                    if request.url.split('?').next() == Some("/v5/user/query-api") {
                        assert_eq!(
                            request.base_url.as_deref(),
                            Some("https://api-demo.bybit.com"),
                            "probe_native_opposite_base"
                        );
                        self.probe_count.set(self.probe_count.get() + 1);
                        gate.active.set(gate.active.get() + 1);
                        let _active = ActiveProbe(gate.clone());
                        gate.wait().await;
                        match kind {
                            ProbeKind::Mismatch => json!({"retCode":0,"retMsg":"OK"}),
                            ProbeKind::NoMismatch => {
                                json!({"retCode":10003,"retMsg":"recv window rejected server_timestamp[1791518966503]"})
                            }
                            ProbeKind::Failed => {
                                return Err(RestFailure::from_transport(
                                    TransportError::Timeout,
                                    request,
                                ));
                            }
                        }
                    } else {
                        assert_eq!(request.url.split('?').next(), Some("/private"));
                        json!({"retCode":10003,"retMsg":"invalid key"})
                    }
                }
            };
            Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&data).unwrap()),
            })
        })
    }
}
#[derive(Default)]
pub struct Host {
    pub responses: RefCell<Vec<Value>>,
    pub failures: Cell<usize>,
}
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: HttpFailure, _: &ExchangeName) -> HttpFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_response(&self, response: &RestResponse, _: &RestRequest) {
        self.responses
            .borrow_mut()
            .push(response.body.clone().into_data().into_value().unwrap());
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &HttpFailure) {
        self.failures.set(self.failures.get() + 1);
    }
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
pub struct Signer {
    pub now: Arc<AtomicU64>,
    pub delay: u64,
    pub calls: RefCell<Vec<(u64, String)>>,
}
impl PayloadSigner for Signer {
    fn sign<'a>(
        &'a self,
        request: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        assert_eq!(request.exchange_name, ExchangeName::Bybitinverse);
        assert_eq!(request.key, "key");
        assert_eq!(
            request.scheme,
            PayloadScheme::HmacSha256 {
                encoding: Encoding::Hex
            }
        );
        self.calls
            .borrow_mut()
            .push((self.now.load(Ordering::SeqCst), request.payload.into()));
        Box::pin(async move {
            let mut yielded = false;
            poll_fn(|cx| {
                if yielded {
                    Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            self.now.fetch_add(self.delay, Ordering::SeqCst);
            Ok(PayloadSignerResult {
                signature: "signature".into(),
                headers: None,
            })
        })
    }
}
pub struct Fixture {
    pub api: BybitApi,
    pub owner: Rc<Owner>,
    pub runtime: Rc<Runtime>,
    pub wire: Arc<Wire>,
    pub host: Rc<Host>,
    pub clock: VirtualClock,
    pub wakes: Arc<WakeCount>,
    pub clock_fetches: Arc<AtomicUsize>,
    pub signer: Option<Rc<Signer>>,
}
pub fn fixture(mode: Mode, signer_ms: Option<u64>) -> Fixture {
    let now = Arc::new(AtomicU64::new(1000));
    let wakes = Arc::new(WakeCount::default());
    let runtime = Rc::new(Runtime {
        now: now.clone(),
        jobs: RefCell::new(Vec::new()),
        console: RefCell::new(Vec::new()),
        inline: Cell::new(false),
        waker: Waker::from(wakes.clone()),
    });
    let name = if matches!(&mode, Mode::Probe { .. }) {
        ExchangeName::Bybit
    } else {
        ExchangeName::Bybitinverse
    };
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
    let owner = Rc::new(Owner {
        base: BaseExchange::new(
            name.clone(),
            options.clone(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            Default::default(),
        ),
        capabilities: Default::default(),
        accesses: RefCell::new(Vec::new()),
    });
    let wire = Arc::new(Wire {
        mode,
        now: now.clone(),
        requests: RefCell::new(Vec::new()),
        starts: RefCell::new(Vec::new()),
        probe_count: Cell::new(0),
    });
    let host = Rc::new(Host::default());
    let clock_fetches = Arc::new(AtomicUsize::new(0));
    let fetches = clock_fetches.clone();
    let epoch = now.clone();
    let mut clock = VirtualClock::default();
    clock.now = Arc::new(move || epoch.load(Ordering::SeqCst) as f64);
    clock.runtime = "browser".into();
    clock.venue_time_source = Some(Arc::new(move || {
        fetches.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Some(2000.0)) })
    }));
    let signer = signer_ms.map(|delay| {
        Rc::new(Signer {
            now: now.clone(),
            delay,
            calls: RefCell::new(Vec::new()),
        })
    });
    let ids = Arc::new(AtomicUsize::new(0));
    let owner_view: Rc<dyn Exchange> = owner.clone();
    let api = create_api(
        BybitApiOptions {
            http: HttpClientOptions {
                params_serializer: None,
                response_transform: None,
                exchange: name,
                options,
                request: RestRequest::default(),
                provider: None,
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host: host.clone(),
                runtime: runtime.clone(),
                store: None,
                stats: Some(Default::default()),
                logs: Some(Default::default()),
                every: Vec::new(),
                periodic: Vec::new(),
                next_id: Arc::new(move || format!("id-{}", ids.fetch_add(1, Ordering::SeqCst) + 1)),
                signing_now: Arc::new(|| 1000),
            },
            owner: Rc::downgrade(&owner_view),
            is_demo: false,
            rest_base_url: None,
            clock: clock.clone(),
            retry_clock: Rc::new(RetryClock(now)),
            payload_signer: signer.clone().map(|value| value as Rc<dyn PayloadSigner>),
        },
        &TransportRegistry::default(),
    )
    .unwrap();
    Fixture {
        api,
        owner,
        runtime,
        wire,
        host,
        clock,
        wakes,
        clock_fetches,
        signer,
    }
}
pub fn request() -> RestRequest {
    RestRequest {
        method: "get".into(),
        url: "/private".into(),
        ..Default::default()
    }
}
pub fn drive<F: Future>(future: F, runtime: &Runtime) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..100 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(&runtime.waker))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("factory boundary future did not settle")
}
pub fn source(script: &str, floor: usize) -> Value {
    let output = Command::new("node")
        .env(
            "SAFE_CEX_ROOT",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../.."),
        )
        .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source boundary oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["declaredCases"], floor);
    assert_eq!(value["cases"].as_array().unwrap().len(), floor);
    value
}
