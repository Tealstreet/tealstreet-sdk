pub use crate::client_order_id::{
    ClientOrderIdPlaceOrderOpts, ClientOrderIdRule, ClientOrderLookup, PlacedOrderRef,
};
use crate::client_order_id::{
    client_order_id_shape_message, client_order_id_split_message, is_regular_client_order_shape,
};
use crate::client_order_id::{default_mint_candidates, order_carries_client_order_id};
use crate::error_reporting::{
    ExchangeErrorInput, assign, raw_formatted_context, read_http_failure_context,
    record_http_failure,
};
use crate::external::ExternalPublicData;
use crate::js_runtime::value::{
    Js, from_json, from_retained_input, from_typed, to_json, to_retained_input, to_typed,
};
use crate::lifecycle::{SignerWaitGuard, SignerWaiter, WatchdogGuard, WatchdogOwner};
use crate::order_operations::order_options;
use crate::snapshots::{AccountMarketOverlay, MarketChanges, PositionFetch, market_changes};
use crate::socket_lifecycle::{SocketSlot, socket_diagnostics};
use crate::websocket::BaseWebSocket;
use crate::worker::CallableMethodsInWorker;
use crate::{
    capabilities::ExchangeCapabilities,
    runtime::{
        ArgsListener, Cleanup, ExchangeError, ExchangeFuture, ExchangeRuntime, Listener,
        ListenerCleanup,
    },
};
use safe_cex_store::Store;
use safe_cex_store::StoreUpdate;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportProvider,
    secondary::{RateLimitWrapper, SecondaryKeyClients},
    signing::{
        HttpSigningRequest, HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse,
        SignedConfig,
    },
};
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeName, ExchangeOptions, Market, OHLCVOptions, Order,
    PlaceOrderOpts, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use safe_cex_types::{
    ExchangeOptionsExternalPublicData, FetchedLeverage, OHLCVSOptions, OrderBook, OrderSide,
    OrderType, Timeframe,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};
use std::{
    sync::Arc,
    task::{Poll, Waker},
};

type ListenerRows = Rc<RefCell<HashMap<String, Vec<(String, ArgsListener, bool)>>>>;

struct WorkerReplyGuard {
    listeners: ListenerRows,
    event: String,
    id: String,
}
impl Drop for WorkerReplyGuard {
    fn drop(&mut self) {
        if let Some(rows) = self.listeners.borrow_mut().get_mut(&self.event) {
            rows.retain(|row| row.0 != self.id);
        }
    }
}

fn emit_listeners(listeners: &ListenerRows, event: &str, args: &[Js]) -> Result<(), ExchangeError> {
    let callbacks = listeners.borrow().get(event).cloned().unwrap_or_default();
    for (id, callback, once) in callbacks {
        if once && let Some(rows) = listeners.borrow_mut().get_mut(event) {
            rows.retain(|row| row.0 != id);
        }
        callback(event, args)?;
    }
    Ok(())
}

/// Only shared preflight can construct this payload; venue hooks cannot deserialize it.
/// Venue identity and payload privacy are checked by diagnostic-matched compile-fail tests.
pub struct ValidatedOrder<V: Exchange> {
    order: V::PlaceOpts,
    venue: std::marker::PhantomData<fn() -> *const V>,
}
impl<V: Exchange> ValidatedOrder<V> {
    pub(crate) fn new(order: V::PlaceOpts) -> Self {
        Self {
            order,
            venue: std::marker::PhantomData,
        }
    }
    pub(crate) fn into_raw(self) -> V::PlaceOpts {
        self.order
    }
    pub fn options(&self) -> &V::PlaceOpts {
        &self.order
    }
    pub fn assert_single_native_client_order_id(
        &self,
        base: &BaseExchange,
        count: usize,
    ) -> Result<(), ExchangeError> {
        base.assert_single_native_client_order_id(
            std::borrow::Borrow::<PlaceOrderOpts>::borrow(&self.order)
                .client_order_id
                .as_deref(),
            count,
        )
    }
}
pub struct ValidatedBatch<V: Exchange> {
    orders: Vec<ValidatedOrder<V>>,
}
impl<V: Exchange> ValidatedBatch<V> {
    pub(crate) fn new(orders: Vec<V::PlaceOpts>) -> Self {
        Self {
            orders: orders.into_iter().map(ValidatedOrder::new).collect(),
        }
    }
    pub fn orders(&self) -> &[ValidatedOrder<V>] {
        &self.orders
    }
}
#[derive(Debug, Clone)]
pub struct SubscribedCandle {
    pub candle: Candle,
    pub symbol: String,
    pub interval: safe_cex_types::ModelField<safe_cex_types::AnyTimeframe>,
}

#[derive(Default)]
pub struct ExchangeSettings {
    pub tick_interval: Option<f64>,
    pub positions_supported: Option<bool>,
    pub supports_public_only: bool,
    pub client_order_id_rule: Option<ClientOrderIdRule>,
}

#[derive(Clone)]
pub(crate) enum SocketRegistration {
    Owned(Rc<BaseWebSocket>),
    Borrowed(std::rc::Weak<BaseWebSocket>),
    Lookup(Rc<dyn Fn() -> Option<Rc<BaseWebSocket>>>),
}
impl SocketRegistration {
    fn upgrade(&self) -> Option<Rc<BaseWebSocket>> {
        match self {
            Self::Owned(socket) => Some(socket.clone()),
            Self::Borrowed(socket) => socket.upgrade(),
            Self::Lookup(lookup) => lookup(),
        }
    }
}

/// A missing patch leaves the field unchanged; Remove clears it explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityFieldPatch {
    Unchanged,
    Set(String),
    Remove,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentityPatch {
    pub application_id: IdentityFieldPatch,
    pub vault: IdentityFieldPatch,
}
#[derive(Clone)]
pub(crate) struct SharedExchangeOptions(Rc<RefCell<ExchangeOptions>>);
impl SharedExchangeOptions {
    pub(crate) fn snapshot(&self) -> ExchangeOptions {
        self.0.borrow().clone()
    }
}

pub struct BaseExchange {
    owned_tasks: Rc<crate::owned_tasks::OwnedTasks>,
    pub name: ExchangeName,
    pub(crate) options: SharedExchangeOptions,
    pub store: Rc<dyn Store>,
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub settings: ExchangeSettings,
    pub init_time: f64,
    pub is_demo: bool,
    pub is_spot: bool,
    pub(crate) account_market_overlay: RefCell<Option<crate::snapshots::AccountMarketOverlay>>,
    pub(crate) position_fetches: RefCell<HashMap<String, Rc<crate::snapshots::PositionFetch>>>,
    pub external_public_data_provider: RefCell<Option<Rc<dyn crate::external::ExternalPublicData>>>,
    pub signing_http_client: Option<Rc<dyn crate::signing::SigningHttpClient>>,
    pub secondary_clients: RefCell<Option<Rc<safe_cex_transport::secondary::SecondaryKeyClients>>>,
    pub signers: crate::runtime::ExchangeSigners,
    pub(crate) last_http_failure: RefCell<Option<crate::error_reporting::HttpFailureRecord>>,
    pub(crate) signer_waiters: Rc<RefCell<Vec<Rc<crate::lifecycle::SignerWaiter>>>>,
    disposed: Cell<bool>,
    pub(crate) online: Cell<bool>,
    pub(crate) is_nuking: Cell<bool>,
    pub(crate) last_reconnect_emit: Cell<f64>,
    pub(crate) watchdog: RefCell<Option<Box<dyn crate::runtime::ExchangeTask>>>,
    pub(crate) watchdog_generation: Cell<u64>,
    pub(crate) venue_weak: RefCell<Option<std::rc::Weak<dyn Exchange>>>,
    watchdog_base: RefCell<Option<std::rc::Weak<BaseExchange>>>,
    pub(crate) reopen_resync: Rc<crate::websocket::ReopenResync>,
    pub(crate) sockets: RefCell<[Option<SocketRegistration>; 8]>,
    auth_failures: Cell<u8>,
    active_symbols: RefCell<Vec<String>>,
    listeners: ListenerRows,
    unsubscribe: RefCell<Option<Cleanup>>,
}
fn not_implemented<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

/// Object-safe view supplied automatically to every concrete venue.
#[doc(hidden)]
pub trait ExchangeView {
    fn as_exchange(&self) -> &dyn Exchange;
}
impl<T: Exchange> ExchangeView for T {
    fn as_exchange(&self) -> &dyn Exchange {
        self
    }
}

/// Venue hooks use shared models. Placement entrypoints on BaseExchange validate first.
pub trait Exchange: std::any::Any + ExchangeView {
    type PlaceOpts: Clone
        + std::borrow::Borrow<PlaceOrderOpts>
        + From<PlaceOrderOpts>
        + serde::de::DeserializeOwned
    where
        Self: Sized;

    fn name(&self) -> &ExchangeName {
        self.base().name()
    }
    fn can_sign(&self) -> bool {
        self.base().can_sign()
    }
    fn is_public_data_only(&self) -> bool {
        self.base().is_public_data_only(self.as_exchange())
    }
    fn needs_account_market_overlay(&self) -> bool {
        self.base().needs_account_market_overlay(self.as_exchange())
    }

