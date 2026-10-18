//! Production Exchange dispatch over the shared JavaScript value/promise host.
//! Class methods compose the venue's existing source ports over the same core.
use crate::{
    lifecycle::BreakoutpropExchange,
    native_reads::NativeReads,
    runtime::{BreakoutRuntime, OrderUpdate, RuntimeReads},
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedOrder},
    js_runtime::{
        promise::{Promise, await_js},
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
};
use safe_cex_types::{
    Candle, ExchangeAccount, Market, OHLCVOptions, Order, OrderBook, PlaceOrderOpts, Position,
    PositionSide, PublicTrade, Ticker, UpdateOrderOpts, ValidateAccountResult,
};
use std::rc::Rc;

pub(crate) fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BreakoutpropExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<BreakoutpropExchange>(
            Exchange::place_orders,
        ),
        ..Default::default()
    }
}

fn wire<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(Into::into)
}
fn decoded<T: serde::de::DeserializeOwned>(value: &Js) -> Result<T, ExchangeError> {
    to_typed(value).map_err(Into::into)
}
fn report(exchange: &BreakoutpropExchange, message: &str) -> Result<(), ExchangeError> {
    exchange.base.emit_and_log_error(
        safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.into()),
        None,
        None,
    )
}

pub(crate) fn instance(exchange: &Rc<BreakoutpropExchange>) -> Result<Js, ExchangeError> {
    let object = Js::object_with_class("BreakoutpropExchange", vec![]);
    for name in [
        "start",
        "getAccount",
        "validateAccount",
        "fetchPositions",
        "fetchOrders",
        "fetchMarkets",
        "fetchTickers",
        "fetchOHLCV",
        "fetchSymbolMaxLeverage",
        "setLeverage",
        "changePositionMode",
        "changeMarginType",
        "placeOrder",
        "updateOrder",
        "cancelOrders",
        "scheduleCancel",
        "fetchPublicTrades",
    ] {
        let weak = Rc::downgrade(exchange);
        object.define(
            name,
            Js::function(name, move |_, args| {
                let exchange = weak
                    .upgrade()
                    .ok_or_else(|| Thrown::error("Error", "Breakout Prop exchange was dropped"))?;
                let runtime = exchange.base.runtime.clone();
                let pending = native_call(exchange, name, args);
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move { pending.await.map_err(Into::into) }),
                )))
            }),
        )?;
    }
    Ok(object)
}

fn native_call(
    exchange: Rc<BreakoutpropExchange>,
    name: &'static str,
    args: Vec<Js>,
) -> ExchangeFuture<'static, Js> {
    // The unsupported members are inherited unchanged from TS BaseExchange:
    // getAccount, fetchSymbolMaxLeverage, setLeverage, changePositionMode,
    // changeMarginType, scheduleCancel and fetchPublicTrades.
    if matches!(
        name,
        "getAccount"
            | "fetchSymbolMaxLeverage"
            | "setLeverage"
            | "changePositionMode"
            | "changeMarginType"
            | "scheduleCancel"
            | "fetchPublicTrades"
    ) {
        return Box::pin(async { Err(ExchangeError::new("Not implemented")) });
    }
    let reads = Rc::new(NativeReads {
        exchange: exchange.clone(),
        host: exchange.host.runtime_host(),
    });
    let ctx = Rc::new(BreakoutRuntime {
        exchange: exchange.clone(),
        reads: reads.clone(),
    });
    match name {
        "start" => {
            let pending = ctx.start();
            Box::pin(async move {
                pending.await?;
                Ok(Js::Undefined)
            })
        }
        "validateAccount" => {
            let pending = exchange.get_session_token();
            Box::pin(async move {
                wire(
                    &crate::account_auth::validate_account(|| pending, &|v| {
                        report(
                            &exchange,
                            &safe_cex_types::js_coercion::js_string(Some(&v))
                                .map_err(|e| ExchangeError::new(e.to_string()))?,
                        )
                    })
                    .await?,
                )
            })
        }
        "fetchMarkets" => {
            let pending = reads.fetch_markets();
            Box::pin(async move { wire(&pending.await?) })
        }
        "fetchOHLCV" => {
            let opts = decoded(&arg(&args, 0));
            let pending = opts.map(|opts| reads.fetch_ohlcv(opts));
            Box::pin(async move { wire(&pending?.await?) })
        }
        "fetchTickers" => {
            let pending = reads.fetch_tickers();
            Box::pin(async move { wire(&pending.await?) })
        }
        "fetchPositions" => {
            let pending = reads.fetch_portfolio_raw();
            Box::pin(async move {
                let symbols: Option<Vec<String>> = if arg(&args, 0).is_undefined() {
                    None
                } else {
                    Some(decoded(&arg(&args, 0))?)
                };
                let utc = |n| reads.local_to_utc(n);
                let result = crate::portfolio_reads::fetch_positions(
                    || pending,
                    exchange.base.store.as_ref(),
                    &utc,
                    symbols.as_deref(),
                    &|message| report(&exchange, message),
                    |e| e,
                )
                .await?;
                wire(&result)
            })
        }
        "fetchOrders" => {
            let pending = reads.fetch_portfolio_raw();
            Box::pin(async move {
                let utc = |n| reads.local_to_utc(n);
                let log = |message: &str| report(&exchange, message);
                let result = crate::portfolio_reads::fetch_orders(
                    || pending,
                    exchange.base.store.as_ref(),
                    &utc,
                    &log,
                    |e| e,
                )
                .await?;
                wire(&result)
            })
        }
        "placeOrder" => {
            let opts = decoded(&arg(&args, 0));
            let pending = opts.map(|opts| ctx.place_order(opts));
            Box::pin(async move { wire(&pending?.await?) })
        }
        "cancelOrders" => {
            let orders = decoded(&arg(&args, 0));
            let pending = orders.map(|orders| ctx.cancel_orders(orders));
            Box::pin(async move {
                pending?.await?;
                Ok(Js::Undefined)
            })
        }
        "updateOrder" => {
            let prepared = (|| -> Result<_, ExchangeError> {
                let opts = arg(&args, 0);
                let order: Order = decoded(&opts.get("order")?)?;
                let update = opts.get("update")?;
                let price = if update.has_own("price") {
                    Some(if update.get("price")?.is_undefined() {
                        None
                    } else {
                        Some(update.get("price")?.to_number()?)
                    })
                } else {
                    None
                };
                let amount = if update.has_own("amount") {
                    Some(update.get("amount")?.to_number()?)
                } else {
                    None
                };
                Ok(ctx.update_order(order, OrderUpdate { price, amount }))
            })();
            Box::pin(async move { wire(&prepared?.await?) })
        }
        _ => Box::pin(async move {
            Err(ExchangeError::new(format!(
                "Unbound Breakout Prop member {name}"
            )))
        }),
    }
}

