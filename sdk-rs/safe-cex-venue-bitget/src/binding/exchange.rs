//! Production Bitget venue: the assembled class graph behind the typed Exchange trait.
#[cfg(test)]
#[path = "iterator_tests.rs"]
mod iterator_tests;
use super::{
    globals,
    promise::await_js,
    rt::{BitgetExtra, Rt, Shared, Variant},
    store,
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::assembly;
use safe_cex_exchange::js_runtime::scope::ScopeShared;
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    journal::HistoricalPositionOptions,
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeName, ExchangeOptions, Execution, ExecutionsOptions,
    IncomeHistoryOptions, IncomeRecord, Market, OHLCVOptions, Order, OrderBook, PlaceOrderOpts,
    Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts, ValidateAccountResult,
};
use std::rc::Rc;

pub struct BitgetDependencies {
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
        client_order_id_rule: Some(super::hosts::assembly::client_order_id_rule()),
    }
}

pub struct BitgetExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl BitgetExchange {
    // ts: BitgetExchange.constructor (and the USDC / inverse subclasses by name)
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BitgetDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let variant = match name {
            ExchangeName::Bitget => Variant::Classic,
            ExchangeName::Bitgetusdc => Variant::Usdc,
            ExchangeName::Bitgetinverse => Variant::Inverse,
            other => {
                return Err(ExchangeError::new(format!(
                    "BitgetExchange cannot construct {}",
                    serde_json::to_value(&other)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default()
                )));
            }
        };
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            let globals = globals::build(weak.clone());
            super::hosts::register_all(&globals, weak.clone());
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals,
                BitgetExtra {
                    variant,
                    name: name.clone(),
                    options: options.clone(),
                    external_public_data: deps.external_public_data.clone(),
                },
            )
        });
        let rt = Rt {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store::facade(store.clone());
        let receiver = match variant {
            Variant::Classic => {
                assembly::BitgetExchange::new(rt.clone(), js_name, js_options, js_store)
                    .map_err(err)?
                    .receiver
            }
            Variant::Usdc => {
                assembly::BitgetUSDCExchange::new(rt.clone(), js_name, js_options, js_store)
                    .map_err(err)?
                    .inner
                    .receiver
            }
            Variant::Inverse => {
                assembly::BitgetInverseExchange::new(rt.clone(), js_name, js_options, js_store)
                    .map_err(err)?
                    .inner
                    .receiver
            }
        };
        *shared.exchange.borrow_mut() = receiver.clone();
        super::getters::install(&rt.with_this(receiver.clone()));
        super::prototype::install(&rt.with_this(receiver.clone()));
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
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(
                &self.rt.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    // ts: BitgetExchange.listenOHLCV / listenOrderBook return the socket's cleanup.
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let listener = Js::function("callback", move |_, args| {
            callback(to_typed(&super::value::arg(&args, 0))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(member, vec![first, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            Ok(())
        }))
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(
                &self.rt.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            Ok(())
        })
    }
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BitgetExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<BitgetExchange>(Exchange::place_orders),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        fetch_income_history: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_income_history(opts)
        }),
        fetch_income_history_range: CapabilityBinding::implemented(|venue, (start, end, kind)| {
            venue.fetch_income_history_range(start, end, kind)
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

impl Exchange for BitgetExchange {
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
    // ts: BitgetExchange.placeOrder — the ValidatedOrder carries the client-order-ID
    // contract, already enforced by BaseExchange.place_order before this call.
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
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchPositions", a)
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchOrders", a)
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // ts: base.ts:799 — BitgetExchange inherits fetchSymbolMaxLeverage, which rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
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
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&isolated), opt_js(symbol));
        self.invoke_unit("changeMarginType", a)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.invoke_unit("cancelSymbolOrders", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    // ts: base.ts:989 — BitgetExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — BitgetExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", Js::str(&symbol), callback)
    }
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        let a = args!(opt_js(options));
        self.invoke("fetchIncomeHistory", a)
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        let a = args!(js(&start), js(&end), opt_js(kind));
        self.invoke("fetchIncomeHistoryRange", a)
    }
    // ts: BitgetExchange.fetchHistoricalPositions folds closed round trips from fills.
    fn fetch_historical_positions(
        &self,
        options: HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        let opts = Js::object(vec![
            ("account", Js::str(&options.account)),
            ("from", options.from.map_or(Js::Undefined, Js::Number)),
            ("to", options.to.map_or(Js::Undefined, Js::Number)),
        ]);
        self.invoke("fetchHistoricalPositions", vec![opts])
    }
    // ts: BitgetExchange.cancelAllOrders / BitgetInverseExchange.cancelAllOrders
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
}
