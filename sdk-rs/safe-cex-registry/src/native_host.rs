//! The production `RustVenueHost`: venues share one native runtime and get a
//! fresh terminal HTTP adapter per construction. Requires a tokio `LocalSet`.
use crate::rust_venues::{
    BackpackDependencies, BitunixDependencies, BlofinDependencies, BlofinHttp, RustVenueHost,
};
use crate::{
    external_public_data::{
        ExternalPublicDataProvider, GatewayEvent, GatewayHost, GatewayListener, GatewaySocket,
        GatewayTimerCallback,
    },
    public_data_host::{MarketBatch, PublicDataParent, PublicDataUpdate, TickerBatch},
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    external::ExternalPublicData,
    runtime::{ExchangeTask, ListenerCleanup, LogSeverity, RemoteSigner},
    websocket::{WsEvent, WsSocket},
};
use safe_cex_exchange::{
    js_runtime::base_exchange::{ExternalPublicDataFactory, HostedSigner},
    native::native_runtime,
    runtime::ExchangeError,
    websocket::WsRuntime,
};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_transport::{RestAdapter, native::reqwest_adapter};
use safe_cex_types::{
    Candle, ExchangeOptionsExternalPublicData, OHLCVOptions, OrderBook, StoreDataLoaded,
};
use safe_cex_venue_binance::exchange_classes::ExchangeDependencies;
use safe_cex_venue_okx::binding::exchange::OkxDependencies;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    num::NonZeroU64,
    rc::Rc,
    sync::Arc,
};

pub type AdapterFactory = Rc<dyn Fn() -> Result<Arc<dyn RestAdapter>, ExchangeError>>;
pub use safe_cex_venue_hyperliquid::rate_limit::RateLimiter as HyperliquidRateLimiter;

/// Create once per native LocalSet and share across every account host.
#[allow(clippy::arc_with_non_send_sync)]
pub fn hyperliquid_rate_limiter() -> Rc<HyperliquidRateLimiter> {
    Rc::new(HyperliquidRateLimiter::new(
        Arc::new(safe_cex_venue_hyperliquid::binding::RuntimeClock::new(
            native_runtime(),
        )),
        1200.0,
        60_000.0,
    ))
}

pub struct NativeVenueHost {
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
    remote: Option<HostedSigner>,
    kraken_nonces: Rc<safe_cex_venue_kraken::api::FuturesModule>,
    kraken_spot_ids: Rc<RefCell<safe_cex_venue_kraken::binding::SpotIds>>,
    hyperliquid_limiter: Rc<HyperliquidRateLimiter>,
}
/// Direct transport and real HTTP, as `directTransport` is the TypeScript default.
pub fn native_venue_host(limiter: Rc<HyperliquidRateLimiter>) -> Rc<NativeVenueHost> {
    native_venue_host_with(
        native_runtime(),
        Rc::new(|| {
            reqwest_adapter()
                .map(|adapter| Arc::new(adapter) as Arc<dyn RestAdapter>)
                .map_err(|e| ExchangeError::new(e.to_string()))
        }),
        limiter,
    )
}
/// A host over a caller-chosen runtime and terminal adapter (another transport or a test origin).
pub fn native_venue_host_with(
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
    limiter: Rc<HyperliquidRateLimiter>,
) -> Rc<NativeVenueHost> {
    Rc::new(NativeVenueHost {
        runtime,
        adapter,
        remote: None,
        kraken_nonces: Rc::default(),
        kraken_spot_ids: kraken_spot_ids(),
        hyperliquid_limiter: limiter,
    })
}
/// One account's host whose requests sign through a hosted `remoteSigner`
/// (exchange-server's browser relay) under `account_name`; no secret is held.
pub fn native_venue_host_signed(
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
    signer: Rc<dyn RemoteSigner>,
    account_name: Option<String>,
    limiter: Rc<HyperliquidRateLimiter>,
) -> Rc<NativeVenueHost> {
    Rc::new(NativeVenueHost {
        runtime,
        adapter,
        remote: Some((signer, account_name)),
        kraken_nonces: Rc::default(),
        kraken_spot_ids: kraken_spot_ids(),
        hyperliquid_limiter: limiter,
    })
}
fn kraken_spot_ids() -> Rc<RefCell<safe_cex_venue_kraken::binding::SpotIds>> {
    safe_cex_venue_kraken::spot_client_ids(|bytes| {
        getrandom::getrandom(bytes).map_err(|_| safe_cex_types::js_coercion::JsCoercionError {
            name: "Error",
            message: "operating system entropy is unavailable",
        })
    })
}

