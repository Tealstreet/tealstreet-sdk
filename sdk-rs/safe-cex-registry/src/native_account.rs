//! One native account behind an owned structured-value surface for the napi host.
//! Failures retain `{name, message, code?}`. Requires a tokio LocalSet.
use crate::native_host::{bind_http_events, native_venue_host_signed, native_venue_host_with};
use crate::native_store::NativeStoreDelivery;
use crate::native_value::NativeValue;
use crate::rust_venues::rust_constructor;
use safe_cex_exchange::{
    client_order_id::ClientOrderLookup,
    exchange::Exchange,
    js_runtime::value::{Js, Thrown},
    native::{ProxyTokenSource, native_runtime, native_tealstreet_runtime},
    runtime::{Cleanup, ExchangeError, ExchangeRuntime, ListenerCleanup, RemoteSigner},
    throttle::{ThrottleManager, symbol_metadata_throttler},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportFuture,
    native::reqwest_adapter,
    query_signing::{HttpSigningRequest, HttpSigningResponse},
    signing::{RemoteSignerRequest, RemoteSignerResponse},
};
use safe_cex_types::{ExchangeName, ExchangeOptions, PlaceOrderOpts};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::Arc,
};

pub type NativeAccountFuture<T> = Pin<Box<dyn Future<Output = Result<T, NativeValue>>>>;

// The JSON command surface admits object records. Flattened ignored fields keep
// its existing surplus-field behavior while refusing Serde positional structs.
#[derive(serde::Deserialize)]
struct NativeOrderId {
    id: String,
    #[serde(flatten)]
    _extra: BTreeMap<String, Value>,
}
#[derive(serde::Deserialize)]
struct NativeHistoricalPositionOptions {
    account: String,
    from: Option<f64>,
    to: Option<f64>,
    #[serde(flatten)]
    _extra: BTreeMap<String, Value>,
}
/// `sign` answers one remote-signer request (ts: RemoteSigner.sign); `proxy_token`
/// selects the Tealstreet transport (ts: registerTealstreetTransport + accessToken).
pub struct NativeAccountSpec {
    /// Shared by every account on the caller's LocalSet, including hosted ones.
    pub hyperliquid_limiter: Rc<crate::native_host::HyperliquidRateLimiter>,
    pub exchange: String,
    pub options: NativeValue,
    pub account_name: Option<String>,
    pub sign: Option<Rc<dyn Fn(NativeValue) -> NativeAccountFuture<NativeValue>>>,
    pub proxy_token: Option<Rc<dyn Fn() -> NativeAccountFuture<String>>>,
}
pub struct NativeAccount {
    replay_runtime: Option<Rc<safe_cex_exchange::native::NativeRuntime>>,
    exchange: Rc<dyn Exchange>,
    store: Rc<dyn Store>,
    signer: Option<Rc<HostSigner>>,
    streams: RefCell<BTreeMap<u32, NativeStream>>,
    next_stream: Cell<u32>,
    startup_token: Cell<u64>,
    symbol_metadata_throttler: Rc<ThrottleManager<String>>,
    store_deliveries: Rc<RefCell<Vec<Weak<NativeStoreDelivery>>>>,
    stream_listeners: Rc<RefCell<Vec<Rc<dyn Fn(&str, &NativeValue)>>>>,
}

