//! CoinbaseExchange create/cancel and REST fallback field bodies.
//! Called SDK/transport/timer operations are explicit host capabilities.
use crate::{
    starbase_auth::{CoinbaseStarbasePromise, CoinbaseStarbasePromiseSpawner},
    types::{
        OrderRequestConfig, OrderRequestConfigLimitLimitGtc, OrderRequestConfigMarketMarketIoc,
        OrderRequestConfigStopLimitStopLimitGtc, PlaceOrderRequest, PlaceOrderRequestSide,
        StopDirection,
    },
    utils::get_coinbase_usd_equivalent_alias_product_id,
};
use indexmap::IndexMap;
use safe_cex_num::{adjust, js_number, js_number_string, js_string, js_truthy};
use safe_cex_types::{
    Market, Order, OrderExtra, OrderSide, OrderStatus, OrderTimeInForce, OrderType,
    PreparedOrderOpts,
};
use serde_json::{Map, Value, json};
use std::{cell::RefCell, rc::Rc, task::Poll};

/// JSON-domain thrown value; None is JavaScript undefined. Host exceptions retain their value.
#[derive(Debug, Clone, PartialEq)]
pub struct CoinbaseOrderFailure {
    pub value: Option<Value>,
}

/// Exchange-owned polling field, retaining JavaScript's f64 comparisons.
#[derive(Debug, Default)]
pub struct CoinbaseOrderPolling {
    pub next_orders_poll_at: f64,
}

/// Parsed response record with source object/array field identities. Primitive fields copy.
#[derive(Debug, Clone)]
pub struct CoinbaseOrderResponse {
    /// Replace a root property by inserting a new handle. Mutating the retained
    /// object/array contents through its existing handle preserves shallow aliases.
    pub fields: Rc<RefCell<IndexMap<String, Rc<RefCell<Value>>>>>,
}
impl CoinbaseOrderResponse {
    pub fn from_json(mut fields: Map<String, Value>) -> Self {
        safe_cex_num::normalize_object_order(&mut fields);
        for value in fields.values_mut() {
            safe_cex_num::normalize_json_order(value);
        }
        Self {
            fields: Rc::new(RefCell::new(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, Rc::new(RefCell::new(value))))
                    .collect(),
            )),
        }
    }
    fn get(&self, key: &str) -> Option<Value> {
        self.fields.borrow().get(key).map(|v| v.borrow().clone())
    }
    fn shallow_copy(&self) -> Self {
        Self {
            fields: Rc::new(RefCell::new(
                self.fields
                    .borrow()
                    .iter()
                    .map(|(key, value)| {
                        let copy = if value.borrow().is_object() || value.borrow().is_array() {
                            value.clone()
                        } else {
                            Rc::new(RefCell::new(value.borrow().clone()))
                        };
                        (key.clone(), copy)
                    })
                    .collect(),
            )),
        }
    }
}
/// Exact inline source Order fields, retaining shallow `info` references.
#[derive(Debug)]
pub struct CoinbaseOptimisticOrder {
    pub id: String,
    pub parent_id: String,
    pub status: OrderStatus,
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub extra: OrderExtra,
    pub reduce_only: bool,
    pub info: CoinbaseOrderResponse,
    pub last_update: f64,
}

