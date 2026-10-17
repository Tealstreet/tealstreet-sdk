//! Raw conformance engine. Inverse price/PnL remains UNSOLVED / REFUSED.
use safe_cex_num::multiply;
use safe_cex_types::{
    Market, Order, OrderRiskProfile, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, Ticker,
};
use serde::Serialize;
use std::{cmp::Ordering, collections::HashMap};

#[derive(Debug, Clone)]
pub struct OrdersRiskMaps<'a> {
    pub ticker_by_symbol: HashMap<String, &'a Ticker>,
    pub market_by_symbol: HashMap<String, &'a Market>,
}

pub fn build_orders_risk_maps<'a>(
    markets: &'a [Market],
    tickers: &'a [Ticker],
) -> OrdersRiskMaps<'a> {
    OrdersRiskMaps {
        ticker_by_symbol: tickers.iter().map(|t| (t.symbol.clone(), t)).collect(),
        market_by_symbol: markets.iter().map(|m| (m.symbol.clone(), m)).collect(),
    }
}
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 && (a.is_sign_negative() || b.is_sign_negative()) {
        -0.0
    } else {
        a.min(b)
    }
}
fn compare_difference(a: f64, b: f64) -> Ordering {
    (a - b).partial_cmp(&0.0).unwrap_or(Ordering::Equal)
}
// Comparator-safe stable binary insertion: JS treats a NaN comparison as zero.
// Rust's total-order sort may panic on that source comparator. Large nontransitive
// arrays have engine-dependent JS ordering; see the divergent manifest note.
fn js_sort<T>(rows: &mut [T], compare: impl Fn(&T, &T) -> Ordering) {
    for i in 1..rows.len() {
        let (mut left, mut right) = (0, i);
        while left < right {
            let mid = (left + right) / 2;
            if compare(&rows[i], &rows[mid]) == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        rows[left..=i].rotate_right(1);
    }
}
fn opposite(side: impl Into<safe_cex_types::ModelField<PositionSide>>) -> PositionSide {
    let side = side.into();
    if side == PositionSide::Long {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn closing_side(side: impl Into<safe_cex_types::ModelField<PositionSide>>) -> OrderSide {
    let side = side.into();
    if side == PositionSide::Long {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}
fn side_name(side: impl Into<safe_cex_types::ModelField<PositionSide>>) -> &'static str {
    let side = side.into();
    if side == PositionSide::Long {
        "Long"
    } else {
        "Short"
    }
}
/// Catchable source-language failure; callers can clear displayed risk instead of aborting wasm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskError {
    name: &'static str,
    message: &'static str,
    key: Option<String>,
    member: Option<safe_cex_num::ProtoMember>,
}
impl RiskError {
    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn message(&self) -> &'static str {
        self.message
    }
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    pub fn member(&self) -> Option<safe_cex_num::ProtoMember> {
        self.member
    }
    fn formatting(message: &'static str) -> Self {
        Self {
            name: "RangeError",
            message,
            key: None,
            member: None,
        }
    }
}
impl std::fmt::Display for RiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for RiskError {}
// ts-bug: plain-object risk buckets inherit Object.prototype members.
fn require_array_bucket(key: &str, message: &'static str) -> Result<(), RiskError> {
    // ts-bug: plain-object buckets inherit non-array values; source .push throws.
    // Keep the catchable failure, never a panic or a silently skipped partial result.
    if let safe_cex_num::TableHit::Inherited(member) =
        safe_cex_num::js_table_lookup::<&str, ()>(&[], key)
    {
        return Err(RiskError {
            name: "TypeError",
            message,
            key: Some(key.to_owned()),
            member: Some(member),
        });
    }
    Ok(())
}

fn format_rounded_number(val: &str, precision: f64) -> Result<String, RiskError> {
    let value = safe_cex_num::parse_float(val);
    if value.is_nan() {
        return Ok("0.0".to_owned());
    }
    // Intl checks the original Number option against bounds BEFORE flooring it.
    // NaN, -0.5 and 100.9 throw; a NaN value already returned above.
    if !precision.is_finite() || !(0.0..=100.0).contains(&precision) {
        return Err(RiskError::formatting(
            "minimumFractionDigits value is out of range.",
        ));
    }
    let precision = precision.trunc() as usize;
    if value.is_infinite() {
        return Ok(if value.is_sign_negative() {
            "-∞"
        } else {
            "∞"
        }
        .to_owned());
    }
    let text = safe_cex_num::js_number_string(value.abs());
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i32>()
                .map_err(|_| RiskError::formatting("invalid numeric exponent"))?,
        ),
        None => (text.as_str(), 0),
    };
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits = mantissa.replace('.', "");
    let expanded = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    let (integer, fraction) = expanded.split_once('.').unwrap_or((&expanded, ""));
    let mut scaled = integer.as_bytes().to_vec();
    scaled.extend(fraction.bytes().take(precision));
    scaled.extend(std::iter::repeat_n(
        b'0',
        precision.saturating_sub(fraction.len()),
    ));
    if fraction
        .as_bytes()
        .get(precision)
        .is_some_and(|d| *d >= b'5')
    {
        let mut carry = true;
        for digit in scaled.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            scaled.insert(0, b'1');
        }
    }
    let scaled: String = scaled.into_iter().map(char::from).collect();
    let split = scaled.len() - precision;
    let integer = &scaled[..split];
    let mut grouped = String::new();
    for (i, c) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    if precision > 0 {
        grouped.push('.');
        grouped.push_str(&scaled[split..]);
    }
    if value.is_sign_negative() {
        grouped.insert(0, '-');
    }
    Ok(grouped)
}

