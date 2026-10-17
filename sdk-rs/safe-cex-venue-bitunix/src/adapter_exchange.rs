//! SDK dispatch reaches the ported Bitunix method bodies through the concrete venue.
use crate::adapter::BitunixVenue;
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
};
use safe_cex_types::*;
use std::rc::Rc;

impl BitunixVenue {
    pub(crate) fn capabilities_table() -> ExchangeCapabilities {
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
impl Exchange for BitunixVenue {
    type PlaceOpts = PlaceOrderOpts;
    fn validate_order_capabilities(&self, options: &Self::PlaceOpts) -> Result<(), ExchangeError> {
        self.validate_placement_input(options)
    }
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.dispose_venue()
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        BitunixVenue::start(&self.owner())
    }
    // TS BitunixExchange inherits BaseExchange.getAccount (base.ts:734): rejects 'Not implemented'.
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        BitunixVenue::validate_account(&self.owner())
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        BitunixVenue::fetch_markets(&self.owner())
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        BitunixVenue::fetch_tickers(&self.owner())
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        BitunixVenue::fetch_positions(&self.owner())
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        BitunixVenue::fetch_orders(&self.owner())
    }
    fn place_order(&self, o: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        BitunixVenue::place_order(&self.owner(), o.options().clone())
    }
    fn place_orders(&self, o: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        BitunixVenue::place_orders(
            &self.owner(),
            o.orders().iter().map(|o| o.options().clone()).collect(),
        )
    }
    fn update_order(&self, o: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        BitunixVenue::update_order(&self.owner(), o)
    }
    fn cancel_orders(&self, o: Vec<Order>) -> ExchangeFuture<'_, ()> {
        BitunixVenue::cancel_orders(&self.owner(), o)
    }
    fn cancel_symbol_orders(&self, s: String) -> ExchangeFuture<'_, ()> {
        BitunixVenue::cancel_symbol_orders(&self.owner(), s)
    }
    fn set_leverage(&self, s: String, n: f64, _: Option<PositionSide>) -> ExchangeFuture<'_, ()> {
        BitunixVenue::set_leverage(&self.owner(), s, n)
    }
    fn change_position_mode(&self, h: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        BitunixVenue::change_position_mode(&self.owner(), h)
    }
    fn change_margin_type(&self, i: bool, s: Option<String>) -> ExchangeFuture<'_, ()> {
        BitunixVenue::change_margin_type(&self.owner(), i, s)
    }
    // TS BitunixExchange inherits BaseExchange.fetchSymbolMaxLeverage (base.ts:799): rejects 'Not implemented'.
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // TS BitunixExchange inherits BaseExchange.scheduleCancel (base.ts:989): rejects 'Not implemented'.
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // TS BitunixExchange inherits BaseExchange.fetchPublicTrades (base.ts:1138): rejects 'Not implemented'.
    fn fetch_public_trades(
        &self,
        _: String,
        _: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_ohlcv(&self, o: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        BitunixVenue::fetch_ohlcv(&self.owner(), o)
    }
    fn fetch_historical_fills(&self, o: ExecutionsOptions) -> ExchangeFuture<'_, Vec<Execution>> {
        BitunixVenue::fetch_historical_fills(&self.owner(), o)
    }
    fn fetch_income_history(
        &self,
        o: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        BitunixVenue::fetch_income_history(&self.owner(), o)
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        BitunixVenue::fetch_income_history_range(&self.owner(), start, end, kind)
    }
    fn fetch_closed_pnl(
        &self,
        o: HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        BitunixVenue::fetch_closed_pnl(&self.owner(), o)
    }
    fn fetch_historical_orders(
        &self,
        o: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        BitunixVenue::fetch_historical_orders(&self.owner(), o)
    }
    fn fetch_historical_positions(
        &self,
        o: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        BitunixVenue::fetch_historical_positions(&self.owner(), o)
    }
    fn update_symbol_metadata(&self, s: String) -> ExchangeFuture<'_, ()> {
        BitunixVenue::update_symbol_metadata(&self.owner(), s)
    }
    fn listen_ohlcv(
        &self,
        o: OHLCVOptions,
        cb: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        BitunixVenue::listen_ohlcv(&self.owner(), o, cb)
    }
    fn listen_order_book(
        &self,
        s: String,
        cb: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let me = self.owner();
        crate::exchange_lifecycle::listen_order_book(s, cb, |s, cb| {
            me.socket("futures").listen_order_book(s, cb)
        })
    }
}
impl BitunixVenue {
    fn listen_ohlcv(
        self: &Rc<Self>,
        o: OHLCVOptions,
        cb: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let me = self.clone();
        let interval = crate::adapter::to_value(&o.interval)?
            .as_str()
            .ok_or_else(|| ExchangeError::new("Timeframe must serialize as a string"))?
            .to_owned();
        crate::session_control::listen_ohlcv(
            o,
            cb,
            |o| Ok(crate::adapter::units(&o.symbol)),
            |symbol| {
                Ok(me
                    .state
                    .borrow()
                    .symbol_settings
                    .contains_key(&crate::adapter::utf8(&symbol)?))
            },
            |symbol| {
                // TS calls updateSymbolMetadata without awaiting it; its rejection is unhandled.
                let task = me.update_symbol_metadata(crate::adapter::utf8(&symbol)?);
                me.spawn("Bitunix updateSymbolMetadata", task);
                Ok(())
            },
            |o, cb| me.socket("futures").listen_ohlcv(o.symbol, interval, cb),
            |o| Ok(crate::adapter::units(&o.symbol)),
        )
    }
}
