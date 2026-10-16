use safe_cex_types::{OrderSide, OrderType, PlaceOrderOpts};
use serde_json::Value;

// ts: BinanceExchange.isAlgoOrderType
pub fn is_algo_order_type(order_type: OrderType) -> bool {
    matches!(
        order_type,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}

// ts: BinanceExchange.getOrderPositionSide
pub fn get_order_position_side(is_hedged: bool, opts: &PlaceOrderOpts) -> &'static str {
    if !is_hedged {
        return "BOTH";
    }
    let long = opts.side == OrderSide::Buy;
    let flip = is_algo_order_type(opts.r#type) || opts.reduce_only.unwrap_or(false);
    if long != flip { "LONG" } else { "SHORT" }
}

// ts: isBinancePositionModeAlreadySetError
pub fn is_binance_position_mode_already_set_error(error: Option<&Value>) -> bool {
    let code = error
        .and_then(|e| e.get("response"))
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("code"));
    matches!(code, Some(Value::String(s)) if s == "-4059")
        || matches!(code, Some(Value::Number(n)) if n.as_f64() == Some(-4059.0))
}

// ts: BinanceExchange.detectBalanceIssue (deprecated source method)
pub fn detect_balance_issue(_account_data: Option<&Value>) -> bool {
    false
}

// ts: BinanceExchange.isPostOnlyRejectionError
pub fn is_post_only_rejection_error(
    error: Option<&Value>,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let data = error
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"));
    let code = data
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .or_else(|| error.and_then(|v| v.get("code")));
    if matches!(code, Some(Value::Number(n)) if n.as_f64() == Some(-5022.0)) {
        return Ok(true);
    }
    let empty = Value::String(String::new());
    let message = data
        .and_then(|v| v.get("msg"))
        .filter(|v| safe_cex_num::js_truthy(Some(v)))
        .or_else(|| {
            error
                .and_then(|v| v.get("message"))
                .filter(|v| safe_cex_num::js_truthy(Some(v)))
        })
        .unwrap_or(&empty);
    let message = safe_cex_num::js_string(Some(message))?.to_lowercase();
    Ok(message.contains("post only order will be rejected")
        || message.contains("could not be executed as maker"))
}
