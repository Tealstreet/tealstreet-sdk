//! Production `impl Exchange` for HyperliquidExchange. The constructor runs the
//! ported class constructor inside the realm; every trait method invokes the
//! instance's ported method and converts its result at the typed boundary.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../tests/support/api_signing_audit.rs"]
mod api_signing_audit;

#[cfg(test)]
#[path = "../tests/support/cloid_audit.rs"]
mod cloid_audit;

#[cfg(test)]
#[path = "../tests/support/order_error_audit.rs"]
mod order_error_audit;

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../tests/support/signing_audit.rs"]
mod signing_audit;

use crate::{
    cloid::hyperliquid_client_order_id_rule,
    core_exchange_class,
    exchange::HyperliquidExchange,
    host::{
        Host,
        facade_exchange::base_exchange_class,
        facade_rest::{RestDependencies, bind_api_factories},
        facade_socket::{SocketDependencies, base_websocket_class},
        facade_store::store_object,
        imports,
    },
    private_class, public_class,
    rate_limit::RateLimiter,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    client_order_id::ClientOrderLookup,
    exchange::{
        AccountIdentityPatch, BaseExchange, Exchange, ExchangeSettings, IdentityFieldPatch,
        ValidatedBatch, ValidatedOrder,
    },
    js_runtime::base_exchange::ExternalPublicDataFactory,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeSigners, ListenerCleanup},
    websocket::WsRuntime,
    ws_parent::WsExchangeParent,
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::{
    ExchangeAccount, ExchangeName, ExchangeOptions, Market, Order, PlaceOrderOpts, Position,
    PositionSide, PublicTrade, Ticker, UpdateOrderOpts, ValidateAccountResult,
};
use std::{rc::Rc, sync::Arc};

/// What the embedding host supplies: its runtime (clock, tasks, sockets), the
/// composed HTTP transport, and the process-wide Hyperliquid weight limiter.
pub struct HyperliquidDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Arc<dyn RestAdapter>,
    pub limiter: Rc<RateLimiter>,
    pub signers: ExchangeSigners,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