impl RustVenueHost for NativeVenueHost {
    #[expect(clippy::arc_with_non_send_sync)]
    fn asterdex(
        &self,
    ) -> Result<safe_cex_venue_asterdex::native_exchange::AsterDEXDependencies, ExchangeError> {
        if self.remote.is_some() {
            return Err(ExchangeError::new(
                "remoteSigner is not wired for asterdex on the native host",
            ));
        }
        let runtime = self.runtime.clone();
        let adapter = self.adapter.clone();
        Ok(
            safe_cex_venue_asterdex::native_exchange::AsterDEXDependencies {
                runtime: self.runtime.clone(),
                http: Rc::new(move |options| {
                    if options.remote_signer.is_some() {
                        return Err(ExchangeError::new(
                            "remoteSigner is not wired for asterdex on the native host",
                        ));
                    }
                    Ok(safe_cex_exchange::js_runtime::client::rest_http_host(
                        runtime.clone(),
                        Arc::new(AsterRest {
                            runtime: runtime.clone(),
                            adapter: adapter.clone(),
                            options: options.clone(),
                        }),
                    ))
                }),
                external_public_data: Some(gateway_factory(self.runtime.clone())),
            },
        )
    }

    #[allow(clippy::arc_with_non_send_sync)]
    fn hyperliquid(
        &self,
        options: &safe_cex_types::ExchangeOptions,
    ) -> Result<safe_cex_venue_hyperliquid::binding::HyperliquidDependencies, ExchangeError> {
        let base_url = if options.testnet.unwrap_or(false) {
            "https://api.hyperliquid-testnet.xyz"
        } else {
            "https://api.hyperliquid.xyz"
        };
        let client = native_client(
            &self.runtime,
            &self.adapter,
            safe_cex_types::ExchangeName::Hyperliquid,
            options,
            safe_cex_transport::RestRequest {
                base_url: Some(base_url.into()),
                ..Default::default()
            },
            self.remote.clone(),
            Some(Arc::new(|params| {
                fn query(value: &serde_json::Value) -> safe_cex_types::QueryValue {
                    use safe_cex_types::QueryValue as Q;
                    match value {
                        serde_json::Value::Null => Q::Null,
                        serde_json::Value::Bool(value) => Q::Bool(*value),
                        serde_json::Value::Number(value) => {
                            Q::Number(value.as_f64().expect("JSON number"))
                        }
                        serde_json::Value::String(value) => Q::String(value.clone()),
                        serde_json::Value::Array(values) => {
                            Q::Array(values.iter().map(query).collect())
                        }
                        serde_json::Value::Object(values) => {
                            Q::Object(values.iter().map(|(k, v)| (k.clone(), query(v))).collect())
                        }
                    }
                }
                safe_cex_transport::query::render_query(
                    &query(params),
                    safe_cex_transport::query::QueryProfile {
                        arrays: safe_cex_transport::query::QueryArrayFormat::Repeat,
                        encode: true,
                        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                    },
                )
                .map_err(safe_cex_transport::TransportError::JavaScript)
            })),
        )?;
        if !options.public_data_only.unwrap_or(false) {
            safe_cex_exchange::http::install_transport_adapter(
                &client,
                &safe_cex_transport::TransportRegistry::default(),
                options,
                Some(&self.runtime.transport()),
                safe_cex_transport::RestAdapterVariant::Default,
            );
        }
        Ok(
            safe_cex_venue_hyperliquid::binding::HyperliquidDependencies {
                runtime: self.runtime.clone(),
                http: Arc::new(HyperliquidHttp(client)),
                limiter: self.hyperliquid_limiter.clone(),
                signers: safe_cex_exchange::runtime::ExchangeSigners {
                    remote_signer: self.remote.as_ref().map(|(signer, _)| signer.clone()),
                    payload_signer: None,
                },
            },
        )
    }
    fn hosted_signer(&self) -> Option<HostedSigner> {
        self.remote.clone()
    }