async fn source_yield() {
    let mut first = true;
    std::future::poll_fn(move |context| {
        if first {
            first = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
/// Native receiver operations called by the mapped field bodies, in source order.
/// Transport capabilities execute invocation effects eagerly and return their pending result.
pub trait CoinbaseOrderHost {
    fn now_ms(&mut self) -> f64;
    fn orders_loaded(&mut self) -> bool;
    fn orders(&mut self) -> Vec<Rc<Order>>;
    fn market_by_symbol(&mut self, symbol: &str) -> Option<Rc<Market>>;
    fn markets_by_id(&mut self) -> &IndexMap<String, Rc<Market>>;
    fn resolve_caller_client_order_id(
        &mut self,
        opts: &PreparedOrderOpts,
    ) -> Result<Option<String>, CoinbaseOrderFailure>;
    fn uuid(&mut self) -> String;
    fn post_create_order(
        &mut self,
        endpoint: &'static str,
        payload: &PlaceOrderRequest,
    ) -> CoinbaseStarbasePromise<Result<CoinbaseOrderResponse, CoinbaseOrderFailure>>;
    fn post_cancel_orders(
        &mut self,
        endpoint: &'static str,
        ids: &[String],
    ) -> CoinbaseStarbasePromise<Result<(), CoinbaseOrderFailure>>;
    fn add_or_update_order(
        &mut self,
        order: Rc<CoinbaseOptimisticOrder>,
    ) -> Result<(), CoinbaseOrderFailure>;
    fn update_orders_status(
        &mut self,
        ids: &[String],
        status: OrderStatus,
    ) -> Result<(), CoinbaseOrderFailure>;
    fn schedule_open_orders_rest_reconciliation(
        &mut self,
        reason: &'static str,
    ) -> Result<(), CoinbaseOrderFailure>;
    fn schedule_private_user_stream_write_check(&mut self) -> Result<(), CoinbaseOrderFailure>;
    fn log_warning(&mut self, message: &'static str) -> Result<(), CoinbaseOrderFailure>;
    fn emit_and_log_error(&mut self, args: &[Option<&Value>]) -> Result<(), CoinbaseOrderFailure>;
}

fn error(message: impl Into<String>) -> CoinbaseOrderFailure {
    CoinbaseOrderFailure {
        value: Some(json!({"name":"Error", "message":message.into()})),
    }
}

fn coercion(e: safe_cex_num::JsCoercionError) -> CoinbaseOrderFailure {
    CoinbaseOrderFailure {
        value: Some(json!({"name":e.name,"message":e.message})),
    }
}

fn message(err: Option<&Value>) -> Option<&Value> {
    err.and_then(|err| err.get("response"))
        .and_then(|response| response.get("data"))
        .and_then(|data| data.get("message"))
        .filter(|v| js_truthy(Some(v)))
        .or_else(|| err.and_then(|err| err.get("message")))
}

// ts: CoinbaseExchange.shouldPollOrders
pub fn should_poll_orders(
    state: &mut CoinbaseOrderPolling,
    host: &mut impl CoinbaseOrderHost,
) -> bool {
    if host.orders_loaded() {
        return false;
    }
    let now = host.now_ms();
    if now >= state.next_orders_poll_at {
        state.next_orders_poll_at = now + 10_000.0;
        return true;
    }
    false
}

// ts: CoinbaseExchange.mergeRestOrdersWithFreshWsState
pub fn merge_rest_orders_with_fresh_ws_state(
    host: &mut impl CoinbaseOrderHost,
    rest: Rc<Vec<Rc<Order>>>,
) -> Rc<Vec<Rc<Order>>> {
    let rest_ids: std::collections::HashSet<_> = rest.iter().map(|o| o.id.as_str()).collect();
    let now = host.now_ms();
    let preserved: Vec<_> = host
        .orders()
        .into_iter()
        .filter(|existing| {
            !rest_ids.contains(existing.id.as_str())
                && existing.status == OrderStatus::Open
                && now - existing.last_update < 5_000.0
        })
        .collect();
    if preserved.is_empty() {
        return rest;
    }
    Rc::new(rest.iter().cloned().chain(preserved).collect())
}

// ts: CoinbaseExchange.resolveMarketByCoinbaseProductId
pub fn resolve_market_by_coinbase_product_id(
    host: &mut impl CoinbaseOrderHost,
    product_id: &str,
) -> Option<Rc<Market>> {
    let markets = host.markets_by_id();
    let exact = markets.get(product_id);
    let alias_id = get_coinbase_usd_equivalent_alias_product_id(product_id);
    let alias = alias_id.as_ref().and_then(|id| markets.get(id));
    if product_id.ends_with("-USD") && alias.is_some() {
        return alias.cloned();
    }
    exact.or(alias).cloned()
}

// ts: CoinbaseExchange.formatCreateOrder
pub fn format_create_order(
    host: &mut impl CoinbaseOrderHost,
    opts: &PreparedOrderOpts,
) -> Result<PlaceOrderRequest, CoinbaseOrderFailure> {
    let market = host
        .market_by_symbol(&opts.symbol)
        .ok_or_else(|| error(format!("Market {} not found", opts.symbol)))?;
    let p_price = market.precision.price;
    let p_amount = market.precision.amount;
    let amount = adjust(opts.amount, p_amount);
    let price = opts
        .price
        .filter(|price| *price != 0.0 && !price.is_nan() && opts.r#type != OrderType::Market)
        .map(|price| adjust(price, p_price));
    let mut config = OrderRequestConfig {
        market_market_ioc: None,
        sor_limit_ioc: None,
        limit_limit_gtc: None,
        limit_limit_gtd: None,
        stop_limit_stop_limit_gtc: None,
        stop_limit_stop_limit_gtd: None,
        trigger_bracket_gtc: None,
        trigger_bracket_gtd: None,
    };
    if opts.r#type == OrderType::Market {
        config.market_market_ioc = Some(OrderRequestConfigMarketMarketIoc {
            base_size: js_number_string(amount),
            quote_size: None,
        });
    } else if opts.r#type == OrderType::StopLoss {
        let stop_is_above = opts.side == OrderSide::Buy && opts.r#type == OrderType::StopLoss;
        let stop_price = price.filter(|p| *p != 0.0 && !p.is_nan()).unwrap_or(0.0);
        let limit_price = adjust(
            stop_price * if stop_is_above { 1.01 } else { 0.99 },
            p_price,
        );
        config.stop_limit_stop_limit_gtc = Some(OrderRequestConfigStopLimitStopLimitGtc {
            base_size: js_number_string(amount),
            limit_price: js_number_string(limit_price),
            stop_price: js_number_string(stop_price),
            stop_direction: if stop_is_above {
                StopDirection::StopDirectionStopUp
            } else {
                StopDirection::StopDirectionStopDown
            },
        });
    } else {
        config.limit_limit_gtc = Some(OrderRequestConfigLimitLimitGtc {
            base_size: js_number_string(amount),
            limit_price: price
                .map(js_number_string)
                .unwrap_or_else(|| "undefined".into()),
            post_only: Some(opts.time_in_force == Some(OrderTimeInForce::PostOnly)),
        });
    }
    let client_order_id = host
        .resolve_caller_client_order_id(opts)?
        .unwrap_or_else(|| format!("tealstreet-{}", host.uuid()));
    Ok(PlaceOrderRequest {
        client_order_id,
        product_id: opts.symbol.clone(),
        side: if opts.side == OrderSide::Buy {
            PlaceOrderRequestSide::Buy
        } else {
            PlaceOrderRequestSide::Sell
        },
        leverage: Some("10".into()),
        order_configuration: config,
        self_trade_prevention_id: None,
        margin_type: None,
        retail_portfolio_id: None,
    })
}

// ts: CoinbaseExchange.buildOptimisticOrderFromCreate
pub fn build_optimistic_order_from_create(
    host: &mut impl CoinbaseOrderHost,
    payload: &PlaceOrderRequest,
    order_id: &str,
    response: &CoinbaseOrderResponse,
) -> Result<Option<Rc<CoinbaseOptimisticOrder>>, CoinbaseOrderFailure> {
    if payload.product_id.is_empty() {
        return Ok(None);
    }
    let Some(market) = resolve_market_by_coinbase_product_id(host, &payload.product_id) else {
        return Ok(None);
    };
    if market.is_spot != Some(true)
        || market.is_expiring == Some(true)
        || market.contract_size != Some(1.0)
    {
        return Ok(None);
    }
    let (base, price) =
        if let Some(config) = response.get("order_configuration").filter(|v| !v.is_null()) {
            let limit = ["limit_limit_gtc", "limit_limit_gtd", "sor_limit_ioc"]
                .iter()
                .find_map(|key| config.get(key).filter(|v| js_truthy(Some(v))));
            let Some(limit) = limit else {
                return Ok(None);
            };
            (
                limit.get("base_size").cloned(),
                limit.get("limit_price").cloned(),
            )
        } else {
            let c = &payload.order_configuration;
            let limit = c
                .limit_limit_gtc
                .as_ref()
                .map(|v| (&v.base_size, &v.limit_price))
                .or_else(|| {
                    c.limit_limit_gtd
                        .as_ref()
                        .map(|v| (&v.base_size, &v.limit_price))
                })
                .or_else(|| {
                    c.sor_limit_ioc
                        .as_ref()
                        .map(|v| (&v.base_size, &v.limit_price))
                });
            let Some((base, price)) = limit else {
                return Ok(None);
            };
            (
                Some(Value::String(base.clone())),
                Some(Value::String(price.clone())),
            )
        };
    let amount = js_number(base.as_ref()).map_err(coercion)?;
    let price = js_number(price.as_ref()).map_err(coercion)?;
    if !amount.is_finite() || amount <= 0.0 || !price.is_finite() || price <= 0.0 {
        return Ok(None);
    }
    let info = response.shallow_copy();
    info.fields.borrow_mut().insert(
        "optimisticFromCreateOrder".into(),
        Rc::new(RefCell::new(Value::Bool(true))),
    );
    Ok(Some(Rc::new(CoinbaseOptimisticOrder {
        id: order_id.into(),
        parent_id: payload.client_order_id.clone(),
        status: OrderStatus::Open,
        symbol: market.symbol.clone(),
        r#type: OrderType::Limit,
        side: if payload.side == PlaceOrderRequestSide::Buy {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        price,
        amount,
        filled: 0.0,
        remaining: amount,
        extra: OrderExtra {
            remaining_native: Some(amount),
            filled_in_placement_units: None,
            remaining_notional: None,
            conditional_order_price: None,
            reject_reason: None,
            is_from_chaser: None,
            chaser_id: None,
            chaser_attempts: None,
            chaser_max_attempts: None,
        },
        reduce_only: false,
        info,
        last_update: host.now_ms(),
    })))
}

// ts: CoinbaseExchange.emitOrdersFetchError
pub fn emit_orders_fetch_error(
    state: &mut CoinbaseOrderPolling,
    host: &mut impl CoinbaseOrderHost,
    err: Option<Value>,
) -> Result<(), CoinbaseOrderFailure> {
    if err
        .as_ref()
        .and_then(|e| e.get("response"))
        .and_then(|r| r.get("status"))
        .and_then(Value::as_f64)
        == Some(429.0)
    {
        state.next_orders_poll_at = host.now_ms() + 60_000.0;
        host.log_warning(
            "Coinbase open-order REST fallback rate limited (429); waiting before retrying",
        )?;
        return Ok(());
    }
    host.emit_and_log_error(&[err.as_ref(), message(err.as_ref())])
}

// ts: CoinbaseExchange.placeOrder
pub fn place_order<H: CoinbaseOrderHost + 'static>(
    host: Rc<RefCell<H>>,
    opts: impl Into<PreparedOrderOpts>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> CoinbaseStarbasePromise<Result<Vec<String>, CoinbaseOrderFailure>> {
    let opts = opts.into();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            host.borrow_mut().resolve_caller_client_order_id(&opts)?;
            let payload = format_create_order(&mut *host.borrow_mut(), &opts)?;
            let result = async {
                let request = host
                    .borrow_mut()
                    .post_create_order("/api/v3/brokerage/orders", &payload);
                let data = request.await?;
                if !js_truthy(data.get("success").as_ref()) {
                    let failure = data.get("error_response");
                    let fallback = data.get("failure_reason");
                    let reason = failure
                        .as_ref()
                        .and_then(|e| e.get("new_order_failure_reason"))
                        .filter(|v| js_truthy(Some(v)))
                        .or_else(|| {
                            failure
                                .as_ref()
                                .and_then(|e| e.get("preview_failure_reason"))
                                .filter(|v| js_truthy(Some(v)))
                        })
                        .or_else(|| fallback.as_ref().filter(|v| js_truthy(Some(v))));
                    return Err(error(
                        reason
                            .map(|v| js_string(Some(v)))
                            .transpose()
                            .map_err(coercion)?
                            .unwrap_or_else(|| "Unknown error".into()),
                    ));
                }
                let success = data.get("success_response");
                let fallback = data.get("order_id");
                let id = success
                    .as_ref()
                    .and_then(|v| v.get("order_id"))
                    .filter(|v| !v.is_null())
                    .or(fallback.as_ref());
                if !js_truthy(id) {
                    return Err(error(
                        "Coinbase create-order response did not include an order ID",
                    ));
                }
                // The declared response domain supplies string order IDs.
                let id = id
                    .and_then(Value::as_str)
                    .ok_or_else(|| error("Coinbase order ID must be a string"))?
                    .to_owned();
                let optimistic_order = build_optimistic_order_from_create(
                    &mut *host.borrow_mut(),
                    &payload,
                    &id,
                    &data,
                )?;
                if let Some(order) = optimistic_order {
                    host.borrow_mut().add_or_update_order(order)?;
                }
                host.borrow_mut()
                    .schedule_open_orders_rest_reconciliation("place-order")?;
                host.borrow_mut()
                    .schedule_private_user_stream_write_check()?;
                Ok(vec![id])
            }
            .await;
            match result {
                Ok(ids) => Ok(ids),
                Err(err) => {
                    let detail = message(err.value.as_ref());
                    host.borrow_mut().emit_and_log_error(&[detail])?;
                    // new Error(undefined) has an empty message, unlike String(undefined).
                    let text = detail
                        .map(|v| js_string(Some(v)))
                        .transpose()
                        .map_err(coercion)?
                        .unwrap_or_default();
                    Err(error(text))
                }
            }
        }),
        spawn,
    )
}

// ts: CoinbaseExchange.cancelOrders
pub fn cancel_orders<H: CoinbaseOrderHost + 'static>(
    host: Rc<RefCell<H>>,
    orders: Vec<Rc<Order>>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> CoinbaseStarbasePromise<Result<(), CoinbaseOrderFailure>> {
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let result: Result<(), CoinbaseOrderFailure> = async {
                let ids: Vec<_> = orders.iter().map(|o| o.id.clone()).collect();
                let series_result: Result<(), CoinbaseOrderFailure> = async {
                    for lot in ids.chunks(100) {
                        // p-iteration awaits each dense array element before calling the lot callback.
                        source_yield().await;
                        let lot_result = async {
                            let request = host
                                .borrow_mut()
                                .post_cancel_orders("/api/v3/brokerage/orders/batch_cancel", lot);
                            request.await?;
                            host.borrow_mut()
                                .update_orders_status(lot, OrderStatus::Canceled)
                        }
                        .await;
                        // forEachSeries awaits the async callback, including rejected callbacks.
                        source_yield().await;
                        lot_result?;
                    }
                    Ok(())
                }
                .await;
                // cancelOrders awaits forEachSeries, on success and rejection alike.
                source_yield().await;
                series_result?;
                host.borrow_mut()
                    .schedule_open_orders_rest_reconciliation("cancel-order")?;
                host.borrow_mut()
                    .schedule_private_user_stream_write_check()?;
                Ok(())
            }
            .await;
            match result {
                Ok(()) => Ok(()),
                Err(err) => host
                    .borrow_mut()
                    .emit_and_log_error(&[message(err.value.as_ref())]),
            }
        }),
        spawn,
    )
}
