//! Production KuCoin Futures and Spot venues: the assembled JS class graph of
//! the ported bodies on the shared js_runtime, behind the typed Exchange trait.
use crate::binding::{self, Extra, Shared, globals, host::W, http::ApiHost, ops};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedOrder},
    http::HttpClientOptions,
    js_runtime::{
        base_exchange::{BaseExchangeParts, HostedSigner},
        globals::{install_builtins, install_enums},
        library::install_library,
        promise::await_js,
        scope::ScopeShared,
        sockets, store,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{TransportRegistry, rate_limit::RateLimitClock};
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeOptions, Execution, ExecutionsOptions, Market, OHLCVOptions,
    Order, OrderBook, PlaceOrderOpts, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use std::{cell::RefCell, rc::Rc};

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/support/native_reconnect.rs"]
mod native_reconnect_tests;

/// Host collaborators of both KuCoin classes.
pub struct KucoinDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// One hosted account, shared with source options and typed availability.
    pub hosted_signer: Option<HostedSigner>,
    /// Shared HTTP client options for createAPI/createKucoinSpotAPI (adapter, host, stats).
    pub http: Rc<dyn Fn(&ExchangeOptions) -> HttpClientOptions>,
    pub registry: Rc<TransportRegistry>,
    pub clock: VirtualClock,
    pub retry_clock: Rc<dyn RateLimitClock>,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

struct Assembly {
    shared: Rc<Shared>,
    this: Js,
}

fn assemble(
    mut parts: BaseExchangeParts,
    deps: KucoinDependencies,
    spot: bool,
) -> Result<Assembly, ExchangeError> {
    parts.settings.client_order_id_rule = Some(globals::client_order_id_rule(spot));
    let name = js(&parts.name)?;
    let options = js(&parts.options)?;
    let store = store::facade(parts.store.clone());
    let http = Rc::new_cyclic(|me| ApiHost {
        me: me.clone(),
        runtime: deps.runtime.clone(),
        options: parts.options.clone(),
        hosted_signer: deps.hosted_signer.clone(),
        spot,
        http: deps.http.clone(),
        registry: deps.registry.clone(),
        clock: deps.clock.clone(),
        retry_clock: deps.retry_clock.clone(),
        api: RefCell::new(None),
    });
    let runtime = deps.runtime.clone();
    let mut failure = None;
    let shared: Rc<Shared> = Rc::new_cyclic(|weak: &W| {
        let g = Js::object(vec![]);
        install_builtins(&g, weak.clone());
        install_enums(&g);
        install_library(&g, weak.clone());
        if let Err(e) = globals::install(&g, weak.clone(), &runtime) {
            failure = Some(e);
        }
        ScopeShared::new(
            runtime.clone(),
            http.clone(),
            parts.store.clone(),
            g,
            Extra {
                supers: RefCell::default(),
                parts: RefCell::new(None),
                api: http,
            },
        )
    });
    if let Some(e) = failure {
        return Err(err(e));
    }
    // The socket context exists only once the scope does.
    shared
        .globals
        .define(
            "BaseWebSocket",
            sockets::base_websocket_class(&shared.sockets),
        )
        .map_err(err)?;
    *shared.extra.parts.borrow_mut() = Some(parts);
    let weak = Rc::downgrade(&shared);
    let class = define_classes(&weak, spot).map_err(err)?;
    let this = class
        .construct(vec![name, options, store], None)
        .map_err(err)?;
    *shared.exchange.borrow_mut() = this.clone();
    Ok(Assembly { shared, this })
}

fn define_classes(weak: &W, spot: bool) -> Result<Js, Thrown> {
    let shared = binding::host::shared(weak)?;
    let language = ops::private_class(weak);
    let construction = ops::construction();
    let def = |name: &str, class: Js| shared.globals.define(name, class);
    if spot {
        def(
            "KucoinSpotPrivateWebsocket",
            crate::spot_private_class::define_spot_private_class(language.clone())?,
        )?;
        def(
            "KucoinSpotPublicWebsocket",
            crate::spot_public_class::define_spot_public_class(language.clone())?,
        )?;
        let bodies = weak.clone();
        let class = crate::spot_exchange_class::define_spot_exchange_class(Rc::new(
            crate::spot_exchange_class::SpotExchangeOps {
                language,
                construction,
                body: Rc::new(move |r| ops::spot_body(&bodies, r)),
                regexp: Rc::new(globals::regexp),
            },
        ))?;
        def("KucoinSpotExchange", class.clone())?;
        Ok(class)
    } else {
        def(
            "KucoinPrivateWebsocket",
            crate::futures_private_class::define_futures_private_class(language.clone())?,
        )?;
        def(
            "KucoinPublicWebsocket",
            crate::futures_public_class::define_futures_public_class(language, construction)?,
        )?;
        let class = binding::futures::define(weak)?;
        def("KucoinExchange", class.clone())?;
        Ok(class)
    }
}

/// Own fields of the receiver and its sockets close over the receiver; clear
/// them when the venue goes so the JS graph does not keep itself alive.
fn release(this: &Js) {
    let mut objects = vec![this.clone()];
    for key in ["publicWebsocket", "privateWebsocket"] {
        if let Ok(v) = this.get(key) {
            objects.push(v);
        }
    }
    if let Ok(list) = this.get("snapshotWebsockets") {
        objects.extend(list.array_values().unwrap_or_default());
    }
    for object in objects {
        if let Js::Object(o) = object {
            o.props.borrow_mut().clear();
        }
    }
}

/// The assembled receiver both venue structs dispatch through.
struct Core {
    shared: Rc<Shared>,
    this: Js,
}

impl Core {
    fn new(
        parts: BaseExchangeParts,
        deps: KucoinDependencies,
        spot: bool,
    ) -> Result<Self, ExchangeError> {
        let Assembly { shared, this } = assemble(parts, deps, spot)?;
        Ok(Self { shared, this })
    }

    /// Publish the venue to the socket cores and the isPublicDataOnly getter.
    fn attach(&self, venue: Rc<dyn Exchange>) {
        *self.shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&venue));
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Result<Vec<Js>, ExchangeError>,
    ) -> ExchangeFuture<'static, T> {
        let this = self.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args?).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }

    fn invoke_unit(
        &self,
        member: &'static str,
        args: Result<Vec<Js>, ExchangeError>,
    ) -> ExchangeFuture<'static, ()> {
        let this = self.this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args?).map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }

    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let runtime = self.shared.runtime.clone();
        let handler = Js::function("", move |_, a| {
            match to_typed::<T>(&arg(&a, 0)) {
                Ok(v) => callback(v),
                Err(e) => {
                    runtime.report_exception(&err(e), &serde_json::json!({"source": member}).into())
                }
            }
            Ok(Js::Undefined)
        });
        let cleanup = self
            .this
            .call_method(member, vec![first, handler])
            .map_err(err)?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }

    fn dispose(&self) -> Result<(), ExchangeError> {
        self.this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        release(&self.this);
        self.shared.extra.supers.borrow_mut().clear();
    }
}

