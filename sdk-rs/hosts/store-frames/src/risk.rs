//! Order-line policy formerly produced by OrdersRiskProfilePostprocessor.
use indexmap::IndexMap;
use safe_cex_num::{math_min, multiply};
use safe_cex_types::{
    ModelField, Order, OrderRiskProfile, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, StoreData,
};
use std::collections::HashMap;
type Risks = IndexMap<String, Vec<OrderRiskProfile>>;
fn price(o: &Order) -> f64 {
    o.price.unwrap_or(f64::NAN)
}
fn full(o: &Order) -> bool {
    o.r#type != Some(OrderType::Limit) && (o.amount == 0.0 || o.filled + o.remaining == 0.0)
}
fn opposite(s: PositionSide) -> PositionSide {
    if s == PositionSide::Long {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn number(value: f64) -> String {
    // Intl.NumberFormat rounds the shortest decimal representation half away
    // from zero. Binary toFixed/format rounding differs at values like 1.005.
    if value.is_nan() {
        return "0.0".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-∞"
        } else {
            "∞"
        }
        .into();
    }
    let decimal = safe_cex_num::js_number_string(value.abs());
    let (mantissa, exponent) = decimal
        .split_once('e')
        .map_or((decimal.as_str(), 0), |(m, e)| {
            (m, e.parse::<i32>().expect("decimal exponent"))
        });
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits: Vec<u8> = mantissa.bytes().filter(|&b| b != b'.').collect();
    let target = point + 2;
    let round_up = target >= 0 && digits.get(target as usize).is_some_and(|&d| d >= b'5');
    let mut cents = if target <= 0 {
        vec![b'0']
    } else {
        let mut out = digits[..digits.len().min(target as usize)].to_vec();
        out.resize(target as usize, b'0');
        out
    };
    if round_up {
        let mut carry = true;
        for digit in cents.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            cents.insert(0, b'1');
        }
    }
    let first = cents
        .iter()
        .position(|&b| b != b'0')
        .unwrap_or(cents.len() - 1);
    let mut cents = cents[first..].to_vec();
    while cents.len() < 3 {
        cents.insert(0, b'0');
    }
    let whole = cents.len() - 2;
    let mut out = if value.is_sign_negative() && value != 0.0 {
        "-".to_owned()
    } else {
        String::new()
    };
    for (i, &digit) in cents[..whole].iter().enumerate() {
        if i > 0 && (whole - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(digit));
    }
    out.push('.');
    out.push(char::from(cents[whole]));
    out.push(char::from(cents[whole + 1]));
    out
}

