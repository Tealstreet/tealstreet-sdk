//! Native class construction and canonical source-owned slots. The separately
//! indexed method bodies use live receiver projections rather than copied state.
use crate::{
    api_factory::{CoinbaseApiHost, CoinbaseApiReceiver, create_api},
    exchange_lifecycle::{LifecycleReceiver, LifecycleTimer},
    market_position_reads::MarketPositionReceiver,
    portfolios_balances::{Portfolio, PortfolioFailure, PortfolioReceiver},
    private_order_snapshot::CoinbasePrivateOrderSnapshot,
    private_stream_dispatch::{PrivateStreamHost, PrivateStreamReceiver},
    public_stream_dispatch::{PublicStreamHost, PublicStreamReceiver},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::Rc,
};
pub type ClassMint = Rc<dyn Fn() -> Result<String, PortfolioFailure>>;
pub type ClassApiFactory = Rc<dyn Fn(Frame, Frame) -> Result<Frame, PortfolioFailure>>;
pub type ClassRateLimit = Rc<dyn Fn(Frame) -> Result<Frame, PortfolioFailure>>;
/// Identity for imported BaseExchange/BaseWebSocket leaf construction. The same
/// retained object is passed to both clients and both socket parent constructors.
pub type ClassIdentity = Frame;
pub type ClassFindResult = Promise<Result<Portfolio, PortfolioFailure>>;
pub type ClassSupportedResult =
    Promise<Result<crate::portfolios_balances::SupportedTypes, PortfolioFailure>>;
pub(crate) type ClassFindSlot<H> =
    crate::class_methods::ClassMethodSlot<CoinbaseExchange<H>, ClassFindResult>;
pub(crate) type ClassSupportedSlot<H> =
    crate::class_methods::ClassMethodSlot<CoinbaseExchange<H>, ClassSupportedResult>;
pub type ClassFindMethod<H> =
    crate::class_methods::BoundClassMethod<CoinbaseExchange<H>, ClassFindResult>;
pub type ClassSupportedMethod<H> =
    crate::class_methods::BoundClassMethod<CoinbaseExchange<H>, ClassSupportedResult>;