    fn dispose(&self) -> Result<(), ExchangeError> {
        self.base().dispose()
    }
    fn find_order_by_client_order_id(
        &self,
        symbol: String,
        id: String,
    ) -> ExchangeFuture<'_, ClientOrderLookup> {
        Box::pin(async move {
            self.base()
                .find_order_by_client_order_id(self.as_exchange(), &symbol, &id)
                .await
        })
    }
    fn refresh_markets(&self) -> ExchangeFuture<'_, MarketChanges> {
        Box::pin(async move { self.base().refresh_markets(self.as_exchange()).await })
    }
    fn set_all_leverage(&self, leverage: f64) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.base()
                .set_all_leverage(self.as_exchange(), leverage)
                .await
        })
    }
    fn nuke(&self, tries: u32) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.base().nuke(self.as_exchange(), tries).await })
    }
    fn nuke_positions(&self, tries: u32) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.base().nuke_positions(self.as_exchange(), tries).await })
    }

    fn validate_order_capabilities(&self, _options: &Self::PlaceOpts) -> Result<(), ExchangeError>
    where
        Self: Sized,
    {
        Ok(())
    }

    fn sign_ws_auth<'a>(
        &'a self,
        request: &'a safe_cex_crypto::WsAuthRequest,
    ) -> ExchangeFuture<'a, safe_cex_crypto::WsAuthResponse> {
        Box::pin(async move { Ok(self.base().sign_ws_auth(request)) })
    }
    /// Dispatch the one remote-signer operation through the concrete venue hooks.
    fn sign_remote_request(
        &self,
        request: RemoteSignerRequest,
    ) -> ExchangeFuture<'_, RemoteSignerResponse> {
        Box::pin(async move {
            match request {
                RemoteSignerRequest::Http(request) => self
                    .sign_http_request(request)
                    .await
                    .map(RemoteSignerResponse::Http),
                RemoteSignerRequest::WsAuth { request, .. } => self
                    .sign_ws_auth(&request)
                    .await
                    .map(RemoteSignerResponse::WsAuth),
            }
        })
    }
    fn sign_http_request(
        &self,
        request: HttpSigningRequest,
    ) -> ExchangeFuture<'_, HttpSigningResponse> {
        Box::pin(self.base().sign_http_request(request))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.base().listen_ohlcv(options, callback)
    }
    fn listen_ohlcvs(&self, options: OHLCVSOptions, callback: Rc<dyn Fn(SubscribedCandle)>) {
        self.base().listen_ohlcvs(options, callback);
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.base().listen_order_book(symbol, callback)
    }

    fn place_order(&self, _order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>>
    where
        Self: Sized,
    {
        not_implemented()
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn fetch_historical_fills(
        &self,
        _options: safe_cex_types::ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::Execution>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_income_history(
        &self,
        _options: Option<safe_cex_types::IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::IncomeRecord>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_income_history_range(
        &self,
        _start: f64,
        _end: f64,
        _kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::IncomeRecord>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_closed_pnl(
        &self,
        _options: safe_cex_types::HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::ClosedPnlEntry>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_historical_orders(
        &self,
        _options: Option<safe_cex_types::OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::HistoricalOrder>> {
        Box::pin(async { Ok(vec![]) })
    }
    // Options are opt-in per venue: these reject naming the venue, never an empty chain.
    fn fetch_option_chain(
        &self,
        _query: safe_cex_types::OptionChainQuery,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::OptionChainRow>> {
        let error = crate::capabilities::options_unsupported(self.name(), "Option chains");
        Box::pin(async move { Err(error) })
    }
    fn fetch_option_expiries(&self, _underlying: String) -> ExchangeFuture<'_, Vec<f64>> {
        let error = crate::capabilities::options_unsupported(self.name(), "Option expiries");
        Box::pin(async move { Err(error) })
    }
    fn fetch_account_greeks(&self) -> ExchangeFuture<'_, Vec<safe_cex_types::AccountGreeks>> {
        let error = crate::capabilities::options_unsupported(self.name(), "Account greeks");
        Box::pin(async move { Err(error) })
    }

    fn after_method_called_in_worker(
        &self,
        _result: &serde_json::Value,
        _method: crate::worker::CallableMethodsInWorker,
        _args: &[serde_json::Value],
    ) -> Result<(), ExchangeError> {
        Ok(())
    }

    fn class_name(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("BaseExchange")
    }
    fn fetch_historical_positions(
        &self,
        _options: crate::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        Box::pin(async move {
            Err(ExchangeError::new(format!(
                "{}: no journal records path declared. Override fetchHistoricalPositions — fold from fills via foldClosedPositionRecords, or map this venue's reported closed axis.",
                self.class_name()
            )))
        })
    }
    fn base(&self) -> &BaseExchange;
    fn capabilities(&self) -> &ExchangeCapabilities;
    fn recover_after_offline(&self) -> Result<bool, ExchangeError> {
        self.base()
            .recover_after_offline(|socket| socket.recover_after_offline(true).map(Some))
    }

    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        not_implemented()
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        not_implemented()
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        not_implemented()
    }
    fn fetch_and_set_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async move { self.base().fetch_and_set_orders(self, symbols).await })
    }
    fn fetch_and_set_private_snapshot(&self, tries: u32) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.base()
                .fetch_and_set_private_snapshot(self, tries)
                .await
        })
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        not_implemented()
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        not_implemented()
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        not_implemented()
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        Box::pin(async move { self.base().fetch_leverage_for_symbol(self, &symbol).await })
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        not_implemented()
    }
    fn set_leverage(
        &self,
        _symbol: String,
        _leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn update_order(&self, _options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        not_implemented()
    }
    fn cancel_orders(&self, _orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.base().cancel_symbol_orders(self, &symbol).await })
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.base().cancel_all_orders(self).await })
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        not_implemented()
    }
    fn fetch_ohlcv(&self, _options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn update_symbol_metadata(&self, _symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>>
    where
        Self: Sized,
    {
        Box::pin(BaseExchange::place_orders_sequential(self, orders))
    }
}

fn normalize_symbols(symbols: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    symbols
        .iter()
        .filter(|s| !safe_cex_num::js_trim(s).is_empty() && seen.insert((*s).clone()))
        .cloned()
        .collect()
}
impl BaseExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        runtime: Rc<dyn ExchangeRuntime>,
        settings: ExchangeSettings,
    ) -> Self {
        let listeners: ListenerRows = Rc::default();
        let events = listeners.clone();
        let unsubscribe = store.subscribe(Box::new(move |data, _| {
            let data = from_typed(data).map_err(|error| {
                safe_cex_types::StoreSubscriberError::new(error.message())
                    .with_cause(Rc::new(error.to_exchange_error()))
            })?;
            emit_listeners(&events, "update", &[data]).map_err(Into::into)
        }));
        let active = options
            .tuning
            .as_ref()
            .and_then(|t| t.active_ticker_symbols.as_ref())
            .map_or_else(Vec::new, |s| normalize_symbols(s));
        Self {
            name,
            options: SharedExchangeOptions(Rc::new(RefCell::new(options))),
            store,
            init_time: runtime.now_ms(),
            runtime,
            settings,
            is_demo: false,
            is_spot: false,
            signers: Default::default(),
            signing_http_client: None,
            secondary_clients: RefCell::new(None),
            account_market_overlay: RefCell::new(None),
            position_fetches: RefCell::new(HashMap::new()),
            external_public_data_provider: RefCell::new(None),
            last_http_failure: RefCell::new(None),
            signer_waiters: Rc::new(RefCell::new(Vec::new())),
            disposed: Cell::new(false),
            online: Cell::new(true),
            is_nuking: Cell::new(false),
            last_reconnect_emit: Cell::new(0.0),
            watchdog: RefCell::new(None),
            owned_tasks: Rc::new(crate::owned_tasks::OwnedTasks::default()),
            watchdog_generation: Cell::new(0),
            sockets: RefCell::new(Default::default()),
            venue_weak: RefCell::new(None),
            watchdog_base: RefCell::new(None),
            reopen_resync: Rc::default(),
            auth_failures: Cell::new(0),
            active_symbols: RefCell::new(active),
            listeners,
            unsubscribe: RefCell::new(Some(Box::new(unsubscribe))),
        }
    }
    pub fn on(&self, event: &str, callback: Listener) -> String {
        self.on_args(
            event,
            Rc::new(move |event, args| callback(event, args.first().unwrap_or(&Js::Undefined))),
        )
    }
    pub fn on_args(&self, event: &str, callback: ArgsListener) -> String {
        let id = self.runtime.unique_id();
        self.listeners
            .borrow_mut()
            .entry(event.into())
            .or_default()
            .push((id.clone(), callback, false));
        id
    }
    pub fn once(&self, event: &str, callback: Listener) -> String {
        self.once_args(
            event,
            Rc::new(move |event, args| callback(event, args.first().unwrap_or(&Js::Undefined))),
        )
    }
    pub fn once_args(&self, event: &str, callback: ArgsListener) -> String {
        let id = self.runtime.unique_id();
        self.listeners
            .borrow_mut()
            .entry(event.into())
            .or_default()
            .push((id.clone(), callback, true));
        id
    }
    pub fn off(&self, event: &str, id: Option<&str>) {
        self.retain_event_listeners(event, |candidate| {
            Ok::<_, std::convert::Infallible>(id.is_some_and(|id| id != candidate))
        })
        .unwrap_or_else(|never| match never {});
    }
    pub(crate) fn retain_event_listeners<E>(
        &self,
        event: &str,
        mut retain: impl FnMut(&str) -> Result<bool, E>,
    ) -> Result<Vec<String>, E> {
        let snapshot = self
            .listeners
            .borrow()
            .get(event)
            .cloned()
            .unwrap_or_default();
        let mut retained = Vec::new();
        for row in snapshot {
            if retain(&row.0)? {
                retained.push(row);
            }
        }
        let ids = retained.iter().map(|row| row.0.clone()).collect();
        let displaced = {
            let mut listeners = self.listeners.borrow_mut();
            if retained.is_empty() {
                listeners.remove(event)
            } else {
                listeners.insert(event.into(), retained)
            }
        };
        drop(displaced);
        Ok(ids)
    }
    pub fn emit(&self, event: &str, data: Js) -> Result<(), ExchangeError> {
        self.emit_args(event, &[data])
    }
    pub fn emit_args(&self, event: &str, args: &[Js]) -> Result<(), ExchangeError> {
        emit_listeners(&self.listeners, event, args)
    }

    pub fn name(&self) -> &ExchangeName {
        &self.name
    }
    fn unsubscribe_from_store(&self) {
        let unsubscribe = self.unsubscribe.borrow_mut().take();
        if let Some(unsubscribe) = unsubscribe {
            unsubscribe();
        }
    }
    pub fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    /// The writable TS `isDisposed` field. Venue overrides assign it before
    /// their own cleanup; assignment alone must not run `BaseExchange.dispose`.
    pub fn set_disposed_flag(&self, disposed: bool) {
        self.disposed.set(disposed);
    }
    pub fn is_online(&self) -> bool {
        self.online.get()
    }
    pub fn get_active_ticker_symbols(&self) -> Vec<String> {
        self.active_symbols.borrow().clone()
    }
    pub fn set_active_ticker_symbols(&self, symbols: &[String]) -> Result<(), ExchangeError> {
        let next = normalize_symbols(symbols);
        if *self.active_symbols.borrow() == next {
            return Ok(());
        }
        *self.active_symbols.borrow_mut() = next.clone();
        self.emit("activeTickerSymbols:changed", from_typed(&next)?)?;
        Ok(())
    }
    pub fn get_ticker_symbols_for_subscription(&self, available: &[String]) -> Vec<String> {
        let active = self.get_active_ticker_symbols();
        let filtered: Vec<_> = available
            .iter()
            .filter(|s| active.contains(s))
            .cloned()
            .collect();
        if filtered.is_empty() {
            available.to_vec()
        } else {
            filtered
        }
    }
    pub fn log(&self, message: &str, severity: &str) -> Result<(), ExchangeError> {
        self.log_value(&Js::str(message), severity)
    }
    pub fn log_value(&self, message: &Js, severity: &str) -> Result<(), ExchangeError> {
        if self
            .options()
            .tuning
            .as_ref()
            .and_then(|t| t.console_logging)
            != Some(false)
        {
            self.runtime
                .console_values(severity, &[message.clone(), Js::str(severity)]);
        }
        let name = serde_json::to_value(&self.name).unwrap();
        let mut units = format!("[{}] ", name.as_str().unwrap())
            .encode_utf16()
            .collect::<Vec<_>>();
        units.extend_from_slice(
            &message
                .to_js_string()
                .map_err(|error| error.to_exchange_error())?,
        );
        self.emit_args("log", &[Js::utf16(&units), Js::str(severity)])
    }
    pub fn reject_preflight<T>(&self, message: String) -> Result<T, ExchangeError> {
        self.emit_and_log_error(
            crate::error_reporting::ExchangeErrorInput::Message(message.clone()),
            None,
            None,
        )?;
        Err(ExchangeError::new(message))
    }
    pub fn resolve_caller_client_order_id(
        &self,
        opts: impl Into<safe_cex_types::PreparedOrderOpts>,
    ) -> Result<Option<String>, ExchangeError> {
        let opts = opts.into();
        let Some(id) = &opts.client_order_id else {
            return Ok(None);
        };
        let name = serde_json::to_value(&self.name).unwrap();
        let Some(rule) = &self.settings.client_order_id_rule else {
            return self.reject_preflight(format!(
                "{} cannot honor a caller clientOrderId",
                name.as_str().unwrap()
            ));
        };
        let regular = is_regular_client_order_shape(&opts);
        if !rule
            .is_honorable_shape
            .as_ref()
            .map_or(regular, |check| check(&opts))
        {
            return self.reject_preflight(client_order_id_shape_message(&rule.label, &opts));
        }
        if !(rule.is_valid)(id) {
            return self.reject_preflight(format!(
                "{} clientOrderId must {}",
                rule.label, rule.requirement
            ));
        }
        self.assert_caller_client_order_id_unsplit(&opts)?;
        Ok(Some(id.clone()))
    }
    fn would_split_order(&self, opts: &safe_cex_types::PreparedOrderOpts) -> bool {
        self.store
            .markets_by_symbol()
            .get(&opts.symbol)
            .is_some_and(|market| {
                market.limits.amount.max > 0.0
                    && safe_cex_num::adjust(opts.amount, market.precision.amount)
                        > market.limits.amount.max
            })
    }
    pub fn assert_single_native_client_order_id(
        &self,
        id: Option<&str>,
        count: usize,
    ) -> Result<(), ExchangeError> {
        if id.is_some() && count != 1 {
            let name = serde_json::to_value(&self.name).expect("exchange name");
            let label = self
                .settings
                .client_order_id_rule
                .as_ref()
                .map(|rule| rule.label.as_str())
                .unwrap_or_else(|| name.as_str().expect("name string"));
            self.reject_preflight(client_order_id_split_message(label))
        } else {
            Ok(())
        }
    }
    fn assert_caller_client_order_id_unsplit(
        &self,
        opts: &safe_cex_types::PreparedOrderOpts,
    ) -> Result<(), ExchangeError> {
        if opts.client_order_id.is_some() && self.would_split_order(opts) {
            let name = serde_json::to_value(&self.name).expect("exchange name");
            let label = self
                .settings
                .client_order_id_rule
                .as_ref()
                .map(|rule| rule.label.as_str())
                .unwrap_or_else(|| name.as_str().expect("name string"));
            self.reject_preflight(format!("{label} order exceeds the market's max size for one order; a caller clientOrderId cannot span several orders"))
        } else {
            Ok(())
        }
    }

    pub fn assert_caller_client_order_ids<O: Clone + Into<safe_cex_types::PreparedOrderOpts>>(
        &self,
        orders: &[O],
    ) -> Result<(), ExchangeError> {
        let mut seen = HashSet::new();
        for order in orders {
            if let Some(id) = self.resolve_caller_client_order_id(order.clone())?
                && !seen.insert(id.clone())
            {
                let label = &self.settings.client_order_id_rule.as_ref().unwrap().label;
                return self.reject_preflight(format!(
                    "{label} clientOrderId {id} is duplicated in one batch"
                ));
            }
        }
        Ok(())
    }
    pub async fn place_order<O: std::borrow::Borrow<PlaceOrderOpts> + 'static>(
        &self,
        venue: &dyn Exchange,
        order: O,
    ) -> Result<Vec<String>, ExchangeError> {
        venue
            .capabilities()
            .place_order
            .call_raw(venue, crate::capabilities::RawOrder::Typed(Box::new(order)))
            .await
    }
    pub async fn place_orders<O: std::borrow::Borrow<PlaceOrderOpts> + 'static>(
        &self,
        venue: &dyn Exchange,
        orders: Vec<O>,
    ) -> Result<Vec<String>, ExchangeError> {
        venue
            .capabilities()
            .place_orders
            .call_raw(
                venue,
                orders
                    .into_iter()
                    .map(|order| crate::capabilities::RawOrder::Typed(Box::new(order)))
                    .collect(),
            )
            .await
    }
    pub async fn place_order_json(
        &self,
        venue: &dyn Exchange,
        order: Value,
    ) -> Result<Vec<String>, ExchangeError> {
        venue
            .capabilities()
            .place_order
            .call_raw(venue, crate::capabilities::RawOrder::Json(order))
            .await
    }
    pub async fn place_orders_json(
        &self,
        venue: &dyn Exchange,
        orders: Vec<Value>,
    ) -> Result<Vec<String>, ExchangeError> {
        venue
            .capabilities()
            .place_orders
            .call_raw(
                venue,
                orders
                    .into_iter()
                    .map(crate::capabilities::RawOrder::Json)
                    .collect(),
            )
            .await
    }
    pub async fn place_orders_sequential<V: Exchange>(
        venue: &V,
        orders: ValidatedBatch<V>,
    ) -> Result<Vec<String>, ExchangeError> {
        let mut ids = vec![];
        for order in orders.orders {
            ids.extend(
                venue
                    .capabilities()
                    .place_order
                    .call_raw(
                        venue,
                        crate::capabilities::RawOrder::Typed(Box::new(order.into_raw())),
                    )
                    .await?,
            );
        }
        Ok(ids)
    }
    pub async fn start(&self, venue: &dyn Exchange) -> Result<(), ExchangeError> {
        venue.capabilities().start.call(venue, ()).await
    }
    pub fn record_auth_failure(&self) {
        self.auth_failures
            .set(self.auth_failures.get().saturating_add(1).min(7));
    }
    pub fn record_auth_success(&self) {
        self.auth_failures.set(0);
    }
    pub fn auth_failure_streak(&self) -> u8 {
        self.auth_failures.get()
    }
    pub fn get_tick_interval(&self) -> Option<f64> {
        let override_interval = self
            .options()
            .extra
            .as_ref()
            .and_then(|e| e.tick_interval)
            .filter(|x| *x > 0.0);
        let configured = override_interval.or(self.settings.tick_interval);
        let name = serde_json::to_value(&self.name).unwrap();
        safe_cex_transport::polling::get_tick_interval(
            name.as_str().unwrap(),
            configured,
            u32::from(self.auth_failures.get()),
        )
    }
    /// Own work beyond the spawning call's return; dropping the handle is inert.
    /// Resolve source-specific error handling inside the future, including named discards.
    pub fn start_owned_operation(
        &self,
        future: impl std::future::Future<Output = ()> + 'static,
    ) -> crate::OwnedOperation {
        self.owned_tasks
            .start(self.runtime.as_ref(), |_| Box::pin(future), |_| {})
    }
    /// Publish a shared handle in build before the runtime can poll the work.
    pub fn start_shared_operation<T, Build, Work>(&self, build: Build) -> crate::SharedOperation<T>
    where
        T: Clone + 'static,
        Build: FnOnce(crate::SharedOperation<T>) -> Work,
        Work: std::future::Future<Output = Result<T, ExchangeError>> + 'static,
    {
        crate::shared_operation::start(&self.owned_tasks, self.runtime.as_ref(), build)
    }
    pub fn start_interval(
        &self,
        period_ms: u64,
        callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
    ) -> Result<crate::OwnedOperation, ExchangeError> {
        self.owned_tasks
            .timer(self.runtime.clone(), period_ms, true, callback)
    }
    pub fn start_timeout(
        &self,
        delay_ms: u64,
        callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
    ) -> Result<crate::OwnedOperation, ExchangeError> {
        self.owned_tasks
            .timer(self.runtime.clone(), delay_ms, false, callback)
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.disposed.set(true);
        self.owned_tasks.close();
        self.unsubscribe_from_store();
        self.dispose_signer_waiters();
        let provider = self.external_public_data_provider.borrow_mut().take();
        if let Some(provider) = provider {
            provider.dispose();
        }
        self.stop_watchdog();
        for socket in self.get_all_sockets() {
            socket.dispose();
        }
        for event in [
            "update",
            "fill",
            "error",
            "listenOHLCV",
            "subscribeOHLCV",
            "unsubscribeOHLCV",
            "listenOrderBook",
            "subscribeOrderBook",
            "unsubscribeOrderBook",
            "updatePositions",
            "updatePositions:done",
            "callMethod",
            "callMethod:done",
            "log",
            "warning",
            "ws-reconnected",
            "remoteSignerStatus",
            "markets:changed",
            "activeTickerSymbols:changed",
            "grafana-error",
            "grafana-http-request",
            "grafana-http-response",
            "grafana-info",
            "grafana-pws",
            "grafana-pws-sub",
        ] {
            self.off(event, None);
        }
        self.store.reset()?;
        Ok(())
    }
}

