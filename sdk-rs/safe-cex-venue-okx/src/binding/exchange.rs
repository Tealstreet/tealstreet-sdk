//! Production OKX venue: the assembled OKX class graph behind the typed Exchange trait.
use super::{
    globals,
    members::Variant,
    ops::{OkxExtra, OkxScope, OkxShared, lifecycle_ops},
};
use crate::family_lifecycle;
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    client_order_id::ClientOrderLookup,
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    journal::HistoricalPositionOptions,
    js_runtime::{
        base_exchange::ExternalPublicDataFactory,
        client::JsHttpHost,
        promise::await_js,
        scope::{Scope, ScopeShared},
        store,
        value::{Js, Thrown, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
    worker::CallableMethodsInWorker,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct OkxDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

pub struct OkxExchange {
    scope: OkxScope,
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

impl OkxExchange {
    // ts: OKXExchange / OKXDemoExchange / OKXEUExchange / OKXEUDemoExchange constructors
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: OkxDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if !matches!(
            name,
            ExchangeName::Okx
                | ExchangeName::Okxdemo
                | ExchangeName::Okxeu
                | ExchangeName::Okxeudemo
        ) {
            return Err(ExchangeError::new(format!(
                "OKXExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<OkxShared>| {
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals::build(weak.clone()),
                OkxExtra {
                    name: name.clone(),
                    options: options.clone(),
                    external_public_data: deps.external_public_data.clone(),
                },
            )
        });
        let constructing = Scope {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        let h = lifecycle_ops(&constructing);
        let (js_name, js_options) = (js(&name)?, js(&options)?);
        let js_store = store::facade(store.clone())?;
        let receiver = match shared.extra.variant() {
            Variant::Global | Variant::Eu => {
                family_lifecycle::new_exchange(&h, js_name, js_options, js_store)
                    .map_err(err)?
                    .receiver
            }
            Variant::Demo => {
                family_lifecycle::new_demo_exchange(&h, js_name, js_options, js_store)
                    .map_err(err)?
                    .base
            }
            Variant::EuDemo => {
                family_lifecycle::new_eu_demo_exchange(&h, js_name, js_options, js_store)
                    .map_err(err)?
                    .base
                    .base
            }
        };
        *shared.exchange.borrow_mut() = receiver.clone();
        let venue = Rc::new(Self {
            scope: constructing.with_this(receiver),
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(venue)
    }

    fn call(&self, member: &str, args: Vec<Js>) -> Result<Js, ExchangeError> {
        self.scope.this.call_method(member, args).map_err(err)
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let pending = self.call(member, args);
        Box::pin(async move {
            let value = await_js(&self.scope.exchange_runtime(), pending?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }

    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let pending = self.call(member, args);
        Box::pin(async move {
            await_js(&self.scope.exchange_runtime(), pending?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }

    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let reporter = self.scope.shared.runtime.clone();
        let listener = Js::function("callback", move |_, a| {
            let value = a.into_iter().next().unwrap_or(Js::Undefined);
            match to_typed::<T>(&value) {
                Ok(typed) => callback(typed),
                Err(error) => reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "okx listener payload"}).into(),
                ),
            }
            Ok(Js::Undefined)
        });
        let cleanup = self.call(member, vec![first, listener])?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<OkxExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<OkxExchange>(Exchange::place_orders),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        fetch_income_history: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_income_history(opts)
        }),
        fetch_income_history_range: CapabilityBinding::implemented(|venue, (start, end, kind)| {
            venue.fetch_income_history_range(start, end, kind)
        }),
        fetch_closed_pnl: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_closed_pnl(opts)
        }),
        fetch_historical_orders: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_orders(opts)
        }),
        ..Default::default()
    }
}

impl Exchange for OkxExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.scope.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.call("dispose", vec![]).map(|_| ())
    }
    fn find_order_by_client_order_id(
        &self,
        symbol: String,
        id: String,
    ) -> ExchangeFuture<'_, ClientOrderLookup> {
        let pending = self.call(
            "findOrderByClientOrderId",
            vec![Js::str(&symbol), Js::str(&id)],
        );
        Box::pin(async move {
            // ts: ClientOrderLookup = { found: true, order } | { found: false, definitive }
            let lookup = await_js(&self.scope.exchange_runtime(), pending?)
                .await
                .map_err(err)?;
            if lookup.get("found").map_err(err)?.truthy() {
                let order: Order = to_typed(&lookup.get("order").map_err(err)?).map_err(err)?;
                return Ok(ClientOrderLookup::Found(Box::new(order)));
            }
            Ok(ClientOrderLookup::NotFound {
                definitive: lookup.get("definitive").map_err(err)?.truthy(),
            })
        })
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
        self.listen("listenOrderBook", js(&symbol)?, callback)
    }
    // ts: OKXExchange.placeOrder; BaseExchange.place_order validated the client order ID.
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
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
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
    fn fetch_closed_pnl(
        &self,
        options: HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        let a = args!(js(&options));
        self.invoke("fetchClosedPnl", a)
    }
    fn fetch_historical_orders(
        &self,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        let a = args!(opt_js(options));
        self.invoke("fetchHistoricalOrders", a)
    }
    fn after_method_called_in_worker(
        &self,
        result: &serde_json::Value,
        method: CallableMethodsInWorker,
        args: &[serde_json::Value],
    ) -> Result<(), ExchangeError> {
        let mut call = vec![js(result)?, js(&method)?];
        for arg in args {
            call.push(js(arg)?);
        }
        self.call("afterMethodCalledInWorker", call).map(|_| ())
    }
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
    // ts: base.ts:734 — OKXExchange inherits getAccount, which rejects.
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
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
    // ts: base.ts:799 — OKXExchange inherits fetchSymbolMaxLeverage, which rejects.
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
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.invoke_unit("cancelSymbolOrders", a)
    }
    // ts: base.ts:989 — OKXExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — OKXExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/support/binding_harness.rs"]
mod binding_harness;

#[cfg(test)]
#[path = "../../tests/support/callback_binding.rs"]
mod callback_binding;

#[cfg(test)]
#[path = "../../tests/support/index_lookup_binding.rs"]
mod index_lookup_binding;

#[cfg(test)]
#[path = "../../tests/support/amendment_binding.rs"]
mod amendment_binding;

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../tests/support/divergence_binding.rs"]
mod divergence_binding;