#[derive(Clone)]
pub(crate) struct Binding {
    pub realm: Host,
    pub instance: Js,
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
pub(crate) fn base_exchange_rejection<T: 'static>(
    runtime: &Rc<dyn ExchangeRuntime>,
) -> ExchangeFuture<'static, T> {
    safe_cex_exchange::eager::start_eager(runtime, async {
        Err(ExchangeError::new("Not implemented"))
    })
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
        let mut base = BaseExchange::new(
            ExchangeName::Hyperliquid,
            options.clone(),
            store.clone(),
            runtime.clone(),
            settings,
        );
        base.signers = deps.signers;
        let mut venue = Self::from_base(base);
        venue.capabilities = capabilities();
        let venue = Rc::new(venue);
        let as_exchange: Rc<dyn Exchange> = venue.clone();
        let parent = WsExchangeParent::new(&as_exchange);

        let mut realm = Host::new(deps.runtime.clone(), deps.http.clone(), store.clone());
        imports::bind(&realm);
        let thrown = |r: &mut Host, e: Thrown| r.exchange_error(&e);
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
        let base_class =
            base_exchange_class(&mut realm, venue.base.clone(), deps.external_public_data);
        let class = core_exchange_class::create_core_exchange_class(&mut realm, &base_class)
            .map_err(|e| thrown(&mut realm, e))?;
        realm.bind_import("HyperliquidExchange", class.clone());
        let js_options = realm
            .encode_typed(&options)
            .map_err(|e| thrown(&mut realm, e))?;
        // Startup resolves legacy identities by mutating the shared JS options.
        // Publish each successful write immediately to native privacy readers,
        // including callbacks that run before start() returns or rejects.
        let identity_owner = venue.base.clone();
        js_options
            .set_write_observer(Some(Rc::new(move |_, key, value| {
                let application_id = key == "applicationId".encode_utf16().collect::<Vec<_>>();
                if !application_id && key != "vault".encode_utf16().collect::<Vec<_>>() {
                    return Ok(());
                }
                let value = match value {
                    Js::Undefined => IdentityFieldPatch::Remove,
                    Js::String(units) => {
                        IdentityFieldPatch::Set(String::from_utf16(units).map_err(|_| {
                            Thrown::type_error("Native account identity requires scalar Unicode")
                        })?)
                    }
                    _ => {
                        return Err(Thrown::type_error(
                            "Native account identity must be a string",
                        ));
                    }
                };
                let patch = if application_id {
                    AccountIdentityPatch {
                        application_id: value,
                        vault: IdentityFieldPatch::Unchanged,
                    }
                } else {
                    AccountIdentityPatch {
                        application_id: IdentityFieldPatch::Unchanged,
                        vault: value,
                    }
                };
                identity_owner.patch_account_identity(patch);
                Ok(())
            })))
            .map_err(|error| error.to_exchange_error())?;
        let js_store = store_object(&realm, store).map_err(|error| error.to_exchange_error())?;
        let mut scope = realm.with_lexical(&class);
        let instance = scope
            .construct(
                &class,
                &[Js::str("hyperliquid"), js_options, js_store],
                None,
            )
            .map_err(|e| thrown(&mut scope, e))?;
        *venue.binding.borrow_mut() = Some(Binding {
            realm: scope,
            instance,
        });
        Ok(venue)
    }

    fn bound(&self) -> Result<Binding, ExchangeError> {
        self.binding.borrow().clone().ok_or_else(|| {
            ExchangeError::new("Hyperliquid exchange was built without its runtime binding")
        })
    }
    fn launch<T: 'static, F: std::future::Future<Output = Result<T, ExchangeError>> + 'static>(
        &self,
        body: impl FnOnce(Binding) -> F + 'static,
    ) -> ExchangeFuture<'static, T> {
        let binding = self.bound();
        safe_cex_exchange::eager::start_eager(
            &self.base.runtime,
            async move { body(binding?).await },
        )
    }
    fn typed<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &'static str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown> + 'static,
    ) -> ExchangeFuture<'static, T> {
        self.launch(move |binding| async move { binding.typed(name, args).await })
    }
    fn unit(
        &self,
        name: &'static str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown> + 'static,
    ) -> ExchangeFuture<'static, ()> {
        self.launch(move |binding| async move { binding.unit(name, args).await })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &str,
        first: impl serde::Serialize,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let Binding {
            mut realm,
            instance,
        } = self.bound()?;
        let listener = realm.native("callback", 1, move |realm, _, args| {
            let value = args.first().cloned().unwrap_or(Js::Undefined);
            callback(realm.to_typed::<T>(&value)?);
            Ok(Js::Undefined)
        });
        let first = realm
            .encode_typed(&first)
            .map_err(|error| realm.exchange_error(&error))?;
        let cleanup = realm
            .call_method(&instance, name, &[first, listener])
            .map_err(|error| realm.exchange_error(&error))?;
        Ok(Box::new(move || {
            realm
                .call(&cleanup, &Js::Undefined, &[])
                .map(|_| ())
                .map_err(|error| realm.exchange_error(&error))
        }))
    }
}

impl Binding {
    /// Invoke the instance's ported method and await its (possibly async) result.
    pub(crate) async fn invoke(
        &self,
        name: &str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown>,
    ) -> Result<(Host, Js), ExchangeError> {
        let (mut realm, instance) = (self.realm.clone(), self.instance.clone());
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
    pub(crate) async fn typed<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown>,
    ) -> Result<T, ExchangeError> {
        let (realm, value) = self.invoke(name, args).await?;
        realm.to_typed(&value).map_err(|e| realm.exchange_error(&e))
    }
    pub(crate) async fn unit(
        &self,
        name: &str,
        args: impl FnOnce(&mut Host) -> Result<Vec<Js>, Thrown>,
    ) -> Result<(), ExchangeError> {
        self.invoke(name, args).await.map(|_| ())
    }
}