impl BaseExchange {
    pub fn note_http_response(&self, response: &Value, url: Option<&str>) {
        self.note_http_failure(record_http_failure(response, self.runtime.now_ms(), url));
    }
    pub(crate) fn note_http_failure(
        &self,
        failure: Option<crate::error_reporting::HttpFailureRecord>,
    ) {
        *self.last_http_failure.borrow_mut() = failure;
    }
    pub fn emit_and_log_error(
        &self,
        input: ExchangeErrorInput,
        msg: Option<&str>,
        extra_data: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        use crate::js_runtime::value::{Thrown, error_object};
        let (source, mut host_error) = match input {
            ExchangeErrorInput::Missing => (Js::Undefined, None),
            ExchangeErrorInput::Message(text) => (Js::str(&text), None),
            ExchangeErrorInput::Raw(value) => (value, None),
            ExchangeErrorInput::Error(error) => {
                let source = Thrown::from((*error).clone()).value;
                for (key, value) in [
                    ("context", error.context.as_ref().map(from_json)),
                    ("config", error.config.as_ref().map(from_json)),
                    (
                        "originalError",
                        error.original_error.as_ref().map(from_json),
                    ),
                    (
                        "enhancedMessage",
                        error.enhanced_message.as_deref().map(Js::str),
                    ),
                    ("url", error.url.as_deref().map(Js::str)),
                    ("method", error.method.as_deref().map(Js::str)),
                ] {
                    if let Some(value) = value {
                        source.define(key, value)?;
                    }
                }
                (source, Some(*error))
            }
        };
        if self.is_disposed() {
            self.runtime.console_values(
                "warning",
                &[
                    Js::str("exchange error (disposed, suppressing)"),
                    source.clone(),
                ],
            );
            return Ok(());
        }
        let extra = extra_data
            .filter(|value| value.is_object())
            .cloned()
            .map(from_retained_input)
            .unwrap_or_else(|| Js::object(vec![]));
        let suppress = extra
            .get("suppressConsoleLog")?
            .strict_equals(&Js::Bool(true));
        if !suppress {
            self.runtime
                .console_values("error", &[Js::str("exchange error"), source.clone()]);
            self.runtime.console_values("trace", &[]);
        }
        let sentry_tags = extra.get("sentryTags")?;
        let sentry_tags = if sentry_tags.is_object() {
            sentry_tags
        } else {
            Js::object(vec![])
        };
        let sentry_extra = extra.get("sentryExtra")?;
        let sentry_extra = if sentry_extra.is_object() {
            sentry_extra
        } else {
            Js::object(vec![])
        };
        let legacy = extra.spread_object()?;
        for key in ["sentryTags", "sentryExtra", "suppressConsoleLog"] {
            legacy.delete(&key.encode_utf16().collect::<Vec<_>>())?;
        }
        let msg = msg.filter(|text| !text.is_empty()).map(Js::str);
        let context = Js::object(vec![]);
        let (formatted, error_object) = if !source.truthy() || source.is_string() {
            let formatted = if !source.truthy() {
                msg.clone()
                    .unwrap_or_else(|| Js::str("Unknown Exchange Error"))
            } else {
                let prefix = msg.clone().unwrap_or_else(|| Js::str("Exchange Error"));
                let mut units = prefix.to_js_string()?.to_vec();
                units.extend(": ".encode_utf16());
                units.extend_from_slice(source.as_str().expect("source string"));
                Js::utf16(&units)
            };
            let error = error_object("Error", "");
            error.set("message", formatted.clone())?;
            error.set("name", Js::str("ExchangeError"))?;
            host_error = Some(Thrown::new(error.clone()).to_exchange_error());
            (formatted, error)
        } else {
            let formatted =
                if source.get("context")?.truthy() && source.get("enhancedMessage")?.truthy() {
                    let formatted = match &msg {
                        Some(msg) => msg.clone(),
                        None => source.get("enhancedMessage")?,
                    };
                    assign(&context, &raw_formatted_context(&source)?)?;
                    assign(&context, &legacy)?;
                    formatted
                } else {
                    let formatted = match &msg {
                        Some(msg) => msg.clone(),
                        None => {
                            let message = source.get("message")?;
                            if message.truthy() {
                                message
                            } else if matches!(source, Js::SymbolIterator) {
                                Js::str("Symbol(Symbol.iterator)")
                            } else {
                                Js::String(source.to_js_string()?)
                            }
                        }
                    };
                    if source.get("url")?.truthy() || source.get("method")?.truthy() {
                        for key in ["url", "method", "originalError"] {
                            context.define(key, source.get(key)?)?;
                        }
                        let name = source.get("name")?;
                        context.define(
                            "errorType",
                            if name.truthy() {
                                name
                            } else {
                                Js::str("TradeTunnelError")
                            },
                        )?;
                        assign(&context, &legacy)?;
                    }
                    formatted
                };
            (formatted, source.clone())
        };
        assign(&context, &legacy)?;
        let correlation = if source.is_nullish() || !source.get("context")?.truthy() {
            let record = self.last_http_failure.borrow_mut().take();
            read_http_failure_context(record.as_ref(), self.runtime.now_ms())
                .map(|context| serde_json::to_value(context).expect("context"))
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };
        let name = serde_json::to_value(&self.name).expect("exchange");
        let tags = Js::object(vec![
            ("exchange", from_json(&name)),
            ("module", Js::str("Exchange")),
            ("severity", Js::str("error")),
        ]);
        assign(&tags, &context)?;
        assign(&tags, &sentry_tags)?;
        assign(&tags, &from_json(&correlation))?;
        let options = self.options();
        let options = Js::object(vec![
            (
                "testnet",
                options.testnet.map(Js::Bool).unwrap_or(Js::Undefined),
            ),
            (
                "publicDataOnly",
                options
                    .public_data_only
                    .map(Js::Bool)
                    .unwrap_or(Js::Undefined),
            ),
        ]);
        let data = Js::object(vec![
            ("originalMessage", formatted.clone()),
            ("exchangeOptions", options),
        ]);
        assign(&data, &context)?;
        assign(&data, &sentry_extra)?;
        let metadata = Js::object(vec![]);
        if self.runtime.is_worker() {
            let report = Js::object(vec![
                ("error", error_object),
                ("tags", tags),
                ("extra", data),
            ]);
            metadata.define("sendToSentry", Js::Bool(true))?;
            metadata.define("sentryData", report)?;
        } else {
            let report = Js::object(vec![("tags", tags), ("extra", data)]);
            if let Some(error) = host_error {
                self.runtime
                    .report_exception(&error, &to_retained_input(&report)?);
            } else if error_object.truthy() {
                self.runtime
                    .report_raw_exception(&error_object, &to_retained_input(&report)?);
            }
        }
        let enhanced_context = if source.type_of() == "object" && !source.is_nullish() {
            source.get("context")?
        } else {
            Js::Undefined
        };
        metadata.define("enhancedContext", enhanced_context)?;
        metadata.define("suppressConsoleLog", Js::Bool(suppress))?;
        self.emit_args("error", &[formatted, metadata])
    }
}

