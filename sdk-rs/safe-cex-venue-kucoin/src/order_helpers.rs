//! Pure futures order decisions, kept separate from transport orchestration.
use safe_cex_types::{
    MarginType, Order, OrderSide, OrderStopTriggerType, OrderType, PlaceOrderOpts, Position,
    PositionMode, PositionSide,
};

// ts: KucoinExchange.getOrderPlanType
pub fn get_order_plan_type(order_type: OrderType) -> Result<&'static str, String> {
    match order_type {
        OrderType::StopLoss => Ok("pos_loss"),
        OrderType::TakeProfit => Ok("pos_profit"),
        OrderType::TrailingStopLoss => Ok("track_plan"),
        _ => Err(format!(
            "Unknown order type: {}",
            order_type_text(order_type)
        )),
    }
}
// ts: KucoinExchange.getOrderSideV2
pub fn get_order_side_v2(
    opts: &PlaceOrderOpts,
    mode: PositionMode,
) -> (&'static str, Option<&'static str>) {
    let buy = opts.side == OrderSide::Buy;
    let mut side = if buy { "Buy" } else { "Sell" };
    let mut trade_side = None;
    if mode == PositionMode::Hedged {
        if opts.reduce_only == Some(true) {
            trade_side = Some("Close");
            side = if buy { "Sell" } else { "Buy" };
        } else {
            trade_side = Some("Open");
        }
    }
    (side, trade_side)
}
// ts: KucoinExchange.getOrderSide
pub fn get_order_side(opts: &PlaceOrderOpts, mode: PositionMode) -> &'static str {
    match (mode, opts.reduce_only == Some(true), opts.side) {
        (PositionMode::OneWay, _, OrderSide::Buy) => "buy_single",
        (PositionMode::OneWay, _, OrderSide::Sell) => "sell_single",
        (_, true, OrderSide::Buy) => "close_short",
        (_, true, OrderSide::Sell) => "close_long",
        (_, false, OrderSide::Buy) => "open_long",
        (_, false, OrderSide::Sell) => "open_short",
    }
}
// ts: KucoinExchange.getAlgoOrderSide
pub fn get_algo_order_side(opts: &PlaceOrderOpts) -> Result<&'static str, String> {
    match (opts.r#type, opts.side) {
        (OrderType::StopLoss | OrderType::TakeProfit, OrderSide::Buy) => Ok("short"),
        (OrderType::StopLoss | OrderType::TakeProfit, OrderSide::Sell) => Ok("long"),
        (OrderType::TrailingStopLoss, OrderSide::Buy) => Ok("close_short"),
        (OrderType::TrailingStopLoss, OrderSide::Sell) => Ok("close_long"),
        _ => Err(format!(
            "Unknown algo order side: {} - {}",
            order_type_text(opts.r#type),
            if opts.side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            }
        )),
    }
}
// ts: KucoinExchange.getPositionForOrder
pub fn get_position_for_order<'a>(
    opts: &PlaceOrderOpts,
    positions: &'a [Position],
) -> Option<&'a Position> {
    let target_side = target_side(opts);
    positions
        .iter()
        .find(|p| p.symbol == opts.symbol && p.side == target_side)
}
// ts: KucoinExchange.getKucoinMarginMode
pub fn get_kucoin_margin_mode(position: Option<&Position>) -> Option<&'static str> {
    match position.and_then(|p| p.margin_type) {
        Some(MarginType::Cross) => Some("CROSS"),
        Some(MarginType::Isolated) => Some("ISOLATED"),
        None => None,
    }
}
// ts: KucoinExchange.getKucoinPositionSide
pub fn get_kucoin_position_side(
    opts: &PlaceOrderOpts,
    position: Option<&Position>,
) -> Option<&'static str> {
    if position.and_then(|p| p.position_mode) != Some(PositionMode::Hedged) {
        return None;
    }
    Some(if target_side(opts) == PositionSide::Long {
        "LONG"
    } else {
        "SHORT"
    })
}
// ts: KucoinExchange.mapStopPriceType
pub fn map_stop_price_type(trigger: Option<OrderStopTriggerType>) -> &'static str {
    match trigger {
        Some(OrderStopTriggerType::MarkPrice) => "MP",
        Some(OrderStopTriggerType::IndexPrice) => "IP",
        _ => "TP",
    }
}
// ts: KucoinExchange.getKucoinStopDirection
pub fn get_kucoin_stop_direction(opts: &PlaceOrderOpts) -> Result<&'static str, String> {
    match (opts.r#type, opts.side) {
        (OrderType::StopLoss, OrderSide::Buy) | (OrderType::TakeProfit, OrderSide::Sell) => {
            Ok("up")
        }
        (OrderType::StopLoss, OrderSide::Sell) | (OrderType::TakeProfit, OrderSide::Buy) => {
            Ok("down")
        }
        _ => Err(format!(
            "Unsupported KuCoin stop direction for order type: {}",
            order_type_text(opts.r#type)
        )),
    }
}
// ts: KucoinExchange.isVirtualTPSLOrder
pub fn is_virtual_tpsl_order(order: &Order) -> bool {
    order.id.ends_with("__stop_loss") || order.id.ends_with("__take_profit")
}
// ts: KucoinExchange.isAlgoOrder
pub fn is_algo_order(opts: &PlaceOrderOpts) -> bool {
    matches!(
        opts.r#type,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
fn target_side(opts: &PlaceOrderOpts) -> PositionSide {
    if (opts.reduce_only == Some(true) || is_algo_order(opts)) == (opts.side == OrderSide::Buy) {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
pub(crate) fn order_type_text(order_type: OrderType) -> &'static str {
    match order_type {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    }
}

// ts: KucoinExchange.parseMarginType
pub fn parse_margin_type(
    margin_mode: Option<&serde_json::Value>,
    cross_mode: Option<&serde_json::Value>,
) -> Result<MarginType, safe_cex_num::JsCoercionError> {
    let mode = if safe_cex_num::js_truthy(margin_mode) {
        safe_cex_num::js_string(margin_mode)?
    } else {
        String::new()
    }
    .to_uppercase();
    Ok(match mode.as_str() {
        "ISOLATED" | "FIXED" => MarginType::Isolated,
        "CROSS" | "CROSSED" => MarginType::Cross,
        _ if cross_mode == Some(&serde_json::Value::Bool(false)) => MarginType::Isolated,
        _ => MarginType::Cross,
    })
}
// ts: KucoinExchange.parsePositionMode
pub fn parse_position_mode(
    value: Option<&serde_json::Value>,
) -> Result<PositionMode, safe_cex_num::JsCoercionError> {
    let mode = if safe_cex_num::js_truthy(value) {
        safe_cex_num::js_string(value)?
    } else {
        String::new()
    }
    .to_uppercase();
    Ok(
        if matches!(mode.as_str(), "1" | "HEDGE_MODE" | "HEDGE" | "DOUBLE_HOLD") {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        },
    )
}
// ts: KucoinExchange.parseRowPositionMode
pub fn parse_row_position_mode(
    row: &serde_json::Map<String, serde_json::Value>,
) -> Result<PositionMode, safe_cex_num::JsCoercionError> {
    if safe_cex_num::js_truthy(row.get("positionSide"))
        && row.get("positionSide").and_then(serde_json::Value::as_str) != Some("BOTH")
    {
        return Ok(PositionMode::Hedged);
    }
    parse_position_mode(if safe_cex_num::js_truthy(row.get("positionMode")) {
        row.get("positionMode")
    } else {
        row.get("holdMode")
    })
}
