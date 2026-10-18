//! Single order placement and cancel/replacement bodies.
use crate::public_control::JsString;
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
pub type Task<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
pub type Options =
    safe_cex_types::PreparedOrderOpts<JsString, f64, safe_cex_types::ModelField<bool>>;
#[derive(Clone, Debug)]
pub enum Field {
    Text(JsString),
    Number(f64),
    Bool(bool),
}
pub type Request = Vec<(&'static str, Field)>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}
pub trait Host: 'static {
    type Error: 'static;
    type Market: Clone;
    type Existing: Clone;
    type Target;
    fn resolve_caller_id(&mut self, opts: &Options) -> Result<Option<JsString>, Self::Error>;
    fn market(&mut self, symbol: &[u16]) -> Result<Option<Self::Market>, Self::Error>;
    fn market_id(&mut self, market: &Self::Market) -> Result<JsString, Self::Error>;
    fn precision(&mut self, market: &Self::Market, price: bool) -> Result<f64, Self::Error>;
    fn trigger_by(
        &mut self,
        tp: bool,
        explicit: Option<OrderStopTriggerType>,
    ) -> Result<JsString, Self::Error>;
    fn post(
        &mut self,
        endpoint: &str,
        request: Request,
    ) -> Result<Task<JsString, Self::Error>, Self::Error>;
    fn log_target(&mut self) -> Result<Self::Target, Self::Error>;
    fn log(
        &mut self,
        target: Self::Target,
        message: JsString,
        severity: Severity,
    ) -> Result<(), Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn error(&mut self, message: JsString, range: bool) -> Self::Error;
    fn find_order(&mut self, id: &[u16]) -> Result<Option<Self::Existing>, Self::Error>;
    fn order_options(&mut self, order: &Self::Existing) -> Result<Options, Self::Error>;
    fn order_id(&mut self, order: &Self::Existing) -> Result<JsString, Self::Error>;
    fn cancel(&mut self, order: Self::Existing) -> Result<Task<(), Self::Error>, Self::Error>;
    fn place(&mut self, options: Options) -> Result<Task<Vec<JsString>, Self::Error>, Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn join(parts: &[&[u16]]) -> JsString {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}
fn truth(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn format<H: Host>(h: &mut H, m: &H::Market, n: f64, price: bool) -> Result<JsString, H::Error> {
    let precision = h.precision(m, price)?;
    let factor = 1.0 / precision;
    let rounded = safe_cex_num::math_round(n * factor) / factor;
    let text = safe_cex_num::js_number_string(precision);
    let decimals = if precision >= 1.0 {
        0
    } else {
        text.find('.').map_or(0, |at| text.len() - at - 1)
    };
    let text = if decimals > 0 {
        safe_cex_num::js_to_fixed(rounded, decimals as f64).map_err(|s| h.error(u(&s), true))?
    } else {
        safe_cex_num::js_number_string(safe_cex_num::math_round(rounded))
    };
    Ok(u(&text))
}
fn warning<H: Host>(h: &mut H, prefix: &str, e: &H::Error) -> Result<(), H::Error> {
    let target = h.log_target()?;
    let text = h.error_text(e)?;
    h.log(target, join(&[&u(prefix), &text]), Severity::Warning)
}
fn caught<H: Host, T>(h: &mut H, e: H::Error, prefix: &[u16]) -> Result<T, H::Error> {
    let target = h.log_target()?;
    let text = h.error_text(&e)?;
    h.log(target, join(&[prefix, &text]), Severity::Error)?;
    Err(e)
}
struct Plan<H: Host> {
    market: H::Market,
    separate: bool,
    response: Task<JsString, H::Error>,
}
// ts: BackpackExchange.placeOrder
pub fn place_order<H: Host>(host: Rc<RefCell<H>>, opts: Options) -> Task<Vec<JsString>, H::Error> {
    let caller = host.borrow_mut().resolve_caller_id(&opts);
    let caller = match caller {
        Ok(v) => v,
        Err(e) => return Box::pin(std::future::ready(Err(e))),
    };
    let prepared = (|| {
        let mut h = host.borrow_mut();
        let market = h
            .market(&opts.symbol)?
            .ok_or_else(|| h.error(join(&[&u("Market not found: "), &opts.symbol]), false))?;
        let post = opts.time_in_force == Some(OrderTimeInForce::PostOnly);
        let stop = matches!(
            opts.r#type.into_value(),
            Some(OrderType::StopLoss | OrderType::TakeProfit)
        );
        let trigger = stop || opts.trigger_price.is_some();
        let limit = opts.r#type == OrderType::Limit;
        let attached = opts.take_profit.is_some() || opts.stop_loss.is_some();
        let api_market = stop || opts.r#type == OrderType::Market;
        let mut req = vec![
            ("symbol", Field::Text(h.market_id(&market)?)),
            (
                "side",
                Field::Text(u(if opts.side == OrderSide::Buy {
                    "Bid"
                } else {
                    "Ask"
                })),
            ),
            (
                "orderType",
                Field::Text(u(if api_market { "Market" } else { "Limit" })),
            ),
        ];
        if let Some(id) = caller {
            req.push((
                "clientId",
                Field::Number(safe_cex_num::number_from_string(&String::from_utf16_lossy(
                    &id,
                ))),
            ));
        }
        if !api_market {
            req.push((
                "timeInForce",
                Field::Text(u(if post {
                    "GTC"
                } else {
                    crate::order_utils::map_time_in_force(opts.time_in_force)
                })),
            ));
            req.push(("postOnly", Field::Bool(post)));
        }
        if trigger {
            if let Some(n) = opts.trigger_price.or(opts.price).filter(|n| truth(*n)) {
                req.push((
                    "triggerPrice",
                    Field::Text(format(&mut *h, &market, n, true)?),
                ));
            }
            req.push((
                "triggerQuantity",
                Field::Text(format(&mut *h, &market, opts.amount, false)?),
            ));
            let tp = opts.r#type == OrderType::TakeProfit;
            req.push((
                "triggerBy",
                Field::Text(h.trigger_by(
                    tp,
                    if tp {
                        opts.tp_trigger_type
                    } else {
                        opts.sl_trigger_type
                    },
                )?),
            ));
            if stop {
                req.push(("reduceOnly", Field::Bool(true)));
            }
        } else {
            req.push((
                "quantity",
                Field::Text(format(&mut *h, &market, opts.amount, false)?),
            ));
        }
        if limit {
            if let Some(n) = opts.price.filter(|n| truth(*n)) {
                req.push(("price", Field::Text(format(&mut *h, &market, n, true)?)));
            }
        }
        if opts.reduce_only == true && !stop {
            req.push(("reduceOnly", Field::Bool(true)));
        }
        let separate = limit && attached;
        if !separate {
            for (tp, n) in [(true, opts.take_profit), (false, opts.stop_loss)] {
                if let Some(n) = n {
                    let (prefix, explicit) = if tp {
                        ("takeProfit", opts.tp_trigger_type)
                    } else {
                        ("stopLoss", opts.sl_trigger_type)
                    };
                    let (key1, key2, key3) = if prefix == "takeProfit" {
                        (
                            "takeProfitTriggerPrice",
                            "takeProfitLimitPrice",
                            "takeProfitTriggerBy",
                        )
                    } else {
                        (
                            "stopLossTriggerPrice",
                            "stopLossLimitPrice",
                            "stopLossTriggerBy",
                        )
                    };
                    req.push((key1, Field::Text(format(&mut *h, &market, n, true)?)));
                    req.push((key2, Field::Text(format(&mut *h, &market, n, true)?)));
                    req.push((key3, Field::Text(h.trigger_by(tp, explicit)?)));
                }
            }
        }
        let response = h.post("/api/v1/order", req.clone())?;
        Ok::<_, H::Error>(Plan::<H> {
            market,
            separate,
            response,
        })
    })();
    let plan = match prepared {
        Ok(v) => v,
        Err(e) => {
            return Box::pin(std::future::ready(caught(
                &mut *host.borrow_mut(),
                e,
                &u("Error placing order: "),
            )));
        }
    };
    Box::pin(async move {
        let result = async {
            let Plan {
                market,
                separate,
                response,
            } = plan;
            let mut ids = vec![response.await?];
            if separate {
                for (tp, n) in [(true, opts.take_profit), (false, opts.stop_loss)] {
                    if let Some(n) = n {
                        let step = async {
                            let response = {
                                let mut h = host.borrow_mut();
                                let request = vec![
                                    ("symbol", Field::Text(h.market_id(&market)?)),
                                    (
                                        "side",
                                        Field::Text(u(if opts.side == OrderSide::Buy {
                                            "Ask"
                                        } else {
                                            "Bid"
                                        })),
                                    ),
                                    ("orderType", Field::Text(u("Market"))),
                                    (
                                        "triggerPrice",
                                        Field::Text(format(&mut *h, &market, n, true)?),
                                    ),
                                    (
                                        "triggerQuantity",
                                        Field::Text(format(&mut *h, &market, opts.amount, false)?),
                                    ),
                                    (
                                        "triggerBy",
                                        Field::Text(h.trigger_by(
                                            tp,
                                            if tp {
                                                opts.tp_trigger_type
                                            } else {
                                                opts.sl_trigger_type
                                            },
                                        )?),
                                    ),
                                    ("reduceOnly", Field::Bool(true)),
                                ];
                                h.post("/api/v1/order", request)?
                            };
                            response.await
                        }
                        .await;
                        match step {
                            Ok(id) => ids.push(id),
                            Err(e) => warning(
                                &mut *host.borrow_mut(),
                                if tp {
                                    "Error placing TP order: "
                                } else {
                                    "Error placing SL order: "
                                },
                                &e,
                            )?,
                        }
                    }
                }
            }
            Ok(ids)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => caught(&mut *host.borrow_mut(), e, &u("Error placing order: ")),
        }
    })
}
#[derive(Clone, Debug)]
pub struct Update {
    pub id: JsString,
    pub amount: Option<Option<f64>>,
    pub price: Option<Option<f64>>,
}
// ts: BackpackExchange.updateOrder
pub fn update_order<H: Host>(
    host: Rc<RefCell<H>>,
    update: Update,
) -> Task<Vec<JsString>, H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        let old = h
            .find_order(&update.id)?
            .ok_or_else(|| h.error(join(&[&u("Order not found: "), &update.id]), false))?;
        let opts = h.order_options(&old)?;
        let stop = matches!(
            opts.r#type.into_value(),
            Some(OrderType::StopLoss | OrderType::TakeProfit)
        );
        let amount = update.amount.unwrap_or(Some(opts.amount));
        let price = update.price.unwrap_or(opts.price);
        if amount.is_none_or(|n| !truth(n) || n <= 0.0) {
            let text = amount.map_or_else(|| "undefined".into(), safe_cex_num::js_number_string);
            let original_amount = h.order_options(&old)?.amount;
            return Err(h.error(
                join(&[
                    &u("Invalid order amount: "),
                    &u(&text),
                    &u(". Original amount: "),
                    &u(&safe_cex_num::js_number_string(original_amount)),
                ]),
                false,
            ));
        }
        let target = h.log_target()?;
        let id = h.order_id(&old)?;
        let now = h.order_options(&old)?;
        let kind = match now.r#type.into_value() {
            Some(OrderType::Market) => "market",
            Some(OrderType::Limit) => "limit",
            Some(OrderType::StopLoss) => "stop_market",
            Some(OrderType::TakeProfit) => "take_profit_market",
            Some(OrderType::Conditional) => "conditional",
            Some(OrderType::TrailingStopLoss) => "trailing_stop_market",
            None => "undefined",
        };
        let text = join(&[
            &u("Updating order "),
            &id,
            &u(": type="),
            &u(kind),
            &u(", amount="),
            &u(&safe_cex_num::js_number_string(now.amount)),
            &u("->"),
            &u(&safe_cex_num::js_number_string(amount.unwrap())),
            &u(", price="),
            &u(&now
                .price
                .map_or_else(|| "undefined".into(), safe_cex_num::js_number_string)),
            &u("->"),
            &u(&price.map_or_else(|| "undefined".into(), safe_cex_num::js_number_string)),
        ]);
        h.log(target, text, Severity::Info)?;
        let cancel = h.cancel(old.clone())?;
        Ok::<_, H::Error>((old, stop, amount.unwrap(), price, cancel))
    })();
    Box::pin(async move {
        let (old, stop, amount, price, cancel) = prepared?;
        cancel.await?;
        let options = {
            let mut h = host.borrow_mut();
            let o = h.order_options(&old)?;
            Options {
                symbol: o.symbol,
                r#type: o.r#type,
                side: o.side,
                amount,
                price,
                time_in_force: o.time_in_force,
                reduce_only: if stop {
                    true.into()
                } else {
                    o.reduce_only.into_property()
                },
                trigger_price: None,
                take_profit: None,
                stop_loss: None,
                tp_trigger_type: None,
                sl_trigger_type: None,
                client_order_id: None,
                ..Default::default()
            }
        };
        let result = async {
            let response = host.borrow_mut().place(options)?;
            let ids = response.await?;
            let mut h = host.borrow_mut();
            let target = h.log_target()?;
            let id = h.order_id(&old)?;
            let joined = ids
                .iter()
                .enumerate()
                .flat_map(|(i, s)| {
                    if i == 0 {
                        s.clone()
                    } else {
                        join(&[&u(", "), s])
                    }
                })
                .collect::<JsString>();
            h.log(
                target,
                join(&[&u("Order updated successfully: "), &id, &u(" -> "), &joined]),
                Severity::Info,
            )?;
            Ok(ids)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                let target = h.log_target()?;
                let id = h.order_id(&old)?;
                let text = h.error_text(&e)?;
                h.log(
                    target,
                    join(&[
                        &u("Failed to place replacement order after cancelling "),
                        &id,
                        &u(": "),
                        &text,
                    ]),
                    Severity::Error,
                )?;
                Err(e)
            }
        }
    })
}