pub(crate) fn opt<T: serde::Serialize>(r: &mut Host, v: &Option<T>) -> Result<Js, Thrown> {
    match v {
        Some(v) => r.encode_typed(v),
        None => Ok(Js::Undefined),
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
            Ok(Binding {
                mut realm,
                instance,
            }) => realm
                .call_method(&instance, "dispose", &[])
                .map(|_| ())
                .map_err(|e| realm.exchange_error(&e)),
            Err(_) => self.base.dispose(),
        }
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("start", |_| Ok(vec![])))
    }
    fn listen_ohlcv(
        &self,
        options: safe_cex_types::OHLCVOptions,
        callback: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", options, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", symbol, callback)
    }
    fn fetch_ohlcv(
        &self,
        options: safe_cex_types::OHLCVOptions,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::Candle>> {
        Box::pin(self.typed("fetchOHLCV", move |r| Ok(vec![r.encode_typed(&options)?])))
    }
    fn fetch_historical_fills(
        &self,
        options: safe_cex_types::ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::Execution>> {
        Box::pin(self.typed("fetchHistoricalFills", move |r| {
            Ok(vec![r.encode_typed(&options)?])
        }))
    }
    fn fetch_historical_orders(
        &self,
        options: Option<safe_cex_types::OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::HistoricalOrder>> {
        Box::pin(self.typed("fetchHistoricalOrders", move |r| {
            Ok(vec![opt(r, &options)?])
        }))
    }
    fn fetch_income_history(
        &self,
        options: Option<safe_cex_types::IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::IncomeRecord>> {
        Box::pin(self.typed("fetchIncomeHistory", move |r| Ok(vec![opt(r, &options)?])))
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::IncomeRecord>> {
        Box::pin(self.typed("fetchIncomeHistoryRange", move |r| {
            Ok(vec![Js::Number(start), Js::Number(end), opt(r, &kind)?])
        }))
    }
    fn fetch_and_set_private_snapshot(&self, tries: u32) -> ExchangeFuture<'_, ()> {
        Box::pin(self.unit("fetchAndSetPrivateSnapshot", move |_| {
            Ok(vec![Js::Number(tries as f64)])
        }))
    }
    fn find_order_by_client_order_id(
        &self,
        symbol: String,
        id: String,
    ) -> ExchangeFuture<'_, ClientOrderLookup> {
        self.launch(move |binding| async move {
            let (mut realm, result) = binding
                .invoke("findOrderByClientOrderId", move |_| {
                    Ok(vec![Js::str(&symbol), Js::str(&id)])
                })
                .await?;
            let found = realm
                .get_str(&result, "found")
                .map_err(|error| realm.exchange_error(&error))?;
            if found.truthy() {
                let order = realm
                    .get_str(&result, "order")
                    .map_err(|error| realm.exchange_error(&error))?;
                let order = realm
                    .to_typed(&order)
                    .map_err(|error| realm.exchange_error(&error))?;
                Ok(ClientOrderLookup::Found(Box::new(order)))
            } else {
                let definitive = realm
                    .get_str(&result, "definitive")
                    .map_err(|error| realm.exchange_error(&error))?
                    .truthy();
                Ok(ClientOrderLookup::NotFound { definitive })
            }
        })
    }
    fn fetch_historical_positions(
        &self,
        options: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        Box::pin(self.typed("fetchHistoricalPositions", move |realm| {
            let opts = realm.new_object();
            realm.define_str(&opts, "account", Js::str(&options.account))?;
            if let Some(from) = options.from {
                realm.define_str(&opts, "from", Js::Number(from))?;
            }
            if let Some(to) = options.to {
                realm.define_str(&opts, "to", Js::Number(to))?;
            }
            Ok(vec![opts])
        }))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        self.typed("placeOrder", move |r| {
            Ok(vec![r.encode_typed(order.options())?])
        })
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        self.typed("placeOrders", move |r| {
            let list: Vec<&PlaceOrderOpts> = orders.orders().iter().map(|o| o.options()).collect();
            Ok(vec![r.encode_typed(&list)?])
        })
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(self.typed("validateAccount", |_| Ok(vec![])))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.typed("fetchPositions", move |r| Ok(vec![opt(r, &symbols)?]))
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.typed("fetchOrders", move |r| Ok(vec![opt(r, &symbols)?]))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.unit("setLeverage", move |r| {
            Ok(vec![Js::str(&symbol), Js::Number(leverage), opt(r, &side)?])
        })
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection(&self.base.runtime)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.unit("changeMarginType", move |r| {
            Ok(vec![Js::Bool(isolated), opt(r, &symbol)?])
        })
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.typed("updateOrder", move |r| Ok(vec![r.encode_typed(&options)?]))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.unit("cancelOrders", move |r| Ok(vec![r.encode_typed(&orders)?]))
    }
    /// None omits the argument (clear), Some(None) passes null (clear), Some(Some(t)) arms at t.
    fn schedule_cancel(&self, time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        self.unit("scheduleCancel", move |_| {
            Ok(match time {
                None => vec![],
                Some(None) => vec![Js::Null],
                Some(Some(t)) => vec![Js::Number(t)],
            })
        })
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        base_exchange_rejection(&self.base.runtime)
    }
}
