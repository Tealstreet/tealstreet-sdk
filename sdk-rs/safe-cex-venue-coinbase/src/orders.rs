//! REST-order normalization from coinbase.orders.ts.
use crate::utils::normalize_cfm_contract_size;
use safe_cex_num::{add, divide, multiply, number_from_string, subtract};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Market, Order, OrderExtra, OrderSide, OrderStatus, OrderType};

struct ResolvedOrderConfig<'a> {
    kind: OrderType,
    base_size: Option<&'a Input>,
    quote_size: Option<&'a Input>,
    price: Option<&'a Input>,
}

// ts: parseUnsignedDecimal
fn parse_unsigned_decimal(value: Option<&Input>) -> Option<f64> {
    let value = String::from_utf16(value?.as_text()?).ok()?;
    if value.is_empty() {
        return None;
    }
    let parsed = number_from_string(&value);
    (parsed.is_finite() && parsed >= 0.0).then_some(parsed)
}

// ts: resolveOrderConfiguration
fn resolve_order_configuration(order: &Input) -> Option<ResolvedOrderConfig<'_>> {
    let config = order.get("order_configuration")?;
    let first = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| config.get(name).filter(|v| v.is_truthy()))
    };
    if let Some(market) = first(&["market_market_ioc", "market_market_fok"]) {
        return Some(ResolvedOrderConfig {
            kind: OrderType::Market,
            base_size: market.get("base_size"),
            quote_size: market.get("quote_size"),
            price: order.get("average_filled_price"),
        });
    }
    if let Some(limit) = first(&[
        "limit_limit_gtc",
        "limit_limit_gtd",
        "limit_limit_fok",
        "sor_limit_ioc",
    ]) {
        return Some(ResolvedOrderConfig {
            kind: OrderType::Limit,
            base_size: limit.get("base_size"),
            quote_size: limit.get("quote_size"),
            price: limit.get("limit_price"),
        });
    }
    if let Some(stop_limit) = first(&["stop_limit_stop_limit_gtc", "stop_limit_stop_limit_gtd"]) {
        return Some(ResolvedOrderConfig {
            kind: OrderType::StopLoss,
            base_size: stop_limit.get("base_size"),
            quote_size: None,
            price: stop_limit
                .get("stop_price")
                .filter(|v| v.is_truthy())
                .or_else(|| stop_limit.get("limit_price")),
        });
    }
    None
}

// ts: resolveAmountNative
fn resolve_amount_native(config: &ResolvedOrderConfig<'_>) -> Option<f64> {
    if let Some(amount) = parse_unsigned_decimal(config.base_size) {
        return Some(amount);
    }
    let quote_amount = parse_unsigned_decimal(config.quote_size)?;
    let order_price = parse_unsigned_decimal(config.price)?;
    if order_price == 0.0 {
        return None;
    }
    Some(divide(quote_amount, order_price, None))
}

// ts: mapCoinbaseRestOrder
// ts-bug: mapCoinbaseRestOrder reads inherited ORDER_STATUS object members.
// CORRECTED: OrderStatus admits actual statuses, never a function/prototype object.
pub fn map_coinbase_rest_order(
    raw_order: &Input,
    market: &Market,
    last_update: f64,
) -> Option<Order> {
    let resolved = resolve_order_configuration(raw_order)?;
    let amount_native = resolve_amount_native(&resolved)?;
    let filled_native = parse_unsigned_decimal(raw_order.get("filled_size")).unwrap_or(0.0);
    if amount_native <= 0.0 || filled_native > amount_native {
        return None;
    }
    let contract_size = if market.is_expiring.unwrap_or(false) {
        normalize_cfm_contract_size(&Input::Float(market.contract_size?))?
    } else {
        1.0
    };
    let status = match raw_order
        .get("status")?
        .as_text()
        .and_then(|v| String::from_utf16(v).ok())?
        .as_str()
    {
        "PENDING" | "OPEN" | "QUEUED" | "CANCEL_QUEUED" | "EDIT_QUEUED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELLED" | "EXPIRED" | "FAILED" => OrderStatus::Canceled,
        _ => return None,
    };
    // ts-bug: mapCoinbaseRestOrder retains NaN/Infinity produced from finite sizes by safe-math.
    let remaining_native = subtract(amount_native, filled_native);
    let filled = multiply(filled_native, contract_size);
    let remaining = multiply(remaining_native, contract_size);
    Some(Order {
        client_order_id: (None).into(),
        id: raw_order
            .get("order_id")?
            .as_text()
            .and_then(|v| String::from_utf16(v).ok())?
            .as_str()
            .to_owned(),
        parent_id: match raw_order.get("client_order_id") {
            None | Some(Input::Null | Input::Undefined) => None,
            Some(id) => Some(
                id.as_text()
                    .and_then(|v| String::from_utf16(v).ok())?
                    .as_str()
                    .to_owned(),
            ),
        }
        .into(),
        status: status.into(),
        symbol: market.symbol.clone(),
        r#type: (resolved.kind).into(),
        side: if raw_order.get("side").is_some_and(|v| v.text_equals("BUY")) {
            safe_cex_types::ModelField::Value(OrderSide::Buy)
        } else {
            safe_cex_types::ModelField::Value(OrderSide::Sell)
        },
        price: (parse_unsigned_decimal(resolved.price).unwrap_or(0.0)).into(),
        amount: add(filled, remaining),
        filled,
        remaining,
        extra: (Some(OrderExtra {
            remaining_native: Some(remaining_native),
            filled_in_placement_units: None,
            remaining_notional: None,
            conditional_order_price: None,
            reject_reason: None,
            is_from_chaser: None,
            chaser_id: None,
            chaser_attempts: None,
            chaser_max_attempts: None,
        }))
        .into(),
        reduce_only: false.into(),
        info: (Some(raw_order.clone().into())).into(),
        last_update,
        close: (None).into(),
        time_in_force: (None).into(),
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
    })
}
