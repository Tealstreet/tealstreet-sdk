//! Typed public binding to the actual COIN-M parent receiver.
use super::BinanceInverseExchange;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{
    Candle, Market, OHLCVOptions, Order, OrderBook, PlaceOrderOpts, Position, PositionSide,
    PublicTrade, Ticker, UpdateOrderOpts, ValidateAccountResult,
};

impl BinanceInverseExchange {
    pub(crate) fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let pending = self
            .scope
            .this
            .call_method(member, args)
            .map_err(|e| e.to_exchange_error());
        Box::pin(async move {
            let value = await_js(&self.scope.exchange_runtime(), pending?)
                .await
                .map_err(|e| e.to_exchange_error())?;
            to_typed(&value).map_err(|e| e.to_exchange_error())
        })
    }
}
fn fail(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}
impl Exchange for BinanceInverseExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.scope.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "BinanceInverseExchange"
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", vec![])
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        match from_typed(&options) {
            Ok(value) => self.invoke("fetchOHLCV", vec![value]),
            Err(e) => Box::pin(async move { Err(fail(e)) }),
        }
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        self.invoke(
            "fetchPublicTrades",
            vec![Js::str(&symbol), limit.map_or(Js::Undefined, Js::Number)],
        )
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.scope
            .this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(fail)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", vec![])
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: std::rc::Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", from_typed(&options).map_err(fail)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: std::rc::Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", Js::str(&symbol), callback)
    }
    fn place_order(
        &self,
        order: safe_cex_exchange::exchange::ValidatedOrder<Self>,
    ) -> ExchangeFuture<'_, Vec<String>> {
        match from_typed(order.options()) {
            Ok(value) => self.invoke("placeOrder", vec![value]),
            Err(e) => Box::pin(async move { Err(fail(e)) }),
        }
    }
    fn place_orders<'a>(
        &'a self,
        orders: safe_cex_exchange::exchange::ValidatedBatch<Self>,
    ) -> ExchangeFuture<'a, Vec<String>> {
        let orders: Vec<_> = orders
            .orders()
            .iter()
            .map(|order| order.options().clone())
            .collect();
        match from_typed(&orders) {
            Ok(value) => self.invoke("placeOrders", vec![value]),
            Err(e) => Box::pin(async move { Err(fail(e)) }),
        }
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        match from_typed(&options) {
            Ok(value) => self.invoke("updateOrder", vec![value]),
            Err(e) => Box::pin(async move { Err(fail(e)) }),
        }
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        match from_typed(&orders) {
            Ok(value) => self.invoke_unit("cancelOrders", vec![value]),
            Err(e) => Box::pin(async move { Err(fail(e)) }),
        }
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelSymbolOrders", vec![Js::str(&symbol)])
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("setLeverage", vec![Js::str(&symbol), Js::Number(leverage)])
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.invoke_unit(
            "changePositionMode",
            vec![
                Js::Bool(hedged),
                symbol.as_deref().map_or(Js::Undefined, Js::str),
            ],
        )
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.invoke_unit(
            "changeMarginType",
            vec![
                Js::Bool(isolated),
                symbol.as_deref().map_or(Js::Undefined, Js::str),
            ],
        )
    }
    fn after_method_called_in_worker(
        &self,
        result: &serde_json::Value,
        method: safe_cex_exchange::worker::CallableMethodsInWorker,
        args: &[serde_json::Value],
    ) -> Result<(), ExchangeError> {
        let mut params = vec![
            from_typed(result).map_err(fail)?,
            from_typed(&method).map_err(fail)?,
        ];
        for arg in args {
            params.push(from_typed(arg).map_err(fail)?);
        }
        self.scope
            .this
            .call_method("afterMethodCalledInWorker", params)
            .map_err(fail)?;
        Ok(())
    }
}

impl BinanceInverseExchange {
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let pending = self.scope.this.call_method(member, args).map_err(fail);
        Box::pin(async move {
            await_js(&self.scope.exchange_runtime(), pending?).await.map_err(fail)?;
            Ok(())
        })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &str,
        first: Js,
        callback: std::rc::Rc<dyn Fn(T)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let reporter = self.scope.shared.runtime.clone();
        let listener = Js::function("callback", move |_, args| {
            let value = safe_cex_exchange::js_runtime::value::arg(&args, 0);
            match to_typed::<T>(&value) {
                Ok(value) => callback(value),
                Err(error) => reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source":"binanceinverse listener payload"}).into(),
                ),
            }
            Ok(Js::Undefined)
        });
        let cleanup = self
            .scope
            .this
            .call_method(member, vec![first, listener])
            .map_err(fail)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map_err(fail)?;
            Ok(())
        }))
    }
}
pub(crate) fn capabilities() -> ExchangeCapabilities {
    use safe_cex_exchange::capabilities::{
        CapabilityBinding, PlaceOrderBinding, PlaceOrdersBinding,
    };
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BinanceInverseExchange>(
            Exchange::place_order,
        ),
        place_orders: PlaceOrdersBinding::implemented::<BinanceInverseExchange>(
            Exchange::place_orders,
        ),
        ..Default::default()
    }
}