pub trait BaseSigningOptionsView {
    fn key_truthy(&self) -> bool;
    fn secret_truthy(&self) -> bool;
    fn remote_signer_truthy(&self) -> bool;
    fn payload_signer_truthy(&self) -> bool;
}
pub fn can_sign_with(options: &impl BaseSigningOptionsView) -> bool {
    options.key_truthy()
        && (options.secret_truthy()
            || options.remote_signer_truthy()
            || options.payload_signer_truthy())
}
pub fn is_public_data_only_with(
    public_data_only_is_true: impl FnOnce() -> bool,
    can_sign: impl FnOnce() -> bool,
) -> bool {
    public_data_only_is_true() || !can_sign()
}
struct BaseSigningView<'a>(&'a BaseExchange);
impl BaseSigningOptionsView for BaseSigningView<'_> {
    fn key_truthy(&self) -> bool {
        !self.0.options().key.is_empty()
    }
    fn secret_truthy(&self) -> bool {
        !self.0.options().secret.is_empty()
    }
    fn remote_signer_truthy(&self) -> bool {
        self.0.signers.remote_signer.is_some()
    }
    fn payload_signer_truthy(&self) -> bool {
        self.0.signers.payload_signer.is_some()
    }
}

impl BaseExchange {
    /// Owned snapshot: no RefCell borrow escapes into venue callbacks or awaits.
    pub fn options(&self) -> ExchangeOptions {
        self.options.snapshot()
    }
    pub fn patch_account_identity(&self, patch: AccountIdentityPatch) {
        let mut options = self.options.0.borrow_mut();
        match patch.application_id {
            IdentityFieldPatch::Unchanged => {}
            IdentityFieldPatch::Set(value) => options.application_id = Some(value),
            IdentityFieldPatch::Remove => options.application_id = None,
        }
        match patch.vault {
            IdentityFieldPatch::Unchanged => {}
            IdentityFieldPatch::Set(value) => options.vault = Some(value),
            IdentityFieldPatch::Remove => options.vault = None,
        }
    }
    pub fn can_sign(&self) -> bool {
        can_sign_with(&BaseSigningView(self))
    }
    pub fn is_public_data_only<V: Exchange + ?Sized>(&self, venue: &V) -> bool {
        is_public_data_only_with(
            || self.options().public_data_only == Some(true),
            || venue.can_sign(),
        )
    }
    pub fn is_private_only(&self) -> bool {
        self.options().private_only == Some(true)
    }
    pub fn uses_external_public_data(&self) -> bool {
        self.options().external_public_data.is_some()
    }
    pub fn needs_account_market_overlay<V: Exchange + ?Sized>(&self, venue: &V) -> bool {
        self.uses_external_public_data() && !venue.is_public_data_only()
    }
    pub fn wait_for_remote_signer(&self) -> ExchangeFuture<'_, bool> {
        let Some(signer) = &self.signers.remote_signer else {
            return Box::pin(async { Ok(true) });
        };
        if signer.is_available() {
            return Box::pin(async { Ok(true) });
        }
        if let Err(error) = self.emit("remoteSignerStatus", Js::str("disconnected")) {
            return Box::pin(async move { Err(error) });
        }
        if self.is_disposed() {
            return Box::pin(async { Ok(false) });
        }
        if signer.is_available() {
            if let Err(error) = self.emit("remoteSignerStatus", Js::str("connected")) {
                return Box::pin(async move { Err(error) });
            }
            return Box::pin(async { Ok(true) });
        }
        let listeners = self.listeners.clone();
        let identity = Rc::new(());
        let finished_identity = identity.clone();
        let waiters = Rc::downgrade(&self.signer_waiters);
        let runtime = self.runtime.clone();
        let waiter = Rc::new(SignerWaiter {
            settled: Cell::new(false),
            on_error: Box::new(move |error| {
                runtime.report_exception(
                    error,
                    &json!({"operation":"remote signer availability"}).into(),
                )
            }),
            identity,
            on_finish: Box::new(move || {
                if let Some(waiters) = waiters.upgrade() {
                    waiters
                        .borrow_mut()
                        .retain(|row| !Rc::ptr_eq(&row.identity, &finished_identity));
                }
            }),
            result: RefCell::new(None),
            waker: RefCell::new(None),
            unsubscribe: RefCell::new(None),
            on_available: Box::new(move || {
                emit_listeners(&listeners, "remoteSignerStatus", &[Js::str("connected")])
            }),
        });
        let guard = SignerWaitGuard(waiter.clone());
        self.signer_waiters.borrow_mut().push(waiter.clone());
        let weak = Rc::downgrade(&waiter);
        let cleanup = signer.on_availability_change(Rc::new(move |available| {
            if available && let Some(waiter) = weak.upgrade() {
                waiter.finish(true);
            }
        }));
        if waiter.settled.get() {
            cleanup();
        } else {
            *waiter.unsubscribe.borrow_mut() = Some(cleanup);
        }
        Box::pin(async move {
            let _guard = guard;
            std::future::poll_fn(|cx| {
                if let Some(value) = *waiter.result.borrow() {
                    Poll::Ready(Ok(value))
                } else {
                    *waiter.waker.borrow_mut() = Some(cx.waker().clone());
                    Poll::Pending
                }
            })
            .await
        })
    }
    pub(crate) fn dispose_signer_waiters(&self) {
        let waiters = std::mem::take(&mut *self.signer_waiters.borrow_mut());
        for waiter in waiters {
            waiter.finish(false);
        }
    }
}

