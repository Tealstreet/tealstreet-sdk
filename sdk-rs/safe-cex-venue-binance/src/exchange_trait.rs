//! Typed SDK dispatch into the maintained concrete exchange method bodies.
use super::*;
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{Exchange, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeFuture, ListenerCleanup},
};
use safe_cex_types::*;

pub(super) fn capabilities<V: Exchange>(futures: bool) -> ExchangeCapabilities {
    let mut table = ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<V>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<V>(Exchange::place_orders),
        ..Default::default()
    };
    if futures {
        table.fetch_historical_fills =
            CapabilityBinding::implemented(|venue, opts| venue.fetch_historical_fills(opts));
        table.fetch_historical_orders =
            CapabilityBinding::implemented(|venue, opts| venue.fetch_historical_orders(opts));
        table.fetch_income_history =
            CapabilityBinding::implemented(|venue, opts| venue.fetch_income_history(opts));
        table.fetch_income_history_range =
            CapabilityBinding::implemented(|venue, (from, to, kind)| {
                venue.fetch_income_history_range(from, to, kind)
            });
    }
    table
}

async fn call<T: serde::de::DeserializeOwned>(
    owner: &Rc<ExchangeOwner>,
    method: Method,
    args: Result<Vec<EventValue>, Failure>,
) -> Result<T, ExchangeError> {
    let args = args.map_err(|e| (*e).clone())?;
    let value = class_jobs::reaction(owner.invoke(method, args))
        .await
        .map_err(|e| (*e).clone())?;
    typed(&value).map_err(|e| (*e).clone())
}
async fn income_call(
    owner: &Rc<ExchangeOwner>,
    method: Method,
    args: Result<Vec<EventValue>, Failure>,
) -> Result<Vec<IncomeRecord>, ExchangeError> {
    let args = args.map_err(|e| (*e).clone())?;
    let value = class_jobs::reaction(owner.invoke(method, args))
        .await
        .map_err(|e| (*e).clone())?;
    income_records(&value).map_err(|e| (*e).clone())
}
async fn unit(
    owner: &Rc<ExchangeOwner>,
    method: Method,
    args: Result<Vec<EventValue>, Failure>,
) -> Result<(), ExchangeError> {
    let args = args.map_err(|e| (*e).clone())?;
    class_jobs::reaction(owner.invoke(method, args))
        .await
        .map(|_| ())
        .map_err(|e| (*e).clone())
}
fn optional_argument<T: serde::Serialize>(value: Option<T>) -> Result<Vec<EventValue>, Failure> {
    Ok(vec![
        value
            .map(decoded)
            .transpose()?
            .unwrap_or(EventValue::Undefined),
    ])
}
fn argument(value: impl serde::Serialize) -> Result<Vec<EventValue>, Failure> {
    Ok(vec![decoded(value)?])
}
fn cleanup(
    result: Result<crate::public_listeners::Unsubscribe, Failure>,
) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
    let cleanup = result.map_err(|error| (*error).clone())?;
    Ok(Box::new(move || {
        cleanup().map_err(|error| (*error).clone())
    }))
}

/// TS `BaseExchange` default these classes inherit: `await Promise.reject(new Error('Not implemented'))`
/// (base.ts getAccount L734, fetchSymbolMaxLeverage L799, setLeverage L808, changePositionMode L832,
/// changeMarginType L836, scheduleCancel L989, fetchPublicTrades L1138). No request, no emit.
fn base_exchange_rejection<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