pub struct KucoinExchange {
    receiver: Core,
    capabilities: ExchangeCapabilities,
}

pub struct KucoinSpotExchange {
    receiver: Core,
    capabilities: ExchangeCapabilities,
}

impl KucoinExchange {
    // ts: KucoinExchange.constructor
    pub fn new(
        parts: BaseExchangeParts,
        deps: KucoinDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let venue = Rc::new(Self {
            receiver: Core::new(parts, deps, false)?,
            capabilities: Self::capabilities_table(),
        });
        venue.receiver.attach(venue.clone());
        Ok(venue)
    }
    fn capabilities_table() -> ExchangeCapabilities {
        ExchangeCapabilities {
            start: CapabilityBinding::implemented(|venue, ()| venue.start()),
            place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
            place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
            ..Default::default()
        }
    }
}

impl KucoinSpotExchange {
    // ts: KucoinSpotExchange.constructor
    pub fn new(
        parts: BaseExchangeParts,
        deps: KucoinDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let venue = Rc::new(Self {
            receiver: Core::new(parts, deps, true)?,
            capabilities: Self::capabilities_table(),
        });
        venue.receiver.attach(venue.clone());
        Ok(venue)
    }
    fn capabilities_table() -> ExchangeCapabilities {
        ExchangeCapabilities {
            start: CapabilityBinding::implemented(|venue, ()| venue.start()),
            place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
            place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
            fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
                venue.fetch_historical_fills(opts)
            }),
            ..Default::default()
        }
    }
}