impl BaseExchange {
    pub fn mint_client_order_id(
        &self,
        options: impl Into<safe_cex_types::PreparedOrderOpts>,
    ) -> Option<String> {
        let options = options.into();
        let rule = self.settings.client_order_id_rule.as_ref()?;
        if !rule.is_honorable_shape.as_ref().map_or_else(
            || is_regular_client_order_shape(&options),
            |check| check(&options),
        ) {
            return None;
        }
        let markets = self.store.markets_by_symbol();
        let market = markets.get(&options.symbol)?;
        if market.limits.amount.max > 0.0
            && safe_cex_num::adjust(options.amount, market.precision.amount)
                > market.limits.amount.max
        {
            return None;
        }
        drop(markets);
        let candidates = match &rule.mint {
            Some(mint) => vec![mint()],
            None => default_mint_candidates(self.runtime.as_ref()),
        };
        candidates
            .into_iter()
            .find(|candidate| (rule.is_valid)(candidate))
    }

    pub async fn place_order_with_client_order_id<
        O: std::borrow::Borrow<PlaceOrderOpts> + 'static,
    >(
        &self,
        venue: &dyn Exchange,
        options: O,
    ) -> Result<PlacedOrderRef, ExchangeError> {
        if std::borrow::Borrow::<PlaceOrderOpts>::borrow(&options)
            .client_order_id
            .is_none()
        {
            let name = serde_json::to_value(&self.name).unwrap();
            return self.reject_preflight(format!(
                "{} placeOrderWithClientOrderId requires a clientOrderId",
                name.as_str().unwrap()
            ));
        }
        let client_order_id = venue
            .base()
            .resolve_caller_client_order_id(std::borrow::Borrow::<PlaceOrderOpts>::borrow(
                &options,
            ))?
            .unwrap();
        let order_ids = self.place_order(venue, options).await?;
        Ok(PlacedOrderRef {
            order_ids,
            client_order_id,
        })
    }

    pub async fn find_order_by_client_order_id(
        &self,
        venue: &dyn Exchange,
        symbol: &str,
        id: &str,
    ) -> Result<ClientOrderLookup, ExchangeError> {
        if self.settings.client_order_id_rule.is_none() {
            return Ok(ClientOrderLookup::NotFound { definitive: false });
        }
        for order in self.store.orders().iter() {
            if order.symbol == symbol && order_carries_client_order_id(order, id)? {
                return Ok(ClientOrderLookup::Found(Box::new(order.clone())));
            }
        }
        let orders = venue.fetch_orders(Some(vec![symbol.into()])).await?;
        for order in orders {
            if order.symbol == symbol && order_carries_client_order_id(&order, id)? {
                return Ok(ClientOrderLookup::Found(Box::new(order)));
            }
        }
        Ok(ClientOrderLookup::NotFound { definitive: false })
    }

    pub async fn cancel_symbol_orders<E: Exchange + ?Sized>(
        &self,
        venue: &E,
        symbol: &str,
    ) -> Result<(), ExchangeError> {
        let orders: Vec<_> = self
            .store
            .orders()
            .iter()
            .filter(|order| order.symbol == symbol)
            .cloned()
            .collect();
        if !orders.is_empty() {
            venue.cancel_orders(orders).await?;
        }
        Ok(())
    }

    pub async fn cancel_all_orders<E: Exchange + ?Sized>(
        &self,
        venue: &E,
    ) -> Result<(), ExchangeError> {
        let mut seen = HashSet::new();
        let symbols: Vec<_> = self
            .store
            .orders()
            .iter()
            .filter_map(|order| {
                seen.insert(order.symbol.clone())
                    .then_some(order.symbol.clone())
            })
            .collect();
        for symbol in symbols {
            if !self.is_disposed() {
                venue.cancel_symbol_orders(symbol).await?;
            }
        }
        Ok(())
    }

    pub fn derive_algo_orders_from_normal_orders_opts(
        &self,
        options: &[PlaceOrderOpts],
    ) -> Vec<PlaceOrderOpts> {
        let mut result = Vec::new();
        for order in options {
            let side = if order.side == OrderSide::Buy {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
            for (price, kind) in [
                (order.stop_loss, OrderType::StopLoss),
                (order.take_profit, OrderType::TakeProfit),
            ] {
                if let Some(price) = price.filter(|price| *price != 0.0 && !price.is_nan()) {
                    let mut derived = order_options(order.symbol.clone(), side, kind, order.amount);
                    derived.price = Some(price);
                    if kind == OrderType::StopLoss {
                        derived.sl_trigger_type = order.sl_trigger_type;
                    } else {
                        derived.tp_trigger_type = order.tp_trigger_type;
                    }
                    derived.max_slippage_pct = order.max_slippage_pct;
                    result.push(derived);
                }
            }
        }
        result
    }

    pub async fn nuke(&self, venue: &dyn Exchange, tries: u32) -> Result<(), ExchangeError> {
        self.nuke_inner(venue, tries, true).await
    }

    pub async fn nuke_positions(
        &self,
        venue: &dyn Exchange,
        tries: u32,
    ) -> Result<(), ExchangeError> {
        self.nuke_inner(venue, tries, false).await
    }

    async fn nuke_inner(
        &self,
        venue: &dyn Exchange,
        mut tries: u32,
        cancel: bool,
    ) -> Result<(), ExchangeError> {
        loop {
            if !self.is_disposed() && !self.is_nuking.get() {
                let guard = crate::order_operations::NukeGuard::new(&self.is_nuking);
                let result: Result<(), ExchangeError> = async {
                    let positions: Vec<_> = self
                        .store
                        .positions()
                        .iter()
                        .filter(|position| position.contracts > 0.0)
                        .cloned()
                        .collect();
                    for position in positions {
                        let side = if position.side == PositionSide::Long {
                            OrderSide::Sell
                        } else {
                            OrderSide::Buy
                        };
                        let mut order = order_options(
                            position.symbol.clone(),
                            side,
                            OrderType::Market,
                            position.contracts,
                        );
                        order.reduce_only = Some(true);
                        if let Err(error) = self.place_order(venue, order).await {
                            self.log(
                                &format!(
                                    "Nuke: failed to close position on {}: {}",
                                    position.symbol, error
                                ),
                                "error",
                            )?;
                        }
                    }
                    if cancel {
                        venue.cancel_all_orders().await?;
                    }
                    Ok(())
                }
                .await;
                guard.finish();
                result?;
            }
            // ts-bug: BaseExchange.nuke retries from the store even after disposal.
            if tries >= 3
                || !self
                    .store
                    .positions()
                    .iter()
                    .any(|position| position.contracts > 0.0)
            {
                return Ok(());
            }
            self.runtime.sleep(100).await?;
            tries += 1;
        }
    }

    pub async fn nuke_orders(&self, venue: &dyn Exchange) -> Result<(), ExchangeError> {
        if self.is_disposed() || self.is_nuking.get() {
            return Ok(());
        }
        let guard = crate::order_operations::NukeGuard::new(&self.is_nuking);
        let result = venue.cancel_all_orders().await;
        guard.finish();
        result
    }

    pub fn is_tp_sl_or_trailing_order(&self, order: &Order) -> bool {
        matches!(
            order.r#type,
            safe_cex_types::ModelField::Value(
                OrderType::TakeProfit | OrderType::StopLoss | OrderType::TrailingStopLoss
            )
        )
    }

    pub fn is_tp_sl_order(&self, order: &Order) -> bool {
        matches!(
            order.r#type,
            safe_cex_types::ModelField::Value(OrderType::TakeProfit | OrderType::StopLoss)
        )
    }
}

impl BaseExchange {
    pub async fn refresh_markets(
        &self,
        venue: &dyn Exchange,
    ) -> Result<MarketChanges, ExchangeError> {
        let old = self.store.markets().to_vec();
        let new = venue.fetch_markets().await?;
        let changes = market_changes(&old, &new);
        // ts-bug: BaseExchange.refreshMarkets ignores changed fields when IDs stay equal.
        if !changes.added.is_empty() || !changes.removed.is_empty() {
            let total = new.len();
            self.store.update(
                StoreUpdate {
                    markets: Some(new),
                    ..Default::default()
                },
                false,
            )?;
            self.emit(
                "markets:changed",
                Js::object(vec![
                    ("added", from_typed(&changes.added)?),
                    ("removed", from_typed(&changes.removed)?),
                ]),
            )?;
            self.log(
                &format!(
                    "Markets refreshed: +{} -{} (total: {total})",
                    changes.added.len(),
                    changes.removed.len()
                ),
                "info",
            )?;
        }
        Ok(changes)
    }

