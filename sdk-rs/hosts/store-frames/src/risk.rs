//! Order-line policy formerly produced by OrdersRiskProfilePostprocessor.
use crate::model::{OrderRiskProfile, Projection};
use safe_cex_engine::model::*;
use std::collections::{BTreeMap, HashMap};
type Risks = BTreeMap<String, Vec<OrderRiskProfile>>;
fn price(o: &Order) -> f64 {
    o.price
}
fn full(o: &Order) -> bool {
    o.kind != OrderType::Limit && (o.amount == 0.0 || o.filled + o.remaining == 0.0)
}
fn multiply(a: f64, b: f64) -> f64 {
    a * b
}
fn math_min(a: f64, b: f64) -> f64 {
    a.min(b)
}
fn opposite(s: PositionSide) -> PositionSide {
    if s == PositionSide::Long {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let value = if value == 0.0 { 0.0 } else { value };
    let fixed = format!("{value:.2}");
    let (whole, fraction) = fixed.split_once('.').expect("fixed precision");
    let mut out = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > usize::from(whole.starts_with('-')) && (whole.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{out}.{fraction}")
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
pub fn calculate(data: &Projection) -> Risks {
    let mut open: HashMap<&str, Vec<&Order>> = HashMap::new();
    for o in data.orders.iter().filter(|o| o.status == OrderStatus::Open) {
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
            p.side = opposite(p.side);
            positions.push(p);
        }
    }
    let (primary, next) = process(data, &open, &counts, &positions, false);
    let (mut secondary, _) = process(data, &open, &counts, &next, true);
    secondary.extend(primary);
    secondary
}
fn process(
    data: &Projection,
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
        let side = p.side;
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
                o.side == closing
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
                (o.side == closing || o.kind == OrderType::Limit)
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
                ));
                let mut flipped = p.clone();
                flipped.side = opposite(side);
                flipped.entry_price = price(o);
                flipped.contracts = 0.0;
                next.push(flipped);
                break;
            }
            if o.kind == OrderType::Limit && o.side == closing && hedged && !o.reduce_only {
                continue;
            }
            let pnl = multiply(math_min(size, o.remaining), distance);
            let rolling = multiply(size, distance);
            realized += pnl;
            let mut text = line(pnl, rolling);
            if !hedged && o.remaining > size && !o.reduce_only {
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
            ));
            size -= o.remaining;
            if size <= 0.0 {
                let mut flipped = p.clone();
                flipped.side = opposite(side);
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
            if full(o) || o.kind == OrderType::TrailingStopLoss {
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
                ));
                break;
            } else if o.kind == OrderType::Limit {
                entry = (o.remaining * price(o) + size * entry) / (size + o.remaining);
                let rolling = mult * multiply(size, distance);
                let text = if size == 0.0 {
                    format!(
                        "Opens {}",
                        if o.side == OrderSide::Buy {
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
                ));
                size += o.remaining;
                if size < 0.0 {
                    break;
                }
            } else {
                let on_trigger = if o.reduce_only {
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
) -> OrderRiskProfile {
    OrderRiskProfile {
        is_looking_like_a_full_stop: full.then_some(true),
        order_pnl: pnl,
        total_realized_pnl: realized,
        rolling_unrealized_pnl: rolling,
        order_line_pos_side: side,
        position_size: size,
        order_line_text: text,
        fee,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_fixed_precision_labels() {
        assert_eq!(number(1234.5), "1,234.50");
        assert_eq!(number(-0.0), "0.00");
        assert_eq!(number(f64::NAN), "—");
    }
}