struct NativeStream {
    active: Rc<Cell<bool>>,
    cleanup: Option<ListenerCleanup>,
}
impl NativeAccount {
    pub fn exchange_name(&self) -> ExchangeName {
        self.exchange.base().name.clone()
    }
    /// Invalidate startup delivery before an asynchronous host disposal is polled.
    pub fn cancel_startup(&self) {
        self.startup_token
            .set(self.startup_token.get().wrapping_add(1));
    }
    fn dispose_streams(&self) -> Result<(), ExchangeError> {
        let streams = std::mem::take(&mut *self.streams.borrow_mut());
        // Stop every host callback before invoking any potentially reentrant cleanup.
        for stream in streams.values() {
            stream.active.set(false);
        }
        let mut first_error = None;
        for stream in streams.into_values() {
            if let Some(cleanup) = stream.cleanup
                && let Err(error) = cleanup()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

type AvailabilityListeners = Rc<RefCell<Vec<(u64, Rc<dyn Fn(bool)>)>>>;
/// ts: the hosted RemoteSigner; the host pushes availability changes.
struct HostSigner {
    sign: Rc<dyn Fn(NativeValue) -> NativeAccountFuture<NativeValue>>,
    available: Cell<bool>,
    listeners: AvailabilityListeners,
    next: Cell<u64>,
}
fn error_text(error: &NativeValue) -> String {
    match error.get("message") {
        Some(NativeValue::String(message)) => String::from_utf16_lossy(message),
        _ => format!("{error:?}"),
    }
}
fn callback_error(error: NativeValue) -> TransportError {
    TransportError::Rejected(safe_cex_transport::provider::TransportErrorOrigin::new(
        safe_cex_exchange::js_runtime::value::owned::OwnedCallbackError::new(error),
    ))
}

impl safe_cex_transport::signing::RemoteSigner for HostSigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        let sign = self.sign.clone();
        Box::pin(async move {
            let wire =
                NativeValue::encode(&request).map_err(|e| TransportError::Host(e.to_string()))?;
            let response = sign(wire).await.map_err(callback_error)?;
            match request {
                RemoteSignerRequest::Http(_) => response.decode().map(RemoteSignerResponse::Http),
                RemoteSignerRequest::WsAuth { .. } => {
                    response.decode().map(RemoteSignerResponse::WsAuth)
                }
            }
            .map_err(|error| TransportError::Host(error.to_string()))
        })
    }
    // ts: createRemoteSigningAdapter signs every config, query params included.
    fn sign_query(&self, request: HttpSigningRequest) -> TransportFuture<'_, HttpSigningResponse> {
        let sign = self.sign.clone();
        Box::pin(async move {
            let mut wire =
                NativeValue::encode(&request).map_err(|e| TransportError::Host(e.to_string()))?;
            if let NativeValue::Object(fields) = &mut wire {
                fields.insert(
                    0,
                    ("type".encode_utf16().collect(), NativeValue::string("http")),
                );
            } else {
                return Err(TransportError::Host(
                    "signing request must be an object".into(),
                ));
            }
            let response = sign(wire).await.map_err(callback_error)?;
            response
                .decode()
                .map_err(|error| TransportError::Host(error.to_string()))
        })
    }
}
impl RemoteSigner for HostSigner {
    fn is_available(&self) -> bool {
        self.available.get()
    }
    fn on_availability_change(&self, callback: Rc<dyn Fn(bool)>) -> Cleanup {
        let id = self.next.get();
        self.next.set(id + 1);
        self.listeners.borrow_mut().push((id, callback));
        let listeners = Rc::downgrade(&self.listeners);
        Box::new(move || {
            if let Some(listeners) = listeners.upgrade() {
                listeners.borrow_mut().retain(|(row, _)| *row != id);
            }
        })
    }
}

fn exchange_error(error: &ExchangeError) -> NativeValue {
    let projected = Thrown::from(error.clone()).value;
    let mut fields = Vec::with_capacity(3);
    for field in ["name", "message"] {
        let value = match projected.get(field) {
            Ok(value) => value,
            Err(cause) => {
                return host_error(format!("cannot read native error {field}: {cause:?}"));
            }
        };
        match NativeValue::from_js(&value) {
            Ok(value) => fields.push((field, value)),
            Err(cause) => {
                return host_error(format!("cannot retain native error {field}: {cause}"));
            }
        }
    }
    if let Some(code) = &error.code {
        match NativeValue::from_js(code) {
            Ok(code) => fields.push(("code", code)),
            Err(cause) => return host_error(format!("cannot retain native error code: {cause}")),
        }
    }
    NativeValue::object(fields)
}
fn host_error(message: impl Into<String>) -> NativeValue {
    NativeValue::object([
        ("name", NativeValue::string("Error")),
        ("message", NativeValue::string(&message.into())),
    ])
}

/// Build an account through its bound registry Rust constructor.
pub fn open_native_account(spec: NativeAccountSpec) -> Result<Rc<NativeAccount>, NativeValue> {
    open_native_account_with_replay(spec, None)
}