pub trait CoinbaseConstructionHost:
    CoinbaseApiHost<Client = Frame> + crate::portfolios_balances::PortfolioHost
{
    type Public: crate::public_market_listeners::PublicMarketListenerHost;
    type Private: CoinbasePrivateClassHost;
    fn construct_base(
        &self,
        receiver: ClassIdentity,
        name: String,
        options: Frame,
        store: Frame,
    ) -> Result<(), PortfolioFailure>;
    fn pattern_client_order_id_rule(
        &self,
        name: &'static str,
        pattern: &'static str,
        mint: ClassMint,
    ) -> Result<Frame, PortfolioFailure>;
    fn uuid(&self) -> Result<String, PortfolioFailure>;
    fn create_secondary(
        &self,
        receiver: Rc<CoinbaseExchange<Self>>,
        options: Frame,
        factory: ClassApiFactory,
        rate_limit: ClassRateLimit,
    ) -> Result<Frame, PortfolioFailure>
    where
        Self: Sized;
    fn rate_limit(&self, client: Frame, max_rps: f64) -> Result<Frame, PortfolioFailure>;
    fn construct_public_base(
        &self,
        parent: ClassIdentity,
    ) -> Result<Self::Public, PortfolioFailure>;
    fn construct_private_base(
        &self,
        parent: ClassIdentity,
    ) -> Result<Self::Private, PortfolioFailure>;
}
pub struct CoinbaseExchange<H: CoinbaseConstructionHost> {
    pub host: Rc<H>,
    pub identity: ClassIdentity,
    pub options: Rc<RefCell<Frame>>,
    find_selected_portfolio: RefCell<ClassFindSlot<H>>,
    get_supported_types: RefCell<ClassSupportedSlot<H>>,
    pub store: Frame,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub client_order_id_rule: RefCell<Option<Frame>>,
    pub xhr: RefCell<Option<Frame>>,
    pub unlimited_xhr: RefCell<Option<Frame>>,
    pub public_websocket: RefCell<Option<Rc<CoinbasePublicWebsocket<H::Public>>>>,
    pub private_websocket: RefCell<Option<Rc<CoinbasePrivateWebsocket<H::Private>>>>,
    pub grafana_endpoints_every: &'static [&'static str],
    pub grafana_endpoints_periodic: &'static [&'static str],
    pub selected_portfolio: Rc<RefCell<Option<Portfolio>>>,
    pub next_orders_poll_at: Cell<f64>,
    pub order_write_reconcile_timeout: Rc<RefCell<Option<LifecycleTimer>>>,
    pub private_user_stream_write_check_timeout: Rc<RefCell<Option<LifecycleTimer>>>,
    pub last_private_user_stream_message_at: Cell<f64>,
    pub has_warned_private_user_stream_degraded: Cell<bool>,
    pub support_spot: Cell<bool>,
}
impl<H: CoinbaseConstructionHost + 'static> CoinbaseExchange<H> {
    pub fn new(
        host: Rc<H>,
        name: String,
        options: Frame,
        store: Frame,
        spawn: CoinbaseStarbasePromiseSpawner,
    ) -> Result<Rc<Self>, PortfolioFailure> {
        let identity = Frame::Object(Rc::default());
        host.construct_base(identity.clone(), name, options.clone(), store.clone())?;
        // Derived field initializers follow super and precede the constructor body.
        let mint_host = host.clone();
        let mint: ClassMint = Rc::new(move || Ok(format!("tealstreet-{}", mint_host.uuid()?)));
        let rule = host.pattern_client_order_id_rule("Coinbase", r"^\S+$", mint)?;
        let exchange = Rc::new(Self {
            host: host.clone(),
            identity: identity.clone(),
            options: Rc::new(RefCell::new(options.clone())),
            find_selected_portfolio: RefCell::new(ClassFindSlot::<H>::new(|exchange| {
                crate::portfolios_balances::find_selected_portfolio(
                    exchange.portfolio_receiver(exchange.host.clone(), false),
                )
            })),
            get_supported_types: RefCell::new(ClassSupportedSlot::<H>::new(|exchange| {
                crate::portfolios_balances::get_supported_types(
                    exchange.portfolio_receiver(exchange.host.clone(), false),
                )
            })),
            store,
            spawn: spawn.clone(),
            client_order_id_rule: RefCell::new(Some(rule)),
            xhr: RefCell::new(None),
            unlimited_xhr: RefCell::new(None),
            public_websocket: RefCell::new(None),
            private_websocket: RefCell::new(None),
            grafana_endpoints_every: crate::tables::GRAFANA_ENDPOINTS_EVERY,
            grafana_endpoints_periodic: crate::tables::GRAFANA_ENDPOINTS_PERIODIC,
            selected_portfolio: Rc::new(RefCell::new(None)),
            next_orders_poll_at: Cell::new(0.0),
            order_write_reconcile_timeout: Rc::new(RefCell::new(None)),
            private_user_stream_write_check_timeout: Rc::new(RefCell::new(None)),
            last_private_user_stream_message_at: Cell::new(0.0),
            has_warned_private_user_stream_degraded: Cell::new(false),
            support_spot: Cell::new(true),
        });
        let factory_host = host.clone();
        let factory_spawn = spawn.clone();
        let factory: ClassApiFactory = Rc::new(move |options, receiver| {
            create_api(Rc::new(CoinbaseApiReceiver {
                host: factory_host.clone(),
                options,
                exchange: receiver,
                spawn: factory_spawn.clone(),
            }))
        });
        let rate_host = host.clone();
        let rate_limit: ClassRateLimit = Rc::new(move |client| rate_host.rate_limit(client, 3.0));
        let limited = host.create_secondary(
            exchange.clone(),
            options.clone(),
            factory.clone(),
            rate_limit,
        )?;
        *exchange.xhr.borrow_mut() = Some(limited);
        let unlimited = factory(options, identity.clone())?;
        *exchange.unlimited_xhr.borrow_mut() = Some(unlimited);
        let public = CoinbasePublicWebsocket::new(
            host.construct_public_base(identity.clone())?,
            identity.clone(),
        );
        *exchange.public_websocket.borrow_mut() = Some(Rc::new(public));
        let private = CoinbasePrivateWebsocket::new(
            host.construct_private_base(identity.clone())?,
            identity,
            spawn,
        );
        *exchange.private_websocket.borrow_mut() = Some(Rc::new(private));
        Ok(exchange)
    }
    pub fn portfolio_receiver<P>(self: &Rc<Self>, host: P, spot: bool) -> Rc<PortfolioReceiver<P>> {
        Rc::new(PortfolioReceiver {
            host,
            spawn: self.spawn.clone(),
            selected: self.selected_portfolio.clone(),
            public_only: Cell::new(matches!(
                self.options.borrow().get("publicDataOnly"),
                Some(Frame::Bool(true))
            )),
            support_spot: Cell::new(self.support_spot.get()),
            spot,
            class: Some(Rc::new(ClassPortfolioBinding {
                exchange: self.clone(),
            })),
        })
    }
    pub fn capture_finder(self: &Rc<Self>) -> ClassFindMethod<H> {
        self.find_selected_portfolio.borrow().capture(self.clone())
    }
    pub fn capture_supported(self: &Rc<Self>) -> ClassSupportedMethod<H> {
        self.get_supported_types.borrow().capture(self.clone())
    }
    /// Source property assignment stores a function value, including its bound origin.
    pub fn set_finder(self: &Rc<Self>, method: ClassFindMethod<H>) {
        *self.find_selected_portfolio.borrow_mut() = ClassFindSlot::assigned(method, self);
    }
    pub fn set_supported(self: &Rc<Self>, method: ClassSupportedMethod<H>) {
        *self.get_supported_types.borrow_mut() = ClassSupportedSlot::assigned(method, self);
    }
    pub fn find_selected(self: &Rc<Self>) -> ClassFindResult {
        // Clone/capture drops the field borrow before calling reentrant code.
        self.capture_finder().call()
    }
    pub fn supported_types(self: &Rc<Self>) -> ClassSupportedResult {
        self.capture_supported().call()
    }
    pub fn get_account(
        self: &Rc<Self>,
    ) -> Promise<Result<crate::exchange_lifecycle::LifecycleAccountRoot, PortfolioFailure>> {
        let receiver = self.clone();
        crate::exchange_lifecycle::get_account_from_selection(
            self.selected_portfolio.clone(),
            move || Ok(receiver.find_selected()),
            &self.spawn,
        )
    }
    pub fn is_public_data_only(&self) -> bool {
        safe_cex_exchange::exchange::is_public_data_only_with(
            || {
                matches!(
                    self.options.borrow().get("publicDataOnly"),
                    Some(Frame::Bool(true))
                )
            },
            || safe_cex_exchange::exchange::can_sign_with(&ClassSigningView(self)),
        )
    }
    pub fn market_receiver<M>(&self, host: M) -> Rc<MarketPositionReceiver<M>> {
        Rc::new(MarketPositionReceiver {
            host,
            spawn: self.spawn.clone(),
            selected: self.selected_portfolio.clone(),
        })
    }
    pub fn lifecycle_receiver<L>(&self, host: L) -> Rc<LifecycleReceiver<L>> {
        Rc::new(LifecycleReceiver {
            host,
            spawn: self.spawn.clone(),
            selected: self.selected_portfolio.clone(),
            order_write_reconcile_timeout: self.order_write_reconcile_timeout.clone(),
            private_user_stream_write_check_timeout: self
                .private_user_stream_write_check_timeout
                .clone(),
        })
    }
}
// Rc forwarding keeps the factory callback rooted in the original host without
// moving/copying its source module cache, transport registry or signer objects.
impl<H: CoinbaseApiHost> CoinbaseApiHost for Rc<H> {
    type Client = H::Client;
    fn safe_create(
        &self,
        c: crate::api_factory::ApiCreateConfig,
        o: Frame,
        e: Frame,
    ) -> Result<Self::Client, PortfolioFailure> {
        (**self).safe_create(c, o, e)
    }
    fn serialize_repeat(&self, p: Frame) -> Result<String, PortfolioFailure> {
        (**self).serialize_repeat(p)
    }
    fn network_error_on_read(&self) -> Result<Frame, PortfolioFailure> {
        (**self).network_error_on_read()
    }
    fn retry(&self, c: Self::Client, n: f64, r: Frame) -> Result<(), PortfolioFailure> {
        (**self).retry(c, n, r)
    }
    fn install_interceptor(
        &self,
        c: Self::Client,
        i: crate::api_factory::ApiInterceptor,
    ) -> Result<(), PortfolioFailure> {
        (**self).install_interceptor(c, i)
    }
    fn install_transport_adapter(
        &self,
        c: Self::Client,
        o: Frame,
        n: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        (**self).install_transport_adapter(c, o, n)
    }
    fn replace_url_params(
        &self,
        u: Option<Frame>,
        p: Option<Frame>,
    ) -> Result<crate::api_factory::ApiRequest, PortfolioFailure> {
        (**self).replace_url_params(u, p)
    }
    fn mark_skip_remote_signing(
        &self,
        c: crate::api_factory::ApiRequest,
    ) -> Result<(), PortfolioFailure> {
        (**self).mark_skip_remote_signing(c)
    }
    fn proxy_prefix(
        &self,
        e: &'static str,
        o: Frame,
        c: crate::api_factory::ApiRequest,
    ) -> Result<Vec<u16>, PortfolioFailure> {
        (**self).proxy_prefix(e, o, c)
    }
    fn headers(&self, f: Frame, b: bool) -> Result<Frame, PortfolioFailure> {
        (**self).headers(f, b)
    }
    fn sign(
        &self,
        k: Option<Frame>,
        s: Option<Frame>,
        u: Vec<u16>,
    ) -> Result<Promise<Result<Frame, PortfolioFailure>>, PortfolioFailure> {
        (**self).sign(k, s, u)
    }
}
pub struct CoinbasePublicWebsocket<H> {
    pub parent: ClassIdentity,
    pub receiver: Rc<PublicStreamReceiver<H>>,
    pub listeners: Rc<crate::public_market_listeners::PublicListenerReceiver<H>>,
}
impl<H> CoinbasePublicWebsocket<H> {
    pub fn new(host: H, parent: ClassIdentity) -> Self {
        let receiver = Rc::new(PublicStreamReceiver::new(host));
        let listeners = Rc::new(crate::public_market_listeners::PublicListenerReceiver {
            stream: receiver.clone(),
            inner_state: RefCell::new(indexmap::IndexMap::new()),
            connectivity:
                crate::public_market_listeners::PublicListenerConnectivity::ReceiverSocket,
        });
        Self {
            parent,
            receiver,
            listeners,
        }
    }
}
impl<H: PublicStreamHost> CoinbasePublicWebsocket<H> {
    pub fn connect_and_subscribe(
        &self,
    ) -> Result<(), Rc<crate::public_stream_dispatch::PublicStreamFailure>> {
        crate::public_stream_dispatch::connect_and_subscribe(&self.receiver)
    }
    pub fn on_open(&self) -> Result<(), Rc<crate::public_stream_dispatch::PublicStreamFailure>> {
        crate::public_stream_dispatch::on_open_with_reset(&self.receiver, || {
            for state in self.listeners.inner_state.borrow().values() {
                let mut state = state.borrow_mut();
                state.updates = Vec::new();
                state.is_snapshot_loaded = false;
            }
            Ok(())
        })
    }
    pub fn on_message(
        &self,
        raw: &str,
    ) -> Result<(), Rc<crate::public_stream_dispatch::PublicStreamFailure>> {
        crate::public_stream_dispatch::on_message(&self.receiver, raw)
    }
}
impl<H: crate::public_market_listeners::PublicMarketListenerHost + 'static>
    CoinbasePublicWebsocket<H>
{
    pub fn listen_ohlcv(
        &self,
        opts: crate::public_market_reads::PublicOHLCVOptionsRoot,
        callback: crate::public_market_listeners::PublicCandleCallback,
    ) -> Result<
        crate::public_market_listeners::PublicCleanup,
        Rc<crate::public_stream_dispatch::PublicStreamFailure>,
    > {
        crate::public_market_listeners::listen_ohlcv(self.listeners.clone(), opts, callback)
    }
    pub fn listen_order_book(
        &self,
        symbol: String,
        callback: crate::public_market_listeners::PublicBookCallback,
    ) -> Result<
        crate::public_market_listeners::PublicCleanup,
        Rc<crate::public_stream_dispatch::PublicStreamFailure>,
    > {
        crate::public_market_listeners::listen_order_book(self.listeners.clone(), symbol, callback)
    }
}
/// Imported market, clock, Store and emitter leaves for the native private class.
/// Snapshot and event dispatch are bound by the class rather than delegated.
pub trait CoinbasePrivateClassHost: PrivateStreamHost {
    fn private_order_markets(
        &self,
    ) -> Result<crate::order_sync::PrivateOrderMarkets, PortfolioFailure>;
    fn resolve_private_order_market(
        &self,
        product: String,
        markets: crate::order_sync::PrivateOrderMarkets,
    ) -> Result<Option<crate::market_position_reads::ReadMarket>, PortfolioFailure>;
    fn private_order_console_error(
        &self,
        message: &str,
        value: &str,
    ) -> Result<(), PortfolioFailure>;
    fn private_order_now_ms(&self) -> Result<f64, PortfolioFailure>;
    fn private_orders_loaded(&self) -> Result<Frame, PortfolioFailure>;
    fn publish_private_order_snapshot(
        &self,
        orders: crate::order_sync::SyncedOrders<Frame>,
        loaded: Frame,
    ) -> Result<(), PortfolioFailure>;
    fn add_private_orders(
        &self,
        orders: crate::order_sync::SyncedOrders<Frame>,
    ) -> Result<(), PortfolioFailure>;
    fn emit_private_order_fill(
        &self,
        fill: crate::order_sync::PrivateOrderFill<Frame>,
    ) -> Result<(), PortfolioFailure>;
}
pub struct CoinbasePrivateWebsocket<H> {
    pub parent: ClassIdentity,
    pub receiver: Rc<PrivateStreamReceiver<H>>,
    pub snapshot:
        Rc<RefCell<CoinbasePrivateOrderSnapshot<crate::order_sync::SyncedOrderRoot<Frame>>>>,
}
impl<H> CoinbasePrivateWebsocket<H> {
    pub fn new(host: H, parent: ClassIdentity, spawn: CoinbaseStarbasePromiseSpawner) -> Self {
        Self {
            parent,
            receiver: Rc::new(PrivateStreamReceiver {
                host,
                spawn,
                ws: RefCell::new(None),
                subscription_attempt: Cell::new(0.0),
            }),
            snapshot: Rc::new(RefCell::new(CoinbasePrivateOrderSnapshot::default())),
        }
    }
}
impl<H: CoinbasePrivateClassHost + 'static> CoinbasePrivateWebsocket<H> {
    pub fn connect_and_subscribe(&self) -> Result<(), PortfolioFailure> {
        crate::private_stream_dispatch::connect_and_subscribe(&self.receiver)
    }
    pub fn on_open(&self) -> Promise<Result<(), PortfolioFailure>> {
        let snapshot = self.snapshot.clone();
        crate::private_stream_dispatch::on_open_with_reset(self.receiver.clone(), move || {
            snapshot.borrow_mut().reset_order_snapshot_assembly();
            Ok(())
        })
    }
    pub fn on_message(&self, raw: &str) -> Result<(), PortfolioFailure> {
        crate::private_stream_dispatch::on_message_with_orders(&self.receiver, raw, |event| {
            self.handle_user_event_orders(event)
        })
    }
    pub fn handle_user_event_orders(&self, event: Frame) -> Result<(), PortfolioFailure> {
        crate::order_sync::handle_user_event_orders(
            &PrivateClassOrderAdapter { class: self },
            event,
        )
    }
    pub fn reset_order_snapshot_assembly(&self) {
        self.snapshot.borrow_mut().reset_order_snapshot_assembly();
    }
    pub fn merge_snapshot_orders(&self, orders: crate::order_sync::SyncedOrders<Frame>) {
        self.snapshot
            .borrow_mut()
            .merge_snapshot_orders(&orders.borrow());
    }
    pub fn apply_order_snapshot_batch(
        &self,
        raw_count: usize,
        orders: crate::order_sync::SyncedOrders<Frame>,
    ) -> Result<(), PortfolioFailure> {
        let completed = self
            .snapshot
            .borrow_mut()
            .prepare_order_snapshot_batch(raw_count, &orders.borrow());
        if let Some(orders) = completed {
            let loaded = self.receiver.host.private_orders_loaded()?;
            let Frame::Object(loaded) = loaded else {
                return Err(Rc::new(
                    crate::portfolios_balances::PortfolioThrown::from_value(Some(
                        serde_json::json!({"name":"TypeError","message":"Store loaded capability must return its record"}),
                    )),
                ));
            };
            let mut values = loaded.borrow().clone();
            values.insert("orders".encode_utf16().collect(), Frame::Bool(true));
            self.receiver.host.publish_private_order_snapshot(
                Rc::new(RefCell::new(orders)),
                Frame::Object(Rc::new(RefCell::new(values))),
            )?;
        }
        Ok(())
    }
}
struct PrivateClassOrderAdapter<'a, H> {
    class: &'a CoinbasePrivateWebsocket<H>,
}
impl<H: CoinbasePrivateClassHost + 'static> crate::order_sync::PrivateOrderEventHost<Frame>
    for PrivateClassOrderAdapter<'_, H>
{
    fn markets_by_id(&self) -> Result<crate::order_sync::PrivateOrderMarkets, PortfolioFailure> {
        self.class.receiver.host.private_order_markets()
    }
    fn resolve_coinbase_market(
        &self,
        product: String,
        markets: crate::order_sync::PrivateOrderMarkets,
    ) -> Result<Option<crate::market_position_reads::ReadMarket>, PortfolioFailure> {
        self.class
            .receiver
            .host
            .resolve_private_order_market(product, markets)
    }
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure> {
        self.class
            .receiver
            .host
            .emit_and_log_error(Some(Frame::String(message.encode_utf16().collect())))
    }
    fn console_error(&self, message: &str, value: &str) -> Result<(), PortfolioFailure> {
        self.class
            .receiver
            .host
            .private_order_console_error(message, value)
    }
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure> {
        self.class.receiver.host.private_order_now_ms()
    }
    fn snapshot_active(&self) -> Result<bool, PortfolioFailure> {
        Ok(self.class.snapshot.borrow().has_pending_snapshot())
    }
    fn merge_snapshot_orders(
        &self,
        orders: crate::order_sync::SyncedOrders<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.class.merge_snapshot_orders(orders);
        Ok(())
    }
    fn apply_order_snapshot_batch(
        &self,
        raw_count: f64,
        orders: crate::order_sync::SyncedOrders<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.class
            .apply_order_snapshot_batch(raw_count as usize, orders)
    }
    fn add_or_update_orders(
        &self,
        orders: crate::order_sync::SyncedOrders<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.class.receiver.host.add_private_orders(orders)
    }
    fn emit_fill(
        &self,
        fill: crate::order_sync::PrivateOrderFill<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.class.receiver.host.emit_private_order_fill(fill)
    }
}
pub struct CoinbaseSpotExchange<H: CoinbaseConstructionHost> {
    pub exchange: Rc<CoinbaseExchange<H>>,
}
impl<H: CoinbaseConstructionHost + 'static> CoinbaseSpotExchange<H> {
    pub fn new(
        host: Rc<H>,
        name: String,
        options: Frame,
        store: Frame,
        spawn: CoinbaseStarbasePromiseSpawner,
    ) -> Result<Self, PortfolioFailure> {
        let exchange = CoinbaseExchange::new(host, name, options, store, spawn)?;
        exchange.support_spot.set(true);
        *exchange.get_supported_types.borrow_mut() = ClassSupportedSlot::<H>::new(|exchange| {
            crate::portfolios_balances::get_spot_supported_types(
                exchange.portfolio_receiver(exchange.host.clone(), true),
            )
        });
        *exchange.find_selected_portfolio.borrow_mut() = ClassFindSlot::<H>::new(|exchange| {
            crate::portfolios_balances::find_selected_spot_portfolio(
                exchange.portfolio_receiver(exchange.host.clone(), true),
            )
        });
        Ok(Self { exchange })
    }
    pub fn portfolio_receiver<P>(&self, host: P) -> Rc<PortfolioReceiver<P>> {
        self.exchange.portfolio_receiver(host, true)
    }
}
impl<H: CoinbaseConstructionHost> Deref for CoinbaseSpotExchange<H> {
    type Target = CoinbaseExchange<H>;
    fn deref(&self) -> &Self::Target {
        &self.exchange
    }
}

