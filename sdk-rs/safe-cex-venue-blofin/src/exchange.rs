//! Owned BlofinExchange composition. Common base/runtime/HTTP capabilities are explicit.
use crate::{
    modes::{self, PositionSideOptions},
    order_formatters::{self, FormatHost, OrderPayload},
    order_reads::{self, OrderReadHost},
    order_rest::{self, WriteHost},
    order_services::{self, ServiceError, ServiceHost},
    position_reads::{
        self, BalanceAndPositions, HistoricalPositionsOptions, PositionMetadata, PositionReadHost,
    },
    private_stream_data::{BlofinPrivateWebsocket, PrivateDataHost},
    public_stream_data::{
        BlofinPublicWebsocket, BookCallback, CandleCallback, PublicCleanup, PublicDataError,
        PublicDataHost,
    },
    rest_lifecycle::{self, ConstructorFields},
    utility::UtilityInput,
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_store::{Store, StoreMutationError};
use safe_cex_transport::RestAdapter;
use safe_cex_types::{
    Balance, Candle, ClosedPnlEntry, ExchangeName, ExchangeOptions, Execution, ExecutionsOptions,
    FetchedLeverage, HistoricalOrder, HistoricalTradeOptions, IncomeHistoryOptions, IncomeRecord,
    MarginType, Market, OHLCVOptions, Order, OrderHistoryOptions, OrderStopTriggerType, Position,
    PositionMode, PositionRecord, PreparedOrderOpts, Ticker,
};
use serde_json::{Map, Value};
use std::{cell::Cell, rc::Rc};

/// A listener call returns the exact public-socket cleanup capability. Concrete
/// exchange construction always installs BlofinPublicWebsocket; this trait also
/// makes forwarding argument/return identity independently observable.
pub trait BlofinPublicListeners {
    type Error;
    fn listen_candles(
        &self,
        opts: OHLCVOptions,
        callback: CandleCallback<Self::Error>,
    ) -> Result<PublicCleanup<Self::Error>, PublicDataError<Self::Error>>;
    fn listen_book(
        &self,
        symbol: String,
        callback: BookCallback<Self::Error>,
    ) -> Result<PublicCleanup<Self::Error>, PublicDataError<Self::Error>>;
}
impl<H: PublicDataHost> BlofinPublicListeners for Rc<BlofinPublicWebsocket<H>> {
    type Error = H::Error;
    fn listen_candles(
        &self,
        opts: OHLCVOptions,
        callback: CandleCallback<Self::Error>,
    ) -> Result<PublicCleanup<Self::Error>, PublicDataError<Self::Error>> {
        BlofinPublicWebsocket::listen_ohlcv(self, opts, callback)
    }
    fn listen_book(
        &self,
        symbol: String,
        callback: BookCallback<Self::Error>,
    ) -> Result<PublicCleanup<Self::Error>, PublicDataError<Self::Error>> {
        BlofinPublicWebsocket::listen_order_book(self, symbol, callback)
    }
}
pub fn listen_ohlcv<P: BlofinPublicListeners>(
    socket: &P,
    opts: OHLCVOptions,
    callback: CandleCallback<P::Error>,
) -> Result<PublicCleanup<P::Error>, PublicDataError<P::Error>> {
    socket.listen_candles(opts, callback)
}
pub fn listen_order_book<P: BlofinPublicListeners>(
    socket: &P,
    symbol: String,
    callback: BookCallback<P::Error>,
) -> Result<PublicCleanup<P::Error>, PublicDataError<P::Error>> {
    socket.listen_book(symbol, callback)
}

pub type CommonHttpFactory =
    Box<dyn FnOnce(&ExchangeOptions, &str, i64) -> Result<Rc<dyn RestAdapter>, String>>;
pub type BlofinApiFactory = fn(
    &ExchangeOptions,
    Rc<dyn crate::api::ApiRuntime>,
    CommonHttpFactory,
) -> Result<Rc<dyn RestAdapter>, String>;
pub fn create_native_api(
    options: &ExchangeOptions,
    runtime: Rc<dyn crate::api::ApiRuntime>,
    safe_http: CommonHttpFactory,
) -> Result<Rc<dyn RestAdapter>, String> {
    crate::api::create_api(options, runtime, safe_http)
        .map(|api| Rc::new(api) as Rc<dyn RestAdapter>)
}

pub struct ExchangeContext {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub store: Rc<dyn Store>,
    pub auth_failure_streak: Cell<f64>,
    pub disposed: Cell<bool>,
}
/// Host owns the inherited BaseExchange and applies the common HTTP/secondary-key
/// factory with native crate::api::create_api signing. Socket hosts MUST refer to
/// this same context. Construction binds private parent callbacks to a Weak owner.
/// All venue REST/member bodies are bound below, never delegated to host stubs.
pub trait BlofinExchangeHost: 'static {
    type Error: std::fmt::Display + 'static;
    type Base: 'static;
    type Public: PublicDataHost<Error = Self::Error>;
    type Private: PrivateDataHost<Error = Self::Error>;
    fn super_constructor(&self, context: &Rc<ExchangeContext>) -> Result<Self::Base, Self::Error>;
    fn initialize_fields(
        &self,
        base: &Self::Base,
        fields: &ConstructorFields,
    ) -> Result<(), Self::Error>;
    fn create_xhr_with_secondary_key(
        &self,
        base: &Self::Base,
        options: &ExchangeOptions,
        factory: BlofinApiFactory,
    ) -> Result<Rc<dyn RestAdapter>, Self::Error>;
    fn assign_xhr(&self, base: &Self::Base, xhr: Rc<dyn RestAdapter>) -> Result<(), Self::Error>;
    fn public_host(
        &self,
        base: &Self::Base,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<Self::Public>, Self::Error>;
    fn private_host(
        &self,
        base: &Self::Base,
        context: &Rc<ExchangeContext>,
    ) -> Result<Rc<Self::Private>, Self::Error>;
    fn assign_public_socket(
        &self,
        base: &Self::Base,
        socket: Rc<BlofinPublicWebsocket<Self::Public>>,
    ) -> Result<(), Self::Error>;
    fn assign_private_socket(
        &self,
        base: &Self::Base,
        socket: Rc<BlofinPrivateWebsocket<Self::Private>>,
    ) -> Result<(), Self::Error>;
    fn super_dispose(&self, base: &Self::Base) -> Result<(), Self::Error>;
    fn emit_and_log_error(
        &self,
        base: &Self::Base,
        message: Option<&Value>,
    ) -> Result<(), Self::Error>;
    fn log(
        &self,
        base: &Self::Base,
        message: &str,
        severity: LogSeverity,
    ) -> Result<(), Self::Error>;
    fn console_warn(&self, message: &str) -> Result<(), Self::Error>;
    fn emit_detailed_error(
        &self,
        base: &Self::Base,
        error: &ServiceError<Self::Error>,
        hint: Option<&Value>,
    ) -> Result<(), Self::Error>;
    fn uuid(&self) -> String;
    fn nanoid(&self) -> String;
    fn clock_ms(&self) -> f64;
    fn wall_clock_ms(&self) -> f64;
    fn sleep(&self, delay: f64) -> rest_lifecycle::LifecycleFuture<'_, (), Self::Error>;
    fn init_external_public_data(
        &self,
        base: &Self::Base,
    ) -> rest_lifecycle::LifecycleFuture<'_, bool, Self::Error>;
    fn schedule_tick(
        &self,
        callback: Rc<dyn Fn() -> rest_lifecycle::LifecycleFuture<'static, (), Self::Error>>,
        delay: f64,
    ) -> Result<(), Self::Error>;
    /// The inherited base implements metadata/multiplier/auth-backoff policy and
    /// reads the same auth-failure cell used by the native API runtime.
    fn get_tick_interval(&self, base: &Self::Base) -> Option<f64>;
    fn broker_id_enabled(&self) -> bool;
    fn resolve_caller_client_order_id(
        &self,
        base: &Self::Base,
        rule: &safe_cex_exchange::client_order_id::ClientOrderIdRule,
        opts: &PreparedOrderOpts,
    ) -> Result<Option<String>, Self::Error>;
    fn assert_single_native_client_order_id(
        &self,
        base: &Self::Base,
        id: Option<&str>,
        count: usize,
    ) -> Result<(), Self::Error>;
    fn assert_caller_client_order_ids(
        &self,
        base: &Self::Base,
        opts: &[PreparedOrderOpts],
    ) -> Result<(), Self::Error>;
    fn public_error(&self, error: PublicDataError<Self::Error>) -> Self::Error;
    fn service_error(&self, error: ServiceError<Self::Error>) -> Self::Error;
}

pub struct BlofinExchange<H: BlofinExchangeHost> {
    pub context: Rc<ExchangeContext>,
    pub fields: ConstructorFields,
    pub xhr: Rc<dyn RestAdapter>,
    pub public_websocket: Rc<BlofinPublicWebsocket<H::Public>>,
    pub private_websocket: Rc<BlofinPrivateWebsocket<H::Private>>,
    pub base: H::Base,
    host: Rc<H>,
}
impl<H: BlofinExchangeHost> BlofinExchange<H> {
    pub fn new(
        host: Rc<H>,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<Rc<Self>, H::Error> {
        let context = Rc::new(ExchangeContext {
            name: name.clone(),
            options,
            store,
            auth_failure_streak: Cell::new(0.0),
            disposed: Cell::new(false),
        });
        let mut builder = ConstructorBridge {
            host: host.clone(),
            context: context.clone(),
            fields: None,
            xhr: None,
            public: None,
            private: None,
        };
        let base = rest_lifecycle::construct_exchange(
            &mut builder,
            name,
            &context.options,
            context.store.as_ref(),
        )?;
        let exchange = Rc::new(Self {
            context,
            fields: builder.fields.expect("initialized fields"),
            xhr: builder.xhr.expect("assigned xhr"),
            public_websocket: builder.public.expect("assigned public"),
            private_websocket: builder.private.expect("assigned private"),
            base,
            host,
        });
        exchange.bind_private_parent();
        Ok(exchange)
    }
    pub fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: CandleCallback<H::Error>,
    ) -> Result<PublicCleanup<H::Error>, PublicDataError<H::Error>> {
        listen_ohlcv(&self.public_websocket, opts, callback)
    }
    pub fn listen_order_book(
        &self,
        symbol: String,
        callback: BookCallback<H::Error>,
    ) -> Result<PublicCleanup<H::Error>, PublicDataError<H::Error>> {
        listen_order_book(&self.public_websocket, symbol, callback)
    }
    pub fn dispose(&self) -> Result<(), H::Error> {
        self.host.super_dispose(&self.base)?;
        self.public_websocket
            .dispose()
            .map_err(|e| self.host.public_error(e))?;
        self.private_websocket.dispose()
    }
    fn service(&self) -> ServiceHost<'_, H::Error> {
        ServiceHost {
            emit_and_log_error: Box::new(move |message| {
                self.host.emit_and_log_error(&self.base, message)
            }),
            log: Box::new(move |msg, level| self.host.log(&self.base, msg, level)),
        }
    }
    fn position_host<'a>(
        &'a self,
        service: &'a ServiceHost<'a, H::Error>,
    ) -> PositionReadHost<'a, H::Error> {
        PositionReadHost {
            service,
            modes: &self.fields.account_modes,
            auth_failure_streak: &self.context.auth_failure_streak,
            disposed: &self.context.disposed,
            console_warn: Box::new(move |s| self.host.console_warn(s)),
            emit_error_with_detail: Box::new(move |e, hint| {
                self.host.emit_detailed_error(&self.base, e, hint)
            }),
        }
    }
    fn format_host(&self) -> FormatHost<'_, H::Error> {
        FormatHost {
            get_margin_type: Box::new(move |s| {
                let s = s.to_owned();
                Box::pin(async move { self.get_or_fetch_margin_type(Some(&s), false).await })
            }),
            get_position_mode: Box::new(move |s| {
                let s = s.to_owned();
                Box::pin(async move {
                    self.get_or_fetch_position_mode(Some(&s), false)
                        .await
                        .map(Some)
                })
            }),
            uuid: Box::new(move || Ok(self.host.uuid())),
            resolve_caller_client_order_id: Box::new(move |o| {
                self.host
                    .resolve_caller_client_order_id(
                        &self.base,
                        &self.fields.client_order_id_rule,
                        o,
                    )
                    .map_err(ServiceError::Host)
            }),
            assert_single_native_client_order_id: Box::new(move |id, n| {
                self.host
                    .assert_single_native_client_order_id(&self.base, id, n)
                    .map_err(ServiceError::Host)
            }),
            cancel_algo_orders: Box::new(move |orders| {
                Box::pin(async move { self.cancel_algo_orders(&orders).await })
            }),
            broker_id_enabled: self.host.broker_id_enabled(),
        }
    }
    fn write_host<'a>(&'a self, service: &'a ServiceHost<'a, H::Error>) -> WriteHost<'a, H::Error> {
        WriteHost {
            service,
            format_normal_order: Box::new(move |opts| {
                Box::pin(async move { self.format_normal_order(&opts).await })
            }),
            format_algo_order: Box::new(move |opts| {
                Box::pin(async move { self.format_algo_order(&opts).await })
            }),
            format_trigger_order: Box::new(move |opts, source| {
                Box::pin(async move { self.format_trigger_order(&opts, source.as_ref()).await })
            }),
            place_order: Box::new(move |opts| {
                Box::pin(async move { self.place_order(&opts).await })
            }),
            get_margin_type: Box::new(move |symbol, refetch| {
                Box::pin(async move {
                    self.get_or_fetch_margin_type(symbol.as_deref(), refetch)
                        .await
                })
            }),
            get_position_mode: Box::new(move || {
                Box::pin(self.get_or_fetch_position_mode(None, false))
            }),
            fetch_positions: Box::new(move |symbols| {
                Box::pin(async move { self.fetch_positions(Some(&symbols)).await })
            }),
            sleep: Box::new(move |delay| self.host.sleep(delay as f64)),
            resolve_caller_client_order_id: Box::new(move |opts| {
                self.host
                    .resolve_caller_client_order_id(
                        &self.base,
                        &self.fields.client_order_id_rule,
                        opts,
                    )
                    .map(|_| ())
                    .map_err(ServiceError::Host)
            }),
            assert_caller_client_order_ids: Box::new(move |opts| {
                self.host
                    .assert_caller_client_order_ids(&self.base, opts)
                    .map_err(ServiceError::Host)
            }),
        }
    }
}
struct ConstructorBridge<H: BlofinExchangeHost> {
    host: Rc<H>,
    context: Rc<ExchangeContext>,
    fields: Option<ConstructorFields>,
    xhr: Option<Rc<dyn RestAdapter>>,
    public: Option<Rc<BlofinPublicWebsocket<H::Public>>>,
    private: Option<Rc<BlofinPrivateWebsocket<H::Private>>>,
}
enum SocketSlot<H: BlofinExchangeHost> {
    Public(Rc<BlofinPublicWebsocket<H::Public>>),
    Private(Rc<BlofinPrivateWebsocket<H::Private>>),
}
impl<H: BlofinExchangeHost> rest_lifecycle::ConstructorHost for ConstructorBridge<H> {
    type Error = H::Error;
    type Base = H::Base;
    type Xhr = Rc<dyn RestAdapter>;
    type Socket = SocketSlot<H>;
    fn super_constructor(
        &mut self,
        _name: ExchangeName,
        _opts: &ExchangeOptions,
        _store: &dyn Store,
    ) -> Result<H::Base, H::Error> {
        self.host.super_constructor(&self.context)
    }
    fn uuid(&self) -> Rc<dyn Fn() -> String> {
        let h = self.host.clone();
        Rc::new(move || h.uuid())
    }
    fn initialize_fields(
        &mut self,
        base: &H::Base,
        fields: ConstructorFields,
    ) -> Result<(), H::Error> {
        self.host.initialize_fields(base, &fields)?;
        self.fields = Some(fields);
        Ok(())
    }
    fn create_xhr_with_secondary_key(
        &mut self,
        base: &H::Base,
        opts: &ExchangeOptions,
    ) -> Result<Self::Xhr, H::Error> {
        self.host
            .create_xhr_with_secondary_key(base, opts, create_native_api)
    }
    fn assign_xhr(&mut self, base: &H::Base, xhr: Self::Xhr) -> Result<(), H::Error> {
        self.host.assign_xhr(base, xhr.clone())?;
        self.xhr = Some(xhr);
        Ok(())
    }
    fn create_public_socket(&mut self, base: &H::Base) -> Result<Self::Socket, H::Error> {
        let host = self.host.public_host(base, &self.context)?;
        BlofinPublicWebsocket::new(host)
            .map(SocketSlot::Public)
            .map_err(|e| self.host.public_error(e))
    }
    fn assign_public_socket(
        &mut self,
        base: &H::Base,
        socket: Self::Socket,
    ) -> Result<(), H::Error> {
        let SocketSlot::Public(socket) = socket else {
            unreachable!("constructor public slot")
        };
        self.host.assign_public_socket(base, socket.clone())?;
        self.public = Some(socket);
        Ok(())
    }
    fn create_private_socket(&mut self, base: &H::Base) -> Result<Self::Socket, H::Error> {
        let host = self.host.private_host(base, &self.context)?;
        BlofinPrivateWebsocket::new(host).map(SocketSlot::Private)
    }
    fn assign_private_socket(
        &mut self,
        base: &H::Base,
        socket: Self::Socket,
    ) -> Result<(), H::Error> {
        let SocketSlot::Private(socket) = socket else {
            unreachable!("constructor private slot")
        };
        self.host.assign_private_socket(base, socket.clone())?;
        self.private = Some(socket);
        Ok(())
    }
}