impl Exchange for BinanceExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.0.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.0.state.dispose().map_err(|e| (*e).clone())
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Start, Ok(vec![])))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(call(&self.0, Method::ValidateAccount, Ok(vec![])))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(call(&self.0, Method::Markets, Ok(vec![])))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        Box::pin(call(&self.0, Method::Tickers, Ok(vec![])))
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(call(&self.0, Method::Positions, Ok(vec![])))
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(call(&self.0, Method::Orders, Ok(vec![])))
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(call(&self.0, Method::OHLCV, argument(opts)))
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::UpdateOrder, argument(opts)))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Cancel, argument(orders)))
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::CancelSymbol,
            Ok(vec![string(symbol)]),
        ))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::PlaceOrder, argument(order.options())))
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(
            &self.0,
            Method::PlaceOrders,
            argument(
                orders
                    .orders()
                    .iter()
                    .map(|order| order.options())
                    .collect::<Vec<_>>(),
            ),
        ))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        cleanup(self.0.state.listen_order_book(
            symbol,
            Rc::new(move |book| {
                callback(book.borrow().clone());
                Ok(())
            }),
        ))
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let result = decoded(opts).and_then(|opts| {
            self.0.state.listen_ohlcv(
                opts,
                Rc::new(move |candle| {
                    callback(typed(&candle)?);
                    Ok(())
                }),
            )
        });
        cleanup(result)
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(call(&self.0, Method::GetAccount, Ok(vec![])))
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::Leverage,
            Ok(vec![string(symbol), number(leverage)]),
        ))
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::ChangePositionMode,
            Ok(vec![
                EventValue::Bool(hedged),
                symbol.map_or(EventValue::Undefined, string),
            ]),
        ))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::ChangeMargin,
            Ok(vec![
                EventValue::Bool(isolated),
                symbol.map_or(EventValue::Undefined, string),
            ]),
        ))
    }
    fn find_order_by_client_order_id(
        &self,
        symbol: String,
        id: String,
    ) -> ExchangeFuture<'_, safe_cex_exchange::client_order_id::ClientOrderLookup> {
        Box::pin(async move {
            let value = class_jobs::reaction(
                self.0
                    .invoke(Method::FindOrder, vec![string(symbol), string(id)]),
            )
            .await
            .map_err(|e| (*e).clone())?;
            if value.read("found").map_err(|e| (*e).clone())?.truthy() {
                let order = value.read("order").map_err(|e| (*e).clone())?;
                Ok(
                    safe_cex_exchange::client_order_id::ClientOrderLookup::Found(Box::new(
                        typed(&order).map_err(|e| (*e).clone())?,
                    )),
                )
            } else {
                Ok(
                    safe_cex_exchange::client_order_id::ClientOrderLookup::NotFound {
                        definitive: value.read("definitive").map_err(|e| (*e).clone())?.truthy(),
                    },
                )
            }
        })
    }
    fn fetch_historical_fills(
        &self,
        opts: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(call(&self.0, Method::HistoricalFills, argument(opts)))
    }
    fn fetch_historical_orders(
        &self,
        opts: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        Box::pin(call(
            &self.0,
            Method::HistoricalOrders,
            optional_argument(opts),
        ))
    }
    fn fetch_income_history(
        &self,
        opts: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        Box::pin(income_call(
            &self.0,
            Method::IncomeHistory,
            optional_argument(opts),
        ))
    }
    fn fetch_income_history_range(
        &self,
        from: f64,
        to: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        Box::pin(income_call(
            &self.0,
            Method::IncomeRange,
            Ok(vec![
                number(from),
                number(to),
                kind.map_or(EventValue::Undefined, string),
            ]),
        ))
    }
    fn fetch_historical_positions(
        &self,
        opts: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<Value>> {
        Box::pin(call(
            &self.0,
            Method::HistoricalPositions,
            Ok(vec![record([
                ("account", string(opts.account)),
                ("from", opts.from.map_or(EventValue::Undefined, number)),
                ("to", opts.to.map_or(EventValue::Undefined, number)),
            ])]),
        ))
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(call(
            &self.0,
            Method::PublicTrades,
            Ok(vec![
                string(symbol),
                limit.map_or(EventValue::Undefined, number),
            ]),
        ))
    }
    fn after_method_called_in_worker(
        &self,
        _result: &Value,
        method: safe_cex_exchange::worker::CallableMethodsInWorker,
        args: &[Value],
    ) -> Result<(), ExchangeError> {
        let method =
            serde_json::to_value(method).expect("worker method has source string spelling");
        self.0
            .state
            .after_method_called_in_worker(method.as_str().expect("worker method string"), args)
            .map_err(|e| (*e).clone())
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection()
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
}

impl Exchange for BinancePMExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.0.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.0.state.dispose().map_err(|e| (*e).clone())
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Start, Ok(vec![])))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(call(&self.0, Method::ValidateAccount, Ok(vec![])))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(call(&self.0, Method::Markets, Ok(vec![])))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        Box::pin(call(&self.0, Method::Tickers, Ok(vec![])))
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(call(&self.0, Method::Positions, Ok(vec![])))
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(call(&self.0, Method::Orders, Ok(vec![])))
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(call(&self.0, Method::OHLCV, argument(opts)))
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::UpdateOrder, argument(opts)))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Cancel, argument(orders)))
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::CancelSymbol,
            Ok(vec![string(symbol)]),
        ))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::PlaceOrder, argument(order.options())))
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(
            &self.0,
            Method::PlaceOrders,
            argument(
                orders
                    .orders()
                    .iter()
                    .map(|order| order.options())
                    .collect::<Vec<_>>(),
            ),
        ))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        cleanup(self.0.state.listen_order_book(
            symbol,
            Rc::new(move |book| {
                callback(book.borrow().clone());
                Ok(())
            }),
        ))
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let result = decoded(opts).and_then(|opts| {
            self.0.state.listen_ohlcv(
                opts,
                Rc::new(move |candle| {
                    callback(typed(&candle)?);
                    Ok(())
                }),
            )
        });
        cleanup(result)
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(call(&self.0, Method::GetAccount, Ok(vec![])))
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::Leverage,
            Ok(vec![string(symbol), number(leverage)]),
        ))
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::ChangePositionMode,
            Ok(vec![
                EventValue::Bool(hedged),
                symbol.map_or(EventValue::Undefined, string),
            ]),
        ))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::ChangeMargin,
            Ok(vec![
                EventValue::Bool(isolated),
                symbol.map_or(EventValue::Undefined, string),
            ]),
        ))
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection()
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        base_exchange_rejection()
    }
}