    pub async fn fetch_leverage_for_symbol<V: Exchange + ?Sized>(
        &self,
        venue: &V,
        symbol: &str,
    ) -> Result<Option<FetchedLeverage>, ExchangeError> {
        let result: Result<_, ExchangeError> = async {
            let leverage = venue.fetch_symbol_max_leverage(symbol.into()).await?;
            let leverage = FetchedLeverage {
                buy_leverage: leverage,
                sell_leverage: leverage,
                margin_type: None,
                position_mode: None,
                is_fetching: None,
            };
            let mut fetched = self.store.fetched_leverage().clone();
            fetched.insert(symbol.into(), leverage.clone());
            self.store.update(
                StoreUpdate {
                    fetched_leverage: Some(fetched),
                    ..Default::default()
                },
                false,
            )?;
            Ok(leverage)
        }
        .await;
        match result {
            Ok(leverage) => Ok(Some(leverage)),
            Err(error) => {
                self.log(
                    &format!(
                        "Error fetching leverage for {symbol}: {}",
                        error.js_error_string()
                    ),
                    "warning",
                )?;
                Ok(None)
            }
        }
    }

    pub async fn set_all_leverage(
        &self,
        venue: &dyn Exchange,
        leverage: f64,
    ) -> Result<(), ExchangeError> {
        let positions = self.store.positions().to_vec();
        for position in positions {
            if !self.is_disposed() {
                venue.set_leverage(position.symbol, leverage, None).await?;
            }
        }
        Ok(())
    }

    pub async fn fetch_and_set_orders<V: Exchange + ?Sized>(
        &self,
        venue: &V,
        symbols: Option<Vec<String>>,
    ) -> Result<Vec<Order>, ExchangeError> {
        let orders = venue.fetch_orders(symbols.clone()).await?;
        if let Some(symbols) = symbols {
            for symbol in symbols {
                let scoped = orders
                    .iter()
                    .filter(|order| order.symbol == symbol)
                    .cloned()
                    .collect();
                self.store.set_orders_for_symbol(&symbol, scoped)?;
            }
        } else {
            let mut loaded = self.store.loaded().clone();
            loaded.orders = true;
            self.store.update(
                StoreUpdate {
                    orders: Some(orders.clone()),
                    loaded: Some(loaded),
                    ..Default::default()
                },
                false,
            )?;
        }
        Ok(orders)
    }

    pub fn remove_orders(&self, ids: &[String]) -> Result<(), ExchangeError> {
        self.store
            .remove_orders(&ids.iter().map(String::as_str).collect::<Vec<_>>())?;
        Ok(())
    }

