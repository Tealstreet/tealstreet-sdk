//! Public Order quantity and trigger projection, before packed-frame publication.
use crate::model::{Market, Order, OrderType, StopTriggerType};
use crate::wire::{self, FrameWriter, section};

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

fn subtract(a: f64, b: f64) -> f64 {
    let factor = 10f64.powi(crate::num::decimals(a).max(crate::num::decimals(b)) as i32);
    crate::num::round_half_up((a - b) * factor) / factor
}

fn divide(a: f64, b: f64) -> f64 {
    let digits = crate::num::decimals(a).max(crate::num::decimals(b)).max(8);
    let factor = 10f64.powi(digits as i32);
    crate::num::round_half_up((a / b) * factor) / factor
}

/// Exact venue extras win. Derived values use native market metadata once, here.
fn extras(order: &Order, market: Option<&Market>, units: u32) -> [Option<f64>; 4] {
    let extra = order.extra.as_ref();
    let native = finite(extra.and_then(|v| v.remaining_native));
    let fill = finite(extra.and_then(|v| v.filled_in_placement_units));
    let mut leaves = native;
    let mut placement_fill = fill;
    let mut notional = None;
    let inverse = market.is_some_and(|m| m.is_inverse == Some(true));
    let size = market.map(|m| m.contract_size.filter(|s| *s > 0.).unwrap_or(1.));
    let regular = matches!(order.kind, OrderType::Limit | OrderType::Market);
    match units {
        3 => {
            if let Some(native) = native {
                notional = inverse.then(|| native * size.unwrap_or(1.));
                if placement_fill.is_none() && regular && order.amount >= native {
                    placement_fill = Some(subtract(order.amount, native));
                }
            } else if !inverse && let Some(size) = size {
                leaves = Some(divide(order.remaining, size));
                if placement_fill.is_none() && regular {
                    placement_fill = Some(divide(order.filled, size));
                }
            }
        }
        _ => {
            if inverse && let Some(native) = native {
                notional = Some(
                    native
                        * if matches!(units, 1 | 4) {
                            size.unwrap_or(1.)
                        } else {
                            1.
                        },
                );
            } else if units == 2 && native.is_none() {
                leaves = Some(subtract(order.amount, order.filled).max(0.));
            }
        }
    }
    if units == 1
        && let Some(size) = size
    {
        leaves = native.or_else(|| Some(divide(order.remaining, size)));
        placement_fill = fill.or_else(|| (!inverse).then(|| divide(order.filled, size)));
    }
    [
        finite(leaves),
        finite(placement_fill),
        finite(extra.and_then(|v| v.remaining_notional)).or(finite(notional)),
        finite(extra.and_then(|v| v.conditional_order_price)),
    ]
}

fn triggers(order: &Order) -> (Option<StopTriggerType>, Option<StopTriggerType>) {
    let extra = order.extra.as_ref();
    let tp = extra.and_then(|e| e.tp_trigger_type).or_else(|| {
        (order.kind == OrderType::TakeProfit)
            .then_some(order.trigger_type)
            .flatten()
    });
    let sl = extra.and_then(|e| e.sl_trigger_type).or_else(|| {
        matches!(
            order.kind,
            OrderType::StopLoss | OrderType::TrailingStopLoss
        )
        .then_some(order.trigger_type)
        .flatten()
    });
    (tp, sl)
}

pub(crate) fn write<'a>(
    orders: &'a [Order],
    market: impl Fn(&str) -> Option<&'a Market>,
    market_ref: impl Fn(&str) -> u32,
    units: u32,
    w: &mut FrameWriter<'a>,
) {
    w.begin_revision(section::ORDERS, 1);
    w.u32(orders.len() as u32);
    for order in orders {
        let refs = [
            w.string(&order.id),
            w.optional_string(order.client_order_id.as_deref()),
            w.optional_string(order.parent_id.as_deref()),
        ];
        w.u32s(refs);
    }
    w.u32s(orders.iter().map(|o| market_ref(&o.symbol)));
    w.u32s(orders.iter().map(|o| {
        wire::order_enums(
            o.status,
            o.kind,
            o.side,
            o.reduce_only,
            o.close,
            o.time_in_force,
            o.trigger_type,
            o.extra.as_ref().and_then(|e| e.reject_reason),
        )
    }));
    w.pad();
    let columns: [fn(&Order) -> f64; 7] = [
        |o| o.price,
        |o| o.amount,
        |o| o.filled,
        |o| o.remaining,
        |o| o.last_update,
        |o| o.take_profit.unwrap_or(f64::NAN),
        |o| o.stop_loss.unwrap_or(f64::NAN),
    ];
    for column in columns {
        w.f64s(orders.iter().map(column));
    }
    let projected: Vec<_> = orders
        .iter()
        .map(|o| extras(o, market(&o.symbol), units))
        .collect();
    for column in 0..4 {
        w.f64s(projected.iter().map(|row| row[column].unwrap_or(f64::NAN)));
    }
    w.u32s(orders.iter().map(|order| {
        let (tp, sl) = triggers(order);
        let trigger = |value: Option<StopTriggerType>| value.map_or(0, |v| v as u32 + 1);
        trigger(tp)
            | (trigger(sl) << crate::model::order_trigger_word::SL_SHIFT)
            | if order.extra.as_ref().is_some_and(|e| e.native_algo) {
                crate::model::order_trigger_word::NATIVE_ALGO
            } else {
                0
            }
    }));
    for order in orders {
        let symbol = w.string(&order.symbol);
        w.u32(symbol);
    }
    w.end();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_quantity_arithmetic_preserves_decimal_lots() {
        assert_eq!(divide(0.3, 0.1), 3.);
        assert_eq!(subtract(0.3, 0.1), 0.2);
    }
}
