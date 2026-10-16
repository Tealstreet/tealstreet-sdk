//! Production `impl Exchange` for HyperliquidExchange. The constructor runs the
//! ported class constructor inside the realm; every trait method invokes the
//! instance's ported method and converts its result at the typed boundary.
use crate::{
    cloid::hyperliquid_client_order_id_rule,
    core_exchange_class,
    exchange::HyperliquidExchange,
    private_class, public_class,
    rate_limit::RateLimiter,
    realm::{
        Realm, Value,
        facade_exchange::base_exchange_class,
        facade_rest::{RestDependencies, bind_api_factories},
        facade_socket::{SocketDependencies, base_websocket_class},
        facade_store::store_object,
        imports,
    },
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
    websocket::WsRuntime,
    ws_parent::WsExchangeParent,
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::*;
use std::{rc::Rc, sync::Arc};

/// What the embedding host supplies: its runtime (clock, tasks, sockets), the
/// composed HTTP transport, and the process-wide Hyperliquid weight limiter.
pub struct HyperliquidDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Arc<dyn RestAdapter>,
    pub limiter: Rc<RateLimiter>,
}

pub(crate) struct Binding {
    pub realm: Realm,
    pub instance: Value,
}

/// The limiter clock over the exchange runtime's monotonic clock and sleep.
pub struct RuntimeClock(Rc<dyn ExchangeRuntime>);
impl RuntimeClock {
    pub fn new(runtime: Rc<dyn ExchangeRuntime>) -> Self {
        Self(runtime)
    }
}
impl RateLimitClock for RuntimeClock {
    fn now_ms(&self) -> u64 {
        self.0.monotonic_now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        let sleep = self.0.sleep(ms);
        Box::pin(async move { sleep.await.map_err(|e| TransportError::Host(e.message)) })
    }
}

/// TS `BaseExchange` default the class inherits: `await Promise.reject(new
/// Error('Not implemented'))` (base.ts getAccount L734, fetchMarkets L754,
/// fetchTickers L759, fetchSymbolMaxLeverage L799, changePositionMode L832,
/// fetchPublicTrades L1138). No request, no emit.
fn base_exchange_rejection<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

impl HyperliquidExchange {
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new(
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: HyperliquidDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let settings = ExchangeSettings {
            tick_interval: Some(2000.0),
            supports_public_only: true,
            client_order_id_rule: Some(hyperliquid_client_order_id_rule()),
            ..Default::default()
        };
        let runtime: Rc<dyn ExchangeRuntime> = deps.runtime.clone();
        let base = BaseExchange::new(
            ExchangeName::Hyperliquid,
            options.clone(),
            store.clone(),
            runtime.clone(),
            settings,
        );
        let mut venue = Self::from_base(base);
        venue.capabilities = capabilities();
        let venue = Rc::new(venue);
        let as_exchange: Rc<dyn Exchange> = venue.clone();
        let parent = WsExchangeParent::new(&as_exchange);

        let mut realm = Realm::new(runtime);
        imports::bind(&realm);
        let thrown = |r: &mut Realm, e: Value| r.exchange_error(&e);
        bind_api_factories(
            &realm,
            venue.base.clone(),
            RestDependencies {
                http: deps.http.clone(),
                limiter: deps.limiter.clone(),
                clock: Arc::new(RuntimeClock(deps.runtime.clone())),
            },
        );
        let socket_base = base_websocket_class(
            &mut realm,
            SocketDependencies {
                runtime: deps.runtime.clone(),
                parent,
                base: venue.base.clone(),
            },
        );
        let public = public_class::create_core_public_class(&mut realm, &socket_base)
            .map_err(|e| thrown(&mut realm, e))?;
        let private = private_class::create_core_private_class(&mut realm, &socket_base)
            .map_err(|e| thrown(&mut realm, e))?;
        realm.bind_import("HyperliquidPublicWebSocket", public);
        realm.bind_import("HyperliquidPrivateWebSocket", private);
        let base_class = base_exchange_class(&mut realm, venue.base.clone());
        let class = core_exchange_class::create_core_exchange_class(&mut realm, &base_class)
            .map_err(|e| thrown(&mut realm, e))?;
        realm.bind_import("HyperliquidExchange", class.clone());
        let js_options = realm
            .from_typed(&options)
            .map_err(|e| thrown(&mut realm, e))?;
        let js_store = store_object(&realm, store);
        let mut scope = realm.with_lexical(&class);
        let instance = scope
            .construct(
                &class,
                &[Value::str("hyperliquid"), js_options, js_store],
                None,
            )
            .map_err(|e| thrown(&mut scope, e))?;
        *venue.binding.borrow_mut() = Some(Binding {
            realm: scope,
            instance,
        });
        Ok(venue)
    }