fn line(pnl: f64, rolling: f64) -> String {
    if pnl == rolling {
        format!("PnL: {}", number(pnl))
    } else {
        format!(
            "RPnL: {}{} UPnL: {}",
            if pnl > 0.0 { "+" } else { "" },
            number(pnl),
            number(rolling)
        )
    }
}
pub fn calculate(data: &StoreData) -> Risks {
    let mut open: HashMap<&str, Vec<&Order>> = HashMap::new();
    for o in data
        .orders
        .iter()
        .filter(|o| o.status == Some(OrderStatus::Open))
    {
        open.entry(&o.symbol).or_default().push(o);
    }
    let mut counts = HashMap::<&str, usize>::new();
    for p in &data.positions {
        *counts.entry(&p.symbol).or_default() += 1;
    }
    let mut positions = Vec::new();
    for p in data
        .positions
        .iter()
        .filter(|p| open.contains_key(p.symbol.as_str()))
    {
        if p.contracts != 0.0 {
            positions.push(p.clone());
            continue;
        }
        let Some(last) = data
            .tickers
            .iter()
            .find(|t| t.symbol == p.symbol)
            .map(|t| t.last)
            .filter(|v| *v != 0.0)
        else {
            continue;
        };
        let mut p = p.clone();
        p.entry_price = last;
        positions.push(p.clone());
        if p.position_mode != Some(PositionMode::Hedged) {
            p.side = ModelField::Value(opposite(p.side.expect("engine side")));
            positions.push(p);
        }
    }
    let (primary, next) = process(data, &open, &counts, &positions, false);
    let (mut secondary, _) = process(data, &open, &counts, &next, true);
    secondary.extend(primary);
    secondary
}
fn process(
    data: &StoreData,
    open: &HashMap<&str, Vec<&Order>>,
    counts: &HashMap<&str, usize>,
    positions: &[Position],
    secondary: bool,
) -> (Risks, Vec<Position>) {
    let mut risks = Risks::new();
    let mut next = Vec::new();
    for p in positions {
        let Some(last) = data
            .tickers
            .iter()
            .find(|t| t.symbol == p.symbol)
            .map(|t| t.last)
            .filter(|v| *v != 0.0)
        else {
            continue;
        };
        let m = data.markets.iter().find(|m| m.symbol == p.symbol);
        let maker = m.and_then(|m| m.maker_fee).unwrap_or(0.0);
        let taker = m.and_then(|m| m.taker_fee).unwrap_or(0.0);
        let side = p.side.expect("engine side");
        let long = side == PositionSide::Long;
        let hedged = counts.get(p.symbol.as_str()).copied().unwrap_or(0) > 1
            || p.position_mode == Some(PositionMode::Hedged);
        let threshold = if secondary { p.entry_price } else { last };
        let closing = if long {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let orders = open
            .get(p.symbol.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut profit: Vec<_> = orders
            .iter()
            .copied()
            .filter(|o| {
                o.side == Some(closing)
                    && if long {
                        price(o) >= threshold
                    } else {
                        price(o) <= threshold
                    }
            })
            .collect();
        let mut loss: Vec<_> = orders
            .iter()
            .copied()
            .filter(|o| {
                (o.side == Some(closing) || o.r#type == Some(OrderType::Limit))
                    && if long {
                        price(o) < threshold
                    } else {
                        price(o) > threshold
                    }
            })
            .collect();
        profit.sort_by(|a, b| {
            (if long {
                price(a) - price(b)
            } else {
                price(b) - price(a)
            })
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
        });
        loss.sort_by(|a, b| {
            (if long {
                price(b) - price(a)
            } else {
                price(a) - price(b)
            })
            .partial_cmp(&0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
        });
        let (mut size, mut realized) = (p.contracts.abs(), 0.0);
        for o in profit {
            if size <= 0.0 {
                break;
            }
            let distance = (price(o) - p.entry_price).abs();
            let distance =
                if (long && price(o) < p.entry_price) || (!long && price(o) > p.entry_price) {
                    -distance
                } else {
                    distance
                };
            let rows = risks.entry(o.id.clone()).or_default();
            if full(o) {
                let pnl = multiply(size, distance);
                realized += pnl;
                rows.push(row(
                    side,
                    pnl,
                    realized,
                    pnl,
                    0.0,
                    multiply(size * price(o), taker),
                    format!("PnL: {}", number(realized)),
                    true,
                    false,
                ));
                let mut flipped = p.clone();
                flipped.side = ModelField::Value(opposite(side));
                flipped.entry_price = price(o);
                flipped.contracts = 0.0;
                next.push(flipped);
                break;
            }
            if o.r#type == Some(OrderType::Limit)
                && o.side == Some(closing)
                && hedged
                && o.reduce_only != Some(true)
            {
                continue;
            }
            let pnl = multiply(math_min(size, o.remaining), distance);
            let rolling = multiply(size, distance);
            realized += pnl;
            let mut text = line(pnl, rolling);
            if !hedged && o.remaining > size && o.reduce_only != Some(true) {
                text.push_str(if long {
                    ", Opens Short"
                } else {
                    ", Opens Long"
                });
            }
            rows.push(row(
                side,
                pnl,
                realized,
                rolling,
                size - o.remaining,
                multiply(math_min(size, o.remaining) * price(o), maker),
                text,
                false,
                false,
            ));
            size -= o.remaining;
            if size <= 0.0 {
                let mut flipped = p.clone();
                flipped.side = ModelField::Value(opposite(side));
                flipped.entry_price = price(o);
                flipped.contracts = size.abs();
                next.push(flipped);
                break;
            }
        }
        size = p.contracts.abs();
        realized = 0.0;
        let mut entry = p.entry_price;
        for o in loss {
            let distance = (price(o) - entry).abs();
            let mult = if (long && price(o) < entry) || (!long && price(o) > entry) {
                -1.0
            } else {
                1.0
            };
            let rows = risks.entry(o.id.clone()).or_default();
            if full(o) || o.r#type == Some(OrderType::TrailingStopLoss) {
                let pnl = mult * multiply(size, distance);
                realized += pnl;
                rows.push(row(
                    side,
                    pnl,
                    realized,
                    pnl,
                    0.0,
                    multiply(size * price(o), taker),
                    format!("PnL: {}", number(realized)),
                    true,
                    false,
                ));
                break;
            } else if o.r#type == Some(OrderType::Limit) {
                entry = (o.remaining * price(o) + size * entry) / (size + o.remaining);
                let rolling = mult * multiply(size, distance);
                let text = if size == 0.0 {
                    format!(
                        "Opens {}",
                        if o.side == Some(OrderSide::Buy) {
                            "Long"
                        } else {
                            "Short"
                        }
                    )
                } else {
                    format!("UPnL: {}", number(rolling))
                };
                rows.push(row(
                    side,
                    0.0,
                    realized,
                    rolling,
                    size + o.remaining,
                    multiply(o.remaining * price(o), maker),
                    text,
                    false,
                    true,
                ));
                size += o.remaining;
                if size < 0.0 {
                    break;
                }
            } else {
                let on_trigger = if o.reduce_only == Some(true) {
                    math_min(size, o.remaining)
                } else {
                    o.remaining
                };
                let pnl = mult * multiply(math_min(size, o.remaining), distance);
                let rolling = mult * multiply(size, distance);
                realized += pnl;
                rows.push(row(
                    side,
                    pnl,
                    realized,
                    rolling,
                    size - on_trigger,
                    multiply(on_trigger * price(o), maker),
                    line(pnl, rolling),
                    false,
                    false,
                ));
                size -= on_trigger;
                if size <= 0.0 {
                    break;
                }
            }
        }
    }
    (risks, next)
}
#[allow(clippy::too_many_arguments)]
fn row(
    side: PositionSide,
    pnl: f64,
    realized: f64,
    rolling: f64,
    size: f64,
    fee: f64,
    text: String,
    full: bool,
    size_before_fee: bool,
) -> OrderRiskProfile {
    OrderRiskProfile {
        is_looking_like_a_full_stop: full.then_some(true),
        order_pnl: pnl,
        total_realized_pnl: realized,
        rolling_unrealized_pnl: rolling,
        order_line_pos_side: ModelField::Value(side),
        position_size: size,
        order_line_text: Some(text),
        fee: Some(fee),
        position_size_before_fee: size_before_fee,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locale_pnl_uses_decimal_half_expand_and_grouping() {
        for (value, expected) in [
            (1.005, "1.01"),
            (2.675, "2.68"),
            (-1.005, "-1.01"),
            (-0.0, "0.00"),
            (999.995, "1,000.00"),
            (0.0000001, "0.00"),
            (f64::INFINITY, "∞"),
            (f64::NAN, "0.0"),
        ] {
            assert_eq!(number(value), expected);
        }
    }
}
