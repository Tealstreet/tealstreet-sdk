//! Order and polling orchestration. Dependencies name already-mapped methods;
//! NativeReads below binds them to the concrete exchange and existing helpers.
use crate::{lifecycle::BreakoutpropExchange, portfolio};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::{
    adjust, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough},
};
use safe_cex_store::StoreUpdate;
use safe_cex_transport::provider::{RestBody, RestRequest};
use safe_cex_types::{
    Market, ModelField, Order, OrderSide, OrderType, PlaceOrderOpts, PreparedOrderOpts, Ticker,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{future::poll_fn, rc::Rc, task::Poll};

pub trait RuntimeReads {
    fn init_external_public_data(&self) -> ExchangeFuture<'static, bool>;
    fn get_account_code(&self) -> ExchangeFuture<'static, String>;
    fn fetch_markets(&self) -> ExchangeFuture<'static, Vec<Market>>;
    fn fetch_tickers(&self) -> ExchangeFuture<'static, Option<Vec<Ticker>>>;
    fn fetch_portfolio_raw(&self) -> ExchangeFuture<'static, Option<JsNumberInput>>;
    fn local_to_utc(&self, local: f64) -> f64;
    fn uuid(&self) -> String;
    fn spawn_order(&self, future: ExchangeFuture<'static, ()>);
    /// Bind the source loop helper: defer this callback by the supplied interval.
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError>;
}
pub struct BreakoutRuntime {
    pub exchange: Rc<BreakoutpropExchange>,
    pub reads: Rc<dyn RuntimeReads>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DxPlacePayload {
    pub order_code: String,
    pub r#type: &'static str,
    pub instrument: String,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    pub position_effect: &'static str,
    pub tif: &'static str,
}
/// The exact numeric payload before JSON.stringify's nonfinite-to-null boundary.
pub fn prepare_order(
    opts: impl Into<PreparedOrderOpts>,
    market: &Market,
    uuid: String,
) -> Result<DxPlacePayload, ExchangeError> {
    let opts = opts.into();
    let quantity = adjust(opts.amount, market.precision.amount);
    let order_type = serde_json::to_value(opts.r#type).expect("order type");
    let kind = crate::exchange::to_dx_order_type(order_type.as_str().unwrap_or("undefined"))
        .map_err(ExchangeError::new)?;
    let price = opts.price.map(|p| adjust(p, market.precision.price));
    let raw_stop = opts
        .trigger_price
        .or(if opts.r#type == OrderType::StopLoss {
            opts.stop_loss
        } else {
            opts.take_profit
        });
    let stop = raw_stop.map(|p| adjust(p, market.precision.price));
    if kind == "LIMIT" && price.is_none() {
        return Err(ExchangeError::new(
            "Breakout Prop limit orders require a price",
        ));
    }
    if kind == "STOP" && stop.is_none() {
        return Err(ExchangeError::new(
            "Breakout Prop stop orders require a trigger price",
        ));
    }
    Ok(DxPlacePayload {
        order_code: format!("tealstreet-{uuid}"),
        r#type: kind,
        instrument: opts.symbol.clone(),
        quantity,
        side: match opts.side {
            ModelField::Value(OrderSide::Buy) => Some("BUY"),
            ModelField::Value(OrderSide::Sell) => Some("SELL"),
            ModelField::Absent | ModelField::Undefined => None,
        },
        limit_price: if kind == "LIMIT" { price } else { None },
        stop_price: if kind == "STOP" { stop } else { None },
        position_effect: if opts.reduce_only == Some(true) {
            "CLOSE"
        } else {
            "OPEN"
        },
        tif: "GTC",
    })
}
#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct {} passthrough {value});
    }
}
pub fn raw_value(input: JsNumberInput) -> Result<Value, ExchangeError> {
    input
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
        .map(|r| r.value)
        .map_err(|e| ExchangeError::new(e.to_string()))
}
/// An explicit property-present patch, including present undefined for price.
#[derive(Default)]
pub struct OrderUpdate {
    pub price: Option<Option<f64>>,
    pub amount: Option<f64>,
}
pub fn prepare_update_order(order: &Order, update: OrderUpdate) -> PreparedOrderOpts {
    PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: order.r#type.into_property(),
        side: order.side.into_property(),
        amount: update.amount.unwrap_or(order.amount),
        price: update.price.unwrap_or_else(|| order.price.into_value()),
        trigger_price: None,
        stop_loss: order.stop_loss.into_value(),
        take_profit: order.take_profit.into_value(),
        reduce_only: order.reduce_only.into_property(),
        time_in_force: order.time_in_force.into_value(),
        trigger_type: order.trigger_type.into_value(),
        tp_trigger_type: order.tp_trigger_type.into_value(),
        sl_trigger_type: order.sl_trigger_type.into_value(),
        close: order.close.into_value(),
        ..Default::default()
    }
}
impl BreakoutRuntime {
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput> {
        match self.exchange.xhr.borrow().clone() {
            Some(client) => client.request(request),
            None => {
                Box::pin(async { Err(ExchangeError::new("DXtrade API construction incomplete")) })
            }
        }
    }
    fn report(&self, error: &ExchangeError, response_message: bool) -> Result<(), ExchangeError> {
        let msg = if response_message {
            error
                .response
                .as_ref()
                .and_then(|r| r.get("data"))
                .and_then(|r| r.get("msg"))
        } else {
            None
        };
        self.exchange.base.emit_and_log_error(
            ExchangeErrorInput::Message(if js_truthy(msg) {
                safe_cex_types::js_coercion::js_string(Some(msg.unwrap()))
                    .map_err(|e| ExchangeError::new(e.to_string()))?
            } else {
                error.message.clone()
            }),
            None,
            None,
        )
    }
    // ts: cancelOrder. Session prefix runs at method call, before first poll.
    pub fn cancel_order(self: &Rc<Self>, code: String) -> ExchangeFuture<'static, String> {
        let pending = self.exchange.get_session_token();
        let ctx = self.clone();
        Box::pin(async move {
            let result = async {
                pending.await?;
                let account = ctx.reads.get_account_code().await?;
                ctx.request(RestRequest {
                    method: "DELETE".into(),
                    url: "accounts/{accountCode}/orders/{orderCode}".into(),
                    params: Some(json!({"accountCode":account,"orderCode":code})),
                    ..Default::default()
                })
                .await?;
                Ok(code)
            }
            .await;
            if let Err(e) = &result {
                ctx.report(e, true)?;
            }
            result
        })
    }
    fn owned_cancel(self: &Rc<Self>, code: String) -> ExchangeFuture<'static, String> {
        let task = Rc::new(OwnedCancel {
            future: std::cell::RefCell::new(Some(self.cancel_order(code))),
            result: std::cell::RefCell::new(None),
            waiters: std::cell::RefCell::new(vec![]),
        });
        let driven = task.clone();
        self.reads
            .spawn_order(Box::pin(poll_fn(move |cx| match driven.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => Poll::Ready(Ok(())),
            })));
        Box::pin(poll_fn(move |cx| task.poll(cx)))
    }
    // ts: cancelOrders. Construct every call in a lot before polling any of them.
    pub fn cancel_orders(self: &Rc<Self>, orders: Vec<Order>) -> ExchangeFuture<'static, ()> {
        let ctx = self.clone();
        // Source starts the first lot synchronously; later lots start after await.
        let mut lots = orders
            .chunks(10)
            .map(|lot| lot.to_vec())
            .collect::<Vec<_>>()
            .into_iter();
        let first = lots
            .next()
            .map(|lot| lot.into_iter().map(|o| ctx.owned_cancel(o.id)).collect());
        Box::pin(async move {
            if let Some(first) = first {
                join_lot(first).await?;
            }
            for lot in lots {
                join_lot(lot.into_iter().map(|o| ctx.owned_cancel(o.id)).collect()).await?;
            }
            Ok(())
        })
    }
    pub fn update_order(
        self: &Rc<Self>,
        order: Order,
        update: OrderUpdate,
    ) -> ExchangeFuture<'static, Value> {
        let opts = prepare_update_order(&order, update);
        let cancel = self.cancel_orders(vec![order]);
        let ctx = self.clone();
        Box::pin(async move {
            cancel.await?;
            ctx.place_prepared_order(opts).await
        })
    }
    pub fn place_order(self: &Rc<Self>, opts: PlaceOrderOpts) -> ExchangeFuture<'static, Value> {
        // Outside source try: caller-ID rejection must not reach this catch.
        let preflight = self.exchange.base.resolve_caller_client_order_id(&opts);
        if let Err(error) = preflight {
            return Box::pin(async move { Err(error) });
        }
        self.place_prepared_order(opts.into())
    }
    fn place_prepared_order(
        self: &Rc<Self>,
        opts: PreparedOrderOpts,
    ) -> ExchangeFuture<'static, Value> {
        let market = self
            .exchange
            .base
            .store
            .markets_by_symbol()
            .get(&opts.symbol)
            .cloned();
        if market.is_none() {
            let error = ExchangeError::new("Market not found");
            let result = self.report(&error, true).and(Err(error));
            return Box::pin(async move { result });
        }
        let pending = if market.is_some() {
            Some(self.exchange.get_session_token())
        } else {
            None
        };
        let ctx = self.clone();
        Box::pin(async move {
            let result = async {
                let market = market.ok_or_else(|| ExchangeError::new("Market not found"))?;
                pending.unwrap().await?;
                let account = ctx.reads.get_account_code().await?;
                // uuid is invoked only after all validation, just before construction.
                let mut payload = prepare_order(&opts, &market, String::new())?;
                payload.order_code = format!("tealstreet-{}", ctx.reads.uuid());
                let data = raw_value(
                    ctx.request(RestRequest {
                        method: "POST".into(),
                        url: "accounts/{accountCode}/orders".into(),
                        params: Some(json!({"accountCode":account})),
                        body: Some(RestBody::Untransformed(
                            serde_json::to_value(payload)
                                .map_err(|e| ExchangeError::new(e.to_string()))?,
                        )),
                        ..Default::default()
                    })
                    .await?,
                )?;
                if data.is_null() {
                    let mut e =
                        ExchangeError::new("Cannot read properties of null (reading 'orderId')");
                    e.name = "TypeError".into();
                    return Err(e);
                }
                let id = data
                    .get("orderId")
                    .filter(|id| js_truthy(Some(id)))
                    .cloned()
                    .ok_or_else(|| ExchangeError::new("No order ID received"))?;
                Ok(id)
            }
            .await;
            if let Err(e) = &result {
                ctx.report(e, true)?;
            }
            result
        })
    }
    fn update(&self, patch: StoreUpdate) -> Result<(), ExchangeError> {
        self.exchange
            .base
            .store
            .update(patch, false)
            .map_err(Into::into)
    }
    fn update_portfolio(&self, input: &JsNumberInput) -> Result<(), ExchangeError> {
        let store = self.exchange.base.store.as_ref();
        let utc = |n| self.reads.local_to_utc(n);
        let report = |s: &str| self.report(&ExchangeError::new(s), false);
        let balance = portfolio::format_balance(input, store, &utc, &report)?;
        let positions = portfolio::format_positions(input, store, None, &utc, &report)?;
        let orders = portfolio::format_orders(input, store, &utc, &report)?;
        let mut loaded = store.loaded().clone();
        loaded.balance = true;
        loaded.positions = true;
        loaded.orders = true;
        self.update(StoreUpdate {
            balance: Some(balance),
            positions: Some(positions),
            orders,
            loaded: Some(loaded),
            ..Default::default()
        })
    }
    fn public_only(&self) -> bool {
        self.exchange
            .base
            .is_public_data_only(self.exchange.as_ref())
    }
    pub fn start(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let pending = self.reads.init_external_public_data();
        let ctx = self.clone();
        Box::pin(async move {
            let external = pending.await?;
            if !ctx.public_only() {
                ctx.reads.get_account_code().await?;
            }
            if !external {
                let markets = ctx.reads.fetch_markets().await?;
                if ctx.exchange.base.is_disposed() {
                    return Ok(());
                }
                let mut loaded = ctx.exchange.base.store.loaded().clone();
                loaded.markets = true;
                ctx.update(StoreUpdate {
                    markets: Some(markets),
                    loaded: Some(loaded),
                    ..Default::default()
                })?;
                let tickers = ctx.reads.fetch_tickers().await?;
                if ctx.exchange.base.is_disposed() {
                    return Ok(());
                }
                let mut loaded = ctx.exchange.base.store.loaded().clone();
                loaded.tickers = true;
                ctx.update(StoreUpdate {
                    tickers,
                    loaded: Some(loaded),
                    ..Default::default()
                })?;
            }
            if ctx.public_only() {
                let mut loaded = ctx.exchange.base.store.loaded().clone();
                loaded.balance = true;
                loaded.positions = true;
                loaded.orders = true;
                ctx.update(StoreUpdate {
                    loaded: Some(loaded),
                    ..Default::default()
                })?;
                return Ok(());
            }
            let input = ctx.reads.fetch_portfolio_raw().await?;
            if ctx.exchange.base.is_disposed() {
                return Ok(());
            }
            if let Some(input) = input {
                if js_truthy(Some(&raw_value(input.clone())?)) {
                    ctx.update_portfolio(&input)?;
                }
            }
            ctx.tick().await
        })
    }
    pub fn tick(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let ctx = self.clone();
        let pending = if !self.exchange.base.is_disposed() && !self.public_only() {
            Some(self.reads.fetch_portfolio_raw())
        } else {
            None
        };
        Box::pin(async move {
            let Some(pending) = pending else {
                return Ok(());
            };
            let result = async {
                let input = pending.await?;
                if ctx.exchange.base.is_disposed() {
                    return Ok(());
                }
                if let Some(input) = input {
                    if js_truthy(Some(&raw_value(input.clone())?)) {
                        ctx.update_portfolio(&input)?;
                    }
                }
                let tickers = ctx.reads.fetch_tickers().await?;
                if ctx.exchange.base.is_disposed() {
                    return Ok(());
                }
                let mut loaded = ctx.exchange.base.store.loaded().clone();
                loaded.tickers = true;
                ctx.update(StoreUpdate {
                    tickers: tickers.clone(),
                    loaded: Some(loaded),
                    ..Default::default()
                })?;
                let table = ctx.exchange.socket()?.message_handlers.clone();
                let tickers = tickers.ok_or_else(|| {
                    ExchangeError::new("Cannot read properties of undefined (reading 'forEach')")
                })?;
                for ticker in &tickers {
                    for prefix in ["poll_book", "poll_kline"] {
                        let list = table
                            .borrow()
                            .iter()
                            .find(|(k, _)| k == &format!("{prefix}.{}", ticker.symbol))
                            .map(|(_, v)| v.clone());
                        if let Some(list) = list {
                            let len = list.borrow().len();
                            for index in 0..len {
                                let cb = list.borrow().get(index).cloned();
                                if let Some(cb) = cb {
                                    cb(ticker)?;
                                }
                            }
                        }
                    }
                }
                Ok::<(), ExchangeError>(())
            }
            .await;
            if let Err(e) = result {
                ctx.report(&e, false)?;
            }
            if !ctx.exchange.base.is_disposed() {
                let next = ctx.clone();
                ctx.reads.schedule_loop(
                    Rc::new(move || next.tick()),
                    ctx.exchange.base.get_tick_interval(),
                )?;
            }
            Ok(())
        })
    }
}
async fn join_lot(mut tasks: Vec<ExchangeFuture<'static, String>>) -> Result<(), ExchangeError> {
    let mut done = vec![false; tasks.len()];
    poll_fn(|cx| {
        for (i, task) in tasks.iter_mut().enumerate() {
            if !done[i] {
                match task.as_mut().poll(cx) {
                    Poll::Pending => {}
                    Poll::Ready(Ok(_)) => done[i] = true,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                }
            }
        }
        if done.iter().all(|d| *d) {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    })
    .await
}

struct OwnedCancel {
    future: std::cell::RefCell<Option<ExchangeFuture<'static, String>>>,
    result: std::cell::RefCell<Option<Result<String, ExchangeError>>>,
    waiters: std::cell::RefCell<Vec<std::task::Waker>>,
}
impl OwnedCancel {
    fn poll(&self, cx: &mut std::task::Context<'_>) -> Poll<Result<String, ExchangeError>> {
        if let Some(r) = self.result.borrow().clone() {
            return Poll::Ready(r);
        }
        {
            let mut waiters = self.waiters.borrow_mut();
            if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
        }
        let pending = self.future.borrow_mut().take();
        if let Some(mut future) = pending {
            match future.as_mut().poll(cx) {
                Poll::Pending => *self.future.borrow_mut() = Some(future),
                Poll::Ready(r) => {
                    *self.result.borrow_mut() = Some(r.clone());
                    for w in std::mem::take(&mut *self.waiters.borrow_mut()) {
                        w.wake();
                    }
                    return Poll::Ready(r);
                }
            }
        }
        Poll::Pending
    }
}