/// Replay is an explicit diagnostic host selection, never an exchange option.
pub fn open_native_account_with_replay(
    spec: NativeAccountSpec,
    epoch: Option<f64>,
) -> Result<Rc<NativeAccount>, NativeValue> {
    let name: ExchangeName = serde_json::from_value(Value::String(spec.exchange.clone()))
        .map_err(|e| host_error(e.to_string()))?;
    let options: ExchangeOptions = spec
        .options
        .decode()
        .map_err(|e| host_error(e.to_string()))?;
    if epoch.is_some()
        && (spec.sign.is_some()
            || spec.proxy_token.is_some()
            || options.public_data_only != Some(true)
            || !options.key.is_empty()
            || !options.secret.is_empty())
    {
        return Err(host_error(
            "replay requires a keyless direct public-data account",
        ));
    }
    if epoch.is_some_and(|epoch| {
        !epoch.is_finite() || epoch < 0.0 || epoch.fract() != 0.0 || epoch > 8.64e15
    }) {
        return Err(host_error("invalid replay epoch"));
    }
    let replay_runtime = epoch.map(safe_cex_exchange::native::native_replay_runtime);
    let runtime: Rc<dyn WsRuntime> = if let Some(runtime) = &replay_runtime {
        runtime.clone()
    } else {
        match spec.proxy_token {
            Some(token) => {
                let source: ProxyTokenSource = Rc::new(move || {
                    let pending = token();
                    Box::pin(async move {
                        pending
                            .await
                            .map_err(|error| TransportError::Host(error_text(&error)))
                    })
                });
                native_tealstreet_runtime(source)
            }
            None => native_runtime(),
        }
    };
    let adapters = Rc::new(|| {
        reqwest_adapter()
            .map(|adapter| Arc::new(adapter) as Arc<dyn RestAdapter>)
            .map_err(|e| ExchangeError::new(e.to_string()))
    });
    let signer = spec.sign.map(|sign| {
        Rc::new(HostSigner {
            sign,
            available: Cell::new(true),
            listeners: Rc::default(),
            next: Cell::new(0),
        })
    });
    let host = match &signer {
        Some(signer) => native_venue_host_signed(
            runtime,
            adapters,
            signer.clone() as Rc<dyn RemoteSigner>,
            spec.account_name,
            spec.hyperliquid_limiter,
        ),
        None => native_venue_host_with(runtime, adapters, spec.hyperliquid_limiter),
    };
    let constructor = rust_constructor(&name, host.clone())
        .ok_or_else(|| host_error(format!("no Rust constructor for {}", spec.exchange)))?;
    let store: Rc<dyn Store> = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
    let exchange = constructor
        .construct(name, options, store.clone())
        .map_err(|error| exchange_error(&error))?;
    bind_http_events(&host, &exchange);
    let symbol_metadata_throttler = symbol_metadata_throttler(exchange.clone());
    Ok(Rc::new(NativeAccount {
        replay_runtime,
        exchange,
        store,
        signer,
        streams: RefCell::default(),
        next_stream: Cell::new(1),
        startup_token: Cell::new(0),
        symbol_metadata_throttler,
        store_deliveries: Rc::default(),
        stream_listeners: Rc::default(),
    }))
}

/// Native parser receipts and controlled replay clock. Only isolated replay
/// accounts opened by the addon can call this diagnostic surface.
pub fn native_account_replay_control(
    account: Rc<NativeAccount>,
    request: NativeValue,
) -> NativeAccountFuture<NativeValue> {
    Box::pin(async move {
        let runtime = account
            .replay_runtime
            .as_ref()
            .ok_or_else(|| host_error("account has no replay host"))?;
        let clock = runtime.replay_clock.as_ref().expect("replay host clock");
        let operation: String = request
            .get("op")
            .ok_or_else(|| host_error("missing replay op"))?
            .decode()
            .map_err(|e| host_error(e.to_string()))?;
        match operation.as_str() {
            "begin" => {
                safe_cex_exchange::replay_clock::activate(clock).map_err(|e| exchange_error(&e))?;
                // Cancel the startup Store tail at the same observation boundary
                // as lodash.cancel() in TS. Do not rewrite any emitted payload.
                for delivery in account
                    .store_deliveries
                    .borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                {
                    delivery.reset_replay_throttle();
                }
            }
            "advance" => {
                let at: f64 = request
                    .get("atMs")
                    .ok_or_else(|| host_error("missing replay time"))?
                    .decode()
                    .map_err(|e| host_error(e.to_string()))?;
                safe_cex_exchange::replay_clock::advance(clock, at)
                    .await
                    .map_err(|e| exchange_error(&e))?;
            }
            "receipt" => {}
            _ => return Err(host_error("unknown replay operation")),
        }
        safe_cex_exchange::native::replay_checkpoint(runtime);
        // A handler return is the synchronous processing boundary. Ready source
        // Promise continuations run at the real host checkpoint before receipt.
        runtime.sleep(0).await.map_err(|e| exchange_error(&e))?;
        safe_cex_exchange::native::replay_checkpoint(runtime);
        encode(
            json!({"atMs": safe_cex_exchange::replay_clock::elapsed(clock), "pendingJobs": safe_cex_exchange::native::replay_pending_jobs(runtime), "rawTasks": safe_cex_exchange::native::replay_raw_tasks(runtime), "transport": safe_cex_exchange::native::native_frame_receipts(runtime), "sockets": safe_cex_exchange::native::native_parser_receipts(runtime) }),
        )
    })
}

