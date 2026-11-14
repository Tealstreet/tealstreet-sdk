//! Coinbase's technical guide specifies base-coin amount/filled_amount for
//! linear USDC perpetuals; inherited inverse-futures descriptions do not apply.
use crate::drb;
use safe_cex_engine::model::*;
use serde_json::{Value, json};

// Validate through S0's closed instrument mapping, but publish S1's native
// symbol so private rows join the public catalogue without a second alias.
fn instrument_symbol(value: &str) -> Result<String, String> {
    drb::from_instrument(value)?;
    Ok(value.to_owned())
}

pub(super) fn number(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("drb invalid numeric {key}"))
}
fn unsigned(v: &Value, key: &str) -> Result<f64, String> {
    number(v, key).and_then(|n| {
        if n >= 0. {
            Ok(n)
        } else {
            Err(format!("drb negative {key}"))
        }
    })
}
pub(super) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("drb missing {key}"))
}
pub(super) fn identity(v: &Value, key: &str) -> Result<String, String> {
    let s = text(v, key)?;
    if s.is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        return Err(format!("drb invalid {key}"));
    }
    Ok(s.to_owned())
}
pub(super) fn timestamp(v: &Value, key: &str) -> Result<f64, String> {
    unsigned(v, key).and_then(|n| {
        if n.fract() == 0. && n <= 253402300799999. {
            Ok(n)
        } else {
            Err(format!("drb invalid {key}"))
        }
    })
}
fn flag(v: &Value, key: &str) -> Result<bool, String> {
    match v.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(format!("drb invalid {key}")),
    }
}
#[derive(Clone, Copy)]
pub(super) struct OrderVersion {
    millis: f64,
    pub(super) causal: Option<u64>,
}
impl OrderVersion {
    pub(super) fn supersedes(self, previous: Self) -> bool {
        // Without both causal stamps, equal milliseconds follow socket order;
        // the caller filters economic duplicates against the current store row.
        self.millis >= previous.millis
            && match (self.causal, previous.causal) {
                (Some(next), Some(old)) => next > old,
                _ => true,
            }
    }
}
pub(super) fn versioned_order(v: Value) -> Result<(Order, OrderVersion), String> {
    // Causal nanoseconds exceed f64's exact integer range; never round them.
    let causal = v
        .get("starbase_last_update_timestamp")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| "drb invalid causal order timestamp".to_owned())
        })
        .transpose()?;
    let order = order(v)?;
    let version = OrderVersion {
        millis: order.last_update,
        causal,
    };
    Ok((order, version))
}
fn optional_number(v: &Value, key: &str) -> Result<Option<f64>, String> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        _ => number(v, key).map(Some),
    }
}
fn side(v: &Value) -> Result<OrderSide, String> {
    match text(v, "direction")? {
        "buy" => Ok(OrderSide::Buy),
        "sell" => Ok(OrderSide::Sell),
        _ => Err("drb invalid direction".into()),
    }
}
pub(super) fn margin(v: &Value) -> Result<Option<MarginType>, String> {
    match v.get("margin_model") {
        None => Ok(None),
        Some(Value::String(s))
            if matches!(
                s.as_str(),
                "cross_sm" | "cross_pm" | "segregated_sm" | "segregated_pm"
            ) =>
        {
            Ok(Some(MarginType::Cross))
        }
        _ => Err("drb unknown margin model".into()),
    }
}
pub(super) fn summary(v: Value) -> Result<Balance, String> {
    if text(&v, "currency")? != "USDC" {
        return Err("drb unexpected collateral currency".into());
    }
    margin(&v)?;
    let total = number(&v, "equity")?;
    let free = unsigned(&v, "available_funds")?;
    unsigned(&v, "initial_margin")?;
    unsigned(&v, "maintenance_margin")?;
    number(&v, "balance")?;
    let upnl = number(&v, "futures_session_upl")?;
    let used = total - free;
    if !used.is_finite() {
        return Err("drb invalid account margin".into());
    }
    Ok(Balance {
        total,
        free,
        used,
        upnl,
    })
}
pub(super) fn position(v: Value) -> Result<Position, String> {
    if text(&v, "kind")? != "future" {
        return Err("drb unsupported position kind".into());
    }
    let symbol = instrument_symbol(text(&v, "instrument_name")?)?;
    let base = number(&v, "size_currency")?;
    let size = number(&v, "size")?;
    let direction = text(&v, "direction")?;
    if !matches!(direction, "buy" | "sell" | "zero")
        || (direction == "zero") != (base == 0. && size == 0.)
        || (direction != "zero" && (base == 0. || size == 0.))
    {
        return Err("drb inconsistent position size".into());
    }
    let mark = unsigned(&v, "mark_price")?;
    let notional = base.abs() * mark;
    if mark <= 0. || !notional.is_finite() {
        return Err("drb invalid position mark".into());
    }
    let leverage = optional_number(&v, "leverage")?.unwrap_or(0.);
    if leverage < 0. {
        return Err("drb invalid position leverage".into());
    }
    let liq = optional_number(&v, "estimated_liquidation_price")?.unwrap_or(0.);
    if liq < 0. {
        return Err("drb invalid liquidation price".into());
    }
    let isolated = flag(&v, "isolated")?;
    let id = if isolated {
        format!(
            "{symbol}:{}",
            v.get("user_id")
                .and_then(Value::as_u64)
                .ok_or("drb invalid isolated user_id")?
        )
    } else {
        symbol.clone()
    };
    Ok(Position {
        id,
        symbol,
        side: if direction == "sell" {
            PositionSide::Short
        } else {
            PositionSide::Long
        },
        entry_price: unsigned(&v, "average_price")?,
        notional,
        leverage,
        margin_type: Some(if isolated {
            MarginType::Isolated
        } else {
            MarginType::Cross
        }),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: number(&v, "floating_profit_loss")?,
        realized_pnl: Some(number(&v, "realized_profit_loss")?),
        contracts: base.abs(),
        liquidation_price: liq,
        margin_size: Some(unsigned(&v, "initial_margin")?),
        last_update: None,
    })
}
pub(super) fn order(v: Value) -> Result<Order, String> {
    let id = identity(&v, "order_id")?;
    let symbol = instrument_symbol(text(&v, "instrument_name")?)?;
    let side = side(&v)?;
    let status = match text(&v, "order_state")? {
        "open" | "untriggered" | "triggered" => OrderStatus::Open,
        "filled" => OrderStatus::Closed,
        "cancelled" | "rejected" => OrderStatus::Canceled,
        _ => return Err("drb unknown order state".into()),
    };
    let kind = match text(&v, "order_type")? {
        "limit" => OrderType::Limit,
        "market" | "market_limit" => OrderType::Market,
        "stop_market" | "stop_limit" => OrderType::StopLoss,
        "take_market" | "take_limit" => OrderType::TakeProfit,
        "trailing_stop" => OrderType::TrailingStopLoss,
        _ => return Err("drb unsupported order type".into()),
    };
    // Starbase executes API market orders as internal limits. Only the two
    // documented original types restore market intent; trigger order types
    // keep their conditional identity even when this metadata is present.
    let original_market = match v.get("original_order_type") {
        None => false,
        Some(Value::String(original)) if matches!(original.as_str(), "market" | "market_limit") => {
            true
        }
        _ => return Err("drb unsupported original order type".into()),
    };
    let kind = if kind == OrderType::Limit && original_market {
        OrderType::Market
    } else {
        kind
    };
    let amount = unsigned(&v, "amount")?;
    let filled = unsigned(&v, "filled_amount")?;
    if filled > amount {
        return Err("drb filled amount exceeds order amount".into());
    }
    let created = timestamp(&v, "creation_timestamp")?;
    let updated = timestamp(&v, "last_update_timestamp")?;
    if updated < created {
        return Err("drb order clock precedes creation".into());
    }
    let price = match v.get("price") {
        Some(Value::String(s)) if s == "market_price" && kind != OrderType::Limit => 0.,
        _ => unsigned(&v, "price")?,
    };
    let conditional = matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    );
    let trigger_type = if conditional {
        Some(match text(&v, "trigger")? {
            "mark_price" => StopTriggerType::MarkPrice,
            "index_price" => StopTriggerType::IndexPrice,
            "last_price" => StopTriggerType::LastPrice,
            _ => return Err("drb invalid trigger".into()),
        })
    } else {
        None
    };
    let trigger_price = if conditional {
        unsigned(&v, "trigger_price")?
    } else {
        price
    };
    let post = flag(&v, "post_only")?;
    let tif = match text(&v, "time_in_force")? {
        "good_til_cancelled" => Some(TimeInForce::GoodTillCancel),
        "good_til_day" => None,
        "immediate_or_cancel" => Some(TimeInForce::ImmediateOrCancel),
        "fill_or_kill" => Some(TimeInForce::FillOrKill),
        _ => return Err("drb unknown time in force".into()),
    };
    let parent_id = if v.get("primary_order_id").is_some() {
        Some(identity(&v, "primary_order_id")?)
    } else if v.get("trigger_order_id").is_some() {
        Some(identity(&v, "trigger_order_id")?)
    } else {
        None
    };
    let label = text(&v, "label")?;
    if label.len() > 64 || label.chars().any(char::is_control) {
        return Err("drb invalid order label".into());
    }
    Ok(Order {
        id,
        parent_id,
        status,
        symbol,
        kind,
        side,
        price: trigger_price,
        amount,
        filled,
        remaining: amount - filled,
        reduce_only: flag(&v, "reduce_only")?,
        close: None,
        client_order_id: (!label.is_empty()).then(|| label.to_owned()),
        time_in_force: if post {
            Some(TimeInForce::PostOnly)
        } else {
            tif
        },
        last_update: updated,
        trigger_type,
        take_profit: None,
        stop_loss: None,
        extra: conditional.then_some(OrderExtra {
            conditional_order_price: Some(price),
            ..Default::default()
        }),
    })
}
pub(super) fn trade(v: &Value) -> Result<Value, String> {
    let id = identity(v, "trade_id")?;
    let order_id = identity(v, "order_id")?;
    let symbol = instrument_symbol(text(v, "instrument_name")?)?;
    let price = unsigned(v, "price")?;
    let qty = unsigned(v, "amount")?;
    if price <= 0. || qty <= 0. {
        return Err("drb invalid trade economics".into());
    }
    if text(v, "fee_currency")? != "USDC" {
        return Err("drb unexpected trade fee currency".into());
    }
    let side = if side(v)? == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    };
    let mut result = json!({"id":id,"timestamp":timestamp(v,"timestamp")?/1000.,"symbol":symbol,"orderId":order_id,"price":price,"qty":qty,"side":side,"commission":number(v,"fee")?});
    if let Some(liquidity) = v.get("liquidity") {
        result["maker"] = json!(match liquidity.as_str() {
            Some("M") => true,
            Some("T") => false,
            _ => return Err("drb invalid trade liquidity".into()),
        });
    }
    Ok(result)
}