    pub async fn fetch_and_set_private_snapshot<V: Exchange + ?Sized>(
        &self,
        venue: &V,
        tries: u32,
    ) -> Result<(), ExchangeError> {
        if venue.is_public_data_only() || self.is_disposed() {
            return Ok(());
        }
        for attempt in 0..tries {
            if self.is_disposed() {
                return Ok(());
            }
            // The source catch includes the committed Store notification, not
            // only the network fetch. A listener failure enters the same retry.
            let attempt_result: Result<(), ExchangeError> = async {
                let orders = venue.fetch_orders(None).await?;
                if self.is_disposed() {
                    return Ok(());
                }
                let mut loaded = self.store.loaded().clone();
                loaded.orders = true;
                self.store.update(
                    StoreUpdate {
                        orders: Some(orders),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )?;
                Ok(())
            }
            .await;
            match attempt_result {
                Ok(()) => return Ok(()),
                Err(error) => {
                    if self.is_disposed() {
                        return Ok(());
                    }
                    self.emit_and_log_error(
                        ExchangeErrorInput::Error(Box::new(error)),
                        None,
                        None,
                    )?;
                    if attempt + 1 < tries {
                        self.runtime.sleep(1000).await?;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_or_fetch_positions_for_symbol(
        &self,
        venue: Rc<dyn Exchange>,
        symbol: &str,
    ) -> Result<Vec<Position>, ExchangeError> {
        if self.settings.positions_supported == Some(false) {
            return Ok(vec![]);
        }
        let found: Vec<_> = self
            .store
            .positions()
            .iter()
            .filter(|position| position.symbol == symbol)
            .cloned()
            .collect();
        if !found.is_empty() {
            return Ok(found);
        }
        let existing = self.position_fetches.borrow().get(symbol).cloned();
        if let Some(existing) = existing {
            return existing.wait().await;
        }
        if !self.store.loaded().positions {
            return Ok(vec![]);
        }
        let name = serde_json::to_value(&self.name).unwrap();
        self.runtime.console(
            "warning",
            &format!(
                "No positions found for {symbol} on {}, fetching...",
                name.as_str().unwrap()
            ),
        );
        let fetch = Rc::new(PositionFetch {
            origin_disposed: Cell::new(false),
            origin_error: RefCell::new(None),
            result: RefCell::new(None),
            wakers: crate::waiters::Waiters::default(),
            task: RefCell::new(None),
        });
        self.position_fetches
            .borrow_mut()
            .insert(symbol.into(), fetch.clone());
        let completion = fetch.clone();
        let requested = symbol.to_owned();
        let guard = crate::snapshots::PositionFetchGuard(completion.clone());
        let task = self.runtime.spawn_local(Box::pin(async move {
            let result = venue.fetch_positions(Some(vec![requested.clone()])).await;
            if let Ok(positions) = &result {
                let base = venue.base();
                completion.origin_disposed.set(base.is_disposed());
                if !base.is_disposed() {
                    base.position_fetches.borrow_mut().remove(&requested);
                    if let Err(error) = base.store.add_or_update_positions(positions.clone()) {
                        *completion.origin_error.borrow_mut() = Some(error.into());
                    }
                }
            }
            completion.finish(result);
            drop(guard);
        }));
        *fetch.task.borrow_mut() = Some(task);
        // ts-bug: BaseExchange.getOrFetchPositionsForSymbol retains rejected/disposed promises.
        let positions = fetch.wait().await?;
        if let Some(error) = fetch.origin_error.borrow().as_ref() {
            return Err(error.clone());
        }
        if fetch.origin_disposed.get() {
            return Ok(vec![]);
        }
        Ok(positions)
    }

    pub fn adapt_external_markets(&self, markets: Vec<Market>) -> Vec<Market> {
        let overlay = self.account_market_overlay.borrow().clone();
        match overlay {
            Some(overlay) => overlay(markets),
            None => markets,
        }
    }

    pub async fn load_account_market_overlay(
        &self,
        load: ExchangeFuture<'_, Option<AccountMarketOverlay>>,
    ) -> Result<(), ExchangeError> {
        let result: Result<(), ExchangeError> = async {
            *self.account_market_overlay.borrow_mut() = load.await?;
            if self.is_disposed() {
                return Ok(());
            }
            let markets = self.store.markets().to_vec();
            if markets.is_empty() {
                return Ok(());
            }
            self.store.update(
                StoreUpdate {
                    markets: Some(self.adapt_external_markets(markets)),
                    ..Default::default()
                },
                false,
            )?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {}
            Err(error) => {
                let response = error.response_json_view();
                let response_message = response
                    .as_ref()
                    .and_then(|response| response.get("data"))
                    .and_then(|data| data.get("msg"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|message| !message.is_empty());
                let message = response_message
                    .or_else(|| (!error.message.is_empty()).then_some(error.message.as_str()))
                    .map(|message| format!("Exchange Error: {message}"));
                self.emit_and_log_error(
                    ExchangeErrorInput::Error(Box::new(error)),
                    message.as_deref(),
                    None,
                )?;
            }
        }
        Ok(())
    }

    pub async fn wait_for_external_public_data_loaded(&self) -> Result<(), ExchangeError> {
        if self.store.loaded().markets && self.store.loaded().tickers {
            return Ok(());
        }
        let deadline = self.runtime.now_ms() + 30_000.0;
        loop {
            self.runtime.sleep(100).await?;
            if self.runtime.now_ms() >= deadline {
                self.log(
                    "Timeout waiting for external public data (markets/tickers)",
                    "warning",
                )?;
                return Ok(());
            }
            if self.is_disposed() {
                return Ok(());
            }
            let loaded = self.store.loaded();
            if loaded.markets && loaded.tickers {
                drop(loaded);
                self.log("External public data loaded (markets + tickers)", "info")?;
                return Ok(());
            }
        }
    }
}

impl BaseExchange {
    pub fn set_socket(&self, slot: SocketSlot, socket: Option<Rc<BaseWebSocket>>) {
        self.sockets.borrow_mut()[slot as usize] = socket.map(SocketRegistration::Owned);
    }
    /// Register an already-owned socket without creating a parent/child Rc cycle.
    /// The concrete venue owner retains the socket lease; lookups preserve slot order.
    pub fn set_borrowed_socket(&self, slot: SocketSlot, socket: &Rc<BaseWebSocket>) {
        self.sockets.borrow_mut()[slot as usize] =
            Some(SocketRegistration::Borrowed(Rc::downgrade(socket)));
    }
    /// Resolve a source-owned socket lazily, including replacement and removal.
    /// The lookup must hold weak owner references to avoid a parent/child cycle.
    pub fn set_socket_lookup(
        &self,
        slot: SocketSlot,
        lookup: Rc<dyn Fn() -> Option<Rc<BaseWebSocket>>>,
    ) {
        self.sockets.borrow_mut()[slot as usize] = Some(SocketRegistration::Lookup(lookup));
    }
    pub fn get_socket(&self, slot: SocketSlot) -> Option<Rc<BaseWebSocket>> {
        self.sockets.borrow()[slot as usize]
            .as_ref()
            .and_then(SocketRegistration::upgrade)
    }
    pub fn get_all_sockets(&self) -> Vec<Rc<BaseWebSocket>> {
        self.sockets
            .borrow()
            .iter()
            .flatten()
            .filter_map(SocketRegistration::upgrade)
            .collect()
    }
    /// Source recoverAfterOffline maps every socket before every(Boolean).
    /// The dispatcher binds each socket's source subclass recovery policy.
    pub fn recover_after_offline(
        &self,
        recover: impl Fn(Rc<crate::websocket::BaseWebSocket>) -> Result<Option<bool>, ExchangeError>,
    ) -> Result<bool, ExchangeError> {
        let values = self
            .get_all_sockets()
            .into_iter()
            .map(|socket| recover(socket).map(|value| value.unwrap_or(true)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(values.into_iter().all(|value| value))
    }
    pub fn get_public_sockets(&self) -> Vec<Rc<BaseWebSocket>> {
        self.sockets.borrow()[1..]
            .iter()
            .flatten()
            .filter_map(SocketRegistration::upgrade)
            .collect()
    }
    pub fn get_private_sockets(&self) -> Vec<Rc<BaseWebSocket>> {
        self.sockets.borrow()[..1]
            .iter()
            .flatten()
            .filter_map(SocketRegistration::upgrade)
            .collect()
    }
    pub fn set_online_status(&self, online: bool) -> Result<(), ExchangeError> {
        let offline = !self.online.replace(online);
        if offline && online {
            self.log(
                "Network came online - triggering WebSocket reconnection",
                "info",
            )?;
            self.recover_after_offline(|socket| socket.recover_after_offline(true).map(Some))?;
        } else if !online {
            self.log(
                "Network went offline - WebSocket reconnects will be paused",
                "info",
            )?;
        }
        Ok(())
    }
    pub fn reconnect(&self) -> Result<(), ExchangeError> {
        for socket in self.get_all_sockets() {
            socket.reconnect()?;
        }
        Ok(())
    }
    pub fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        let now = self.runtime.now_ms();
        if now - self.last_reconnect_emit.get() > 5000.0 {
            self.last_reconnect_emit.set(now);
            self.emit("ws-reconnected", Js::str(""))?;
        }
        Ok(())
    }
    pub fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.start_watchdog();
        self.sync_socket_health_to_store_internal()?;
        Ok(())
    }
    /// Bind custom parent assemblies to their stable shared core owner.
    /// Child/job leases may outlive the SDK trait wrapper; the watchdog is weak.
    pub fn bind_socket_watchdog(self: &Rc<Self>) {
        *self.watchdog_base.borrow_mut() = Some(Rc::downgrade(self));
    }
    pub(crate) fn start_watchdog(&self) {
        if self.watchdog.borrow().is_some() || self.is_disposed() {
            return;
        }
        let owner = self
            .watchdog_base
            .borrow()
            .clone()
            .map(WatchdogOwner::Core)
            .or_else(|| self.venue_weak.borrow().clone().map(WatchdogOwner::Venue));
        let Some(owner) = owner else {
            return;
        };
        let generation = self.watchdog_generation.get().wrapping_add(1);
        self.watchdog_generation.set(generation);
        let completed = Rc::new(Cell::new(false));
        let mut guard = WatchdogGuard {
            owner: owner.clone(),
            generation,
            completed: completed.clone(),
        };
        let sleep = self.runtime.sleep(10000);
        let task = self.runtime.spawn_local(Box::pin(async move {
            let result = sleep.await;
            guard.release();
            if result.is_err() {
                return;
            }
            let _ = owner.with_base(|exchange| {
                if let Err(error) = exchange.audit_socket_health() {
                    exchange
                        .runtime
                        .report_exception(&error, &json!({"operation":"socket watchdog"}).into());
                    return;
                }
                exchange.start_watchdog();
            });
        }));
        if !completed.get() && self.watchdog_generation.get() == generation && !self.is_disposed() {
            *self.watchdog.borrow_mut() = Some(task);
        } else {
            task.cancel();
        }
    }
    pub(crate) fn stop_watchdog(&self) {
        self.watchdog_generation
            .set(self.watchdog_generation.get().wrapping_add(1));
        let task = self.watchdog.borrow_mut().take();
        if let Some(task) = task {
            task.cancel();
        }
    }
    pub(crate) fn audit_socket_health(&self) -> Result<(), ExchangeError> {
        if self.is_disposed() {
            self.stop_watchdog();
            return Ok(());
        }
        let now = self.runtime.now_ms();
        for socket in self.get_all_sockets() {
            let state = socket.diagnostics();
            let received = state.last_message_timestamp.unwrap_or(0.0);
            let connected = socket.is_connected();
            let elapsed = if received > 0.0 {
                now - received
            } else {
                f64::INFINITY
            };
            let stale = socket.get_watchdog_stale_threshold_ms().unwrap_or(30000.0);
            if connected && state.initialized && received > 0.0 && elapsed > stale {
                self.log(
                    &format!(
                        "[Watchdog] Socket stale: {}s since last message, forcing reconnect",
                        (elapsed / 1000.0).round()
                    ),
                    "warning",
                )?;
                socket.reconnect()?;
                continue;
            }
            if connected && !state.initialized {
                let since = if state.last_connected_at > 0.0 {
                    now - state.last_connected_at
                } else {
                    0.0
                };
                if since > 60000.0 {
                    self.log(&format!("[Watchdog] Socket never received messages after {}s, forcing reconnect",(since/1000.0).round()),"warning")?;
                    socket.reconnect()?;
                    continue;
                }
            }
            if connected
                && state.initialized
                && received == 0.0
                && state.last_connected_at > 0.0
                && now - state.last_connected_at > 60000.0
            {
                self.log("[Watchdog] Reconnected socket received no messages within 60s, forcing reconnect","warning")?;
                socket.reconnect()?;
            }
        }
        self.sync_socket_health_to_store_internal()?;
        Ok(())
    }
    pub(crate) fn sync_socket_health_to_store_internal(&self) -> Result<(), ExchangeError> {
        let public = self.get_public_sockets();
        let private = self.get_private_sockets();
        let initialized_public: Vec<_> = public
            .iter()
            .filter(|socket| socket.diagnostics().initialized)
            .collect();
        let initialized_private: Vec<_> = private
            .iter()
            .filter(|socket| socket.diagnostics().initialized)
            .collect();
        let provider = self
            .public_source_health()
            .filter(|(initialized, _, _)| *initialized);
        let public_initialized = !initialized_public.is_empty() || provider.is_some();
        let private_initialized = !initialized_private.is_empty();
        let public_healthy = !public_initialized
            || initialized_public.iter().any(|socket| socket.is_healthy())
            || provider.is_some_and(|(_, healthy, _)| healthy);
        let private_healthy = private.iter().all(|socket| socket.is_recovery_complete())
            && (!private_initialized
                || initialized_private.iter().any(|socket| socket.is_healthy()));
        let mut public_at = initialized_public
            .iter()
            .filter_map(|socket| socket.last_message_timestamp())
            .reduce(f64::max);
        if let Some((_, _, Some(at))) = provider {
            public_at = Some(public_at.map_or(at, |old| old.max(at)));
        }
        let private_at = initialized_private
            .iter()
            .filter_map(|socket| socket.last_message_timestamp())
            .reduce(f64::max);
        let mut health = json!({"publicHealthy":public_healthy,"privateHealthy":private_healthy,"publicInitialized":public_initialized,"privateInitialized":private_initialized});
        let fields = health.as_object_mut().expect("health object");
        if let Some(at) = public_at.filter(|at| *at != 0.0) {
            fields.insert("publicLastMessage".into(), json!(at));
        }
        if let Some(at) = private_at.filter(|at| *at != 0.0) {
            fields.insert("privateLastMessage".into(), json!(at));
        }
        fields.extend(
            socket_diagnostics(&public, false)
                .as_object()
                .expect("public diagnostics")
                .clone(),
        );
        fields.extend(
            socket_diagnostics(&private, true)
                .as_object()
                .expect("private diagnostics")
                .clone(),
        );
        self.publish_socket_health(health)?;
        Ok(())
    }
    pub fn fetch_last_logs_from_websockets(&self) -> Vec<String> {
        self.get_all_sockets()
            .iter()
            .flat_map(|socket| socket.last_log.borrow().iter().cloned().collect::<Vec<_>>())
            .collect()
    }
}

impl BaseExchange {
    pub fn listen_ohlcv_webworker(
        self: &Rc<Self>,
        symbol: String,
        interval: Timeframe,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<crate::runtime::ListenerCleanup, ExchangeError> {
        let sub_id = self.runtime.unique_id();
        let params = Js::object(vec![
            ("symbol", Js::str(&symbol)),
            ("interval", from_typed(&interval)?),
            ("subId", Js::str(&sub_id)),
        ]);
        self.emit("subscribeOHLCV", params.clone())?;
        let listener: Listener = Rc::new(move |_, event| {
            if event.get("symbol")?.strict_equals(&Js::str(&symbol))
                && event
                    .get("interval")?
                    .strict_equals(&from_typed(&interval)?)
                && let Ok(candle) = to_typed(&event.get("candle")?)
            {
                callback(candle);
            }
            Ok(())
        });
        let id = self.on("listenOHLCV", listener);
        let weak = Rc::downgrade(self);
        Ok(Box::new(move || {
            if let Some(base) = weak.upgrade() {
                base.off("listenOHLCV", Some(&id));
                base.emit("unsubscribeOHLCV", params)?;
            }
            Ok(())
        }))
    }
    pub fn listen_order_book_webworker(
        self: &Rc<Self>,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<crate::runtime::ListenerCleanup, ExchangeError> {
        let params = Js::object(vec![
            ("symbol", Js::str(&symbol)),
            ("subId", Js::str(&self.runtime.unique_id())),
        ]);
        self.emit("subscribeOrderBook", params.clone())?;
        let id = self.on(
            "listenOrderBook",
            Rc::new(move |_, event| {
                if event.get("symbol")?.strict_equals(&Js::str(&symbol))
                    && let Ok(book) = to_typed(&event.get("orderbook")?)
                {
                    callback(book);
                }
                Ok(())
            }),
        );
        let weak = Rc::downgrade(self);
        Ok(Box::new(move || {
            if let Some(base) = weak.upgrade() {
                base.off("listenOrderBook", Some(&id));
                base.emit("unsubscribeOrderBook", params)?;
            }
            Ok(())
        }))
    }
    async fn worker_reply(
        &self,
        event: &str,
        done: &str,
        mut request: Value,
        timeout_message: &str,
    ) -> Result<Js, ExchangeError> {
        let request_id = self.runtime.unique_id();
        request["requestId"] = json!(request_id);
        self.emit(event, from_json(&request))?;
        let reply = Rc::new(RefCell::new(None));
        let waker: Rc<RefCell<Option<Waker>>> = Rc::default();
        let result = reply.clone();
        let wake = waker.clone();
        let id = self.on(
            done,
            Rc::new(move |_, value| {
                if value.get("requestId")?.strict_equals(&Js::str(&request_id)) {
                    *result.borrow_mut() = Some(value.clone());
                    let pending = wake.borrow_mut().take();
                    if let Some(pending) = pending {
                        pending.wake();
                    }
                }
                Ok(())
            }),
        );
        let mut timeout = self.runtime.sleep(5_000);
        let guard = WorkerReplyGuard {
            listeners: self.listeners.clone(),
            event: done.into(),
            id,
        };
        let result = std::future::poll_fn(|cx| {
            if let Some(value) = reply.borrow_mut().take() {
                return Poll::Ready(Ok(value));
            }
            if timeout.as_mut().poll(cx).is_ready() {
                return Poll::Ready(Err(ExchangeError::new(timeout_message)));
            }
            *waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        })
        .await;
        drop(guard);
        result
    }
    pub async fn update_positions_in_webworker(
        &self,
        symbols: Option<Vec<String>>,
    ) -> Result<(), ExchangeError> {
        let mut request = json!({});
        if let Some(symbols) = symbols {
            request["symbols"] = json!(symbols);
        }
        let reply = self
            .worker_reply(
                "updatePositions",
                "updatePositions:done",
                request,
                "Timeout updatePositionsInWebworker",
            )
            .await?;
        if reply.get("success")?.strict_equals(&Js::Bool(true)) {
            return Ok(());
        }
        let reason = reply.get("reason")?;
        let reason = reason
            .as_str()
            .map(|text| String::from_utf16_lossy(text))
            .filter(|text| !text.is_empty());
        if let Some(reason) = &reason
            && reason != "retry_too_soon"
        {
            self.runtime.console(
                "error",
                &format!("reject in updatePositionsInWebworker {reason}"),
            );
        }
        Err(ExchangeError::new(reason.unwrap_or_else(|| {
            "Unknown error in updatePositionsInWebworker".into()
        })))
    }
    pub async fn call_method_in_worker(
        &self,
        venue: &dyn Exchange,
        method: CallableMethodsInWorker,
        args: Vec<Value>,
    ) -> Result<Value, ExchangeError> {
        let reply = self
            .worker_reply(
                "callMethod",
                "callMethod:done",
                json!({"method":method,"args":args}),
                "Timeout callMethodInWorker",
            )
            .await?;
        let result = reply.get("result")?;
        if reply.get("success")?.strict_equals(&Js::Bool(true)) {
            let result = to_json(&result)?;
            self.after_method_called_in_worker(venue, &result, method, &args)?;
            return Ok(result);
        }
        let field = |key: &str| {
            if result.is_nullish() {
                Ok(Js::Undefined)
            } else {
                result.get(key)
            }
        };
        let text = |value: &Js| {
            value
                .as_str()
                .map(|text| String::from_utf16_lossy(text))
                .filter(|text| !text.is_empty())
        };
        let message = if result.is_object() {
            field("message")?
        } else {
            result.clone()
        };
        let mut error =
            ExchangeError::new(text(&message).unwrap_or_else(|| "Unknown error".into()));
        if let Some(name) = text(&field("name")?) {
            error.name = name;
        }
        error.stack = text(&field("stack")?);
        let code = field("code")?;
        error.code = (result.has_own("code") || !code.is_undefined()).then_some(code);
        Err(error)
    }
    pub fn after_method_called_in_worker(
        &self,
        venue: &dyn Exchange,
        result: &Value,
        method: CallableMethodsInWorker,
        args: &[Value],
    ) -> Result<(), ExchangeError> {
        venue.after_method_called_in_worker(result, method, args)
    }
}

impl BaseExchange {
    pub async fn init_external_public_data(
        &self,
        create: impl FnOnce(&ExchangeOptionsExternalPublicData) -> Rc<dyn ExternalPublicData>,
    ) -> Result<bool, ExchangeError> {
        let options = self.options();
        let Some(config) = &options.external_public_data else {
            return Ok(false);
        };
        if !self.settings.supports_public_only {
            let name = serde_json::to_value(&self.name).expect("exchange name");
            return Err(ExchangeError::new(format!(
                "Exchange \"{}\" does not support public-only mode (supportsPublicOnly is false). Cannot use externalPublicData with this exchange.",
                name.as_str().expect("name string")
            )));
        }
        match config {
            ExchangeOptionsExternalPublicData::Variant0(config) => self.log(
                &format!("Using external public data gateway: {}", config.url),
                "info",
            ),
            ExchangeOptionsExternalPublicData::Variant1(_) => {
                self.log("Using in-process public data source", "info")
            }
        }?;
        let provider = create(config);
        *self.external_public_data_provider.borrow_mut() = Some(provider.clone());
        provider.connect();
        self.wait_for_external_public_data_loaded().await?;
        Ok(true)
    }
    pub(crate) fn public_source_health(&self) -> Option<(bool, bool, Option<f64>)> {
        let provider = self.external_public_data_provider.borrow().clone();
        provider.map(|p| {
            (
                p.is_initialized(),
                p.is_healthy(),
                p.last_message_timestamp(),
            )
        })
    }
    pub(crate) fn publish_socket_health(&self, health: Value) -> Result<(), ExchangeError> {
        let Some(current) = health.as_object() else {
            return Ok(());
        };
        let mut merged = self
            .store
            .socket_health()
            .and_then(|old| old.as_object().cloned())
            .unwrap_or_default();
        // TS object spread writes undefined for cleared optional scalar diagnostics.
        for key in [
            "publicLastMessage",
            "privateLastMessage",
            "publicCloseCode",
            "privateCloseCode",
        ] {
            merged.shift_remove(key);
        }
        merged.extend(current.clone());
        self.store.update(
            StoreUpdate {
                socket_health: Some(Value::Object(merged)),
                ..Default::default()
            },
            false,
        )?;
        Ok(())
    }

    pub fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let provider = self.external_public_data_provider.borrow().clone();
        if let Some(provider) = provider {
            provider.listen_ohlcv(options, callback)
        } else {
            Ok(Box::new(|| Ok(())))
        }
    }
    pub fn listen_ohlcvs(&self, _options: OHLCVSOptions, _callback: Rc<dyn Fn(SubscribedCandle)>) {}
    pub fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let provider = self.external_public_data_provider.borrow().clone();
        if let Some(provider) = provider {
            provider.listen_order_book(symbol, callback)
        } else {
            Ok(Box::new(|| Ok(())))
        }
    }
    pub fn on_ws_public_close(&self) {}
    pub fn disable_public_websockets(&self) {
        let sockets = self.sockets.borrow().clone();
        for index in [1, 2, 3, 6, 7] {
            if let Some(socket) = sockets[index]
                .as_ref()
                .and_then(SocketRegistration::upgrade)
            {
                socket.disable_connect_and_subscribe();
            }
        }
    }
}

impl BaseExchange {
    pub fn create_xhr_with_secondary_key(
        &self,
        options: &ExchangeOptions,
        provider: &dyn TransportProvider,
        proxy_url: Option<&str>,
        create: impl Fn(&ExchangeOptions) -> Result<Arc<dyn RestAdapter>, TransportError>,
        rate_limit: Option<&RateLimitWrapper<'_>>,
        secondary_rate_limit: Option<&RateLimitWrapper<'_>>,
    ) -> Result<Rc<SecondaryKeyClients>, TransportError> {
        let clients = Rc::new(
            safe_cex_transport::secondary::create_api_with_secondary_key(
                options,
                provider,
                proxy_url,
                create,
                rate_limit,
                secondary_rate_limit,
            )?,
        );
        *self.secondary_clients.borrow_mut() = Some(clients.clone());
        Ok(clients)
    }
    pub async fn sign_http_request(
        &self,
        mut request: HttpSigningRequest,
    ) -> Result<HttpSigningResponse, ExchangeError> {
        let Some(xhr) = &self.signing_http_client else {
            return Ok(HttpSigningResponse {
                id: request.id,
                error: Some("Exchange does not have xhr for signing".into()),
                config: None,
            });
        };
        request.config.params = request
            .config
            .params
            .map(safe_cex_transport::signing::try_parse_params_json);
        request.config.data = request
            .config
            .data
            .map(safe_cex_transport::signing::try_parse_json);
        let captured = xhr.capture(request.config).await?;
        Ok(HttpSigningResponse {
            id: request.id,
            error: None,
            config: Some(SignedConfig {
                headers: captured.headers,
                url: Some(captured.url),
                params: captured.params,
                data: captured.data,
            }),
        })
    }
    pub fn sign_ws_auth(
        &self,
        request: &safe_cex_crypto::WsAuthRequest,
    ) -> safe_cex_crypto::WsAuthResponse {
        safe_cex_crypto::sign_ws_auth(request, &self.options().secret)
    }
}
