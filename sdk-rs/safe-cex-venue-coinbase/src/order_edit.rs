//! Coinbase native-edit-or-fail-closed body. Order/market roots remain live across
//! imported cancellation, HTTP, logging and reconciliation operations.
use crate::{
    decimal::{is_coinbase_unsigned_decimal, is_coinbase_zero_decimal},
    market_position_reads::ReadMarket,
    order_sync::{SyncedOrderRoot, SyncedOrders},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    types::EditOrderRequest,
    utils::normalize_coinbase_usd_equivalent_product_id,
};
use safe_cex_num::{adjust, js_number_string, js_string, js_truthy};
use safe_cex_types::{OrderSide, OrderStatus, OrderType};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
pub type EditAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
#[derive(Clone, Default)]
pub struct EditUpdate {
    pub amount: Option<f64>,
    pub price: Option<f64>,
    pub has_amount: bool,
    pub has_price: bool,
}
pub type EditUpdateRoot = Rc<RefCell<EditUpdate>>;
pub type EditReplacement =
    safe_cex_types::PreparedOrderOpts<String, Option<f64>, safe_cex_types::ModelField<bool>>;
pub trait OrderEditHost {
    fn selected_portfolio_type(&self) -> Result<Option<String>, PortfolioFailure>;
    fn orders(&self) -> Result<SyncedOrders, PortfolioFailure>;
    fn market_by_symbol(&self, symbol: String) -> Result<Option<ReadMarket>, PortfolioFailure>;
    fn cancel_orders(&self, orders: SyncedOrders) -> EditAsync<()>;
    fn place_order(&self, order: EditReplacement) -> EditAsync<Vec<String>>;
    fn post_edit(&self, endpoint: &'static str, payload: EditOrderRequest) -> EditAsync<Value>;
    fn schedule_reconciliation(&self, reason: &'static str) -> Result<(), PortfolioFailure>;
    fn schedule_write_check(&self) -> Result<(), PortfolioFailure>;
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure>;
}
pub struct OrderEditReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn error(message: String) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":"Error","message":message}),
    )))
}
fn coercion_error(e: safe_cex_num::JsCoercionError) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name": e.name, "message": e.message}),
    )))
}
fn reject(
    host: &impl OrderEditHost,
    message: impl Into<String>,
) -> Result<Vec<String>, PortfolioFailure> {
    let message = message.into();
    host.emit_and_log_error(Some(Value::from(message.clone())))?;
    Err(error(message))
}
// Compare decimal values exactly, without converting the raw Coinbase string to
// f64. Scientific notation occurs only in String(the already-native JS number).
fn decimal_key(text: &str) -> Option<(String, i64)> {
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map(|(a, b)| Some((a, b.parse::<i64>().ok()?)))
        .unwrap_or(Some((text, 0)))?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if !whole
        .bytes()
        .chain(fraction.bytes())
        .all(|b| b.is_ascii_digit())
        || whole.is_empty()
    {
        return None;
    }
    let digits = format!("{whole}{fraction}");
    let mut digits = digits.trim_start_matches('0').to_owned();
    if digits.is_empty() {
        return Some(("0".into(), 0));
    }
    let mut power = exponent - fraction.len() as i64;
    while digits.ends_with('0') {
        digits.pop();
        power += 1
    }
    Some((digits, power))
}
fn exact_decimal(raw: Option<&Value>, number: f64) -> bool {
    let Some(raw) = raw.and_then(Value::as_str).and_then(decimal_key) else {
        return false;
    };
    Some(raw) == decimal_key(&js_number_string(number))
}
fn positive_decimal(raw: Option<&Value>) -> bool {
    is_coinbase_unsigned_decimal(raw) && !is_coinbase_zero_decimal(raw)
}
fn prop<'a>(v: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    v.and_then(|v| v.get(key))
}
fn nullish<'a>(a: Option<&'a Value>, b: impl FnOnce() -> Option<&'a Value>) -> Option<&'a Value> {
    if a.is_none_or(Value::is_null) { b() } else { a }
}
fn truthy_or<'a>(a: Option<&'a Value>, b: impl FnOnce() -> Option<&'a Value>) -> Option<&'a Value> {
    if js_truthy(a) { a } else { b() }
}
pub fn update_order<H: OrderEditHost + 'static>(
    r: Rc<OrderEditReceiver<H>>,
    order: SyncedOrderRoot,
    update: EditUpdateRoot,
) -> Promise<Result<Vec<String>, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            if r.host.selected_portfolio_type()?.as_deref() == Some("INTX") {
                let mut new_order = {
                    let o = order.borrow();
                    EditReplacement {
                        symbol: o.fields.symbol.clone(),
                        r#type: o.fields.r#type.into_property(),
                        side: o.fields.side.into_property(),
                        price: o.fields.price.into_value(),
                        amount: Some(o.fields.amount),
                        reduce_only: o.fields.reduce_only.into_property(),
                        ..Default::default()
                    }
                };
                {
                    let u = update.borrow();
                    if u.has_price {
                        new_order.price = u.price
                    }
                    if u.has_amount {
                        new_order.amount = u.amount
                    }
                }
                let orders = Rc::new(RefCell::new(vec![order]));
                r.host.cancel_orders(orders)?.await?;
                return r.host.place_order(new_order)?.await;
            }
            let stored = r.host.orders()?;
            let mut canonical = None;
            let len = stored.borrow().len();
            for i in 0..len {
                let candidate = stored.borrow().get(i).cloned();
                if let Some(candidate) = candidate {
                    if candidate.borrow().fields.id == order.borrow().fields.id {
                        canonical = Some(candidate);
                        break;
                    }
                }
            }
            let canonical = match canonical {
                Some(c) if c.borrow().fields.symbol == order.borrow().fields.symbol => c,
                _ => {
                    let message = format!(
                        "Coinbase cannot safely edit {}: canonical open order is not reconciled",
                        order.borrow().fields.id
                    );
                    return reject(&r.host, message);
                }
            };
            let market_symbol = canonical.borrow().fields.symbol.clone();
            let market = r.host.market_by_symbol(market_symbol)?;
            let Some(market) = market else {
                let message = format!(
                    "Coinbase cannot safely edit {}: market {} is not reconciled",
                    order.borrow().fields.id,
                    canonical.borrow().fields.symbol
                );
                return reject(&r.host, message);
            };
            let proven = {
                let m = market.borrow();
                let info = m.info.as_ref();
                m.is_spot == Some(true)
                    && m.is_expiring != Some(true)
                    && m.contract_size == Some(1.0)
                    && prop(info, "product_type") == Some(&json!("SPOT"))
                    && prop(info, "future_product_details").is_none()
            };
            if !proven {
                let unit = {
                    let m = market.borrow();
                    if m.is_expiring.unwrap_or(false)
                        || prop(m.info.as_ref(), "product_type") == Some(&json!("FUTURE"))
                    {
                        "CFM contract"
                    } else {
                        "unknown"
                    }
                };
                let message = format!(
                    "Coinbase cannot safely edit {}: {unit} size units are not proven for native edit",
                    order.borrow().fields.id
                );
                return reject(&r.host, message);
            }
            let info = canonical.borrow().info.clone();
            // The Ref over `info` is confined to this block. The exchange-core
            // contract is that no RefCell borrow crosses an await, and the
            // post_edit call below is one; two explicit drop(raw) calls used to
            // stand in for that and still left the Ref in scope at the await.
            // Everything read from the raw venue payload is consumed in here, so
            // only the three decided values leave.
            let (reconciled, current_amount, current_price) = {
                let raw = info.borrow();
                let config = prop(Some(&raw), "order_configuration");
                let limit = truthy_or(prop(config, "limit_limit_gtc"), || {
                    truthy_or(prop(config, "limit_limit_gtd"), || {
                        truthy_or(prop(config, "limit_limit_fok"), || {
                            prop(config, "sor_limit_ioc")
                        })
                    })
                });
                let raw_id = raw.get("order_id");
                let product = raw.get("product_id");
                let status = raw.get("status");
                let kind = raw.get("order_type");
                let side = nullish(raw.get("order_side"), || raw.get("side"));
                let zero = Value::from("");
                let stop = nullish(raw.get("stop_price"), || Some(&zero));
                let expected_side = if canonical.borrow().fields.side == OrderSide::Buy {
                    "BUY"
                } else {
                    "SELL"
                };
                let no_trigger = stop
                    .and_then(Value::as_str)
                    .is_some_and(|v| v.is_empty() || is_coinbase_zero_decimal(stop));
                let product_matches = product.and_then(Value::as_str).is_some_and(|p| {
                    p == market.borrow().id
                        || normalize_coinbase_usd_equivalent_product_id(p)
                            == normalize_coinbase_usd_equivalent_product_id(&market.borrow().id)
                });
                let allowed = {
                    let c = canonical.borrow();
                    let o = &c.fields;
                    o.status == OrderStatus::Open
                        && o.r#type == OrderType::Limit
                        && raw_id.and_then(Value::as_str) == Some(o.id.as_str())
                        && product_matches
                        && status.and_then(Value::as_str) == Some("OPEN")
                        && matches!(kind.and_then(Value::as_str), Some("Limit" | "LIMIT"))
                        && side.and_then(Value::as_str) == Some(expected_side)
                        && no_trigger
                        && o.take_profit.is_none()
                        && o.stop_loss.is_none()
                        && o.close != Some(true)
                        && o.reduce_only != true
                };
                if !allowed {
                    return reject(
                        &r.host,
                        "Coinbase native edit supports only exactly reconciled OPEN zero-fill ordinary spot LIMIT orders",
                    );
                }
                let filled = nullish(raw.get("cumulative_quantity"), || raw.get("filled_size"));
                let remaining = nullish(raw.get("leaves_quantity"), || prop(limit, "base_size"));
                let price = nullish(raw.get("limit_price"), || prop(limit, "limit_price"));
                let current_amount = adjust(
                    canonical.borrow().fields.amount,
                    market.borrow().precision.amount,
                );
                let current_remaining = adjust(
                    canonical.borrow().fields.remaining,
                    market.borrow().precision.amount,
                );
                let current_price = adjust(
                    (canonical.borrow().fields.price).to_number(),
                    market.borrow().precision.price,
                );
                let reconciled = is_coinbase_zero_decimal(filled)
                    && positive_decimal(remaining)
                    && positive_decimal(price)
                    && canonical.borrow().fields.filled == 0.0
                    && current_amount == canonical.borrow().fields.amount
                    && current_remaining == canonical.borrow().fields.remaining
                    && current_price == canonical.borrow().fields.price
                    && exact_decimal(remaining, current_amount)
                    && current_remaining == current_amount
                    && exact_decimal(price, current_price);
                (reconciled, current_amount, current_price)
            };
            if !reconciled {
                let message = format!(
                    "Coinbase cannot safely edit {}: full current spot price and zero-fill base size are not reconciled",
                    order.borrow().fields.id
                );
                return reject(&r.host, message);
            }
            let (has_amount, has_price, amount, price) = {
                let u = update.borrow();
                (u.has_amount, u.has_price, u.amount, u.price)
            };
            if has_amount == has_price {
                return reject(
                    &r.host,
                    "Coinbase native edit requires exactly one amount or price update",
                );
            }
            let amended_amount = adjust(
                if has_amount {
                    amount.unwrap_or(f64::NAN)
                } else {
                    current_amount
                },
                market.borrow().precision.amount,
            );
            let amended_price = adjust(
                if has_price {
                    price.unwrap_or(f64::NAN)
                } else {
                    current_price
                },
                market.borrow().precision.price,
            );
            if !amended_amount.is_finite()
                || amended_amount <= 0.0
                || amended_amount < market.borrow().limits.amount.min
                || amended_amount > market.borrow().limits.amount.max
            {
                return reject(
                    &r.host,
                    "Coinbase amended spot size is outside the reconciled market limits",
                );
            }
            if !amended_price.is_finite() || amended_price <= 0.0 {
                return reject(
                    &r.host,
                    "Coinbase amended spot price must be greater than zero",
                );
            }
            if amended_amount == current_amount && amended_price == current_price {
                return reject(
                    &r.host,
                    "Coinbase native edit must change price or size after market-precision normalization",
                );
            }
            let payload = EditOrderRequest {
                order_id: canonical.borrow().fields.id.clone(),
                price: js_number_string(amended_price),
                size: js_number_string(amended_amount),
            };
            let result = async {
                let data = r.host.post_edit("/api/v3/brokerage/orders/edit", payload)?.await?;
                let errors = data.get("errors").and_then(Value::as_array);
                if data.get("success") != Some(&Value::Bool(true))
                    || errors.is_none()
                    || errors.is_some_and(|e| !e.is_empty())
                {
                    let mut reasons = Vec::new();
                    if let Some(errors) = errors {
                        for item in errors {
                            if item.is_null() {
                                return Err(Rc::new(PortfolioThrown::from_value(Some(json!({"name":"TypeError", "message":"Cannot destructure property 'edit_failure_reason' of 'object null' as it is null."})))));
                            }
                            for key in ["edit_failure_reason", "preview_failure_reason"] {
                                if let Some(value) = item.get(key).filter(|v| js_truthy(Some(v))) {
                                    reasons.push(js_string(Some(value)).map_err(coercion_error)?);
                                }
                            }
                        }
                    }
                    let joined = reasons.join(", ");
                    let message = if !joined.is_empty() {
                        joined
                    } else if errors.is_none() {
                        "Coinbase edit-order response did not include the documented errors array".into()
                    } else {
                        "Coinbase edit order failed".into()
                    };
                    return Err(error(message));
                }
                r.host.schedule_reconciliation("edit-order")?;
                r.host.schedule_write_check()?;
                Ok(vec![canonical.borrow().fields.id.clone()])
            }.await;
            match result {
                Ok(v) => Ok(v),
                Err(e) => {
                    let v = e.value.as_ref();
                    let msg = truthy_or(prop(prop(prop(v, "response"), "data"), "message"), || {
                        truthy_or(prop(v, "message"), || None)
                    })
                    .cloned();
                    let message = match msg {
                        Some(v) => Some(v),
                        None => Some(Value::from(js_string(v).map_err(coercion_error)?)),
                    };
                    r.host.emit_and_log_error(message)?;
                    Err(e)
                }
            }
        }),
        &spawn,
    )
}