impl BreakoutpropExchange {
    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Result<Vec<Js>, ExchangeError>,
    ) -> ExchangeFuture<'_, T> {
        let call = args.and_then(|args| {
            self.binding
                .borrow()
                .as_ref()
                .ok_or_else(|| ExchangeError::new("Breakout Prop constructor is incomplete"))?
                .call_method(member, args)
                .map_err(Into::into)
        });
        Box::pin(async move {
            let value = await_js(&self.base.runtime, call?)
                .await
                .map_err(ExchangeError::from)?;
            decoded(&value)
        })
    }
    fn invoke_unit(
        &self,
        member: &'static str,
        args: Result<Vec<Js>, ExchangeError>,
    ) -> ExchangeFuture<'_, ()> {
        let call = args.and_then(|args| {
            self.binding
                .borrow()
                .as_ref()
                .ok_or_else(|| ExchangeError::new("Breakout Prop constructor is incomplete"))?
                .call_method(member, args)
                .map_err(Into::into)
        });
        Box::pin(async move {
            await_js(&self.base.runtime, call?)
                .await
                .map_err(ExchangeError::from)?;
            Ok(())
        })
    }
    fn invoke_order_id(
        &self,
        member: &'static str,
        args: Result<Vec<Js>, ExchangeError>,
    ) -> ExchangeFuture<'_, Vec<String>> {
        let call = args.and_then(|args| {
            self.binding
                .borrow()
                .as_ref()
                .ok_or_else(|| ExchangeError::new("Breakout Prop constructor is incomplete"))?
                .call_method(member, args)
                .map_err(Into::into)
        });
        Box::pin(async move {
            let value = await_js(&self.base.runtime, call?)
                .await
                .map_err(ExchangeError::from)?;
            // The source returns its single opaque orderId; the shared native ABI
            // returns a list of scalar IDs. Preserve numeric/string ID spelling.
            let ids = if value.is_array() {
                value.array_values()?.unwrap_or_default()
            } else {
                vec![value]
            };
            ids.into_iter()
                .map(|id| {
                    if !matches!(id, Js::String(_) | Js::Number(_)) {
                        return Err(ExchangeError::new(
                            "Breakout Prop orderId contains a non-scalar ID",
                        ));
                    }
                    String::from_utf16(&id.to_js_string()?).map_err(|_| {
                        ExchangeError::new("orderId exceeds the native Unicode boundary")
                    })
                })
                .collect()
        })
    }
}

impl Exchange for BreakoutpropExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        BreakoutpropExchange::dispose(self)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", Ok(vec![]))
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", Ok(vec![]))
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", Ok(vec![]))
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke(
            "fetchPositions",
            symbols.map_or(Ok(vec![]), |v| wire(&v).map(|v| vec![v])),
        )
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", Ok(vec![]))
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", Ok(vec![]))
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self.socket()?.listen_ohlcv(
            options,
            Rc::new(move |row| {
                callback(row.borrow().clone());
                Ok(())
            }),
        );
        Ok(Box::new(move || cleanup()))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cleanup = self.socket()?.listen_order_book(
            symbol,
            Rc::new(move |book| {
                callback(book.borrow().clone());
                Ok(())
            }),
        );
        Ok(Box::new(move || cleanup()))
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", wire(&options).map(|options| vec![options]))
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", Ok(vec![]))
    }
    fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
        self.invoke("fetchSymbolMaxLeverage", Ok(vec![Js::str(&symbol)]))
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.invoke_unit(
            "setLeverage",
            Ok(vec![Js::str(&symbol), Js::Number(leverage)]),
        )
    }
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("changePositionMode", Ok(vec![Js::Bool(hedged)]))
    }
    fn change_margin_type(
        &self,
        isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("changeMarginType", Ok(vec![Js::Bool(isolated)]))
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let checked = self.base.resolve_caller_client_order_id(order.options());
        self.invoke_order_id(
            "placeOrder",
            checked.and_then(|_| wire(order.options()).map(|v| vec![v])),
        )
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.invoke_order_id("updateOrder", wire(&opts).map(|v| vec![v]))
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelOrders", wire(&orders).map(|v| vec![v]))
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("scheduleCancel", Ok(vec![]))
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        self.invoke("fetchPublicTrades", Ok(vec![Js::str(&symbol)]))
    }
}
