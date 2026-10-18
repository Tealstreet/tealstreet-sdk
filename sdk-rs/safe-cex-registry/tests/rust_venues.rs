//! Rust venue construction through the registry table and the per-name path flag.
use safe_cex_exchange::{
    exchange::Exchange,
    http::{HttpClientOptions, HttpFuture, HttpHost, safe_cex_axios_create_no_proxy},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_registry::rust_venues::{BitunixDependencies, BlofinDependencies};
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
    private_event_value::{EventOps, EventValue, record, string},
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
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
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
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
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
    request_data: Mutex<Vec<RestRequest>>,
    bodies: Mutex<BTreeMap<String, Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.url.clone());
        self.request_data.lock().unwrap().push(request.clone());
        let body = self
            .bodies
            .lock()
            .unwrap()
            .iter()
            .find(|(path, _)| {
                let route = request.url.split('?').next().unwrap_or_default();
                route.ends_with(path.as_str())
            })
            .map(|(_, body)| body.clone());
        Box::pin(async move {
            let body =
                body.ok_or_else(|| RestFailure::pre_transmit("unscripted URL".into(), request))?;
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
/// The scripted wire behind an `Rc`, as BloFin's HTTP builder returns.
struct SharedWire(Arc<Wire>);
impl RestAdapter for SharedWire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.execute(request)
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
        Ok(EventValue::array(vec![
            client("route"),
            client("primary"),
            client("secondary"),
        ]))
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
    kraken_nonces: Rc<safe_cex_venue_kraken::api::FuturesModule>,
    kraken_spot_ids: Rc<RefCell<safe_cex_venue_kraken::binding::SpotIds>>,
}
impl RustVenueHost for Venues {
    fn lighter(
        &self,
    ) -> Result<safe_cex_venue_lighter::binding::exchange::LighterDependencies, ExchangeError> {
        self.calls.borrow_mut().push("lighter");
        let registry = Rc::new(safe_cex_transport::TransportRegistry::default());
        registry.set_default_transport(self.runtime.transport());
        Ok(
            safe_cex_venue_lighter::binding::exchange::LighterDependencies {
                runtime: self.runtime.clone(),
                http: safe_cex_exchange::js_runtime::client::rest_http_host(
                    self.runtime.clone(),
                    self.wire.clone(),
                ),
                registry,
                signing_entropy: None,
            },
        )
    }
    fn grvt(
        &self,
    ) -> Result<safe_cex_venue_grvt::binding::exchange::GrvtDependencies, ExchangeError> {
        self.calls.borrow_mut().push("grvt");
        Ok(safe_cex_venue_grvt::binding::exchange::GrvtDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host(
                self.runtime.clone(),
                self.wire.clone(),
            ),
            external_public_data: None,
        })
    }
    #[allow(clippy::arc_with_non_send_sync)]
    fn hyperliquid(
        &self,
        _: ExchangeName,
        _: &ExchangeOptions,
    ) -> Result<safe_cex_venue_hyperliquid::binding::HyperliquidDependencies, ExchangeError> {
        self.calls.borrow_mut().push("hyperliquid");
        Ok(
            safe_cex_venue_hyperliquid::binding::HyperliquidDependencies {
                runtime: self.runtime.clone(),
                external_public_data: None,
                http: self.wire.clone(),
                limiter: Rc::new(safe_cex_venue_hyperliquid::rate_limit::RateLimiter::new(
                    Arc::new(safe_cex_venue_hyperliquid::binding::RuntimeClock::new(
                        self.runtime.clone(),
                    )),
                    1200.0,
                    60_000.0,
                )),
                signers: Default::default(),
            },
        )
    }

    fn kraken(&self) -> Result<safe_cex_venue_kraken::KrakenDependencies, ExchangeError> {
        self.calls.borrow_mut().push("kraken");
        Ok(safe_cex_venue_kraken::KrakenDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host(
                self.runtime.clone(),
                self.wire.clone(),
            ),
            external_public_data: None,
            journal: None,
            futures_nonces: self.kraken_nonces.clone(),
            spot_client_ids: Some(self.kraken_spot_ids.clone()),
        })
    }

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
                    adapter: adapter.clone(),
                    host: Rc::new(Host),
                    runtime: runtime.clone(),
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
    fn bitget(&self) -> Result<safe_cex_venue_bitget::BitgetDependencies, ExchangeError> {
        self.calls.borrow_mut().push("bitget");
        let runtime: Rc<dyn ExchangeRuntime> = self.runtime.clone();
        Ok(safe_cex_venue_bitget::BitgetDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host(runtime, self.wire.clone()),
            external_public_data: None,
        })
    }
    fn bitunix(&self) -> Result<BitunixDependencies, ExchangeError> {
        self.calls.borrow_mut().push("bitunix");
        let wire = self.wire.clone();
        let runtime = self.runtime.clone();
        Ok(BitunixDependencies {
            runtime: self.runtime.clone(),
            build: Rc::new(move |options| {
                Ok(safe_cex_axios_create_no_proxy(HttpClientOptions {
                    response_transform: None,
                    params_serializer: None,
                    exchange: ExchangeName::Bitunix,
                    options: options.clone(),
                    request: RestRequest::default(),
                    provider: None,
                    signer: None,
                    account_name: None,
                    adapter: wire.clone(),
                    host: Rc::new(Host),
                    runtime: runtime.clone(),
                    store: None,
                    stats: None,
                    logs: None,
                    every: vec![],
                    periodic: vec![],
                    next_id: Arc::new(|| "id".into()),
                    signing_now: Arc::new(|| 0),
                }))
            }),
            install: Rc::new(|_| Ok(())),
            external_public_data: None,
            remote_signer_account_name: None,
        })
    }
    fn blofin(&self) -> Result<BlofinDependencies, ExchangeError> {
        self.calls.borrow_mut().push("blofin");
        let wire = self.wire.clone();
        Ok(BlofinDependencies {
            runtime: self.runtime.clone(),
            http: Rc::new(move |_, base_url, budget| {
                assert_eq!(
                    base_url, "https://openapi.blofin.com",
                    "REGISTRY blofin REST base"
                );
                assert_eq!(budget, 20_000, "REGISTRY blofin timeout budget");
                Ok(Rc::new(SharedWire(wire.clone())) as Rc<dyn RestAdapter>)
            }),
            external_public_data: None,
            remote_signer_account_name: None,
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
    fn polymarket(
        &self,
    ) -> Result<safe_cex_venue_polymarket::binding::exchange::PolymarketDependencies, ExchangeError>
    {
        self.calls.borrow_mut().push("polymarket");
        Ok(
            safe_cex_venue_polymarket::binding::exchange::PolymarketDependencies {
                runtime: self.runtime.clone(),
                http: safe_cex_exchange::js_runtime::client::rest_http_host(
                    self.runtime.clone(),
                    self.wire.clone(),
                ),
                external_public_data: None,
            },
        )
    }
}
fn venues() -> Rc<Venues> {
    Rc::new(Venues {
        runtime: Rc::new(Runtime::default()),
        wire: Arc::new(Wire::default()),
        imports: Rc::new(Imports::default()),
        calls: RefCell::default(),
        kraken_nonces: Rc::default(),
        kraken_spot_ids: safe_cex_venue_kraken::spot_client_ids(|bytes| {
            bytes.fill(7);
            Ok(())
        }),
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
            ExchangeName::Apexomni,
            ExchangeName::Asterdex,
            ExchangeName::Backpack,
            ExchangeName::Binance,
            ExchangeName::Binancepm,
            ExchangeName::Binancespot,
            // BingX is now assembled on js_runtime and tested on the native host.
            ExchangeName::Bingx,
            ExchangeName::Bitget,
            ExchangeName::Bitgetinverse,
            ExchangeName::Bitgetusdc,
            ExchangeName::Bitrue,
            ExchangeName::Bitunix,
            ExchangeName::Blofin,
            ExchangeName::Breakoutprop,
            ExchangeName::Grvt,
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
            ExchangeName::Hyrotrader,
            ExchangeName::Coinbase,
            ExchangeName::Coinbasespot,
            ExchangeName::Cryptocom,
            ExchangeName::Hyperliquid,
            ExchangeName::Hyperliquidspot,
            ExchangeName::Kraken,
            ExchangeName::Krakenspot,
            ExchangeName::Kodiakfi,
            ExchangeName::Mode,
            ExchangeName::Salsadex,
            ExchangeName::TealstreetDex,
            ExchangeName::What,
            ExchangeName::Woofi,
            ExchangeName::Lighter,
            ExchangeName::LighterRobinhood,
            ExchangeName::Kucoin,
            ExchangeName::Kucoinspot,
            ExchangeName::Mexc,
            ExchangeName::Okx,
            ExchangeName::Okxdemo,
            ExchangeName::Okxeu,
            ExchangeName::Okxeudemo,
            ExchangeName::Paradex,
            ExchangeName::Phemex,
            ExchangeName::Polymarket,
            ExchangeName::Txflow,
            ExchangeName::Woo,
            ExchangeName::Weex,
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
fn bitget_family_constructs_through_the_registry_and_reads_its_account() {
    let opts = json!({"key":"k","secret":"s","passphrase":"p"});
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetinverse,
        ExchangeName::Bitgetusdc,
    ] {
        let host = venues();
        let exchange = construct(name.clone(), &host, opts.clone());
        assert_eq!(exchange.name(), &name, "REGISTRY bitget family name");
        assert_eq!(
            *host.calls.borrow(),
            ["bitget"],
            "REGISTRY bitget host inputs requested once"
        );
    }
    let host = venues();
    let exchange = construct(ExchangeName::Bitget, &host, opts);
    host.wire.bodies.lock().unwrap().insert(
        "/api/v2/spot/account/info".into(),
        json!({"code":"00000","msg":"success","requestTime":1,"data":{"userId":"777","inviterId":"9","parentId":null}}),
    );
    let account = run(&host.runtime, exchange.get_account())
        .unwrap_or_else(|e| panic!("REGISTRY bitget getAccount: {}", e.message));
    assert_eq!(account.user_id, "777", "REGISTRY bitget getAccount userId");
}

#[test]
fn bitunix_constructs_through_the_registry_and_reads_its_markets() {
    let host = venues();
    let exchange = construct(
        ExchangeName::Bitunix,
        &host,
        json!({"key":"k","secret":"s","publicDataOnly":true}),
    );
    assert_eq!(
        exchange.name(),
        &ExchangeName::Bitunix,
        "REGISTRY bitunix name"
    );
    assert_eq!(
        *host.calls.borrow(),
        ["bitunix"],
        "REGISTRY bitunix host inputs requested once"
    );
    host.wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/market/trading_pairs".into(),
        json!({"code":0,"msg":"Success","data":[{
            "symbol":"BTCUSDT","base":"BTC","quote":"USDT","minTradeVolume":"0.0001",
            "maxLimitOrderVolume":"500","maxMarketOrderVolume":"300","basePrecision":3,
            "quotePrecision":1,"maxLeverage":125,"minLeverage":1,"defaultLeverage":20,
            "defaultMarginMode":"2","symbolStatus":"OPEN"
        }]}),
    );
    let markets = run(&host.runtime, exchange.fetch_markets())
        .unwrap_or_else(|e| panic!("REGISTRY bitunix fetchMarkets: {}", e.message));
    assert_eq!(markets.len(), 1, "REGISTRY bitunix markets");
    assert_eq!(markets[0].id, "BTC/USDT", "REGISTRY bitunix market id");
}

#[test]
fn blofin_constructs_through_the_registry_and_reads_its_markets() {
    let host = venues();
    let exchange = construct(
        ExchangeName::Blofin,
        &host,
        json!({"key":"k","secret":"s","passphrase":"p"}),
    );
    assert_eq!(
        exchange.name(),
        &ExchangeName::Blofin,
        "REGISTRY blofin name"
    );
    assert_eq!(
        *host.calls.borrow(),
        ["blofin"],
        "REGISTRY blofin host inputs requested once"
    );
    host.wire.bodies.lock().unwrap().insert(
        "/api/v1/market/instruments".into(),
        json!({"code":"0","msg":"success","data":[{
            "instId":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","contractValue":"0.001",
            "listTime":"1700000000000","expireTime":"0","maxLeverage":"150","minSize":"0.1",
            "lotSize":"0.1","tickSize":"0.1","instType":"SWAP","contractType":"linear",
            "maxLimitSize":"100000","maxMarketSize":"10000","state":"live"
        }]}),
    );
    let markets = run(&host.runtime, exchange.fetch_markets())
        .unwrap_or_else(|e| panic!("REGISTRY blofin fetchMarkets: {}", e.message));
    assert_eq!(markets.len(), 1, "REGISTRY blofin markets");
    assert_eq!(
        markets[0].symbol, "BTCUSDT",
        "REGISTRY blofin market symbol"
    );
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
            // fetchMarkets supplies params: {instType}; the HTTP host puts them in the URL.
            .any(|u| u == "/api/v5/public/instruments?instType=SWAP"),
        "REGISTRY okx fetchMarkets reached the scripted wire"
    );
}

#[test]
fn unknown_name_is_rejected_before_registry_or_module_fallback() {
    let modules = Modules::default();
    let host = venues();
    // ExchangeName is closed: a synthetic name fails at typed admission before
    // either constructor route. Real venue additions cannot invalidate this gate.
    let result = serde_json::from_value::<ExchangeName>(json!("__unbound_registry_control__"))
        .map(|name| native_constructor(name, ConstructionPath::Rust, &modules, host.clone()));
    let error = result.err().expect("unknown registry name must fail");
    assert!(
        error
            .to_string()
            .contains("unknown variant `__unbound_registry_control__`")
    );
    assert!(
        modules.required.borrow().is_empty(),
        "REGISTRY no module-host fallback"
    );
    assert!(
        host.calls.borrow().is_empty(),
        "REGISTRY no venue inputs requested"
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

#[test]
fn kraken_family_constructs_through_the_registry_and_reads_its_markets() {
    for (name, path, body, symbol, base) in [
        (
            ExchangeName::Kraken,
            "/v3/instruments",
            json!({"result":"success","instruments":[{
                "symbol":"PF_XBTUSD","tradeable":true,"base":"BTC","quote":"USD",
                "contractValueTradePrecision":4,"tickSize":0.5,"contractSize":1,
                "marginLevels":[{"initialMargin":0.02}],"contractType":"flexible_futures","status":"open"
            }]}),
            "PF_XBTUSD",
            "BTC",
        ),
        (
            ExchangeName::Krakenspot,
            "/0/public/AssetPairs",
            json!({"error":[],"result":{"XXBTZUSD":{
                "wsname":"XBT/USD","pair_decimals":1,"lot_decimals":8,"ordermin":"0.0001",
                "status":"online","altname":"XBTUSD"
            }}}),
            "XBT/USD",
            "BTC",
        ),
    ] {
        let host = venues();
        host.wire.bodies.lock().unwrap().insert(path.into(), body);
        let exchange = construct(
            name.clone(),
            &host,
            json!({"key":"","secret":"","publicDataOnly":true}),
        );
        assert_eq!(exchange.name(), &name, "REGISTRY Kraken family name");
        assert_eq!(
            *host.calls.borrow(),
            ["kraken"],
            "REGISTRY Kraken host requested once"
        );
        let markets =
            run(&host.runtime, exchange.fetch_markets()).expect("REGISTRY Kraken markets read");
        assert_eq!(markets.len(), 1, "REGISTRY Kraken market count");
        assert_eq!(markets[0].symbol, symbol, "REGISTRY Kraken market symbol");
        assert_eq!(markets[0].base, base, "REGISTRY Kraken normalized base");
        assert_eq!(
            *host.wire.requests.lock().unwrap(),
            [path],
            "REGISTRY Kraken request"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_kraken_dependencies_retain_module_state_per_host() {
    let host = safe_cex_registry::native_host::native_venue_host(
        safe_cex_registry::native_host::hyperliquid_rate_limiter(),
    );
    let first = host.kraken().unwrap();
    let second = host.kraken().unwrap();
    assert!(
        Rc::ptr_eq(&first.futures_nonces, &second.futures_nonces),
        "REGISTRY Kraken module nonce identity"
    );
    assert!(
        Rc::ptr_eq(
            first.spot_client_ids.as_ref().unwrap(),
            second.spot_client_ids.as_ref().unwrap()
        ),
        "REGISTRY Kraken module client ID identity"
    );
    assert!(
        first.external_public_data.is_some(),
        "REGISTRY Kraken native gateway factory"
    );
    let other = safe_cex_registry::native_host::native_venue_host(
        safe_cex_registry::native_host::hyperliquid_rate_limiter(),
    )
    .kraken()
    .unwrap();
    assert!(
        !Rc::ptr_eq(&first.futures_nonces, &other.futures_nonces),
        "REGISTRY separate native runtimes have separate module state"
    );
}

#[test]
fn lighter_family_constructs_through_registry_and_reads_deployment_markets() {
    let host = venues();
    host.wire.bodies.lock().unwrap().insert("/api/v1/orderBookDetails".into(),json!({"code":200,"order_book_details":[{"symbol":"BTC","market_id":1,"market_type":"perp","status":"active","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"10","order_quote_limit":"1000","min_initial_margin_fraction":200,"market_config":{"market_margin_mode":0}}]}));
    for (name, quote) in [
        (ExchangeName::Lighter, "USDC"),
        (ExchangeName::LighterRobinhood, "USDG"),
    ] {
        let exchange = construct(
            name.clone(),
            &host,
            json!({"key":"","secret":"01000000000000000000000000000000000000000000000000000000000000000000000000000000","apiKeyIndex":"3","applicationId":"42"}),
        );
        let markets =
            run(&host.runtime, exchange.fetch_markets()).expect("Lighter registry market read");
        assert_eq!(exchange.name(), &name);
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].symbol, "BTC");
        assert_eq!(markets[0].quote, quote);
        assert_eq!(markets[0].precision.amount, 0.0001);
        exchange.dispose().unwrap();
    }
    assert_eq!(*host.calls.borrow(), ["lighter", "lighter"]);
    assert_eq!(
        *host.wire.requests.lock().unwrap(),
        ["/api/v1/orderBookDetails", "/api/v1/orderBookDetails"]
    );
}

#[test]
fn grvt_constructs_through_registry_and_reads_actual_public_gateway() {
    let host = venues();
    host.wire.bodies.lock().unwrap().insert("/lite/v1/all_instruments".into(),json!({"r":[{"i":"BTC_USDT_Perp","k":"PERPETUAL","b":"BTC","q":"USDT","ms":"0.001","ts":"0.1","ih":"0x030501","bd":3,"qd":6}]}));
    let exchange = construct(ExchangeName::Grvt, &host, json!({"key":"","secret":""}));
    let markets = run(&host.runtime, exchange.fetch_markets()).expect("GRVT registry request");
    assert_eq!(exchange.name(), &ExchangeName::Grvt);
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "BTC_USDT_PERP");
    assert_eq!(*host.calls.borrow(), ["grvt"]);
    assert_eq!(
        host.wire.request_data.lock().unwrap()[0]
            .base_url
            .as_deref(),
        Some("https://market-data.grvt.io")
    );
    assert_eq!(
        host.wire.requests.lock().unwrap()[0],
        "/lite/v1/all_instruments"
    );
}

#[test]
fn polymarket_constructs_through_the_registry_and_calls_live_methods() {
    let host = venues();
    let exchange = construct(
        ExchangeName::Polymarket,
        &host,
        json!({"key":"","secret":"","publicDataOnly":true}),
    );
    assert_eq!(*exchange.base().name(), ExchangeName::Polymarket);
    let account = run(&host.runtime, exchange.get_account()).unwrap();
    assert_eq!(
        account.user_id, "unknown",
        "REGISTRY Polymarket account source body"
    );
    assert_eq!(
        run(
            &host.runtime,
            exchange.fetch_symbol_max_leverage("example-Yes".into())
        )
        .unwrap(),
        1.
    );
    assert_eq!(
        &*host.calls.borrow(),
        &["polymarket"],
        "REGISTRY requests venue inputs exactly once"
    );
}