    fn kraken(&self) -> Result<safe_cex_venue_kraken::KrakenDependencies, ExchangeError> {
        if self.remote.is_some() {
            return Err(ExchangeError::new(
                "remoteSigner is not wired for kraken on the native host",
            ));
        }
        Ok(safe_cex_venue_kraken::KrakenDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host(
                self.runtime.clone(),
                (self.adapter)()?,
            ),
            external_public_data: Some(gateway_factory(self.runtime.clone())),
            journal: None,
            futures_nonces: self.kraken_nonces.clone(),
            spot_client_ids: Some(self.kraken_spot_ids.clone()),
        })
    }

    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError> {
        Ok(safe_cex_venue_binance::native_host::native_dependencies(
            self.runtime.clone(),
            (self.adapter)()?,
            gateway_factory(self.runtime.clone()),
            self.remote.clone(),
        ))
    }
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError> {
        if self.remote.is_some() {
            return Err(ExchangeError::new(
                "remoteSigner is not wired for backpack on the native host",
            ));
        }
        let inputs = safe_cex_venue_backpack::native_host::native_inputs(
            self.runtime.clone(),
            (self.adapter)()?,
        );
        Ok(BackpackDependencies {
            runtime: inputs.runtime,
            build_http: inputs.build_http,
            install_transport: inputs.install_transport,
            date_parse: inputs.date_parse,
        })
    }
    fn bitget(&self) -> Result<safe_cex_venue_bitget::BitgetDependencies, ExchangeError> {
        Ok(safe_cex_venue_bitget::BitgetDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host_signed(
                self.runtime.clone(),
                (self.adapter)()?,
                self.remote.clone(),
            ),
            external_public_data: None,
        })
    }
    fn bitunix(&self) -> Result<BitunixDependencies, ExchangeError> {
        // createAPI builds a lane's client, then installs its transport; the
        // install sees the options of the lane just built.
        let built = Rc::new(RefCell::new(None::<safe_cex_types::ExchangeOptions>));
        let (runtime, adapter, record) =
            (self.runtime.clone(), self.adapter.clone(), built.clone());
        let install_runtime = self.runtime.clone();
        let remote = self.remote.clone();
        Ok(BitunixDependencies {
            runtime: self.runtime.clone(),
            build: Rc::new(move |options| {
                *record.borrow_mut() = Some(options.clone());
                native_client(
                    &runtime,
                    &adapter,
                    safe_cex_types::ExchangeName::Bitunix,
                    options,
                    safe_cex_transport::RestRequest::default(),
                    remote.clone(),
                    None,
                )
            }),
            install: Rc::new(move |client| {
                let options = built.borrow().clone().ok_or_else(|| {
                    ExchangeError::new("bitunix transport install before its client was built")
                })?;
                safe_cex_exchange::http::install_transport_adapter(
                    client,
                    &safe_cex_transport::TransportRegistry::default(),
                    &options,
                    Some(&install_runtime.transport()),
                    safe_cex_transport::RestAdapterVariant::Default,
                );
                Ok(())
            }),
            external_public_data: Some(gateway_factory(self.runtime.clone())),
            remote_signer_account_name: self.remote.as_ref().and_then(|(_, name)| name.clone()),
        })
    }
    fn blofin(&self) -> Result<BlofinDependencies, ExchangeError> {
        Ok(BlofinDependencies {
            runtime: self.runtime.clone(),
            http: blofin_http(
                self.runtime.clone(),
                self.adapter.clone(),
                self.remote.clone(),
            ),
            external_public_data: Some(gateway_factory(self.runtime.clone())),
            remote_signer_account_name: self.remote.as_ref().and_then(|(_, name)| name.clone()),
        })
    }
    fn okx(&self) -> Result<OkxDependencies, ExchangeError> {
        Ok(OkxDependencies {
            runtime: self.runtime.clone(),
            http: safe_cex_exchange::js_runtime::client::rest_http_host_signed(
                self.runtime.clone(),
                (self.adapter)()?,
                self.remote.clone(),
            ),
            external_public_data: Some(gateway_factory(self.runtime.clone())),
        })
    }
}

// The source API factory supplies its base URL after choosing primary/secondary
// options. Route the final post-signing request through the shared HTTP stack.
struct AsterRest {
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
    options: safe_cex_types::ExchangeOptions,
}
impl RestAdapter for AsterRest {
    fn execute(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move {
            let client = native_client(
                &self.runtime,
                &self.adapter,
                safe_cex_types::ExchangeName::Asterdex,
                &self.options,
                request.clone(),
                None,
                None,
            )
            .map_err(|error| {
                safe_cex_transport::RestFailure::pre_transmit(error.message, request.clone())
            })?;
            safe_cex_exchange::http::install_transport_adapter(
                &client,
                &safe_cex_transport::TransportRegistry::default(),
                &self.options,
                Some(&self.runtime.transport()),
                safe_cex_transport::RestAdapterVariant::Default,
            );
            request.base_url = None;
            client.execute(request).await
        })
    }
}