/// ts: RemoteSigner.onAvailabilityChange, driven from the host's signer session.
pub fn native_account_signer_available(account: &NativeAccount, available: bool) {
    let Some(signer) = &account.signer else {
        return;
    };
    signer.available.set(available);
    let listeners: Vec<_> = signer.listeners.borrow().clone();
    for (_, listener) in listeners {
        listener(available);
    }
}

/// The emitter events a host forwards (ts: the events ExchangeWorker listens to).
const NATIVE_ACCOUNT_EVENTS: &[&str] = &[
    "fill",
    "updatePositions",
    "ws-reconnected",
    "remoteSignerStatus",
    "markets:changed",
    "activeTickerSymbols:changed",
    "deactivate",
];
const NATIVE_ACCOUNT_ARGS_EVENTS: &[&str] = &[
    "error",
    "log",
    "warning",
    "grafana-error",
    "grafana-http-request",
    "grafana-http-response",
    "grafana-info",
    "grafana-pws",
    "grafana-pws-sub",
];
/// Packages one event for the native worker boundary.
pub fn native_account_event(event: &str, data: &NativeValue) -> NativeValue {
    let mut fields = vec![("event", NativeValue::string(event))];
    if event != "ws-reconnected" {
        fields.push(("data", data.clone()));
    }
    NativeValue::object(fields)
}

/// Forwards emitter events and worker Store payloads until cleanup runs.
pub fn native_account_watch(
    account: &NativeAccount,
    on_event: Rc<dyn Fn(&str, &NativeValue)>,
    on_store: Rc<dyn Fn(NativeValue)>,
) -> Box<dyn Fn()> {
    let base = account.exchange.base();
    let active = Rc::new(Cell::new(true));
    let stream_active = active.clone();
    let stream_event = on_event.clone();
    let stream_listener: Rc<dyn Fn(&str, &NativeValue)> = Rc::new(move |event, data| {
        if stream_active.get() {
            stream_event(event, data);
        }
    });
    account
        .stream_listeners
        .borrow_mut()
        .push(stream_listener.clone());
    let delivery = NativeStoreDelivery::new(&account.exchange, &account.store, on_store);
    account
        .store_deliveries
        .borrow_mut()
        .push(Rc::downgrade(&delivery));
    let store_delivery = delivery.clone();
    let unsubscribe: Rc<dyn Fn()> = account
        .store
        .subscribe_native(Box::new(move |data, _| {
            store_delivery.update(data).map_err(|error| {
                safe_cex_types::StoreSubscriberError::new(error.message.clone())
                    .with_cause(Rc::new(error))
            })
        }))
        .into();
    let ids: Vec<(&str, String)> = NATIVE_ACCOUNT_EVENTS
        .iter()
        .chain(NATIVE_ACCOUNT_ARGS_EVENTS.iter())
        .map(|event| {
            let forwards_args = NATIVE_ACCOUNT_ARGS_EVENTS.contains(event);
            let forward = on_event.clone();
            let active = active.clone();
            let unsubscribe = unsubscribe.clone();
            let delivery = delivery.clone();
            let metadata = Rc::downgrade(&account.symbol_metadata_throttler);
            let exchange = Rc::downgrade(&account.exchange);
            let id = base.on_args(
                event,
                Rc::new(move |name: &str, args: &[Js]| {
                    if !active.get() {
                        return Ok(());
                    }
                    if name == "deactivate" {
                        // ExchangeWorker clears its delivery pipeline before disposal,
                        // including the Store reset that disposal performs.
                        active.set(false);
                        if let Some(metadata) = metadata.upgrade() {
                            metadata.clear();
                        }
                        delivery.cancel();
                        unsubscribe();
                        if let Some(exchange) = exchange.upgrade() {
                            exchange.dispose()?;
                        }
                        forward("deactivate-status", &NativeValue::Bool(true));
                        return Ok(());
                    }
                    let data = if forwards_args {
                        NativeValue::Array(
                            args.iter()
                                .map(NativeValue::from_js)
                                .collect::<Result<_, _>>()?,
                        )
                    } else {
                        args.first()
                            .map(NativeValue::from_js)
                            .transpose()?
                            .unwrap_or(NativeValue::Null)
                    };
                    forward(name, &data);
                    Ok(())
                }),
            );
            (*event, id)
        })
        .collect();
    let exchange = account.exchange.clone();
    let deliveries = account.store_deliveries.clone();
    let stream_listeners = account.stream_listeners.clone();
    Box::new(move || {
        active.set(false);
        delivery.cancel();
        deliveries
            .borrow_mut()
            .retain(|item| !item.ptr_eq(&Rc::downgrade(&delivery)));
        unsubscribe();
        stream_listeners
            .borrow_mut()
            .retain(|listener| !Rc::ptr_eq(listener, &stream_listener));
        for (event, id) in &ids {
            exchange.base().off(event, Some(id));
        }
    })
}

