//! Market-shaped order policy; arithmetic stays in `order`.
use crate::{handle_rounding_error, js_number_string, math_min, math_round, order};
use safe_cex_types::{
    Market, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, Position, PositionMode,
    PositionSide, Ticker,
};
use std::borrow::{Borrow, BorrowMut};

#[path = "order_metadata.rs"]
mod metadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptValidationError {
    pub name: &'static str,
    pub message: String,
}
impl ScriptValidationError {
    pub fn new(message: String) -> Self {
        Self {
            name: "ScriptValidationError",
            message,
        }
    }
}
impl std::fmt::Display for ScriptValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for ScriptValidationError {}

fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn nearest(value: f64, step: f64) -> f64 {
    handle_rounding_error(math_round(value / step) * step)
}
fn nearest_down(value: f64, step: f64) -> f64 {
    let quotient = value / step;
    let rounded = math_round(quotient);
    let exact = (quotient - rounded).abs() <= crate::math_max(1.0, quotient.abs()) * 1e-12;
    handle_rounding_error(if exact { rounded } else { quotient.floor() } * step)
}
fn amount_step(market: &Market) -> f64 {
    if truthy(market.precision.amount) {
        market.precision.amount
    } else {
        1e-8
    }
}
pub fn round_order_price(raw: f64, market: &Market) -> f64 {
    nearest(
        raw,
        if truthy(market.precision.price) {
            market.precision.price
        } else {
            0.01
        },
    )
}
pub fn round_order_size(raw: f64, market: &Market, exchange_name: Option<&str>) -> f64 {
    if order::should_round_size_down(market, exchange_name) {
        nearest_down(raw, amount_step(market))
    } else {
        nearest(raw, amount_step(market))
    }
}
pub fn round_order_size_to_nearest(raw: f64, market: &Market) -> f64 {
    nearest(raw, amount_step(market))
}
pub fn round_market_close_size(raw: f64, market: &Market) -> f64 {
    order::round_close_size(raw, amount_step(market))
}
pub fn round_market_order_size(raw: f64, market: &Market, reduce_only: bool) -> f64 {
    order::round_market_order_size(raw, amount_step(market), reduce_only)
}
pub fn is_stop_like_order_type(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}

pub fn venue_supports_infinity_stops(
    exchange_name: &str,
) -> Result<Option<bool>, ScriptValidationError> {
    if let Some((_, supported, _)) = metadata::ORDER_METADATA
        .iter()
        .find(|(name, _, _)| *name == exchange_name)
    {
        return Ok(Some(*supported));
    }
    // These ordinary Object.prototype entries are truthy metadata objects/functions
    // with no supportsInfinityStops field: the source returns undefined.
    if matches!(
        exchange_name,
        "constructor"
            | "__proto__"
            | "__defineGetter__"
            | "__defineSetter__"
            | "hasOwnProperty"
            | "__lookupGetter__"
            | "__lookupSetter__"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "toLocaleString"
            | "toString"
            | "valueOf"
    ) {
        return Ok(None);
    }
    Err(ScriptValidationError {
        name: "Error",
        message: format!("Unknown exchange metadata: {exchange_name}"),
    })
}
pub fn venue_auto_downsizes_reduce_orders(exchange_name: &str) -> bool {
    metadata::ORDER_METADATA
        .iter()
        .find(|(name, _, _)| *name == exchange_name)
        .is_some_and(|(_, _, downsize)| *downsize)
}

pub fn needs_fractional_lot_base_conversion(market: &Market) -> bool {
    let contract_size = market.contract_size.filter(|n| truthy(*n)).unwrap_or(1.0);
    market.is_inverse != Some(true) && contract_size > 0.0 && contract_size < 1.0
}

