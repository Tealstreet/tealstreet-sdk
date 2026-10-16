//! Rust venue construction through the registry table and the per-name path flag.
use safe_cex_exchange::{
    exchange::Exchange,
    http::{HttpClientOptions, HttpFuture, HttpHost, safe_cex_axios_create_no_proxy},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_registry::{
    module_host::{ExchangeModule, ModuleError, NativeModuleHost},
    rust_venues::{
        BackpackDependencies, ConstructionPath, RUST_VENUES, RustVenueHost, native_constructor,
    },
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_binance::{
    api_factory::RemoteSignCall,
    class_jobs::NativeFuture,
    exchange_classes::{ExchangeDependencies, ExchangeImports, HttpMethod},
    private_classes::{
        CapturedAction, NativePrivateSocket, PrivateBase, PrivateCallback, PrivateParent,
        PrivateTimers,
    },
    private_event_value::{EventValue, record, string},
    private_rest::{SnapshotCompletion, SnapshotRequestFuture},
    private_session::{Factory, Venue},
    protocol_types::BinanceOrderBookApplyResult,
    public_listeners::{self as listeners, Action, Book, ListenerHost, ListenerState, SyncState},
    request_weight::Failure,
};
use safe_cex_venue_okx::binding::exchange::OkxDependencies;
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

struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Job>>,
}
impl Runtime {
    fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        for (mut job, cancelled) in jobs {
            if !cancelled.get()
                && job
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            {
                self.jobs.borrow_mut().push((job, cancelled));
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1_735_689_600_000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let cancelled = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, cancelled.clone()));
        Box::new(Task(cancelled))
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "registry".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        panic!("unexpected exception report: {}", error.message)
    }
    fn report_raw_exception(&self, _: &Value, _: &Value) {
        panic!("unexpected raw exception report")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("no worker in registry fixture")) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn safe_cex_transport::TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        panic!("registry fixture has no tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        Box::pin(std::future::pending())
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
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
fn run<T>(runtime: &Runtime, future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    for _ in 0..16 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        runtime.pump();
    }
    panic!("registry fixture exceeded its 16-poll bound")
}

// Backpack HTTP: one scripted body per URL, recorded requests.
#[derive(Default)]
struct Wire {
    requests: Mutex<Vec<String>>,
    bodies: Mutex<BTreeMap<String, Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.url.clone());
        let body = self
            .bodies
            .lock()
            .unwrap()
            .iter()
            .find(|(path, _)| request.url.ends_with(path.as_str()))
            .map(|(_, body)| body.clone());
        Box::pin(async move {
            let body =
                body.ok_or_else(|| RestFailure::pre_transmit("unscripted URL".into(), request))?;
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(body),
            })
        })
    }
}
struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: RestFailure, _: &ExchangeName) -> RestFailure {
        error
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

// Binance imports: only what construction and getAccount reach is scripted.
struct NoSocket;
impl NativePrivateSocket for NoSocket {
    fn remove_listener(&self, _: &'static str, _: &PrivateCallback) -> Result<(), Failure> {
        Ok(())
    }
    fn ready_state(&self) -> Result<f64, Failure> {
        Ok(3.0)
    }
    fn close(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn add_listener(&self, _: &'static str, _: PrivateCallback) -> Result<(), Failure> {
        Ok(())
    }
}
#[derive(Default)]
struct Base {
    disposed: Cell<bool>,
}
impl PrivateBase for Base {
    fn disposed(&self) -> bool {
        self.disposed.get()
    }
    fn force_closed(&self) -> bool {
        false
    }
    fn connected(&self) -> bool {
        false
    }
    fn track(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn diagnostic(&self, _: EventValue) -> Result<(), Failure> {
        Ok(())
    }
    fn stats(&self, _: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn set_ping_at(&self, _: f64) -> Result<(), Failure> {
        Ok(())
    }
    fn diagnostic_send(&self, _: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn pong(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn send(&self, _: String) -> Result<(), Failure> {
        panic!("registry fixture sends no private frame")
    }
    fn create(&self, _: String) -> Result<Rc<dyn NativePrivateSocket>, Failure> {
        Ok(Rc::new(NoSocket))
    }
    fn on_close(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn force_close(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn reconnect(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn dispose(&self) -> Result<(), Failure> {
        self.disposed.set(true);
        Ok(())
    }
    fn emit_error(&self, message: String) -> Result<(), Failure> {
        panic!("unexpected private error: {message}")
    }
}
struct Timers;
impl PrivateTimers for Timers {
    fn set(&self, _: CapturedAction, _: f64) -> Result<EventValue, Failure> {
        Ok(record([("id", string("timer"))]))
    }
    fn clear(&self, _: EventValue) -> Result<(), Failure> {
        Ok(())
    }
}
#[derive(Default)]
struct Imports {
    requests: RefCell<Vec<String>>,
}
fn client(label: &str) -> EventValue {
    record([("label", string(label))])
}
impl ExchangeImports for Imports {
    fn factory(
        &self,
        _: Venue,
        kind: Factory,
        _: EventValue,
        _: EventValue,
    ) -> Result<EventValue, Failure> {
        Ok(client(match kind {
            Factory::API => "API",
            Factory::PAPI => "PAPI",
            Factory::FAPI => "FAPI",
            Factory::PublicAPI => "PublicAPI",
        }))
    }
    fn split(
        &self,
        _: Venue,
        _: EventValue,
        _: EventValue,
        _: Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>,
        _: Option<safe_cex_venue_binance::private_session::RateHook>,
    ) -> Result<EventValue, Failure> {
        Ok(EventValue::Array(Rc::new(RefCell::new(vec![
            client("route"),
            client("primary"),
            client("secondary"),
        ]))))
    }
    fn rate_limit(&self, client: EventValue, _: f64) -> Result<EventValue, Failure> {
        Ok(client)
    }
    fn request(
        &self,
        _: EventValue,
        _: HttpMethod,
        path: &'static str,
        _: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        self.requests.borrow_mut().push(path.into());
        let result = match path {
            "/fapi/v3/balance" => Ok(EventValue::from_json(
                json!([{"accountAlias": "registry-alias", "asset": "USDT"}]),
            )),
            _ => Err(Rc::new(ExchangeError::new(format!(
                "unscripted path {path}"
            )))),
        };
        Ok(Box::pin(std::future::ready(SnapshotCompletion {
            reaction_order: 0,
            result,
        })))
    }
    fn register_weight_guard(
        &self,
        _: EventValue,
        _: safe_cex_venue_binance::request_weight::Fulfilled<EventValue>,
        _: safe_cex_venue_binance::request_weight::Rejected<EventValue, Failure>,
    ) -> Result<(), Failure> {
        Ok(())
    }
    fn external_provider(
        &self,
        _: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        panic!("registry fixture has no external provider")
    }
    fn private_base(&self, _: Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure> {
        Ok(Rc::new(Base::default()))
    }
    fn listener_operations(
        &self,
    ) -> Rc<dyn safe_cex_venue_binance::public_listeners::ListenerHost> {
        Rc::new(Listeners::default())
    }
    fn timers(&self) -> Rc<dyn PrivateTimers> {
        Rc::new(Timers)
    }
    fn remote_sign(&self, _: EventValue) -> Result<RemoteSignCall, Failure> {
        panic!("registry fixture has no remote signer")
    }
    fn virtual_now(&self) -> f64 {
        1_735_689_600_000.0
    }
    fn uuid(&self) -> String {
        "registry".into()
    }
    fn dayjs(&self, value: Option<f64>) -> Result<f64, Failure> {
        Ok(value.unwrap_or(1_735_689_600_000.0))
    }
    fn subtract(&self, _: f64, _: f64, _: char) -> Result<f64, Failure> {
        panic!("registry fixture: subtract")
    }
    fn paginate(
        &self,
        _: safe_cex_venue_binance::rest_history::PageCall,
        _: EventValue,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        panic!("registry fixture: paginate")
    }
    fn iso(&self, _: EventValue) -> Result<String, Failure> {
        panic!("registry fixture: iso")
    }
    fn fold_closed_positions(
        &self,
        _: EventValue,
        _: bool,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        panic!("registry fixture: fold_closed_positions")
    }
    fn loop_timer(
        &self,
        _: safe_cex_venue_binance::lifecycle::TickTimer,
        _: EventValue,
    ) -> Result<(), Failure> {
        Ok(())
    }
}

// Public listeners: construction registers the host; no stream runs in this fixture.
#[derive(Default)]
struct Listeners {
    state: ListenerState,
}
impl ListenerHost for Listeners {
    fn state(&self) -> &ListenerState {
        &self.state
    }
    fn send(&self, _: EventValue) -> Result<(), Failure> {
        panic!("registry fixture: listener send")
    }
    fn set_timeout(&self, _: Action, _: f64) -> Result<EventValue, Failure> {
        panic!("registry fixture: listener set_timeout")
    }
    fn clear_timeout(&self, _: EventValue) -> Result<(), Failure> {
        Ok(())
    }
    fn cleanup(&self, _: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn nanoid(&self) -> Result<String, Failure> {
        Ok("registry".into())
    }
    fn get_snapshot(
        &self,
        _: listeners::Venue,
        _: &str,
        _: f64,
    ) -> Result<listeners::Snapshot, Failure> {
        panic!("registry fixture: listener get_snapshot")
    }
    fn spawn(&self, _: listeners::Job) {
        panic!("registry fixture: listener spawn")
    }
    fn reset_sync(&self, _: SyncState) -> Result<(), Failure> {
        panic!("registry fixture: listener reset_sync")
    }
    fn load(&self, _: Book, _: EventValue, _: SyncState) -> Result<(), Failure> {
        panic!("registry fixture: listener load")
    }
    fn buffered(&self, _: Vec<EventValue>, _: EventValue) -> Result<Vec<EventValue>, Failure> {
        panic!("registry fixture: listener buffered")
    }
    fn apply(
        &self,
        _: Book,
        _: EventValue,
        _: SyncState,
        _: bool,
    ) -> Result<BinanceOrderBookApplyResult, Failure> {
        panic!("registry fixture: listener apply")
    }
    fn total(&self, _: Book) -> Result<(), Failure> {
        panic!("registry fixture: listener total")
    }
}

struct Venues {
    runtime: Rc<Runtime>,
    wire: Arc<Wire>,
    imports: Rc<Imports>,
    calls: RefCell<Vec<&'static str>>,
}
impl RustVenueHost for Venues {
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError> {
        self.calls.borrow_mut().push("binance");
        Ok(ExchangeDependencies {
            runtime: self.runtime.clone(),
            imports: self.imports.clone(),
            signers: Default::default(),
            remote_signer_token: None,
            payload_signer_token: None,
        })
    }
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError> {
        self.calls.borrow_mut().push("backpack");
        let adapter = self.wire.clone();
        let runtime = self.runtime.clone();
        Ok(BackpackDependencies {
            runtime: self.runtime.clone(),
            build_http: Box::new(move |_, _| {
                Ok(safe_cex_axios_create_no_proxy(HttpClientOptions {
                    response_transform: None,
                    params_serializer: None,
                    exchange: ExchangeName::Backpack,
                    options: serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                    request: RestRequest::default(),
                    provider: None,
                    signer: None,
                    account_name: None,
                    adapter,
                    host: Rc::new(Host),
                    runtime,
                    store: None,
                    stats: None,
                    logs: None,
                    every: vec![],
                    periodic: vec![],
                    next_id: Arc::new(|| "id".into()),
                    signing_now: Arc::new(|| 0),
                }))
            }),
            install_transport: Rc::new(|_, _| Ok(())),
            date_parse: Rc::new(|_| Ok(1_735_689_600_000.0)),
        })
    }
    fn okx(&self) -> Result<OkxDependencies, ExchangeError> {
        self.calls.borrow_mut().push("okx");
        let runtime: Rc<dyn ExchangeRuntime> = self.runtime.clone();
        Ok(OkxDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host(runtime, self.wire.clone()),
            external_public_data: None,
        })
    }
}
fn venues() -> Rc<Venues> {
    Rc::new(Venues {
        runtime: Rc::new(Runtime::default()),
        wire: Arc::new(Wire::default()),
        imports: Rc::new(Imports::default()),
        calls: RefCell::default(),
    })
}

#[derive(Default)]
struct Modules {
    required: RefCell<Vec<&'static str>>,
}
impl NativeModuleHost for Modules {
    fn require_module(
        &self,
        specifier: &'static str,
    ) -> Result<Rc<dyn ExchangeModule>, ModuleError> {
        self.required.borrow_mut().push(specifier);
        Ok(Rc::new(NoExports))
    }
}
struct NoExports;
impl ExchangeModule for NoExports {
    fn constructor_export(
        &self,
        _: &'static str,
    ) -> Result<Option<safe_cex_registry::module_host::ExchangeConstructor>, ModuleError> {
        Ok(None)
    }
}

fn options(value: Value) -> ExchangeOptions {
    serde_json::from_value(value).unwrap()
}
fn construct(name: ExchangeName, host: &Rc<Venues>, opts: Value) -> Rc<dyn Exchange> {
    let modules = Modules::default();
    let ready = native_constructor(name.clone(), ConstructionPath::Rust, &modules, host.clone())
        .unwrap_or_else(|e| panic!("REGISTRY rust path for {name:?}: {e}"));
    assert!(
        modules.required.borrow().is_empty(),
        "REGISTRY rust path never consults the module host"
    );
    let constructor = run(&host.runtime, ready)
        .unwrap_or_else(|| panic!("REGISTRY rust path returns a constructor for {name:?}"));
    constructor
        .construct(name, options(opts), Rc::new(DefaultStore::new()))
        .unwrap_or_else(|e| panic!("REGISTRY construction: {}", e.message))
}

#[test]
fn table_members_are_exactly_the_bound_venues() {
    let names: Vec<_> = RUST_VENUES.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(
        names,
        [
            ExchangeName::Backpack,
            ExchangeName::Binance,
            ExchangeName::Binancepm,
            ExchangeName::Binancespot,
            ExchangeName::Okx,
            ExchangeName::Okxdemo,
            ExchangeName::Okxeu,
            ExchangeName::Okxeudemo,
        ],
        "REGISTRY table membership is a deliberate, reviewed list"
    );
}

#[test]
fn backpack_constructs_through_the_registry_and_reads_its_account() {
    let host = venues();
    let exchange = construct(
        ExchangeName::Backpack,
        &host,
        json!({"key":"","secret":"","publicDataOnly":true}),
    );
    assert_eq!(
        exchange.name(),
        &ExchangeName::Backpack,
        "REGISTRY backpack name"
    );
    assert_eq!(
        *host.calls.borrow(),
        ["backpack"],
        "REGISTRY backpack host inputs requested once"
    );
    let path = "/api/v1/account".to_string();
    host.wire
        .bodies
        .lock()
        .unwrap()
        .insert(path.clone(), json!({"accountId": "4242"}));
    let account = run(&host.runtime, exchange.get_account());
    let account = account.unwrap_or_else(|e| panic!("REGISTRY backpack getAccount: {}", e.message));
    assert_eq!(
        account.user_id, "4242",
        "REGISTRY backpack getAccount userId"
    );
    assert!(
        host.wire
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|u| u.ends_with(&path)),
        "REGISTRY backpack getAccount reached the scripted wire"
    );
}

#[test]
fn binance_constructs_through_the_registry_and_reads_its_account() {
    let host = venues();
    let exchange = construct(
        ExchangeName::Binance,
        &host,
        json!({"key":"key","secret":"secret"}),
    );
    assert_eq!(
        exchange.name(),
        &ExchangeName::Binance,
        "REGISTRY binance name"
    );
    assert_eq!(
        *host.calls.borrow(),
        ["binance"],
        "REGISTRY binance host inputs requested once"
    );
    let account = run(&host.runtime, exchange.get_account())
        .unwrap_or_else(|e| panic!("REGISTRY binance getAccount: {}", e.message));
    assert_eq!(
        account.user_id, "registry-alias",
        "REGISTRY binance getAccount userId"
    );
    assert_eq!(
        *host.imports.requests.borrow(),
        ["/fapi/v3/balance"],
        "REGISTRY binance getAccount request"
    );
}

#[test]
fn binance_pm_and_spot_construct_through_the_registry_on_the_binance_inputs() {
    for name in [ExchangeName::Binancepm, ExchangeName::Binancespot] {
        let host = venues();
        let exchange = construct(name.clone(), &host, json!({"key":"key","secret":"secret"}));
        assert_eq!(exchange.name(), &name, "REGISTRY binance family name");
        assert_eq!(
            *host.calls.borrow(),
            ["binance"],
            "REGISTRY binance family reuses the binance host inputs once"
        );
    }
}

#[test]
fn okx_family_constructs_through_the_registry_and_reads_its_markets() {
    let opts = json!({"key": "key", "secret": "secret", "passphrase": "pass"});
    for name in [
        ExchangeName::Okx,
        ExchangeName::Okxdemo,
        ExchangeName::Okxeu,
        ExchangeName::Okxeudemo,
    ] {
        let host = venues();
        let exchange = construct(name.clone(), &host, opts.clone());
        assert_eq!(exchange.name(), &name, "REGISTRY okx family name");
        assert_eq!(
            *host.calls.borrow(),
            ["okx"],
            "REGISTRY okx host inputs requested once"
        );
    }
    let host = venues();
    let exchange = construct(ExchangeName::Okx, &host, opts);
    host.wire.bodies.lock().unwrap().insert(
        "/api/v5/public/instruments".into(),
        json!({"code": "0", "msg": "", "data": [{
            "instId": "BTC-USDT-SWAP", "instType": "SWAP", "ctType": "linear", "ctVal": "0.01",
            "ctValCcy": "BTC", "settleCcy": "USDT", "uly": "BTC-USDT", "instFamily": "BTC-USDT",
            "baseCcy": "", "quoteCcy": "", "lotSz": "1", "tickSz": "0.1", "minSz": "1",
            "maxLmtSz": "100000", "maxMktSz": "5000", "maxStopSz": "5000",
            "maxTriggerSz": "100000", "maxTwapSz": "100000", "maxIcebergSz": "100000",
            "lever": "100", "state": "live"
        }]}),
    );
    let markets = run(&host.runtime, exchange.fetch_markets())
        .unwrap_or_else(|e| panic!("REGISTRY okx fetchMarkets: {}", e.message));
    assert_eq!(markets.len(), 1, "REGISTRY okx markets");
    assert_eq!(markets[0].symbol, "BTCUSDT", "REGISTRY okx market symbol");
    assert!(
        host.wire
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|u| u == "/api/v5/public/instruments"),
        "REGISTRY okx fetchMarkets reached the scripted wire"
    );
}

#[test]
fn rust_path_for_an_unbound_name_is_a_named_error_without_module_fallback() {
    let modules = Modules::default();
    let error = match native_constructor(
        ExchangeName::Bybit,
        ConstructionPath::Rust,
        &modules,
        venues(),
    ) {
        Ok(_) => panic!("REGISTRY unbound rust path must fail"),
        Err(error) => error,
    };
    assert_eq!(error.to_string(), "no Rust constructor for bybit");
    assert!(
        modules.required.borrow().is_empty(),
        "REGISTRY no module-host fallback"
    );
}

#[test]
fn module_host_path_is_the_default_and_unchanged() {
    assert_eq!(ConstructionPath::default(), ConstructionPath::ModuleHost);
    let modules = Modules::default();
    let host = venues();
    let ready = native_constructor(
        ExchangeName::Binance,
        ConstructionPath::default(),
        &modules,
        host.clone(),
    )
    .unwrap();
    assert!(
        run(&host.runtime, ready).is_none(),
        "REGISTRY module export lookup result"
    );
    assert_eq!(*modules.required.borrow(), ["./binance/binance.exchange"]);
    assert!(
        host.calls.borrow().is_empty(),
        "REGISTRY module path requests no Rust inputs"
    );
}
