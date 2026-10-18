//! Production Crypto.com venue: the assembled class graph behind the typed Exchange trait.
use super::{
    globals, ops,
    rt::{CryptocomExtra, ExternalPublicDataFactory, Rt, Shared},
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        client::JsHttpHost,
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct CryptocomDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

// ts: exchangeMetadata cryptocom — tickInterval 2000, supportsPublicOnly, /^.{1,36}$/ client IDs.
pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: Some(2000.0),
        positions_supported: None,
        supports_public_only: true,
        client_order_id_rule: Some(globals::client_order_id_rule()),
    }
}

pub struct CryptocomExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
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

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

/// The source BaseExchange default for a member Crypto.com does not override.
fn not_implemented<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

/// Define the module-scope classes and `createAPI`, which need the live scope.
fn define_classes(weak: &std::rc::Weak<Shared>, globals: &Js) -> Result<Js, Thrown> {
    let runtime = ops::runtime_ops(weak.clone());
    let socket = ops::socket_class_ops(weak.clone());
    globals.define(
        "CryptocomPublicWebSocket",
        crate::socket_classes::define_public_socket(socket.clone())?,
    )?;
    globals.define(
        "CryptocomPrivateWebSocket",
        crate::socket_classes::define_private_socket(socket)?,
    )?;
    let api_ops = runtime.clone();
    globals.define(
        "createAPI",
        Js::function("createAPI", move |_, a| {
            crate::runtime::create_api(api_ops.clone(), arg(&a, 0), arg(&a, 1))
        }),
    )?;
    let class = crate::runtime::define_exchange(runtime)?;
    globals.define("CryptocomExchange", class.clone())?;
    Ok(class)
}

impl CryptocomExchange {
    // ts: CryptocomExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: CryptocomDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Cryptocom {
            return Err(ExchangeError::new(format!(
                "CryptocomExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals::build(weak.clone()),
                CryptocomExtra {
                    name: name.clone(),
                    options: options.clone(),
                    classes: Default::default(),
                    external_public_data: deps.external_public_data.clone(),
                },
            )
        });
        let weak = Rc::downgrade(&shared);
        let class = define_classes(&weak, &shared.globals).map_err(err)?;
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store::facade(store.clone());
        let receiver =
            ops::construct(&weak, class, vec![js_name, js_options, js_store]).map_err(err)?;
        *shared.exchange.borrow_mut() = receiver.clone();
        let rt = Rt {
            shared: shared.clone(),
            this: receiver,
        };
        let venue = Rc::new(Self {
            rt,
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(venue)
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let reporter = self.rt.shared.runtime.clone();
        let listener = Js::function("callback", move |_, a| {
            let value = a.into_iter().next().unwrap_or(Js::Undefined);
            match to_typed::<T>(&value) {
                Ok(typed) => callback(typed),
                Err(error) => reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "cryptocom listener payload"}),
                ),
            }
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(member, vec![first, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<CryptocomExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<CryptocomExchange>(Exchange::place_orders),
        ..Default::default()
    }
}

impl Exchange for CryptocomExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        // Source assigns isDisposed through the shared writable facade, then
        // disposes its two sockets without invoking BaseExchange.dispose.
        self.rt
            .this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
    // ts: placeOrder — BaseExchange.place_order already enforced the client-order-ID
    // contract; the body's first statement re-runs resolveCallerClientOrderId.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let list: Vec<PlaceOrderOpts> = orders
            .orders()
            .iter()
            .map(|o| o.options().clone())
            .collect();
        let a = args!(js(&list));
        self.invoke("placeOrders", a)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    // ts: fetchPositions / fetchOrders take no symbol filter.
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", vec![])
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // ts: base.ts:799 — inherited fetchSymbolMaxLeverage rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        not_implemented()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged));
        self.invoke_unit("changePositionMode", a)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&isolated), opt_js(symbol));
        self.invoke_unit("changeMarginType", a)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", from_typed(&options).map_err(err)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", Js::str(&symbol), callback)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.invoke_unit("cancelSymbolOrders", a)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
    // ts: base.ts:989 — inherited scheduleCancel rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    // ts: base.ts:1138 — inherited fetchPublicTrades rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        not_implemented()
    }
    // ts: base.ts:997 — inherited fetchHistoricalFills resolves [].
    fn fetch_historical_fills(
        &self,
        _options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async { Ok(vec![]) })
    }
}

#[cfg(test)]
#[path = "iterable_tests.rs"]
mod iterable_tests;
