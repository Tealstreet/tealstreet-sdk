//! The production `RustVenueHost`: venues share one native runtime and get a
//! fresh terminal HTTP adapter per construction. Requires a tokio `LocalSet`.
use crate::rust_venues::{BackpackDependencies, RustVenueHost};
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
    runtime::{ExchangeTask, ListenerCleanup, LogSeverity},
    websocket::{WsEvent, WsSocket},
};
use safe_cex_exchange::{native::native_runtime, runtime::ExchangeError, websocket::WsRuntime};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_transport::{RestAdapter, native::reqwest_adapter};
use safe_cex_types::{
    Candle, ExchangeOptionsExternalPublicData, OHLCVOptions, OrderBook, StoreDataLoaded,
};
use safe_cex_venue_binance::exchange_classes::ExchangeDependencies;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    num::NonZeroU64,
    rc::Rc,
    sync::Arc,
};

pub type AdapterFactory = Rc<dyn Fn() -> Result<Arc<dyn RestAdapter>, ExchangeError>>;
pub struct NativeVenueHost {
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
}
/// Direct transport and real HTTP, as `directTransport` is the TypeScript default.
pub fn native_venue_host() -> Rc<NativeVenueHost> {
    native_venue_host_with(
        native_runtime(),
        Rc::new(|| {
            reqwest_adapter()
                .map(|adapter| Arc::new(adapter) as Arc<dyn RestAdapter>)
                .map_err(|e| ExchangeError::new(e.to_string()))
        }),
    )
}
/// A host over a caller-chosen runtime and terminal adapter (another transport or a test origin).
pub fn native_venue_host_with(
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
) -> Rc<NativeVenueHost> {
    Rc::new(NativeVenueHost { runtime, adapter })
}
impl RustVenueHost for NativeVenueHost {
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError> {
        Ok(safe_cex_venue_binance::native_host::native_dependencies(
            self.runtime.clone(),
            (self.adapter)()?,
            gateway_factory(self.runtime.clone()),
        ))
    }
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError> {
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
}

type ExternalFactory =
    Rc<dyn Fn(&ExchangeOptionsExternalPublicData, Rc<BaseExchange>) -> Rc<dyn ExternalPublicData>>;
/// ts: BaseExchange#initExternalPublicData. The websocket gateway runs the shared
/// provider over native sockets and timers; an in-process source cannot be named in
/// options (its capability type is uninhabited), so that variant fails visibly.
fn gateway_factory(runtime: Rc<dyn WsRuntime>) -> ExternalFactory {
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
