//! Production WOO venue: the assembled class graph behind the typed Exchange trait.
use super::{
    globals,
    promise::await_js,
    rt::{Rt, Shared, WooExtra},
    store,
    value::{Js, Thrown, from_typed, to_typed},
};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder},
    js_runtime::scope::ScopeShared,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct WooDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn safe_cex_exchange::js_runtime::client::JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<super::rt::ExternalPublicDataFactory>,
}

pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: None,
        positions_supported: Some(true),
        supports_public_only: true,
        client_order_id_rule: Some(super::hosts::members::client_order_id_rule()),
    }
}

pub struct WooExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl WooExchange {
    // ts: WOOXExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: WooDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Woo {
            return Err(ExchangeError::new(format!(
                "WOOXExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let runtime: super::ops::Runtime = deps.runtime.clone();
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals::build(weak.clone()),
                WooExtra {
                    name: name.clone(),
                    options: options.clone(),
                    external_public_data: deps.external_public_data.clone(),
                    classes: Default::default(),
                    metadata: Default::default(),
                },
            )
        });
        let rt = Rt {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        globals::install_socket_base(&rt);
        let weak = Rc::downgrade(&shared);
        super::hosts::socket_control::define_socket_classes(&rt, weak.clone(), runtime.clone())
            .map_err(err)?;
        let class =
            crate::exchange_class::define_exchange_class(super::ops::exchange_class(weak, runtime))
                .map_err(err)?;
        let _ = shared.globals.define("WOOXExchange", class.clone());
        let receiver = class
            .construct(
                vec![
                    from_typed(&name).map_err(err)?,
                    from_typed(&options).map_err(err)?,
                    store::facade(store.clone())?,
                ],
                None,
            )
            .map_err(err)?;
        *shared.exchange.borrow_mut() = receiver.clone();
        let venue = Rc::new(Self {
            rt: rt.with_this(receiver),
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
        let runtime = self.rt.exchange_runtime();
        let result = self.rt.this.call_method(member, args);
        Box::pin(async move {
            let value = await_js(&runtime, result.map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let runtime = self.rt.exchange_runtime();
        let result = self.rt.this.call_method(member, args);
        Box::pin(async move {
            await_js(&runtime, result.map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }
}

impl WooExchange {
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        subject: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let deliver = Js::function("", move |_, a| {
            callback(to_typed(&a.into_iter().next().unwrap_or(Js::Undefined))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(member, vec![subject, deliver])
            .map_err(err)?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

fn not_implemented<T: 'static>() -> ExchangeFuture<'static, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<WooExchange>(Exchange::place_order),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        fetch_historical_orders: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_orders(opts)
        }),
        ..Default::default()
    }
}

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

impl Exchange for WooExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt
            .this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
    // ts: WOOXExchange.placeOrder — the ValidatedOrder carries the client-order-ID
    // contract, already enforced by BaseExchange.place_order before this call.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
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
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchPositions", a)
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        // ts: WOOXExchange.fetchOrders takes no symbol filter.
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // ts: base.ts:799 — WOOXExchange inherits fetchSymbolMaxLeverage, which rejects.
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
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged), opt_js(symbol));
        self.invoke_unit("changePositionMode", a)
    }
    // ts: base.ts:836 — WOOXExchange inherits changeMarginType, which rejects.
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    // ts: WOOXExchange.cancelSymbolOrders / cancelAllOrders override the base loops.
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.invoke_unit("cancelSymbolOrders", a)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
    // ts: WOOXExchange.afterMethodCalledInWorker
    fn after_method_called_in_worker(
        &self,
        result: &serde_json::Value,
        method: safe_cex_exchange::worker::CallableMethodsInWorker,
        args: &[serde_json::Value],
    ) -> Result<(), ExchangeError> {
        let mut a = vec![super::value::from_json(result), js(&method)?];
        a.extend(args.iter().map(super::value::from_json));
        self.rt
            .this
            .call_method("afterMethodCalledInWorker", a)
            .map(|_| ())
            .map_err(err)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    // ts: WOOXExchange.listenOHLCV / listenOrderBook return the cleanup function.
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", js(&symbol)?, callback)
    }
    // ts: base.ts:989 — WOOXExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    // ts: base.ts:1138 — WOOXExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        not_implemented()
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
    }
    // ts: WOOXExchange.fetchHistoricalPositions reconstructs its order-history axis.
    fn fetch_historical_positions(
        &self,
        options: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        let opts = Js::object(vec![
            ("account", Js::str(&options.account)),
            ("from", options.from.map_or(Js::Undefined, Js::Number)),
            ("to", options.to.map_or(Js::Undefined, Js::Number)),
        ]);
        self.invoke("fetchHistoricalPositions", vec![opts])
    }
    fn fetch_historical_orders(
        &self,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        let a = args!(opt_js(options));
        self.invoke("fetchHistoricalOrders", a)
    }
}

#[cfg(test)]
#[path = "../../tests/support/live_order_mapping.rs"]
mod live_order_mapping_tests;

#[cfg(test)]
#[path = "../../tests/support/iterable_binding.rs"]
mod iterable_binding_tests;

#[cfg(test)]
#[path = "../../tests/support/structural_aliases.rs"]
mod structural_alias_tests;

#[cfg(test)]
#[path = "../../tests/support/eager_binding.rs"]
mod eager_binding_tests;

#[cfg(test)]
#[path = "../../tests/support/divergence_binding.rs"]
mod divergence_audit_tests;

#[cfg(test)]
#[path = "../../tests/support/fix_audit_binding.rs"]
mod fix_audit_tests;

#[cfg(test)]
#[path = "../../tests/support/heartbeat_binding.rs"]
mod heartbeat_binding_tests;

#[cfg(test)]
#[path = "../../tests/support/history_row_binding.rs"]
mod history_row_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "native_error_tests.rs"]
mod native_error_tests;