pub fn format_pnl(
    pnl: f64,
    precision: Option<f64>,
    prefix: Option<&str>,
) -> Result<String, RiskError> {
    let precision = precision.unwrap_or(2.0);
    if let Some(prefix) = prefix.filter(|p| !p.is_empty()) {
        let formatted =
            format_rounded_number(&safe_cex_num::js_number_string(pnl.abs()), precision)?;
        Ok(format!(
            "{}{prefix}{formatted}",
            if pnl > 0.0 { "" } else { "-" }
        ))
    } else {
        format_rounded_number(&safe_cex_num::js_number_string(pnl), precision)
    }
}
fn is_full_stop(order: &Order) -> bool {
    order.r#type != OrderType::Limit
        && (order.amount == 0.0 || order.filled + order.remaining == 0.0)
}
pub fn filter_and_sort_orders_in_profit_direction(
    orders: &[Order],
    entry_price: f64,
    side: impl Into<safe_cex_types::ModelField<PositionSide>>,
) -> Vec<Order> {
    let side = side.into();
    let mut filtered: Vec<_> = orders
        .iter()
        .filter(|o| {
            o.status == OrderStatus::Open
                && o.side == closing_side(side)
                && if side == PositionSide::Long {
                    o.price.to_number() >= entry_price
                } else {
                    o.price.to_number() <= entry_price
                }
        })
        .cloned()
        .collect();
    js_sort(&mut filtered, |a, b| {
        if side == PositionSide::Long {
            compare_difference(a.price.to_number(), b.price.to_number())
        } else {
            compare_difference(b.price.to_number(), a.price.to_number())
        }
    });
    filtered
}
pub fn filter_and_sort_orders_in_loss_direction(
    orders: &[Order],
    entry_price: f64,
    side: impl Into<safe_cex_types::ModelField<PositionSide>>,
) -> Vec<Order> {
    let side = side.into();
    let mut filtered: Vec<_> = orders
        .iter()
        .filter(|o| {
            o.status == OrderStatus::Open
                && (o.side == closing_side(side) || o.r#type == OrderType::Limit)
                && if side == PositionSide::Long {
                    o.price.to_number() < entry_price
                } else {
                    o.price.to_number() > entry_price
                }
        })
        .cloned()
        .collect();
    js_sort(&mut filtered, |a, b| {
        if side == PositionSide::Short {
            compare_difference(a.price.to_number(), b.price.to_number())
        } else {
            compare_difference(b.price.to_number(), a.price.to_number())
        }
    });
    filtered
}

pub fn calculate_orders_risk(
    markets: &[Market],
    tickers: &[Ticker],
    positions: &[Position],
    orders: &[Order],
    precomputed: Option<&OrdersRiskMaps<'_>>,
) -> Result<safe_cex_types::object_map::JsObjectMap<Vec<OrderRiskProfile>>, RiskError> {
    let owned;
    let maps = if let Some(maps) = precomputed {
        maps
    } else {
        owned = build_orders_risk_maps(markets, tickers);
        &owned
    };
    let mut buckets: HashMap<String, Vec<Order>> = HashMap::new();
    for order in orders.iter().filter(|o| o.status == OrderStatus::Open) {
        require_array_bucket(
            &order.symbol,
            "openOrdersBySymbol[o.symbol].push is not a function",
        )?;
        buckets
            .entry(order.symbol.clone())
            .or_default()
            .push(order.clone());
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in positions {
        *counts.entry(p.symbol.clone()).or_default() += 1;
    }
    let mut active = Vec::new();
    for p in positions.iter().filter(|p| buckets.contains_key(&p.symbol)) {
        if p.contracts != 0.0 {
            active.push(p.clone());
        } else if let Some(last) = maps
            .ticker_by_symbol
            .get(&p.symbol)
            .map(|t| t.last)
            .filter(|v| truthy(*v))
        {
            let mut fake = p.clone();
            fake.contracts = 0.0;
            fake.entry_price = last;
            active.push(fake.clone());
            if p.position_mode != Some(PositionMode::Hedged) {
                fake.side = (opposite(p.side)).into();
                active.push(fake);
            }
        }
    }
    let (primary, secondary) =
        process_orders_risk_for_positions(maps, &buckets, &counts, &active, false)?;
    let (mut result, _) =
        process_orders_risk_for_positions(maps, &buckets, &counts, &secondary, true)?;
    // Primary arrays overwrite secondary arrays, rather than concatenate.
    result.extend(primary);
    Ok(result)
}
fn risk_line(order_pnl: f64, rolling: f64) -> Result<String, RiskError> {
    Ok(if order_pnl == rolling {
        format!("PnL: {}", format_pnl(order_pnl, None, None)?)
    } else {
        format!(
            "RPnL: {}{} UPnL: {}",
            if order_pnl > 0.0 { "+" } else { "" },
            format_pnl(order_pnl, None, None)?,
            format_pnl(rolling, None, None)?
        )
    })
}
// Direct constructor for the eight-field TS result row.
#[allow(clippy::too_many_arguments)]
fn profile(
    side: safe_cex_types::ModelField<PositionSide>,
    pnl: f64,
    total: f64,
    rolling: f64,
    size: f64,
    text: String,
    fee: f64,
    full: bool,
) -> OrderRiskProfile {
    OrderRiskProfile {
        is_looking_like_a_full_stop: full.then_some(true),
        order_pnl: pnl,
        total_realized_pnl: total,
        rolling_unrealized_pnl: rolling,
        order_line_pos_side: side.into_property(),
        position_size: size,
        order_line_text: Some(text),
        fee: Some(fee),
        position_size_before_fee: false,
    }
}
type RiskProfiles = safe_cex_types::object_map::JsObjectMap<Vec<OrderRiskProfile>>;

fn process_orders_risk_for_positions(
    maps: &OrdersRiskMaps<'_>,
    buckets: &HashMap<String, Vec<Order>>,
    counts: &HashMap<String, usize>,
    positions: &[Position],
    secondary: bool,
) -> Result<(RiskProfiles, Vec<Position>), RiskError> {
    let mut result: indexmap::IndexMap<String, Vec<OrderRiskProfile>> = indexmap::IndexMap::new();
    let mut next = Vec::new();
    for p in positions {
        let Some(last) = maps
            .ticker_by_symbol
            .get(&p.symbol)
            .map(|t| t.last)
            .filter(|v| truthy(*v))
        else {
            continue;
        };
        let market = maps.market_by_symbol.get(&p.symbol);
        let taker = market
            .and_then(|m| m.taker_fee)
            .filter(|v| truthy(*v))
            .unwrap_or(0.0);
        let maker = market
            .and_then(|m| m.maker_fee)
            .filter(|v| truthy(*v))
            .unwrap_or(0.0);
        let hedged = counts.get(&p.symbol).copied().unwrap_or(0) > 1
            || p.position_mode == Some(PositionMode::Hedged);
        let orders = buckets.get(&p.symbol).map_or(&[][..], Vec::as_slice);
        let pivot = if secondary { p.entry_price } else { last };
        let profit = filter_and_sort_orders_in_profit_direction(orders, pivot, p.side);
        let loss = filter_and_sort_orders_in_loss_direction(orders, pivot, p.side);
        let mut size = p.contracts.abs();
        let mut total = 0.0;
        for order in profit {
            if size <= 0.0 {
                break;
            }
            let mut distance = (order.price.to_number() - p.entry_price).abs();
            if (p.side == PositionSide::Long && order.price.to_number() < p.entry_price)
                || (p.side == PositionSide::Short && order.price.to_number() > p.entry_price)
            {
                distance = -distance;
            }
            // Source initializes ordinary buckets before its hedged-order skip,
            // but inherited buckets remain inherited until a reachable push.
            if !is_full_stop(&order)
                && order.r#type == OrderType::Limit
                && order.side == closing_side(p.side)
                && hedged
                && order.reduce_only != true
            {
                if !safe_cex_num::js_table_lookup::<&str, ()>(&[], &order.id).is_present() {
                    result.entry(order.id.clone()).or_default();
                }
                continue;
            }
            require_array_bucket(&order.id, "ordersRisk[order.id].push is not a function")?;
            let rows = result.entry(order.id.clone()).or_default();
            if is_full_stop(&order) {
                let pnl = multiply(size, distance);
                total += pnl;
                rows.push(profile(
                    p.side,
                    pnl,
                    total,
                    multiply(size, distance),
                    0.0,
                    format!("PnL: {}", format_pnl(total, None, None)?),
                    multiply(size * order.price.to_number(), taker),
                    true,
                ));
                let mut fake = p.clone();
                fake.side = (opposite(p.side)).into();
                fake.entry_price = order.price.to_number();
                fake.contracts = 0.0;
                next.push(fake);
                break;
            } else {
                let pnl = multiply(js_min(size, order.remaining), distance);
                let rolling = multiply(size, distance);
                total += pnl;
                let next_size = size - order.remaining;
                let mut text = risk_line(pnl, rolling)?;
                if !hedged && order.remaining > size && order.reduce_only != true {
                    text.push_str(&format!(", Opens {}", side_name(opposite(p.side))));
                }
                rows.push(profile(
                    p.side,
                    pnl,
                    total,
                    rolling,
                    next_size,
                    text,
                    multiply(
                        js_min(size, order.remaining) * order.price.to_number(),
                        maker,
                    ),
                    false,
                ));
                size = next_size;
                if size <= 0.0 {
                    let mut fake = p.clone();
                    fake.side = (opposite(p.side)).into();
                    fake.entry_price = order.price.to_number();
                    fake.contracts = size.abs();
                    next.push(fake);
                    break;
                }
            }
        }
        size = p.contracts.abs();
        total = 0.0;
        let mut adjusted = p.entry_price;
        for order in loss {
            let distance = (order.price.to_number() - adjusted).abs();
            let mult = if (p.side == PositionSide::Long && order.price.to_number() < adjusted)
                || (p.side == PositionSide::Short && order.price.to_number() > adjusted)
            {
                -1.0
            } else {
                1.0
            };
            require_array_bucket(&order.id, "ordersRisk[order.id].push is not a function")?;
            let rows = result.entry(order.id.clone()).or_default();
            if is_full_stop(&order) || order.r#type == OrderType::TrailingStopLoss {
                let pnl = mult * multiply(size, distance);
                total += pnl;
                rows.push(profile(
                    p.side,
                    pnl,
                    total,
                    mult * multiply(size, distance),
                    0.0,
                    format!("PnL: {}", format_pnl(total, None, None)?),
                    multiply(size * order.price.to_number(), taker),
                    true,
                ));
                break;
            } else if order.r#type == OrderType::Limit {
                adjusted = (order.remaining * order.price.to_number() + size * adjusted)
                    / (size + order.remaining);
                let rolling = mult * multiply(size, distance);
                let next_size = size + order.remaining;
                let text = if size == 0.0 {
                    format!(
                        "Opens {}",
                        if order.side == OrderSide::Buy {
                            "Long"
                        } else {
                            "Short"
                        }
                    )
                } else {
                    format!("UPnL: {}", format_pnl(rolling, None, None)?)
                };
                let mut row = profile(
                    p.side,
                    0.0,
                    total,
                    rolling,
                    next_size,
                    text,
                    multiply(order.remaining * order.price.to_number(), maker),
                    false,
                );
                row.position_size_before_fee = true;
                rows.push(row);
                size = next_size;
                if size < 0.0 {
                    break;
                }
            } else {
                let triggered = if order.reduce_only == true {
                    js_min(size, order.remaining)
                } else {
                    order.remaining
                };
                let pnl = mult * multiply(js_min(size, order.remaining), distance);
                let rolling = mult * multiply(size, distance);
                total += pnl;
                let next_size = size - triggered;
                rows.push(profile(
                    p.side,
                    pnl,
                    total,
                    rolling,
                    next_size,
                    risk_line(pnl, rolling)?,
                    multiply(triggered * order.price.to_number(), maker),
                    false,
                ));
                size = next_size;
                if size <= 0.0 {
                    break;
                }
            }
        }
    }
    Ok((result.into_iter().collect(), next))
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskAtPrice {
    pub position_size: f64,
    pub position_side: safe_cex_types::ModelField<PositionSide>,
    pub unrealized_pnl: f64,
    pub realized_pnl: f64,
}

pub fn calculate_risk_at_price(
    price: f64,
    symbol: &str,
    risk: &indexmap::IndexMap<String, Vec<OrderRiskProfile>>,
    positions: &[Position],
    orders: &[Order],
) -> Option<RiskAtPrice> {
    let positions: Vec<_> = positions.iter().filter(|p| p.symbol == symbol).collect();
    let hedged = positions
        .iter()
        .any(|p| p.position_mode == Some(PositionMode::Hedged));
    let open: Vec<_> = positions
        .iter()
        .filter(|p| p.contracts.abs() > 0.0)
        .collect();
    let mut side = safe_cex_types::ModelField::Value(PositionSide::Long);
    let mut entry = 0.0;
    let mut size = 0.0;
    if !open.is_empty() {
        let p = if hedged {
            if open.len() != 1 {
                return None;
            }
            open[0]
        } else {
            if positions.len() != 1 {
                return None;
            }
            positions[0]
        };
        side = p.side;
        entry = p.entry_price;
        size = p.contracts;
    }
    if size == 0.0 {
        let mut sells: Vec<_> = orders
            .iter()
            .filter(|o| {
                o.status == OrderStatus::Open
                    && o.symbol == symbol
                    && o.side == OrderSide::Sell
                    && o.reduce_only != true
            })
            .collect();
        js_sort(&mut sells, |a, b| {
            compare_difference(a.price.to_number(), b.price.to_number())
        });
        let mut buys: Vec<_> = orders
            .iter()
            .filter(|o| {
                o.status == OrderStatus::Open
                    && o.symbol == symbol
                    && o.side == OrderSide::Buy
                    && o.reduce_only != true
            })
            .collect();
        js_sort(&mut buys, |a, b| {
            compare_difference(b.price.to_number(), a.price.to_number())
        });
        if let Some(order) = sells
            .first()
            .filter(|o| buys.is_empty() || price > o.price.to_number())
        {
            side = PositionSide::Short.into();
            entry = order.price.to_number();
            size = order.remaining;
        } else {
            let order = buys
                .first()
                .filter(|o| sells.is_empty() || price < o.price.to_number())?;
            side = PositionSide::Long.into();
            entry = order.price.to_number();
            size = order.remaining;
        }
    }
    let mut total = 0.0;
    let mut reference = entry;
    let mut offset = 0.0;
    let mut candidates: Vec<_> = orders
        .iter()
        .filter(|o| {
            o.status == OrderStatus::Open
                && o.symbol == symbol
                && (if price > entry {
                    o.price.to_number() > entry && o.price.to_number() <= price
                } else {
                    o.price.to_number() < entry && o.price.to_number() >= price
                })
                && risk.get(&o.id).is_some_and(|r| !r.is_empty())
        })
        .collect();
    js_sort(&mut candidates, |a, b| {
        if price > entry {
            compare_difference(b.price.to_number(), a.price.to_number())
        } else {
            compare_difference(a.price.to_number(), b.price.to_number())
        }
    });
    if let Some(order) = candidates.first() {
        let pivot = &risk[&order.id][0];
        size = pivot.position_size;
        offset = pivot.rolling_unrealized_pnl - pivot.order_pnl;
        total = pivot.total_realized_pnl;
        reference = order.price.to_number();
        side = pivot.order_line_pos_side;
    }
    Some(RiskAtPrice {
        position_size: size,
        position_side: side.into_property(),
        unrealized_pnl: (price - reference)
            * size
            * if side == PositionSide::Long {
                1.0
            } else {
                -1.0
            }
            + offset,
        realized_pnl: total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn position(side: PositionSide) -> Position {
        let mut p:Position=serde_json::from_value(serde_json::json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"notional":200,"leverage":5,"unrealizedPnl":10,"contracts":2,"liquidationPrice":0})).unwrap();
        p.side = (side).into();
        p
    }
    fn order(id: &str, price: f64, remaining: f64) -> Order {
        serde_json::from_value(serde_json::json!({"id":id,"symbol":"BTC","status":"open","type":"limit","side":"sell","price":price,"amount":remaining,"filled":0,"remaining":remaining,"reduceOnly":true,"lastUpdate":0})).unwrap()
    }
    fn ticker() -> Ticker {
        serde_json::from_value(serde_json::json!({"id":"BTC","symbol":"BTC","bid":99,"ask":101,"last":100,"mark":100,"index":100,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap()
    }
    #[test]
    fn partial_profit_and_full_stop_crosshair_keep_source_fees_and_pnl() {
        let p = position(PositionSide::Long);
        let tp = order("tp", 110.0, 1.0);
        let mut stop = order("stop", 90.0, 0.0);
        stop.r#type = safe_cex_types::ModelField::Value(OrderType::StopLoss);
        let market:Market=serde_json::from_value(serde_json::json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}},"makerFee":0.001,"takerFee":0.002})).unwrap();
        let orders = vec![tp, stop];
        let positions = vec![p];
        let risk =
            calculate_orders_risk(&[market], &[ticker()], &positions, &orders, None).unwrap();
        let tp = &risk["tp"][0];
        assert_eq!(tp.order_pnl, 10.0);
        assert_eq!(tp.rolling_unrealized_pnl, 20.0);
        assert_eq!(tp.position_size, 1.0);
        assert_eq!(tp.fee, Some(0.11));
        let stop = &risk["stop"][0];
        assert_eq!(stop.order_pnl, -20.0);
        assert_eq!(stop.is_looking_like_a_full_stop, Some(true));
        assert_eq!(stop.fee, Some(0.36));
        let cross = calculate_risk_at_price(115.0, "BTC", &risk, &positions, &orders).unwrap();
        assert_eq!(cross.unrealized_pnl, 15.0);
        assert_eq!(cross.realized_pnl, 10.0);
    }
    #[test]
    fn hedge_ambiguity_and_numeric_truthiness() {
        let mut long = position(PositionSide::Long);
        let mut short = position(PositionSide::Short);
        long.position_mode = (Some(PositionMode::Hedged)).into();
        short.position_mode = (Some(PositionMode::Hedged)).into();
        assert!(
            calculate_risk_at_price(
                105.0,
                "BTC",
                &indexmap::IndexMap::new(),
                &[long.clone(), short.clone()],
                &[]
            )
            .is_none()
        );
        short.contracts = 0.0;
        assert!(
            calculate_risk_at_price(
                105.0,
                "BTC",
                &indexmap::IndexMap::new(),
                &[long, short],
                &[]
            )
            .is_some()
        );
        assert!(js_min(f64::NAN, 1.0).is_nan());
        assert_eq!(js_min(0.0, -0.0).to_bits(), (-0.0_f64).to_bits());
        let mut missing = ticker();
        missing.last = f64::NAN;
        assert!(
            calculate_orders_risk(
                &[],
                &[missing],
                &[position(PositionSide::Long)],
                &[order("tp", 110.0, 1.0)],
                None
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn locale_rounding_prefix_and_parse_float_edges() {
        assert_eq!(format_pnl(2.675, None, None).unwrap(), "2.68");
        assert_eq!(format_pnl(999.995, None, None).unwrap(), "1,000.00");
        assert_eq!(format_pnl(0.0, None, Some("$")).unwrap(), "-$0.00");
        assert_eq!(format_pnl(-0.0, None, None).unwrap(), "0.00");
        assert_eq!(format_pnl(f64::NAN, None, None).unwrap(), "0.0");
        assert_eq!(format_pnl(f64::NEG_INFINITY, None, None).unwrap(), "-∞");
        assert_eq!(
            format_rounded_number("\u{feff}2.675venue", 2.0).unwrap(),
            "2.68"
        );
        assert_eq!(format_rounded_number("\u{85}1", 2.0).unwrap(), "0.0");
        assert_eq!(
            format_rounded_number("1", -0.5).unwrap_err().name(),
            "RangeError"
        );
        assert_eq!(
            format_rounded_number("1", 100.9).unwrap_err().name(),
            "RangeError"
        );
    }
    #[test]
    fn nan_entry_sort_does_not_introduce_a_rust_only_panic() {
        let orders: Vec<_> = (0..64)
            .map(|i| {
                let mut row = order(
                    &format!("nan-{i}"),
                    if i % 11 == 0 {
                        0.0
                    } else {
                        ((i * 37) % 67) as f64
                    },
                    1.0,
                );
                if i % 11 == 0 {
                    row.price = (f64::NAN).into();
                }
                row.reduce_only = false.into();
                row
            })
            .collect();
        let result =
            calculate_risk_at_price(100.0, "BTC", &indexmap::IndexMap::new(), &[], &orders)
                .unwrap();
        assert_eq!(result.position_size, 1.0);
        assert!(result.unrealized_pnl.is_nan());
    }
    #[test]
    fn prototype_bucket_errors_are_catchable() {
        use safe_cex_num::ProtoMember::*;
        let members = [
            ("constructor", Constructor),
            ("__defineGetter__", DefineGetter),
            ("__defineSetter__", DefineSetter),
            ("hasOwnProperty", HasOwnProperty),
            ("__lookupGetter__", LookupGetter),
            ("__lookupSetter__", LookupSetter),
            ("isPrototypeOf", IsPrototypeOf),
            ("propertyIsEnumerable", PropertyIsEnumerable),
            ("toString", ToString),
            ("valueOf", ValueOf),
            ("__proto__", Prototype),
            ("toLocaleString", ToLocaleString),
        ];
        let mut compared = 0;
        for (key, member) in members {
            let mut invalid = order("normal", 110.0, 1.0);
            invalid.symbol = key.to_owned();
            let error = calculate_orders_risk(
                &[],
                &[ticker()],
                &[position(PositionSide::Long)],
                &[invalid],
                None,
            )
            .expect_err("risk::prototype symbol must return TypeError");
            assert_eq!(error.name(), "TypeError");
            assert_eq!(error.key(), Some(key));
            assert_eq!(
                error.member(),
                Some(member),
                "risk::native symbol member {key}"
            );
            assert_eq!(
                error.message(),
                "openOrdersBySymbol[o.symbol].push is not a function",
                "risk::native symbol access point {key}"
            );
            let invalid = order(key, 110.0, 1.0);
            let error = calculate_orders_risk(
                &[],
                &[ticker()],
                &[position(PositionSide::Long)],
                &[invalid],
                None,
            )
            .expect_err("risk::prototype order id must return TypeError");
            assert_eq!(error.name(), "TypeError");
            assert_eq!(error.key(), Some(key));
            assert_eq!(
                error.member(),
                Some(member),
                "risk::native order member {key}"
            );
            assert_eq!(
                error.message(),
                "ordersRisk[order.id].push is not a function",
                "risk::native order access point {key}"
            );
            compared += 2;
        }
        assert_eq!(compared, 24, "risk::native access-point floor");
    }
}

#[cfg(test)]
mod language_tests {
    #[test]
    fn parse_float_uses_shared_ecmascript_leading_whitespace() {
        assert_eq!(safe_cex_num::parse_float("\u{feff}12.5tail"), 12.5);
        assert!(safe_cex_num::parse_float("\u{0085}12.5").is_nan());
        assert!(safe_cex_num::parse_float("\u{001c}12.5").is_nan());
    }
}

impl Serialize for RiskAtPrice {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state =
            serializer.serialize_struct(safe_cex_types::model_field::MODEL_RECORD, 4)?;
        state.serialize_field("positionSize", &self.position_size)?;
        self.position_side
            .serialize_field(&mut state, "positionSide")?;
        state.serialize_field("unrealizedPnl", &self.unrealized_pnl)?;
        state.serialize_field("realizedPnl", &self.realized_pnl)?;
        state.end()
    }
}
