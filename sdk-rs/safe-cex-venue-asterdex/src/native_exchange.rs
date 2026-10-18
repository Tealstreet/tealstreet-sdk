//! Concrete AsterDEX exchange assembled from the ported bodies and shared runtime.
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "native_emitter_tests.rs"]
mod native_emitter_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "native_error_tests.rs"]
mod native_error_tests;
use crate::binding::{self, Rt};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        base_exchange::{BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        promise::await_js,
        value::{Js, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeName, ExchangeOptions, Execution, ExecutionsOptions, Market,
    OHLCVOptions, Order, OrderBook, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use std::rc::Rc;

/// A client host is selected using each primary/secondary transport-options object.
pub type AsterDEXHttpFactory =
    Rc<dyn Fn(&ExchangeOptions) -> Result<Rc<dyn JsHttpHost>, ExchangeError>>;
pub struct AsterDEXDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: AsterDEXHttpFactory,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}
pub struct AsterDEXExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}
impl Drop for AsterDEXExchange {
    fn drop(&mut self) {
        if !self.rt.base().is_disposed() {
            if let Err(error) = self.dispose() {
                self.rt.shared.runtime.report_exception(
                    &error,
                    &serde_json::json!({"source":"AsterDEXExchange.drop"}).into(),
                );
            }
        }
        for (_, task) in self.rt.shared.timers.borrow_mut().drain(..) {
            task.cancel();
        }
    }
}
impl AsterDEXExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: AsterDEXDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Asterdex {
            return Err(ExchangeError::new(
                "AsterDEXExchange requires the asterdex exchange name",
            ));
        }
        let external = deps.external_public_data.clone();
        let rt = binding::assemble(
            BaseExchangeParts {
                name,
                options,
                store,
                settings: ExchangeSettings {
                    supports_public_only: true,
                    positions_supported: Some(true),
                    ..Default::default()
                },
                external_public_data: external,
            },
            deps,
        )?;
        let venue = Rc::new(Self {
            rt,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|venue, ()| venue.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
                    venue.fetch_historical_fills(opts)
                }),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *venue.rt.shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        binding::initialize_sockets(&venue.rt, &erased);
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
            let result = await_js(&runtime, result?).await?;
            Ok(to_typed(&result)?)
        })
    }
    fn unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let runtime = self.rt.exchange_runtime();
        let result = self.rt.this.call_method(member, args);
        Box::pin(async move {
            await_js(&runtime, result?).await?;
            Ok(())
        })
    }
}
fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    Ok(from_typed(value)?)
}
fn optional<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |value| js(&value))
}
macro_rules! args { ($($value:expr),* $(,)?) => { match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($value?),*]) })() { Ok(args) => args, Err(error) => return Box::pin(async move { Err(error) }) } }; }
impl Exchange for AsterDEXExchange {
    type PlaceOpts = crate::types::AsterDEXPlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn can_sign(&self) -> bool {
        crate::exchange_helpers::can_sign(
            &self.base().options(),
            &self.rt.shared.extra.version,
            self.base().options().remote_signer.is_some()
                || self.base().signers.remote_signer.is_some(),
        )
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.unit("start", vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", args!(optional(symbols)))
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", args!(optional(symbols)))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
        self.invoke("fetchSymbolMaxLeverage", args!(js(&symbol)))
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.unit(
            "setLeverage",
            args!(js(&symbol), js(&leverage), optional(side)),
        )
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.unit("changePositionMode", args!(js(&hedged), optional(symbol)))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.unit("changeMarginType", args!(js(&isolated), optional(symbol)))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        if let Err(error) = self
            .base()
            .resolve_caller_client_order_id(&order.options().order)
        {
            return Box::pin(async move { Err(error) });
        }
        self.invoke("placeOrder", args!(js(order.options())))
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.invoke("updateOrder", args!(js(&opts)))
    }
    fn place_orders<'a>(&'a self, batch: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let opts: Vec<&crate::types::AsterDEXPlaceOrderOpts> =
            batch.orders().iter().map(|order| order.options()).collect();
        self.invoke("placeOrders", args!(js(&opts)))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.unit("cancelOrders", args!(js(&orders)))
    }
    // TS inherits base.ts scheduleCancel:989.
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        self.invoke("fetchPublicTrades", args!(js(&symbol), optional(limit)))
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", args!(js(&opts)))
    }
    fn fetch_historical_fills(
        &self,
        opts: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.invoke("fetchHistoricalFills", args!(js(&opts)))
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cb = Rc::new(move |c: crate::ws_helpers::PublicKlineCandle| {
            let timestamp = c
                .timestamp
                .as_ref()
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| {
                    ExchangeError::new(
                        "AsterDEX candle timestamp is outside the numeric Candle model",
                    )
                })?;
            callback(Candle {
                timestamp,
                open: c.open,
                high: c.high,
                low: c.low,
                close: c.close,
                volume: c.volume,
                last_side: None,
            });
            Ok(())
        });
        let clean = self
            .rt
            .shared
            .extra
            .listeners
            .listen_ohlcv(&opts.symbol, opts.interval, cb)?;
        Ok(Box::new(move || clean()))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let clean = self.rt.shared.extra.listeners.listen_order_book(
            &symbol,
            Rc::new(move |book| {
                callback(book);
                Ok(())
            }),
        )?;
        Ok(Box::new(move || clean()))
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt.this.call_method("dispose", vec![])?;
        Ok(())
    }
    fn after_method_called_in_worker(
        &self,
        _: &serde_json::Value,
        method: safe_cex_exchange::worker::CallableMethodsInWorker,
        args: &[serde_json::Value],
    ) -> Result<(), ExchangeError> {
        let method = serde_json::to_value(method).map_err(|e| ExchangeError::new(e.to_string()))?;
        crate::exchange_helpers::after_method_called_in_worker(
            method.as_str().unwrap_or_default(),
            args.first(),
            |key, value| Ok(self.base().store.set_setting(key, value)?),
        )
    }
}