    fn bound(&self) -> Result<(Realm, Value), ExchangeError> {
        self.binding
            .borrow()
            .as_ref()
            .map(|b| (b.realm.clone(), b.instance.clone()))
            .ok_or_else(|| {
                ExchangeError::new("Hyperliquid exchange was built without its runtime binding")
            })
    }
    /// Invoke the instance's ported method and await its (possibly async) result.
    async fn invoke(
        &self,
        name: &str,
        args: impl FnOnce(&mut Realm) -> Result<Vec<Value>, Value>,
    ) -> Result<(Realm, Value), ExchangeError> {
        let (mut realm, instance) = self.bound()?;
        let args = args(&mut realm).map_err(|e| realm.exchange_error(&e))?;
        let result = realm
            .call_method(&instance, name, &args)
            .map_err(|e| realm.exchange_error(&e))?;
        let value = realm
            .await_value(result)
            .await
            .map_err(|e| realm.exchange_error(&e))?;
        Ok((realm, value))
    }
    async fn typed<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        args: impl FnOnce(&mut Realm) -> Result<Vec<Value>, Value>,
    ) -> Result<T, ExchangeError> {
        let (mut realm, value) = self.invoke(name, args).await?;
        realm.to_typed(&value).map_err(|e| realm.exchange_error(&e))
    }
    async fn unit(
        &self,
        name: &str,
        args: impl FnOnce(&mut Realm) -> Result<Vec<Value>, Value>,
    ) -> Result<(), ExchangeError> {
        self.invoke(name, args).await.map(|_| ())
    }
}

fn opt<T: serde::Serialize>(r: &mut Realm, v: &Option<T>) -> Result<Value, Value> {
    match v {
        Some(v) => r.from_typed(v),
        None => Ok(Value::Undefined),
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<HyperliquidExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<HyperliquidExchange>(
            Exchange::place_orders,
        ),
        ..Default::default()
    }
}

impl Exchange for HyperliquidExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn is_public_data_only(&self) -> bool {
        HyperliquidExchange::is_public_data_only(self)
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        match self.bound() {
            Ok((mut realm, instance)) => realm
                .call_method(&instance, "dispose", &[])
                .map(|_| ())
                .map_err(|e| realm.exchange_error(&e)),
            Err(_) => self.base.dispose(),
        }
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("start", |_| Ok(vec![])))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move {
            self.typed("placeOrder", |r| Ok(vec![r.from_typed(order.options())?]))
                .await
        })
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        Box::pin(async move {
            self.typed("placeOrders", |r| {
                let list: Vec<&PlaceOrderOpts> =
                    orders.orders().iter().map(|o| o.options()).collect();
                Ok(vec![r.from_typed(&list)?])
            })
            .await
        })
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        base_exchange_rejection()
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(self.typed("validateAccount", |_| Ok(vec![])))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async move {
            self.typed("fetchPositions", |r| Ok(vec![opt(r, &symbols)?]))
                .await
        })
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async move {
            self.typed("fetchOrders", |r| Ok(vec![opt(r, &symbols)?]))
                .await
        })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        base_exchange_rejection()
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        base_exchange_rejection()
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.unit("setLeverage", |r| {
                Ok(vec![
                    Value::str(&symbol),
                    Value::Number(leverage),
                    opt(r, &side)?,
                ])
            })
            .await
        })
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.unit("changeMarginType", |r| {
                Ok(vec![Value::Bool(isolated), opt(r, &symbol)?])
            })
            .await
        })
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move {
            self.typed("updateOrder", |r| Ok(vec![r.from_typed(&options)?]))
                .await
        })
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.unit("cancelOrders", |r| Ok(vec![r.from_typed(&orders)?]))
                .await
        })
    }
    /// None omits the argument (clear), Some(None) passes null (clear), Some(Some(t)) arms at t.
    fn schedule_cancel(&self, time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.unit("scheduleCancel", |_| {
                Ok(match time {
                    None => vec![],
                    Some(None) => vec![Value::Null],
                    Some(Some(t)) => vec![Value::Number(t)],
                })
            })
            .await
        })
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        base_exchange_rejection()
    }
}