fn arg<T: DeserializeOwned>(args: &[NativeValue], index: usize) -> Result<T, NativeValue> {
    args.get(index)
        .unwrap_or(&NativeValue::Undefined)
        .decode()
        .map_err(|e| host_error(format!("Invalid argument {index}: {e}")))
}
fn encode<T: Serialize>(value: T) -> Result<NativeValue, NativeValue> {
    NativeValue::encode(&value).map_err(|e| host_error(e.to_string()))
}
fn done<T>(result: Result<T, ExchangeError>) -> Result<T, NativeValue> {
    result.map_err(|error| exchange_error(&error))
}

/// Callback APIs cross the command channel as handles. Each notification
/// retains the venue callback value alongside its registration ID.
fn listen(
    account: &Rc<NativeAccount>,
    method: &'static str,
    args: &[NativeValue],
) -> Result<NativeValue, NativeValue> {
    let id = account.next_stream.get();
    let next = id
        .checked_add(1)
        .ok_or_else(|| host_error("stream handles exhausted"))?;
    let active = Rc::new(Cell::new(true));
    let enabled = active.clone();
    let account_weak = Rc::downgrade(account);
    let notify = Rc::new(move |data: Result<NativeValue, ExchangeError>| {
        if enabled.get()
            && let Some(account) = account_weak.upgrade()
        {
            match data {
                Ok(data) => {
                    let data = NativeValue::object([
                        ("subscriptionId", NativeValue::Number(id as f64)),
                        ("data", data),
                    ]);
                    let listeners = account.stream_listeners.borrow().clone();
                    for listener in listeners {
                        listener(method, &data);
                    }
                }
                Err(error) => account
                    .exchange
                    .base()
                    .runtime
                    .report_exception(&error, &json!({"method":method}).into()),
            }
        }
    });
    let venue = account.exchange.as_ref();
    let cleanup = match method {
        "listenOHLCV" => venue
            .listen_ohlcv_retained(
                arg(args, 0)?,
                Rc::new(move |candle| {
                    notify(NativeValue::from_js(candle));
                    Ok(())
                }),
            )
            .map(Some),
        "listenOHLCVs" => {
            venue.listen_ohlcvs(
                arg(args, 0)?,
                Rc::new(move |candle| notify(NativeValue::encode(&candle))),
            );
            Ok(None)
        }
        "listenOrderBook" => venue
            .listen_order_book(
                arg(args, 0)?,
                Rc::new(move |book| notify(NativeValue::encode(&book))),
            )
            .map(Some),
        _ => unreachable!("only declared callback methods reach listen"),
    };
    let cleanup = cleanup.map_err(|error| {
        active.set(false);
        exchange_error(&error)
    })?;
    account
        .streams
        .borrow_mut()
        .insert(id, NativeStream { active, cleanup });
    account.next_stream.set(next);
    encode(id)
}