struct ClassPortfolioBinding<H: CoinbaseConstructionHost> {
    exchange: Rc<CoinbaseExchange<H>>,
}
impl<H: CoinbaseConstructionHost + 'static> crate::portfolios_balances::PortfolioClassBinding
    for ClassPortfolioBinding<H>
{
    fn public_only(&self) -> bool {
        self.exchange.is_public_data_only()
    }
    fn support_spot(&self) -> bool {
        self.exchange.support_spot.get()
    }
    fn find(&self) -> ClassFindResult {
        self.exchange.find_selected()
    }
    fn supported(&self) -> ClassSupportedResult {
        self.exchange.supported_types()
    }
}
struct ClassSigningView<'a, H: CoinbaseConstructionHost>(&'a CoinbaseExchange<H>);
fn class_option_truthy(value: Option<Frame>) -> bool {
    match value {
        None | Some(Frame::Null | Frame::Undefined) => false,
        Some(Frame::Bool(value)) => value,
        Some(Frame::Number(value)) => value != 0.0 && !value.is_nan(),
        Some(Frame::String(value)) => !value.is_empty(),
        Some(Frame::Array(_) | Frame::Object(_)) => true,
    }
}
impl<H: CoinbaseConstructionHost> safe_cex_exchange::exchange::BaseSigningOptionsView
    for ClassSigningView<'_, H>
{
    fn key_truthy(&self) -> bool {
        class_option_truthy(self.0.options.borrow().get("key"))
    }
    fn secret_truthy(&self) -> bool {
        class_option_truthy(self.0.options.borrow().get("secret"))
    }
    fn remote_signer_truthy(&self) -> bool {
        class_option_truthy(self.0.options.borrow().get("remoteSigner"))
    }
    fn payload_signer_truthy(&self) -> bool {
        class_option_truthy(self.0.options.borrow().get("payloadSigner"))
    }
}