impl Exchange for BinanceSpotExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.0.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.0.state.dispose().map_err(|e| (*e).clone())
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Start, Ok(vec![])))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(call(&self.0, Method::ValidateAccount, Ok(vec![])))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(call(&self.0, Method::Markets, Ok(vec![])))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        Box::pin(call(&self.0, Method::Tickers, Ok(vec![])))
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(call(&self.0, Method::Positions, Ok(vec![])))
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(call(&self.0, Method::Orders, Ok(vec![])))
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(call(&self.0, Method::OHLCV, argument(opts)))
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::UpdateOrder, argument(opts)))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(&self.0, Method::Cancel, argument(orders)))
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(unit(
            &self.0,
            Method::CancelSymbol,
            Ok(vec![string(symbol)]),
        ))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(&self.0, Method::PlaceOrder, argument(order.options())))
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(call(
            &self.0,
            Method::PlaceOrders,
            argument(
                orders
                    .orders()
                    .iter()
                    .map(|order| order.options())
                    .collect::<Vec<_>>(),
            ),
        ))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        cleanup(self.0.state.listen_order_book(
            symbol,
            Rc::new(move |book| {
                callback(book.borrow().clone());
                Ok(())
            }),
        ))
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let result = decoded(opts).and_then(|opts| {
            self.0.state.listen_ohlcv(
                opts,
                Rc::new(move |candle| {
                    callback(typed(&candle)?);
                    Ok(())
                }),
            )
        });
        cleanup(result)
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        base_exchange_rejection()
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        base_exchange_rejection()
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        base_exchange_rejection()
    }
    fn set_leverage(
        &self,
        _symbol: String,
        _leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        base_exchange_rejection()
    }
}
