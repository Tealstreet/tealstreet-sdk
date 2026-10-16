//! SDK dispatch reaches the same owned exchange and socket method bodies.
use crate::{
    adapter::BackpackExchange, model_bridge as bridge, order_actions as actions,
    rest_reads as reads,
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::*;
use std::rc::Rc;
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn strings(v: Vec<Vec<u16>>) -> Result<Vec<String>, ExchangeError> {
    v.into_iter()
        .map(|s| String::from_utf16(&s).map_err(|e| ExchangeError::new(e.to_string())))
        .collect()
}
impl BackpackExchange {
    pub fn capabilities_table() -> ExchangeCapabilities {
        ExchangeCapabilities {
            start: CapabilityBinding::implemented(|v, ()| v.start()),
            place_order: PlaceOrderBinding::implemented::<Self>(|v, o| Exchange::place_order(v, o)),
            place_orders: PlaceOrdersBinding::implemented::<Self>(|v, o| {
                Exchange::place_orders(v, o)
            }),
            fetch_historical_fills: CapabilityBinding::implemented(|v, o| {
                v.fetch_historical_fills(o)
            }),
            ..Default::default()
        }
    }
    pub fn owner(&self) -> Rc<Self> {
        self.self_ref.upgrade().expect("Owned Backpack instance")
    }
}
impl Exchange for BackpackExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        BackpackExchange::start(&self.owner())
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        BackpackExchange::dispose(self)
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        let f = BackpackExchange::get_account(&self.owner());
        Box::pin(async move {
            let a = f.await?;
            Ok(ExchangeAccount {
                user_id: crate::native_value::Value::from_prepared(a.user_id).utf8()?,
                sub_id: None,
                affiliate_id: Some(None),
            })
        })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        let f = BackpackExchange::validate_account(&self.owner());
        Box::pin(async move {
            let a = f.await?;
            Ok(ValidateAccountResult {
                error: serde_json::from_value(crate::adapter::prepared_json(
                    &crate::native_value::Value::from_prepared(a.error),
                )?)
                .map_err(|e| ExchangeError::new(e.to_string()))?,
                balance: a.balance.flatten(),
            })
        })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        let f = BackpackExchange::fetch_markets(&self.owner());
        Box::pin(async move { f.await?.into_iter().map(bridge::market).collect() })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        let f = BackpackExchange::fetch_tickers(&self.owner());
        Box::pin(async move { f.await?.into_iter().flatten().map(bridge::ticker).collect() })
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let f = BackpackExchange::fetch_positions(&self.owner());
        Box::pin(async move { f.await?.into_iter().map(bridge::position).collect() })
    }
    fn fetch_orders(&self, s: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let f = BackpackExchange::fetch_orders(
            &self.owner(),
            s.map(|v| v.into_iter().map(|s| u(&s)).collect()),
        );
        Box::pin(async move { Ok(f.await?.into_iter().flatten().collect()) })
    }
    fn fetch_ohlcv(&self, o: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let f = BackpackExchange::fetch_ohlcv(
            &self.owner(),
            reads::OHLCVOptions {
                symbol: u(&o.symbol),
                interval: u(&serde_json::to_value(o.interval)
                    .expect("timeframe")
                    .as_str()
                    .expect("timeframe string")),
                from: o.from,
                to: o.to,
            },
        );
        Box::pin(async move { Ok(f.await?.into_iter().flatten().map(bridge::candle).collect()) })
    }
    fn fetch_historical_fills(&self, o: ExecutionsOptions) -> ExchangeFuture<'_, Vec<Execution>> {
        let f = BackpackExchange::fetch_historical_fills(
            &self.owner(),
            reads::ExecutionsOptions {
                symbol: o.symbol.map(|s| u(&s)),
                order_id: o.order_id.map(|s| u(&s)),
                from: o.from,
                to: o.to,
                limit: o.limit,
            },
        );
        Box::pin(async move {
            f.await?
                .into_iter()
                .map(|e| {
                    Ok(Execution {
                        id: Some(
                            String::from_utf16(&e.id)
                                .map_err(|x| ExchangeError::new(x.to_string()))?,
                        ),
                        timestamp: e.timestamp,
                        symbol: String::from_utf16(&e.symbol)
                            .map_err(|x| ExchangeError::new(x.to_string()))?,
                        order_id: Some(
                            String::from_utf16(&e.order_id)
                                .map_err(|x| ExchangeError::new(x.to_string()))?,
                        ),
                        price: e.price,
                        qty: e.qty,
                        side: e.side,
                        commission: e.commission,
                        commission_asset: e
                            .commission_asset
                            .map(|s| {
                                String::from_utf16(&s)
                                    .map_err(|x| ExchangeError::new(x.to_string()))
                            })
                            .transpose()?,
                        position_side: None,
                        realized_pnl: None,
                        maker: None,
                        venue_reported_revenue: None,
                        venue_reported_revenue_asset: None,
                    })
                })
                .collect()
        })
    }
    fn place_order(&self, o: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let f = BackpackExchange::place_order(&self.owner(), bridge::action(o.options().clone()));
        Box::pin(async move { strings(f.await?) })
    }
    fn place_orders(&self, o: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        BackpackExchange::place_orders(
            &self.owner(),
            o.orders().iter().map(|o| o.options().clone()).collect(),
        )
    }
    fn cancel_orders(&self, o: Vec<Order>) -> ExchangeFuture<'_, ()> {
        BackpackExchange::cancel_orders(&self.owner(), o)
    }
    fn cancel_symbol_orders(&self, s: String) -> ExchangeFuture<'_, ()> {
        BackpackExchange::cancel_symbol_orders(&self.owner(), s)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        BackpackExchange::cancel_all_orders(&self.owner())
    }
    fn set_leverage(&self, _: String, n: f64, _: Option<PositionSide>) -> ExchangeFuture<'_, ()> {
        BackpackExchange::set_leverage(&self.owner(), n)
    }
    fn change_position_mode(&self, h: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        BackpackExchange::change_position_mode(self, h)
    }
    fn change_margin_type(&self, i: bool, s: Option<String>) -> ExchangeFuture<'_, ()> {
        BackpackExchange::change_margin_type(self, i, s.as_deref())
    }
    fn update_order(&self, o: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let (amount, price) = match o.update {
            UpdateOrderOptsUpdate::Variant0(v) => (Some(Some(v.amount)), v.price.map(Some)),
            UpdateOrderOptsUpdate::Variant1(v) => (v.amount.map(Some), Some(Some(v.price))),
        };
        let f = BackpackExchange::update_order(
            &self.owner(),
            actions::Update {
                id: u(&o.order.id),
                amount,
                price,
            },
        );
        Box::pin(async move { strings(f.await?) })
    }
    // TS BackpackExchange inherits BaseExchange.fetchSymbolMaxLeverage (base.ts:799): rejects 'Not implemented'.
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // TS BackpackExchange inherits BaseExchange.scheduleCancel (base.ts:989): rejects 'Not implemented'.
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // TS BackpackExchange inherits BaseExchange.fetchPublicTrades (base.ts:1138): rejects 'Not implemented'.
    fn fetch_public_trades(
        &self,
        _: String,
        _: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn listen_ohlcv(
        &self,
        o: OHLCVOptions,
        cb: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let result = BackpackExchange::listen_ohlcv(
            &self.owner(),
            reads::OHLCVOptions {
                symbol: u(&o.symbol),
                interval: u(serde_json::to_value(o.interval)
                    .expect("timeframe")
                    .as_str()
                    .unwrap()),
                from: o.from,
                to: o.to,
            },
            Rc::new(move |c| {
                cb(bridge::candle(c));
                Ok(())
            }),
        );
        let unsubscribe = result?;
        Ok(Box::new(move || unsubscribe()))
    }
    fn listen_order_book(
        &self,
        s: String,
        cb: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let result = BackpackExchange::listen_order_book(
            &self.owner(),
            u(&s),
            Rc::new(move |book| {
                let snapshot = book.borrow().clone();
                cb(snapshot);
                Ok(())
            }),
        );
        let unsubscribe = result?;
        Ok(Box::new(move || unsubscribe()))
    }
}