// ApiClient supplies its source baseURL on every request. The configured client
// already resolved that baseURL through the selected regional/proxy transport;
// use its default so the source origin cannot overwrite the resolved route.
struct HyperliquidHttp(safe_cex_exchange::http::HttpClient);
impl RestAdapter for HyperliquidHttp {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move { self.execute_with_receipt(request).await.map(|r| r.response) })
    }
    fn execute_with_receipt(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        request.base_url = None;
        Box::pin(self.0.execute_with_receipt(request))
    }
}

/// safeCexAxiosCreate over a fresh terminal adapter; the venue's createAPI then
/// supplies its own baseURL, timeout and interceptors.
fn native_client(
    runtime: &Rc<dyn WsRuntime>,
    adapter: &AdapterFactory,
    exchange: safe_cex_types::ExchangeName,
    options: &safe_cex_types::ExchangeOptions,
    request: safe_cex_transport::RestRequest,
    remote: Option<HostedSigner>,
    params_serializer: Option<safe_cex_exchange::http::ParamsSerializer>,
) -> Result<safe_cex_exchange::http::HttpClient, ExchangeError> {
    let exchange_runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = runtime.clone();
    safe_cex_exchange::http::safe_cex_axios_create(
        safe_cex_exchange::http::HttpClientOptions {
            params_serializer,
            response_transform: None,
            exchange,
            options: options.clone(),
            request,
            provider: Some(runtime.transport()),
            signer: remote.as_ref().map(|(signer, _)| {
                safe_cex_exchange::js_runtime::client::transport_signer(signer.clone())
            }),
            account_name: remote.and_then(|(_, name)| name),
            adapter: adapter()?,
            host: Rc::new(safe_cex_exchange::native::NativeHttpHost),
            runtime: exchange_runtime,
            store: None,
            stats: None,
            logs: None,
            every: vec![],
            periodic: vec![],
            next_id: Arc::new(safe_cex_exchange::native::nanoid),
            signing_now: Arc::new(|| safe_cex_transport::native::wall_clock_ms() as u64),
        },
        &safe_cex_transport::TransportRegistry::default(),
    )
    .map_err(|e| ExchangeError::new(e.to_string()))
}

/// BloFin's common HTTP builder: (options, baseURL, timeout budget) -> adapter.
fn blofin_http(
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
    remote: Option<HostedSigner>,
) -> BlofinHttp {
    Rc::new(move |options, base_url, budget| {
        let request = safe_cex_transport::RestRequest {
            base_url: Some(base_url.to_owned()),
            timeout_ms: Some(budget),
            ..Default::default()
        };
        let client = native_client(
            &runtime,
            &adapter,
            safe_cex_types::ExchangeName::Blofin,
            options,
            request,
            remote.clone(),
            None,
        )
        .map_err(|e| e.message)?;
        Ok(Rc::new(client) as Rc<dyn RestAdapter>)
    })
}

/// ts: BaseExchange#initExternalPublicData. The websocket gateway runs the shared
/// provider over native sockets and timers; an in-process source cannot be named in
/// options (its capability type is uninhabited), so that variant fails visibly.
fn gateway_factory(runtime: Rc<dyn WsRuntime>) -> ExternalPublicDataFactory {
    Rc::new(move |options, base| match options {
        ExchangeOptionsExternalPublicData::Variant0(config) => {
            let host = Rc::new(NativeGatewayHost {
                runtime: runtime.clone(),
                next: Cell::new(0),
                timers: Rc::new(RefCell::new(HashMap::new())),
            });
            let provider = ExternalPublicDataProvider::new(
                Rc::new(StoreParent(base)),
                host,
                config.url.clone(),
                config.exchange_id.clone(),
                config.auth_token.clone(),
            );
            Rc::new(GatewayData {
                provider,
                runtime: runtime.clone(),
            }) as Rc<dyn ExternalPublicData>
        }
        ExchangeOptionsExternalPublicData::Variant1(_) => {
            Rc::new(InProcessUnavailable(runtime.clone()))
        }
    })
}