/// ts: the Exchange methods ExchangeWorker calls, by name; an unknown name rejects.
pub fn native_account_call(
    account: Rc<NativeAccount>,
    method: String,
    args: Vec<NativeValue>,
) -> NativeAccountFuture<NativeValue> {
    Box::pin(async move {
        let venue = account.exchange.as_ref();
        let base = venue.base();
        match method.as_str() {
            "start" => {
                let token = account.startup_token.get();
                done(venue.start().await)?;
                if token != account.startup_token.get() || base.is_disposed() {
                    return Ok(NativeValue::Bool(false));
                }
                let deliveries: Vec<_> = account
                    .store_deliveries
                    .borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                    .collect();
                for delivery in deliveries {
                    done(delivery.snapshot())?;
                }
                Ok(NativeValue::Bool(true))
            }
            "dispose" => {
                account.cancel_startup();
                account.symbol_metadata_throttler.clear();
                let streams = account.dispose_streams();
                let disposed = venue.dispose();
                for delivery in account
                    .store_deliveries
                    .borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                {
                    delivery.cancel();
                }
                done(streams.and(disposed)).map(|_| NativeValue::Null)
            }
            "listenOHLCV" => listen(&account, "listenOHLCV", &args),
            "listenOHLCVs" => listen(&account, "listenOHLCVs", &args),
            "listenOrderBook" => listen(&account, "listenOrderBook", &args),
            "unlisten" => {
                let id: u32 = arg(&args, 0)?;
                let stream = account.streams.borrow_mut().remove(&id);
                if let Some(stream) = stream {
                    stream.active.set(false);
                    if let Some(cleanup) = stream.cleanup {
                        done(cleanup())?;
                    }
                }
                Ok(NativeValue::Null)
            }
            "disablePublicWebsockets" => {
                base.disable_public_websockets();
                Ok(NativeValue::Null)
            }
            "reconnect" => done(base.reconnect()).map(|_| NativeValue::Null),
            "recoverAfterOffline" => done(venue.recover_after_offline()).and_then(encode),
            "waitForRemoteSigner" => done(base.wait_for_remote_signer().await).and_then(encode),
            "getActiveTickerSymbols" => encode(base.get_active_ticker_symbols()),
            "setActiveTickerSymbols" => {
                let symbols: Vec<String> = arg(&args, 0)?;
                done(base.set_active_ticker_symbols(&symbols)).map(|_| NativeValue::Null)
            }
            "fetchLastLogsFromWebsockets" => encode(base.fetch_last_logs_from_websockets()),
            "validateAccount" => done(venue.validate_account().await).and_then(encode),
            "getAccount" => done(venue.get_account().await).and_then(encode),
            "fetchMarkets" => done(venue.fetch_markets().await).and_then(encode),
            "refreshMarkets" => done(venue.refresh_markets().await).and_then(|changes| {
                Ok(NativeValue::object([
                    ("added", encode(changes.added)?),
                    ("removed", encode(changes.removed)?),
                ]))
            }),
            "fetchTickers" => done(venue.fetch_tickers().await).and_then(encode),
            "fetchPositions" => done(venue.fetch_positions(arg(&args, 0)?).await).and_then(encode),
            "fetchOrders" => done(venue.fetch_orders(arg(&args, 0)?).await).and_then(encode),
            "fetchAndSetOrders" => {
                done(venue.fetch_and_set_orders(arg(&args, 0)?).await).and_then(encode)
            }
            "findOrderByClientOrderId" => done(
                venue
                    .find_order_by_client_order_id(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .and_then(|lookup| match lookup {
                ClientOrderLookup::Found(order) => Ok(NativeValue::object([
                    ("found", NativeValue::Bool(true)),
                    ("order", encode(order)?),
                ])),
                ClientOrderLookup::NotFound { definitive } => Ok(NativeValue::object([
                    ("found", NativeValue::Bool(false)),
                    ("definitive", NativeValue::Bool(definitive)),
                ])),
            }),
            "removeOrders" => {
                // TS accepts Pick<Order, 'id'>[], not an array of bare IDs.
                let orders: Vec<NativeOrderId> = arg(&args, 0)?;
                let ids: Vec<String> = orders.into_iter().map(|order| order.id).collect();
                done(base.remove_orders(&ids)).map(|_| NativeValue::Null)
            }
            "fetchAndSetPrivateSnapshot" => {
                done(venue.fetch_and_set_private_snapshot(1).await).map(|_| NativeValue::Undefined)
            }
            "placeOrder" => {
                let order: PlaceOrderOpts = arg(&args, 0)?;
                done(base.place_order(venue, order).await).and_then(encode)
            }
            "placeOrders" => {
                let orders: Vec<PlaceOrderOpts> = arg(&args, 0)?;
                done(base.place_orders(venue, orders).await).and_then(encode)
            }
            "updateOrder" => done(venue.update_order(arg(&args, 0)?).await).and_then(encode),
            "cancelOrders" => {
                done(venue.cancel_orders(arg(&args, 0)?).await).map(|_| NativeValue::Null)
            }
            "cancelSymbolOrders" => {
                done(venue.cancel_symbol_orders(arg(&args, 0)?).await).map(|_| NativeValue::Null)
            }
            "cancelAllOrders" => done(venue.cancel_all_orders().await).map(|_| NativeValue::Null),
            "mintClientOrderId" => {
                done(base.mint_client_order_id(&arg::<safe_cex_types::PlaceOrderOpts>(&args, 0)?))
                    .and_then(encode)
            }
            "placeOrderWithClientOrderId" => {
                let order: PlaceOrderOpts = arg(&args, 0)?;
                done(base.place_order_with_client_order_id(venue, order).await).and_then(|placed| {
                    Ok(NativeValue::object([
                        ("orderIds", encode(placed.order_ids)?),
                        ("clientOrderId", encode(placed.client_order_id)?),
                    ]))
                })
            }
            "fetchOptionChain" => {
                done(venue.fetch_option_chain(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchOptionExpiries" => {
                done(venue.fetch_option_expiries(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchAccountGreeks" => done(venue.fetch_account_greeks().await).and_then(encode),
            "nuke" => done(venue.nuke(arg::<Option<u32>>(&args, 0)?.unwrap_or(0)).await)
                .map(|_| NativeValue::Null),
            "nukePositions" => done(
                venue
                    .nuke_positions(arg::<Option<u32>>(&args, 0)?.unwrap_or(0))
                    .await,
            )
            .map(|_| NativeValue::Null),
            "nukeOrders" => done(base.nuke_orders(venue).await).map(|_| NativeValue::Null),
            "setLeverage" => done(
                venue
                    .set_leverage(arg(&args, 0)?, arg(&args, 1)?, arg(&args, 2)?)
                    .await,
            )
            .map(|_| NativeValue::Undefined),
            "setAllLeverage" => {
                done(venue.set_all_leverage(arg(&args, 0)?).await).map(|_| NativeValue::Null)
            }
            "changePositionMode" => done(
                venue
                    .change_position_mode(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .map(|_| NativeValue::Undefined),
            "changeMarginType" => done(
                venue
                    .change_margin_type(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .map(|_| NativeValue::Undefined),
            "fetchLeverageForSymbol" => {
                done(venue.fetch_leverage_for_symbol(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchSymbolMaxLeverage" => {
                done(venue.fetch_symbol_max_leverage(arg(&args, 0)?).await).and_then(encode)
            }
            "scheduleCancel" => {
                done(venue.schedule_cancel(arg(&args, 0)?).await).map(|_| NativeValue::Undefined)
            }
            "fetchOHLCV" => done(venue.fetch_ohlcv(arg(&args, 0)?).await).and_then(encode),
            "fetchPublicTrades" => done(
                venue
                    .fetch_public_trades(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .and_then(encode),
            "fetchHistoricalPositions" => {
                let options: NativeHistoricalPositionOptions = arg(&args, 0)?;
                done(
                    venue
                        .fetch_historical_positions(
                            safe_cex_exchange::journal::HistoricalPositionOptions {
                                account: options.account,
                                from: options.from,
                                to: options.to,
                            },
                        )
                        .await,
                )
                .and_then(encode)
            }
            "fetchHistoricalFills" => {
                done(venue.fetch_historical_fills(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchIncomeHistory" => {
                done(venue.fetch_income_history(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchIncomeHistoryRange" => done(
                venue
                    .fetch_income_history_range(arg(&args, 0)?, arg(&args, 1)?, arg(&args, 2)?)
                    .await,
            )
            .and_then(encode),
            "fetchClosedPnl" => done(venue.fetch_closed_pnl(arg(&args, 0)?).await).and_then(encode),
            "fetchHistoricalOrders" => {
                done(venue.fetch_historical_orders(arg(&args, 0)?).await).and_then(encode)
            }
            "updateSymbolMetadata" => {
                account.symbol_metadata_throttler.call(arg(&args, 0)?);
                Ok(NativeValue::Null)
            }
            other => Err(host_error(format!("Unknown method {other}"))),
        }
    })
}

#[cfg(test)]
mod emitter_error_tests {
    use super::{NativeAccountFuture, NativeValue, exchange_error, host_error};
    use safe_cex_exchange::{js_runtime::value::Js, runtime::ExchangeError};
    use std::{future::Future, pin::Pin};

    #[test]
    fn account_error_envelope_retains_native_code_presence_and_bits() {
        let cases = [
            None,
            Some(Js::Undefined),
            Some(Js::Null),
            Some(Js::Number(-0.0)),
            Some(Js::Number(f64::from_bits(0x7ff8_0000_0000_0042))),
            Some(Js::Number(f64::INFINITY)),
            Some(Js::Number(f64::NEG_INFINITY)),
            Some(Js::String(vec![0xd800].into())),
            Some(Js::object(vec![("missing", Js::Undefined)])),
        ];
        for code in cases {
            let mut error = ExchangeError::new("native failure");
            error.name = "TypeError".into();
            error.code = code.clone();
            let wire = exchange_error(&error);
            assert_eq!(wire.get("name"), Some(&NativeValue::string("TypeError")));
            assert_eq!(
                wire.get("message"),
                Some(&NativeValue::string("native failure"))
            );
            match code {
                None => assert!(wire.get("code").is_none()),
                Some(Js::Number(expected)) => assert_eq!(
                    wire.get("code").unwrap().as_f64().unwrap().to_bits(),
                    expected.to_bits()
                ),
                Some(Js::Undefined) => {
                    assert!(matches!(wire.get("code"), Some(NativeValue::Undefined)))
                }
                Some(Js::Null) => assert!(matches!(wire.get("code"), Some(NativeValue::Null))),
                Some(Js::String(_)) => {
                    assert_eq!(wire.get("code"), Some(&NativeValue::String(vec![0xd800])))
                }
                Some(Js::Object(_)) => assert_eq!(
                    wire.get("code"),
                    Some(&NativeValue::object([("missing", NativeValue::Undefined)]))
                ),
                Some(_) => panic!("unexpected authored code fixture"),
            }
        }
        let host = host_error("literal host failure");
        assert_eq!(host.get("name"), Some(&NativeValue::string("Error")));
        assert_eq!(
            host.get("message"),
            Some(&NativeValue::string("literal host failure"))
        );
        assert!(host.get("code").is_none());
    }

    #[test]
    fn account_future_returns_owned_native_error_without_json() {
        let future: NativeAccountFuture<bool> =
            Box::pin(async { Err(NativeValue::Number(f64::from_bits(0x7ff8_0000_0000_0042))) });
        let mut expected: Pin<Box<dyn Future<Output = Result<bool, NativeValue>>>> = future;
        let context = &mut std::task::Context::from_waker(std::task::Waker::noop());
        match expected.as_mut().poll(context) {
            std::task::Poll::Ready(Err(NativeValue::Number(number))) => {
                assert_eq!(number.to_bits(), 0x7ff8_0000_0000_0042);
            }
            _ => panic!("native account future must retain its actual error type"),
        }
    }
}
#[cfg(test)]
#[path = "native_account_events_tests.rs"]
mod event_protocol_tests;

#[cfg(test)]
mod model_codec_tests {
    use super::encode;
    use safe_cex_exchange::js_runtime::value::{Js, from_json, to_typed};
    use safe_cex_types::{Order, Position};
    use serde_json::Value;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn account_encoder_preserves_model_presence_before_js_materialization() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../safe-cex-types/tests/support/model-field-corpus.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cases.len(), 126);
        let mut wire = Vec::new();
        for case in &cases {
            let input: Value = serde_json::from_str(case["input"].as_str().unwrap()).unwrap();
            let native = from_json(&input);
            for (field, state) in case["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["states"].as_array().unwrap())
            {
                if state == 1 {
                    native
                        .define(field.as_str().unwrap(), Js::Undefined)
                        .unwrap();
                }
            }
            let encoded = if case["model"] == "order" {
                encode(to_typed::<Order>(&native).unwrap()).unwrap()
            } else {
                encode(to_typed::<Position>(&native).unwrap()).unwrap()
            };
            for (field, state) in case["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["states"].as_array().unwrap())
            {
                let field = field.as_str().unwrap();
                if state == 0 {
                    assert!(encoded.get(field).is_none());
                }
                if state == 1 {
                    assert!(matches!(
                        encoded.get(field),
                        Some(super::NativeValue::Undefined)
                    ));
                }
            }
            wire.push(
                safe_cex_exchange::js_runtime::value::to_json(&encoded.to_js())
                    .unwrap()
                    .to_string(),
            );
        }
        let mut node = Command::new("node").args(["-e", "let text='';process.stdin.on('data',x=>text+=x).on('end',()=>process.stdout.write(JSON.stringify(JSON.parse(text).map(x=>JSON.stringify(JSON.parse(x))))));"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        node.stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&wire).unwrap().as_bytes())
            .unwrap();
        let output = node.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual.len(), 126);
        for (actual, case) in actual.iter().zip(&cases) {
            assert_eq!(
                actual,
                case["wire"].as_str().unwrap(),
                "account/napi field-presence bytes {case}"
            );
        }
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../tests/support/phemex_public_feed.rs"]
mod phemex_public_feed_tests;