impl<H: BlofinExchangeHost> BlofinExchange<H> {
    pub async fn place_order_batch(
        &self,
        payloads: &[OrderPayload],
    ) -> Result<Vec<String>, ServiceError<H::Error>> {
        let service = self.service();
        order_rest::place_order_batch(payloads, self.xhr.as_ref(), &service).await
    }
    pub async fn place_algo_order_batch(
        &self,
        payloads: &[OrderPayload],
    ) -> Result<Vec<Option<Value>>, ServiceError<H::Error>> {
        let service = self.service();
        order_rest::place_algo_order_batch(payloads, self.xhr.as_ref(), &service).await
    }
    pub async fn place_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<Vec<Option<Value>>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::place_order(opts, self.xhr.as_ref(), &host).await
    }
    pub async fn place_orders(
        &self,
        opts: &[PreparedOrderOpts],
    ) -> Result<Vec<Option<Value>>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::place_orders(opts, self.xhr.as_ref(), &host).await
    }
    pub async fn cancel_normal_orders(
        &self,
        orders: &[Order],
    ) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::cancel_normal_orders(
            orders,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn cancel_algo_orders(&self, orders: &[Order]) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::cancel_algo_orders(
            orders,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn cancel_trigger_orders(
        &self,
        orders: &[Order],
    ) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::cancel_trigger_orders(
            orders,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn cancel_orders(&self, orders: &[Order]) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::cancel_orders(
            orders,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn change_margin_type(
        &self,
        isolated: bool,
        symbol: Option<&str>,
    ) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::change_margin_type(
            isolated,
            symbol,
            &self.fields.account_modes,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn set_leverage(
        &self,
        symbol: &str,
        input_leverage: f64,
    ) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::set_leverage(
            symbol,
            input_leverage,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn update_order(
        &self,
        order: &Order,
        update: &UtilityInput,
    ) -> Result<Vec<Option<Value>>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.write_host(&service);
        order_rest::update_order(
            order,
            update,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_paginated_order_rows(
        &self,
        url: &str,
        cursor_key: &str,
        family: &str,
        extra_params: &Map<String, Value>,
    ) -> Result<Vec<UtilityInput>, ServiceError<H::Error>> {
        let service = self.service();
        order_reads::fetch_paginated_order_rows(
            url,
            cursor_key,
            family,
            extra_params,
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn fetch_normal_orders(&self) -> Result<Vec<Order>, ServiceError<H::Error>> {
        let service = self.service();
        let now = || self.host.wall_clock_ms();
        let host = OrderReadHost {
            service: &service,
            sleep: Box::new(move |delay| self.host.sleep(delay)),
            now: &now,
        };
        order_reads::fetch_normal_orders(self.context.store.as_ref(), self.xhr.as_ref(), &host)
            .await
    }
    pub async fn fetch_algo_orders(&self) -> Result<Vec<Order>, ServiceError<H::Error>> {
        let service = self.service();
        let now = || self.host.wall_clock_ms();
        let host = OrderReadHost {
            service: &service,
            sleep: Box::new(move |delay| self.host.sleep(delay)),
            now: &now,
        };
        order_reads::fetch_algo_orders(self.context.store.as_ref(), self.xhr.as_ref(), &host).await
    }
    pub async fn fetch_trigger_orders(&self) -> Result<Vec<Order>, ServiceError<H::Error>> {
        let service = self.service();
        let now = || self.host.wall_clock_ms();
        let host = OrderReadHost {
            service: &service,
            sleep: Box::new(move |delay| self.host.sleep(delay)),
            now: &now,
        };
        order_reads::fetch_trigger_orders(self.context.store.as_ref(), self.xhr.as_ref(), &host)
            .await
    }
    pub async fn fetch_orders_with_retry(
        &self,
        retries: f64,
    ) -> Result<Vec<Order>, ServiceError<H::Error>> {
        let service = self.service();
        let now = || self.host.wall_clock_ms();
        let host = OrderReadHost {
            service: &service,
            sleep: Box::new(move |delay| self.host.sleep(delay)),
            now: &now,
        };
        order_reads::fetch_orders_with_retry(
            retries,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_orders(
        &self,
        _symbols: Option<&[String]>,
    ) -> Result<Vec<Order>, ServiceError<H::Error>> {
        let service = self.service();
        let now = || self.host.wall_clock_ms();
        let host = OrderReadHost {
            service: &service,
            sleep: Box::new(move |delay| self.host.sleep(delay)),
            now: &now,
        };
        order_reads::fetch_orders(
            _symbols,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_historical_orders(
        &self,
        opts: Option<&OrderHistoryOptions>,
    ) -> Result<Vec<HistoricalOrder>, ServiceError<H::Error>> {
        let service = self.service();
        order_reads::fetch_historical_orders(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn fetch_historical_fills(
        &self,
        opts: &ExecutionsOptions,
    ) -> Result<Vec<Execution>, ServiceError<H::Error>> {
        let service = self.service();
        order_reads::fetch_historical_fills(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn fetch_positions_helper(
        &self,
        symbol: Option<&str>,
    ) -> Result<Vec<Position>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_positions_helper(
            symbol,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_positions(
        &self,
        symbols: Option<&[String]>,
    ) -> Result<Vec<Position>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_positions(
            symbols,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_position_metadata(
        &self,
        symbols: &[String],
    ) -> Result<Vec<PositionMetadata>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_position_metadata(
            symbols,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_balance_and_positions(
        &self,
    ) -> Result<BalanceAndPositions, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_balance_and_positions(
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_income_history(
        &self,
        opts: Option<&IncomeHistoryOptions>,
    ) -> Result<Vec<IncomeRecord>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_income_history(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_income_history_range(
        &self,
        start_time: f64,
        end_time: f64,
        income_type: Option<&str>,
    ) -> Result<Vec<IncomeRecord>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_income_history_range(
            start_time,
            end_time,
            income_type,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn fetch_closed_position_rows(
        &self,
        opts: &HistoricalTradeOptions,
    ) -> Result<Vec<UtilityInput>, ServiceError<H::Error>> {
        position_reads::fetch_closed_position_rows(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
        )
        .await
    }
    pub async fn fetch_historical_positions(
        &self,
        opts: &HistoricalPositionsOptions,
    ) -> Result<Vec<PositionRecord>, ServiceError<H::Error>> {
        position_reads::fetch_historical_positions(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
        )
        .await
    }
    pub async fn fetch_closed_pnl(
        &self,
        opts: &HistoricalTradeOptions,
    ) -> Result<Vec<ClosedPnlEntry>, ServiceError<H::Error>> {
        let service = self.service();
        let host = self.position_host(&service);
        position_reads::fetch_closed_pnl(
            opts,
            self.context.options.public_data_only == Some(true),
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &host,
        )
        .await
    }
    pub async fn get_or_fetch_position_mode(
        &self,
        symbol: Option<&str>,
        refetch: bool,
    ) -> Result<PositionMode, ServiceError<H::Error>> {
        let service = self.service();
        order_services::get_or_fetch_position_mode(
            symbol,
            refetch,
            &self.fields.account_modes,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn get_or_fetch_margin_type(
        &self,
        symbol: Option<&str>,
        refetch: bool,
    ) -> Result<MarginType, ServiceError<H::Error>> {
        let service = self.service();
        order_services::get_or_fetch_margin_type(
            symbol,
            refetch,
            &self.fields.account_modes,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn fetch_leverage_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<Option<FetchedLeverage>, ServiceError<H::Error>> {
        let service = self.service();
        order_services::fetch_leverage_for_symbol(
            symbol,
            &self.fields.account_modes,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn change_position_mode(&self, hedged: bool) -> Result<(), ServiceError<H::Error>> {
        let service = self.service();
        order_services::change_position_mode(
            hedged,
            &self.fields.account_modes,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &service,
        )
        .await
    }
    pub async fn place_trigger_order(
        &self,
        payload: &OrderPayload,
    ) -> Result<String, ServiceError<H::Error>> {
        let service = self.service();
        order_services::place_trigger_order(payload, self.xhr.as_ref(), &service).await
    }
    pub async fn format_trigger_order(
        &self,
        opts: &PreparedOrderOpts,
        source: Option<&UtilityInput>,
    ) -> Result<OrderPayload, ServiceError<H::Error>> {
        let host = self.format_host();
        order_formatters::format_trigger_order(opts, source, self.context.store.as_ref(), &host)
            .await
    }
    pub async fn format_algo_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<OrderPayload, ServiceError<H::Error>> {
        let host = self.format_host();
        order_formatters::format_algo_order(opts, self.context.store.as_ref(), &host).await
    }
    pub async fn format_normal_order(
        &self,
        opts: &PreparedOrderOpts,
    ) -> Result<Vec<OrderPayload>, ServiceError<H::Error>> {
        let host = self.format_host();
        order_formatters::format_normal_order(opts, self.context.store.as_ref(), &host).await
    }
}

impl<H: BlofinExchangeHost> BlofinExchange<H> {
    pub fn symbol_to_market_id(&self, symbol: &str) -> Option<String> {
        crate::symbols::symbol_to_market_id(symbol, self.context.store.as_ref())
    }
    pub fn market_id_to_symbol(&self, id: &str) -> Option<String> {
        crate::symbols::market_id_to_symbol(id, self.context.store.as_ref())
    }
    pub fn map_orders(
        &self,
        rows: &crate::orders::OrdersInput,
    ) -> Result<Vec<Order>, safe_cex_types::js_coercion::JsCoercionError> {
        crate::orders::map_orders(rows, self.context.store.as_ref(), || self.host.clock_ms())
    }
    pub fn map_positions(
        &self,
        rows: &crate::positions::PositionsInput,
    ) -> Result<Vec<Position>, crate::positions::PositionMapError<H::Error>> {
        crate::positions::map_positions(rows, self.context.store.as_ref(), |s| {
            self.host
                .emit_and_log_error(&self.base, Some(&Value::String(s.into())))
        })
    }
    pub fn format_tickers(
        &self,
        rows: Option<&crate::tickers::TickerInput>,
    ) -> Result<Vec<Ticker>, H::Error> {
        match rows {
            Some(rows) => crate::tickers::format_tickers(rows, self.context.store.as_ref(), |s| {
                self.host
                    .emit_and_log_error(&self.base, Some(&Value::String(s.into())))
            }),
            None => {
                self.host.emit_and_log_error(
                    &self.base,
                    Some(&Value::String(
                        "Cannot read properties of undefined (reading 'reduce')".into(),
                    )),
                )?;
                Ok(self.context.store.tickers().to_vec())
            }
        }
    }
    pub async fn fetch_tickers_raw(
        &self,
    ) -> Result<Option<crate::tickers::TickerInput>, ServiceError<H::Error>> {
        crate::reads::fetch_tickers_raw(self.xhr.as_ref(), |s| {
            self.host.emit_and_log_error(&self.base, Some(s))
        })
        .await
        .map_err(read_error)
    }
    pub async fn fetch_tickers(&self) -> Result<Vec<Ticker>, ServiceError<H::Error>> {
        crate::reads::fetch_tickers(self.xhr.as_ref(), self.context.store.as_ref(), |s| {
            self.host.emit_and_log_error(&self.base, Some(s))
        })
        .await
        .map_err(read_error)
    }
    pub async fn validate_account(
        &self,
    ) -> Result<rest_lifecycle::AccountValidation, ServiceError<H::Error>> {
        rest_lifecycle::validate_account(self.xhr.as_ref(), &self.service()).await
    }
    pub async fn fetch_markets(&self) -> Result<Vec<Market>, ServiceError<H::Error>> {
        rest_lifecycle::fetch_markets(
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &self.service(),
            || self.host.nanoid(),
        )
        .await
    }
    pub async fn fetch_ohlcv(
        &self,
        opts: &OHLCVOptions,
    ) -> Result<Vec<Candle>, ServiceError<H::Error>> {
        rest_lifecycle::fetch_ohlcv(
            opts,
            self.context.store.as_ref(),
            self.xhr.as_ref(),
            &self.service(),
            || self.host.wall_clock_ms(),
        )
        .await
    }
    pub async fn cancel_symbol_orders(&self, symbol: &str) -> Result<(), ServiceError<H::Error>> {
        order_services::cancel_symbol_orders(
            symbol,
            self.context.store.as_ref(),
            |orders| async move { self.cancel_orders(&orders).await },
        )
        .await
    }
    pub fn apply_confirmed_position_mode(
        &self,
        mode: PositionMode,
    ) -> Result<(), StoreMutationError> {
        modes::apply_confirmed_position_mode(
            &self.fields.account_modes,
            self.context.store.as_ref(),
            mode,
        )
    }
    pub fn after_method_called_in_worker(
        &self,
        result: Option<&UtilityInput>,
        method: &str,
        first_arg: Option<&UtilityInput>,
    ) -> Result<(), StoreMutationError> {
        modes::after_method_called_in_worker(
            &self.fields.account_modes,
            self.context.store.as_ref(),
            result,
            method,
            first_arg,
        )
    }
    pub async fn get_position_side(
        &self,
        opts: &PositionSideOptions<'_>,
    ) -> Result<&'static str, ServiceError<H::Error>> {
        modes::get_position_side(opts, |s| {
            let s = s.to_owned();
            async move {
                self.get_or_fetch_position_mode(Some(&s), false)
                    .await
                    .map(Some)
            }
        })
        .await
    }
    pub fn parse_positive_price(
        &self,
        input: Option<&UtilityInput>,
    ) -> Result<Option<f64>, safe_cex_types::js_coercion::JsCoercionError> {
        crate::utility::parse_positive_price(input)
    }
    pub fn map_blofin_trigger_type(
        &self,
        input: Option<&UtilityInput>,
    ) -> Option<OrderStopTriggerType> {
        crate::utility::map_blofin_trigger_type(input)
    }
    pub fn to_blofin_trigger_type(&self, input: Option<&UtilityInput>) -> &'static str {
        crate::utility::to_blofin_trigger_type(input)
    }
    pub fn is_algo_order(&self, input: Option<&UtilityInput>) -> bool {
        crate::utility::is_algo_order(input)
    }
    pub fn is_already_gone_cancel_row(
        &self,
        input: Option<&UtilityInput>,
    ) -> Result<bool, safe_cex_types::js_coercion::JsCoercionError> {
        crate::utility::is_already_gone_cancel_row(input)
    }
    pub fn add_inline_tpsl_fallback_orders(
        &self,
        orders: &[Order],
    ) -> Result<Vec<Order>, safe_cex_types::js_coercion::JsCoercionError> {
        crate::inline_tpsl::add_inline_tpsl_fallback_orders(orders, || self.host.clock_ms())
    }
    pub async fn start(self: &Rc<Self>) -> Result<(), ServiceError<H::Error>> {
        rest_lifecycle::start(&ExchangeLifecycle {
            owner: self,
            service: self.service(),
        })
        .await
    }
    pub async fn tick(self: &Rc<Self>, count: Option<f64>) -> Result<(), ServiceError<H::Error>> {
        rest_lifecycle::tick(
            &ExchangeLifecycle {
                owner: self,
                service: self.service(),
            },
            count.unwrap_or(0.0),
        )
        .await
    }
}
fn read_error<E>(error: crate::reads::ReadError<E>) -> ServiceError<E> {
    match error {
        crate::reads::ReadError::Ingress(e) => ServiceError::Ingress(e),
        crate::reads::ReadError::Emitter(e) => ServiceError::Host(e),
    }
}
struct ExchangeLifecycle<'a, H: BlofinExchangeHost> {
    owner: &'a Rc<BlofinExchange<H>>,
    service: ServiceHost<'a, H::Error>,
}
impl<H: BlofinExchangeHost> rest_lifecycle::LifecycleHost for ExchangeLifecycle<'_, H> {
    type Error = H::Error;
    fn store(&self) -> &dyn Store {
        self.owner.context.store.as_ref()
    }
    fn service(&self) -> &ServiceHost<'_, H::Error> {
        &self.service
    }
    fn init_external_public_data(&self) -> rest_lifecycle::LifecycleFuture<'_, bool, H::Error> {
        self.owner.host.init_external_public_data(&self.owner.base)
    }
    fn public_data_only(&self) -> bool {
        self.owner.context.options.public_data_only == Some(true)
    }
    fn disposed(&self) -> bool {
        self.owner.context.disposed.get()
    }
    fn auth_failure_streak(&self) -> f64 {
        self.owner.context.auth_failure_streak.get()
    }
    fn tick_interval(&self) -> Option<f64> {
        self.owner.host.get_tick_interval(&self.owner.base)
    }

    fn connect_public(&self) -> Result<(), ServiceError<H::Error>> {
        self.owner
            .public_websocket
            .connect_and_subscribe()
            .map_err(|e| ServiceError::Host(self.owner.host.public_error(e)))
    }
    fn connect_private(&self) -> Result<(), ServiceError<H::Error>> {
        self.owner
            .private_websocket
            .connect_and_subscribe()
            .map_err(ServiceError::Host)
    }
    fn fetch_markets(&self) -> rest_lifecycle::LifecycleFuture<'_, Vec<Market>, H::Error> {
        Box::pin(self.owner.fetch_markets())
    }
    fn fetch_tickers_raw(
        &self,
    ) -> rest_lifecycle::LifecycleFuture<'_, Option<UtilityInput>, H::Error> {
        Box::pin(async move {
            self.owner
                .fetch_tickers_raw()
                .await
                .map(|v| v.map(|v| UtilityInput::from_retained(v.retained().clone())))
        })
    }
    fn format_tickers(
        &self,
        raw: Option<UtilityInput>,
    ) -> Result<Vec<Ticker>, ServiceError<H::Error>> {
        let raw = raw.map(|r| crate::tickers::TickerInput::from_retained(r.json().clone()));
        self.owner
            .format_tickers(raw.as_ref())
            .map_err(ServiceError::Host)
    }
    fn refetch_margin_type(&self) -> rest_lifecycle::LifecycleFuture<'_, (), H::Error> {
        Box::pin(async move {
            self.owner
                .get_or_fetch_margin_type(None, true)
                .await
                .map(|_| ())
        })
    }
    fn refetch_position_mode(&self) -> rest_lifecycle::LifecycleFuture<'_, (), H::Error> {
        Box::pin(async move {
            self.owner
                .get_or_fetch_position_mode(None, true)
                .await
                .map(|_| ())
        })
    }
    fn fetch_balance_positions(
        &self,
    ) -> rest_lifecycle::LifecycleFuture<'_, (Balance, Vec<Position>), H::Error> {
        Box::pin(async move {
            self.owner
                .fetch_balance_and_positions()
                .await
                .map(|v| (v.balance, v.positions))
        })
    }
    fn fetch_orders(&self) -> rest_lifecycle::LifecycleFuture<'_, Vec<Order>, H::Error> {
        Box::pin(self.owner.fetch_orders(None))
    }
    fn schedule_tick(&self, next: Option<f64>, delay: f64) -> Result<(), ServiceError<H::Error>> {
        let owner = self.owner.clone();
        self.owner
            .host
            .schedule_tick(
                Rc::new(move || {
                    let owner = owner.clone();
                    Box::pin(async move { owner.tick(next).await })
                }),
                delay,
            )
            .map_err(ServiceError::Host)
    }
}

impl<H: BlofinExchangeHost> BlofinExchange<H> {
    fn bind_private_parent(self: &Rc<Self>) {
        let orders = Rc::downgrade(self);
        let positions = orders.clone();
        let fetch = orders.clone();
        let host = self.host.clone();
        self.private_websocket
            .bind_parent(crate::private_stream_data::PrivateParentCallbacks {
                map_orders: Rc::new(move |rows| {
                    let owner = orders.upgrade().ok_or_else(|| {
                        crate::private_stream_data::PrivateDataError::Shape(
                            "Parent exchange dropped".into(),
                        )
                    })?;
                    crate::orders::map_orders_for_private_topic(
                        &crate::orders::OrdersInput::from_retained(rows.clone()),
                        owner.context.store.as_ref(),
                        || owner.host.clock_ms(),
                    )
                    .map_err(crate::private_stream_data::PrivateDataError::Coercion)
                }),
                map_positions: Rc::new(move |rows| {
                    let owner = positions.upgrade().ok_or_else(|| {
                        crate::private_stream_data::PrivateDataError::Shape(
                            "Parent exchange dropped".into(),
                        )
                    })?;
                    owner
                        .map_positions(&crate::positions::PositionsInput::from_retained(
                            rows.clone(),
                        ))
                        .map_err(|e| match e {
                            crate::positions::PositionMapError::Coercion(e) => {
                                crate::private_stream_data::PrivateDataError::Coercion(e)
                            }
                            crate::positions::PositionMapError::Emitter(e) => {
                                crate::private_stream_data::PrivateDataError::Host(e)
                            }
                        })
                }),
                fetch_orders: Rc::new(move || {
                    let host = host.clone();
                    let owner = fetch.upgrade();
                    Ok(Box::pin(async move {
                        let owner = owner.ok_or_else(|| {
                            host.service_error(ServiceError::Input(
                                "Parent exchange dropped".into(),
                            ))
                        })?;
                        owner
                            .fetch_orders(None)
                            .await
                            .map_err(|e| host.service_error(e))
                    }))
                }),
            });
    }
}