pub fn down_size_reduce_order<T: Borrow<PlaceOrderOpts> + BorrowMut<PlaceOrderOpts>>(
    market: &Market,
    mut input: T,
    positions: &[Position],
    orders: &[Order],
    price: f64,
    log: &mut dyn FnMut(&str),
) -> T {
    let order = input.borrow();
    if order.reduce_only != Some(true) || !truthy(price) {
        return input;
    }
    let side = if order.side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    let found: Vec<_> = positions
        .iter()
        .filter(|p| p.symbol == order.symbol && p.contracts > 0.0 && p.side == side)
        .collect();
    let Some(position) = found.first() else {
        return input;
    };
    if found.len() > 1 {
        log("autoDownSizeReduceOrders: found multiple positions, using first");
    }
    let fractional = needs_fractional_lot_base_conversion(market);
    let contract_size = market.contract_size.filter(|n| truthy(*n)).unwrap_or(1.0);
    let order_amount_base = if fractional {
        order.amount * contract_size
    } else {
        order.amount
    };
    let mut reduced_size = math_min(order_amount_base, position.contracts);
    for ahead in orders.iter().filter(|o| {
        o.symbol == order.symbol
            && o.side == order.side
            && o.r#type == OrderType::Limit
            && matches!(
                o.status,
                safe_cex_types::ModelField::Value(OrderStatus::Open | OrderStatus::Creating)
            )
            && (position.position_mode != Some(PositionMode::Hedged) || o.reduce_only == true)
            && ((order.side == OrderSide::Buy && o.price.to_number() >= price)
                || (order.side == OrderSide::Sell && o.price.to_number() <= price))
    }) {
        reduced_size -= ahead.amount;
    }
    if reduced_size <= 0.0 {
        return input;
    }
    let rounded = nearest(reduced_size, amount_step(market));
    input.borrow_mut().amount = if fractional && contract_size > 0.0 {
        rounded / contract_size
    } else {
        rounded
    };
    input
}

#[derive(Debug, Clone, Copy)]
pub struct OrderPolicyContext<'a> {
    pub market: &'a Market,
    pub exchange_name: Option<&'a str>,
}
#[derive(Debug, Clone, Copy)]
pub struct ReduceOnlyClampContext<'a> {
    pub policy: OrderPolicyContext<'a>,
    pub positions: &'a [Position],
    pub resting_orders: &'a [Order],
    pub ticker: Option<&'a Ticker>,
}

pub fn prepare_order<T: Borrow<PlaceOrderOpts> + BorrowMut<PlaceOrderOpts> + Clone>(
    input: &T,
    context: OrderPolicyContext<'_>,
) -> Result<T, ScriptValidationError> {
    let opts = input.borrow();
    let amount = if opts.reduce_only == Some(true) && opts.r#type == OrderType::Market {
        round_market_order_size(opts.amount, context.market, true)
    } else {
        round_order_size(opts.amount, context.market, context.exchange_name)
    };
    let infinity_stop = opts.amount == 0.0 && is_stop_like_order_type(opts.r#type);
    if infinity_stop {
        let name = context.exchange_name.filter(|n| !n.is_empty());
        if match name {
            Some(name) => venue_supports_infinity_stops(name)? != Some(true),
            None => true,
        } {
            return Err(ScriptValidationError::new(format!(
                "'{}' does not support Full (Infinity) TP/SL orders",
                context.exchange_name.unwrap_or("this venue")
            )));
        }
    } else if !amount.is_finite() || amount <= 0.0 {
        return Err(ScriptValidationError::new(format!(
            "order size {} rounds to {} for '{}' — below the venue lot step",
            js_number_string(opts.amount),
            js_number_string(amount),
            opts.symbol
        )));
    }
    let symbol = opts.symbol.clone();
    let mut result = input.clone();
    let out = result.borrow_mut();
    out.amount = amount;
    // Preserve source field order and every venue extension on T.
    for (field, label) in [
        (&mut out.price, "price"),
        (&mut out.stop_loss, "stopLoss"),
        (&mut out.take_profit, "takeProfit"),
        (&mut out.trigger_price, "triggerPrice"),
    ] {
        if let Some(value) = *field {
            let rounded = round_order_price(value, context.market);
            if !rounded.is_finite() || rounded <= 0.0 {
                return Err(ScriptValidationError::new(format!(
                    "invalid {label} for '{symbol}': {}",
                    js_number_string(value)
                )));
            }
            *field = Some(rounded);
        }
    }
    Ok(result)
}

pub fn clamp_reduce_only_order<T: Borrow<PlaceOrderOpts> + BorrowMut<PlaceOrderOpts>>(
    input: T,
    context: ReduceOnlyClampContext<'_>,
    log: &mut dyn FnMut(&str),
) -> T {
    let order = input.borrow();
    let Some(name) = context.policy.exchange_name.filter(|n| !n.is_empty()) else {
        return input;
    };
    if order.reduce_only != Some(true) || !venue_auto_downsizes_reduce_orders(name) {
        return input;
    }
    let price = order
        .price
        .filter(|n| truthy(*n))
        .or_else(|| context.ticker.map(|t| t.mark).filter(|n| truthy(*n)))
        .or_else(|| context.ticker.map(|t| t.last).filter(|n| truthy(*n)))
        .unwrap_or(0.0);
    if !truthy(price) {
        return input;
    }
    down_size_reduce_order(
        context.policy.market,
        input,
        context.positions,
        context.resting_orders,
        price,
        log,
    )
}