/// The exchange's Store as the provider parent. The gateway always hands over whole
/// batches, so an owned copy is current; in-process deltas would need live aliases.
struct StoreParent(Rc<BaseExchange>);
impl PublicDataParent for StoreParent {
    fn name(&self) -> String {
        serde_json::to_value(&self.0.name)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    fn now_ms(&self) -> f64 {
        self.0.runtime.now_ms()
    }
    fn loaded(&self) -> StoreDataLoaded {
        self.0.store.loaded().clone()
    }
    fn tickers(&self) -> TickerBatch {
        Rc::new(RefCell::new(self.0.store.tickers().to_vec()))
    }
    fn adapt_markets(&self, markets: MarketBatch) -> Result<MarketBatch, ExchangeError> {
        let adapted = self.0.adapt_external_markets(markets.borrow().clone());
        Ok(Rc::new(RefCell::new(adapted)))
    }
    fn update(&self, update: PublicDataUpdate) -> Result<(), ExchangeError> {
        let patch = StoreUpdate {
            tickers: update.tickers.map(|t| t.borrow().clone()),
            markets: update.markets.map(|m| m.borrow().clone()),
            loaded: update.loaded,
            ..Default::default()
        };
        self.0.store.update(patch, false).map_err(Into::into)
    }
    fn emit_log(&self, message: String, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.0.log(&message, severity.as_str())
    }
}

struct NativeGatewayHost {
    runtime: Rc<dyn WsRuntime>,
    next: Cell<u64>,
    timers: Rc<RefCell<HashMap<u64, Box<dyn ExchangeTask>>>>,
}
impl GatewayHost for NativeGatewayHost {
    fn create_socket(&self, url: &str) -> Result<Rc<dyn GatewaySocket>, ExchangeError> {
        Ok(dial_gateway(self.runtime.clone(), url.to_owned()))
    }
    fn reconnect_delay_ms(&self) -> f64 {
        self.runtime.min_reconnection_delay_ms()
    }
    fn set_timeout(
        &self,
        delay_ms: f64,
        callback: GatewayTimerCallback,
    ) -> Result<NonZeroU64, ExchangeError> {
        let id = self.next.get() + 1;
        self.next.set(id);
        let (runtime, timers) = (self.runtime.clone(), self.timers.clone());
        let task = self.runtime.spawn_local(Box::pin(async move {
            if runtime.sleep(delay_ms.max(0.0) as u64).await.is_err() {
                return;
            }
            timers.borrow_mut().remove(&id);
            if let Err(error) = callback() {
                runtime.report_exception(&error, &serde_json::json!({"operation":"gateway timer"}));
            }
        }));
        self.timers.borrow_mut().insert(id, task);
        Ok(NonZeroU64::new(id).expect("timer ids start at one"))
    }
    fn clear_timeout(&self, timer: NonZeroU64) -> Result<(), ExchangeError> {
        let task = self.timers.borrow_mut().remove(&timer.get());
        if let Some(task) = task {
            task.cancel();
        }
        Ok(())
    }
}

/// A gateway socket whose dial runs on the native runtime; listeners registered
/// right after creation see every event, as with `new WebSocket(url)`.
struct NativeGatewaySocket {
    state: Rc<Cell<u16>>,
    socket: Rc<RefCell<Option<Rc<dyn WsSocket>>>>,
    listeners: Rc<RefCell<Vec<(&'static str, GatewayListener)>>>,
    task: RefCell<Option<Box<dyn ExchangeTask>>>,
}
fn fire(
    runtime: &Rc<dyn WsRuntime>,
    listeners: &Rc<RefCell<Vec<(&'static str, GatewayListener)>>>,
    name: &str,
    event: impl Fn() -> GatewayEvent,
) {
    let targets: Vec<_> = listeners
        .borrow()
        .iter()
        .filter(|(n, _)| *n == name)
        .map(|(_, l)| l.clone())
        .collect();
    for listener in targets {
        if let Err(error) = listener(event()) {
            runtime.report_exception(
                &error,
                &serde_json::json!({"operation":"gateway socket event"}),
            );
        }
    }
}
fn dial_gateway(runtime: Rc<dyn WsRuntime>, url: String) -> Rc<NativeGatewaySocket> {
    let gateway = Rc::new(NativeGatewaySocket {
        state: Rc::new(Cell::new(0)),
        socket: Rc::new(RefCell::new(None)),
        listeners: Rc::new(RefCell::new(vec![])),
        task: RefCell::new(None),
    });
    let (state, slot, listeners, host) = (
        gateway.state.clone(),
        gateway.socket.clone(),
        gateway.listeners.clone(),
        runtime.clone(),
    );
    let task = runtime.spawn_local(Box::pin(async move {
        let socket = match host.dial(url, vec![], Default::default(), false).await {
            Ok(socket) => socket,
            Err(error) => {
                state.set(3);
                fire(&host, &listeners, "error", || {
                    GatewayEvent::Error(Some(error.message.clone()))
                });
                fire(&host, &listeners, "close", || GatewayEvent::Close);
                return;
            }
        };
        *slot.borrow_mut() = Some(socket.clone());
        loop {
            match socket.next_event().await {
                Ok(WsEvent::Open) => {
                    state.set(1);
                    fire(&host, &listeners, "open", || GatewayEvent::Open);
                }
                Ok(WsEvent::Message(SocketFrame::Text(text))) => {
                    fire(&host, &listeners, "message", || {
                        GatewayEvent::Message(text.clone())
                    })
                }
                Ok(WsEvent::Message(_)) => {}
                Ok(WsEvent::Error(message)) => fire(&host, &listeners, "error", || {
                    GatewayEvent::Error(Some(message.clone()))
                }),
                Ok(WsEvent::Close { .. }) | Err(_) => {
                    state.set(3);
                    fire(&host, &listeners, "close", || GatewayEvent::Close);
                    return;
                }
            }
        }
    }));
    *gateway.task.borrow_mut() = Some(task);
    gateway
}
impl GatewaySocket for NativeGatewaySocket {
    fn ready_state(&self) -> u16 {
        self.state.get()
    }
    fn add_listener(
        &self,
        event: &'static str,
        listener: GatewayListener,
    ) -> Result<(), ExchangeError> {
        self.listeners.borrow_mut().push((event, listener));
        Ok(())
    }
    fn send(&self, text: String) -> Result<(), ExchangeError> {
        let socket = self.socket.borrow().clone();
        match socket {
            Some(socket) if self.state.get() == 1 => socket.send(&text),
            _ => Err(ExchangeError::new("public data gateway socket is not open")),
        }
    }
    fn close(&self) -> Result<(), ExchangeError> {
        let socket = self.socket.borrow().clone();
        match socket {
            Some(socket) => socket.close(),
            None => {
                if let Some(task) = self.task.borrow_mut().take() {
                    task.cancel();
                }
                self.state.set(3);
            }
        }
        Ok(())
    }
}

/// The shared provider seen through the core's ExternalPublicData trait.
struct GatewayData {
    provider: Rc<ExternalPublicDataProvider>,
    runtime: Rc<dyn WsRuntime>,
}
fn report(runtime: &Rc<dyn WsRuntime>, result: Result<(), ExchangeError>, operation: &str) {
    if let Err(error) = result {
        runtime.report_exception(&error, &serde_json::json!({ "operation": operation }));
    }
}
impl ExternalPublicData for GatewayData {
    fn connect(&self) {
        report(
            &self.runtime,
            self.provider.connect(),
            "external public data connect",
        );
    }
    fn dispose(&self) {
        report(
            &self.runtime,
            self.provider.dispose(),
            "external public data dispose",
        );
    }
    fn is_initialized(&self) -> bool {
        self.provider.is_initialized()
    }
    fn is_healthy(&self) -> bool {
        self.provider.is_healthy()
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        self.provider.last_message_timestamp()
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self.provider.subscribe_candle(
            Rc::new(RefCell::new(options)),
            Rc::new(move |candle: &Candle| {
                callback(candle.clone());
                Ok(())
            }),
        )?;
        Ok(Box::new(move || cleanup()))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self.provider.subscribe_orderbook(
            symbol,
            Rc::new(move |book: &OrderBook| {
                callback(book.clone());
                Ok(())
            }),
        )?;
        Ok(Box::new(move || cleanup()))
    }
}

const NO_IN_PROCESS: &str =
    "in-process externalPublicData needs a host-registered source; the native host has none";
/// The in-process variant on a host with no source registry: every use fails visibly.
struct InProcessUnavailable(Rc<dyn WsRuntime>);
impl ExternalPublicData for InProcessUnavailable {
    fn connect(&self) {
        report(
            &self.0,
            Err(ExchangeError::new(NO_IN_PROCESS)),
            "external public data connect",
        );
    }
    fn dispose(&self) {}
    fn is_initialized(&self) -> bool {
        false
    }
    fn is_healthy(&self) -> bool {
        false
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        None
    }
    fn listen_ohlcv(
        &self,
        _: OHLCVOptions,
        _: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        Err(ExchangeError::new(NO_IN_PROCESS))
    }
    fn listen_order_book(
        &self,
        _: String,
        _: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        Err(ExchangeError::new(NO_IN_PROCESS))
    }
}
