//! Source-corresponding order routing decisions on explicit field projections.
use safe_cex_types::{OrderSide, OrderType, PositionSide};

/// Only fields read by the source predicate; defined includes present null/zero.
/// Boolean gates encode strict equality to true, rather than general truthiness.
#[derive(Debug, Clone, Copy)]
pub struct WeexBatchOrderFields {
    pub order_type: Option<OrderType>,
    pub trigger_price_defined: bool,
    pub stop_loss_defined: bool,
    pub take_profit_defined: bool,
    pub reduce_only_is_true: bool,
    pub close_is_true: bool,
    pub tp_trigger_type_defined: bool,
    pub sl_trigger_type_defined: bool,
}

pub fn is_batchable_regular_order(opts: &WeexBatchOrderFields) -> bool {
    matches!(opts.order_type, Some(OrderType::Limit | OrderType::Market))
        && !opts.trigger_price_defined
        && !opts.stop_loss_defined
        && !opts.take_profit_defined
        && !opts.reduce_only_is_true
        && !opts.close_is_true
        && !opts.tp_trigger_type_defined
        && !opts.sl_trigger_type_defined
}

/// None represents a type that is not a source enum value; id retains all UTF16.
pub fn is_algo_order(order_type: Option<OrderType>, id: &[u16]) -> bool {
    matches!(
        order_type,
        Some(
            OrderType::Conditional
                | OrderType::StopLoss
                | OrderType::TakeProfit
                | OrderType::TrailingStopLoss
        )
    ) || id
        .windows(2)
        .any(|pair| pair == [u16::from(b'_'), u16::from(b'_')])
}

/// The caller supplies source truthiness of reduceOnly, separately from === true.
pub fn infer_position_side(side: Option<OrderSide>, reduce_only_truthy: bool) -> PositionSide {
    if reduce_only_truthy {
        if side == Some(OrderSide::Buy) {
            PositionSide::Short
        } else {
            PositionSide::Long
        }
    } else if side == Some(OrderSide::Buy) {
        PositionSide::Long
    } else {
        PositionSide::Short
    }
}
