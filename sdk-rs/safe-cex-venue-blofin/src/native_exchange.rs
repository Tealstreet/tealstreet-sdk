//! SDK dispatch for BloFin: the trait reaches the same owned BlofinExchange method bodies.
use crate::{
    exchange::BlofinExchange,
    native_host::{NativeHost, coercion_error, public_error, service_error},
    position_reads::HistoricalPositionsOptions,
    utility::UtilityInput,
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    journal::HistoricalPositionOptions,
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
    worker::CallableMethodsInWorker,
};
use safe_cex_transport::RestAdapter;
use safe_cex_types::*;
use serde_json::Value;
use std::rc::Rc;

/// Placement IDs: strings pass through, other present values use JS String(); absent IDs are dropped.
fn ids(rows: Vec<Option<Value>>) -> Result<Vec<String>, ExchangeError> {
    rows.into_iter()
        .flatten()
        .filter(|v| !v.is_null())
        .map(|v| match v {
            Value::String(s) => Ok(s),
            other => js_coercion::js_string(Some(&other)).map_err(coercion_error),
        })
        .collect()
}

// ts: BlofinExchange
pub struct BlofinNativeExchange {
    inner: Rc<BlofinExchange<NativeHost>>,
    capabilities: ExchangeCapabilities,
}
impl BlofinNativeExchange {
    /// `http` is the common safeCexAxiosCreate stack: (options, baseURL, timeout budget) -> adapter.
    pub fn new(
        base: BaseExchange,
        http: impl Fn(&ExchangeOptions, &str, i64) -> Result<Rc<dyn RestAdapter>, String> + 'static,
        runtime: Rc<dyn WsRuntime>,
        external_public_data: safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory,
        remote_signer_account_name: Option<String>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let name = base.name.clone();
        let options = base.options();
        let store = base.store.clone();
        let host = Rc::new(NativeHost::new(
            base,
            Rc::new(http),
            runtime,
            external_public_data,
            remote_signer_account_name,
        ));
        let inner = BlofinExchange::new(host.clone(), name, options, store)?;
        let venue = Rc::new(Self {
            inner,
            capabilities: Self::capabilities_table(),
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        if let Some(parent) = host.parent.get() {
            *parent.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        }
        Ok(venue)
    }
    fn capabilities_table() -> ExchangeCapabilities {
        ExchangeCapabilities {
            start: CapabilityBinding::implemented(|v, ()| v.start()),
            place_order: PlaceOrderBinding::implemented::<Self>(|v, o| Exchange::place_order(v, o)),
            place_orders: PlaceOrdersBinding::implemented::<Self>(|v, o| {
                Exchange::place_orders(v, o)
            }),
            fetch_historical_fills: CapabilityBinding::implemented(|v, o| {
                v.fetch_historical_fills(o)
            }),
            fetch_income_history: CapabilityBinding::implemented(|v, o| v.fetch_income_history(o)),
            fetch_income_history_range: CapabilityBinding::implemented(|v, (s, e, k)| {
                v.fetch_income_history_range(s, e, k)
            }),
            fetch_closed_pnl: CapabilityBinding::implemented(|v, o| v.fetch_closed_pnl(o)),
            fetch_historical_orders: CapabilityBinding::implemented(|v, o| {
                v.fetch_historical_orders(o)
            }),
            ..Default::default()
        }
    }
}
impl Exchange for BlofinNativeExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.inner.base.get()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "BlofinExchange"
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.inner.dispose()
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.start().await.map_err(service_error) })
    }
    // TS BlofinExchange inherits BaseExchange.getAccount (base.ts:734): rejects 'Not implemented'.
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(async move {
            let result = self.inner.validate_account().await.map_err(service_error)?;
            Ok(ValidateAccountResult {
                error: AccountValidationError::from_value(result.error),
                balance: result.balance,
            })
        })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(async move { self.inner.fetch_markets().await.map_err(service_error) })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        Box::pin(async move { self.inner.fetch_tickers().await.map_err(service_error) })
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async move {
            self.inner
                .fetch_positions(symbols.as_deref())
                .await
                .map_err(service_error)
        })
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async move {
            self.inner
                .fetch_orders(symbols.as_deref())
                .await
                .map_err(service_error)
        })
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        Box::pin(async move {
            self.inner
                .fetch_leverage_for_symbol(&symbol)
                .await
                .map_err(service_error)
        })
    }
    // TS BlofinExchange inherits BaseExchange.fetchSymbolMaxLeverage (base.ts:799): rejects 'Not implemented'.
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .set_leverage(&symbol, leverage)
                .await
                .map_err(service_error)
        })
    }
    fn change_position_mode(&self, hedged: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .change_position_mode(hedged)
                .await
                .map_err(service_error)
        })
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .change_margin_type(isolated, symbol.as_deref())
                .await
                .map_err(service_error)
        })
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move {
            let update = serde_json::to_value(&options.update)
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            ids(self
                .inner
                .update_order(&options.order, &UtilityInput::from_retained(update))
                .await
                .map_err(service_error)?)
        })
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .cancel_orders(&orders)
                .await
                .map_err(service_error)
        })
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .cancel_symbol_orders(&symbol)
                .await
                .map_err(service_error)
        })
    }
    // TS BlofinExchange inherits BaseExchange.scheduleCancel (base.ts:989): rejects 'Not implemented'.
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // TS BlofinExchange inherits BaseExchange.fetchPublicTrades (base.ts:1138): rejects 'Not implemented'.
    fn fetch_public_trades(
        &self,
        _: String,
        _: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        // order_rest::place_order opens with resolveCallerClientOrderId, as TS placeOrder does.
        Box::pin(async move {
            ids(self
                .inner
                .place_order(&safe_cex_types::PreparedOrderOpts::from(order.options()))
                .await
                .map_err(service_error)?)
        })
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        // order_rest::place_orders opens with assertCallerClientOrderIds, as TS placeOrders does.
        Box::pin(async move {
            let opts: Vec<safe_cex_types::PreparedOrderOpts> =
                orders.orders().iter().map(|o| o.options().into()).collect();
            ids(self
                .inner
                .place_orders(&opts)
                .await
                .map_err(service_error)?)
        })
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(async move {
            self.inner
                .fetch_ohlcv(&options)
                .await
                .map_err(service_error)
        })
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async move {
            self.inner
                .fetch_historical_fills(&options)
                .await
                .map_err(service_error)
        })
    }
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        Box::pin(async move {
            self.inner
                .fetch_income_history(options.as_ref())
                .await
                .map_err(service_error)
        })
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        Box::pin(async move {
            self.inner
                .fetch_income_history_range(start, end, kind.as_deref())
                .await
                .map_err(service_error)
        })
    }
    fn fetch_closed_pnl(
        &self,
        options: HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        Box::pin(async move {
            self.inner
                .fetch_closed_pnl(&options)
                .await
                .map_err(service_error)
        })
    }
    fn fetch_historical_orders(
        &self,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        Box::pin(async move {
            self.inner
                .fetch_historical_orders(options.as_ref())
                .await
                .map_err(service_error)
        })
    }
    fn fetch_historical_positions(
        &self,
        options: HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<Value>> {
        Box::pin(async move {
            let records = self
                .inner
                .fetch_historical_positions(&HistoricalPositionsOptions {
                    account: options.account,
                    from: options.from,
                    to: options.to,
                })
                .await
                .map_err(service_error)?;
            records
                .iter()
                .map(|r| serde_json::to_value(r).map_err(|e| ExchangeError::new(e.to_string())))
                .collect()
        })
    }
    fn after_method_called_in_worker(
        &self,
        result: &Value,
        method: CallableMethodsInWorker,
        args: &[Value],
    ) -> Result<(), ExchangeError> {
        let method = serde_json::to_value(method).map_err(|e| ExchangeError::new(e.to_string()))?;
        let result = UtilityInput::from_retained(result.clone());
        let first = args.first().cloned().map(UtilityInput::from_retained);
        self.inner
            .after_method_called_in_worker(
                Some(&result),
                method.as_str().unwrap_or_default(),
                first.as_ref(),
            )
            .map_err(ExchangeError::from)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self
            .inner
            .listen_ohlcv(
                options,
                Rc::new(move |candle| {
                    callback(candle.borrow().clone());
                    Ok(())
                }),
            )
            .map_err(public_error)?;
        Ok(Box::new(move || cleanup().map_err(public_error)))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self
            .inner
            .listen_order_book(
                symbol,
                Rc::new(move |book| {
                    callback(book.borrow().clone());
                    Ok(())
                }),
            )
            .map_err(public_error)?;
        Ok(Box::new(move || cleanup().map_err(public_error)))
    }
}