fn base(shared: &Shared) -> &BaseExchange {
    shared
        .base
        .get()
        .expect("BaseExchange is constructed by assembly")
}

impl Exchange for KucoinExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        base(&self.receiver.shared)
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    // ts: KucoinExchange.dispose
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.receiver.dispose()
    }
    // ts: KucoinExchange.placeOrder — its body's first statement is the
    // resolveCallerClientOrderId guard (order_lifecycle::place_order).
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        self.receiver
            .invoke("placeOrder", js(order.options()).map(|o| vec![o]))
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit("start", Ok(vec![]))
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.receiver.invoke("getAccount", Ok(vec![]))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.receiver.invoke("validateAccount", Ok(vec![]))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.receiver
            .invoke("fetchPositions", opt_js(symbols).map(|s| vec![s]))
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.receiver.invoke("fetchOrders", Ok(vec![]))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.receiver.invoke("fetchMarkets", Ok(vec![]))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.receiver.invoke("fetchTickers", Ok(vec![]))
    }
    // ts: base.ts:799 — KucoinExchange inherits fetchSymbolMaxLeverage, which rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit(
            "setLeverage",
            Ok(vec![Js::str(&symbol), Js::Number(leverage)]),
        )
    }
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        self.receiver
            .invoke_unit("changePositionMode", Ok(vec![Js::Bool(hedged)]))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit(
            "changeMarginType",
            opt_js(symbol).map(|s| vec![Js::Bool(isolated), s]),
        )
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.receiver
            .invoke("updateOrder", js(&options).map(|o| vec![o]))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.receiver
            .invoke_unit("cancelOrders", js(&orders).map(|o| vec![o]))
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.receiver
            .invoke_unit("cancelSymbolOrders", Ok(vec![Js::str(&symbol)]))
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit("cancelAllOrders", Ok(vec![]))
    }
    // ts: base.ts:989 — KucoinExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — KucoinExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.receiver
            .invoke("fetchOHLCV", js(&options).map(|o| vec![o]))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.receiver.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.receiver
            .listen("listenOrderBook", Js::str(&symbol), callback)
    }
}

impl Exchange for KucoinSpotExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        base(&self.receiver.shared)
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    // ts: KucoinSpotExchange.dispose
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.receiver.dispose()
    }
    // ts: KucoinSpotExchange.placeOrder — its body's first statement is the
    // resolveCallerClientOrderId guard (spot_bodies::place_order).
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        self.receiver
            .invoke("placeOrder", js(order.options()).map(|o| vec![o]))
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit("start", Ok(vec![]))
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.receiver.invoke("getAccount", Ok(vec![]))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.receiver.invoke("validateAccount", Ok(vec![]))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.receiver
            .invoke("fetchPositions", opt_js(symbols).map(|s| vec![s]))
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.receiver
            .invoke("fetchOrders", opt_js(symbols).map(|s| vec![s]))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.receiver.invoke("fetchMarkets", Ok(vec![]))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.receiver.invoke("fetchTickers", Ok(vec![]))
    }
    fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
        self.receiver
            .invoke("fetchSymbolMaxLeverage", Ok(vec![Js::str(&symbol)]))
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit(
            "setLeverage",
            opt_js(side).map(|s| vec![Js::str(&symbol), Js::Number(leverage), s]),
        )
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit(
            "changePositionMode",
            opt_js(symbol).map(|s| vec![Js::Bool(hedged), s]),
        )
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit(
            "changeMarginType",
            opt_js(symbol).map(|s| vec![Js::Bool(isolated), s]),
        )
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.receiver
            .invoke("updateOrder", js(&options).map(|o| vec![o]))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.receiver
            .invoke_unit("cancelOrders", js(&orders).map(|o| vec![o]))
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.receiver
            .invoke_unit("cancelSymbolOrders", Ok(vec![Js::str(&symbol)]))
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.receiver.invoke_unit("cancelAllOrders", Ok(vec![]))
    }
    // ts: base.ts:989 — KucoinSpotExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — KucoinSpotExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.receiver
            .invoke("fetchOHLCV", js(&options).map(|o| vec![o]))
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.receiver
            .invoke("fetchHistoricalFills", js(&options).map(|o| vec![o]))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.receiver.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.receiver
            .listen("listenOrderBook", Js::str(&symbol), callback)
    }
}
