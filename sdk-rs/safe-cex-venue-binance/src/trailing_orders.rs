use safe_cex_num::{adjust, js_number_string, math_round};
use safe_cex_types::{
    Market, OrderSide, OrderType, PlaceOrderOpts, Position, PositionSide, Ticker,
};
use serde_json::{Map, Value};

// ts: BinanceExchange.formatCreateTrailingStopLossOrder
pub fn format_create_trailing_stop_loss_order(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    positions: &[Position],
    is_hedged: bool,
    uuid: impl FnOnce() -> String,
) -> Result<Vec<Map<String, Value>>, String> {
    format_trailing_order(opts, market, ticker, positions, is_hedged, uuid, false)
}

// ts: BinancePMExchange.formatCreateTrailingStopLossOrder
pub fn format_pm_create_trailing_stop_loss_order(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    positions: &[Position],
    is_hedged: bool,
    uuid: impl FnOnce() -> String,
) -> Result<Vec<Map<String, Value>>, String> {
    format_trailing_order(opts, market, ticker, positions, is_hedged, uuid, true)
}

fn format_trailing_order(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    positions: &[Position],
    is_hedged: bool,
    uuid: impl FnOnce() -> String,
    portfolio: bool,
) -> Result<Vec<Map<String, Value>>, String> {
    let side = if opts.side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    let position = positions
        .iter()
        .find(|p| p.symbol == opts.symbol && p.side == side);
    let market = market.ok_or_else(|| format!("Market {} not found", opts.symbol))?;
    let ticker = ticker.ok_or_else(|| format!("Ticker {} not found", opts.symbol))?;
    let position = position.ok_or_else(|| {
        format!(
            "Position {} and side {} not found",
            opts.symbol,
            if side == PositionSide::Short {
                "short"
            } else {
                "long"
            }
        )
    })?;
    // max(a,b)-min(a,b) has this magnitude, including NaN for equal infinities.
    let distance = adjust(
        (ticker.last - opts.price.unwrap_or(f64::NAN)).abs(),
        market.precision.price,
    );
    let percentage = math_round(((distance * 100.0) / ticker.last) * 10.0) / 10.0;
    let mut payload = Map::new();
    if !portfolio {
        payload.insert("algoType".into(), Value::String("CONDITIONAL".into()));
    }
    payload.insert("symbol".into(), Value::String(opts.symbol.clone()));
    let position_side = if portfolio {
        crate::pm_order_logic::get_order_position_side(is_hedged, opts)
    } else {
        crate::order_logic::get_order_position_side(is_hedged, opts)
    };
    payload.insert("positionSide".into(), Value::String(position_side.into()));
    let (sides, types) = if portfolio {
        (
            crate::binancepm_types::ORDER_SIDE,
            crate::binancepm_types::ORDER_TYPE,
        )
    } else {
        (
            crate::binance_types::ORDER_SIDE,
            crate::binance_types::ORDER_TYPE,
        )
    };
    if let Some((name, _)) = sides.iter().rfind(|(_, side)| *side == opts.side) {
        payload.insert("side".into(), Value::String((*name).into()));
    }
    if let Some((name, _)) = types
        .iter()
        .rfind(|(_, ty)| *ty == OrderType::TrailingStopLoss)
    {
        payload.insert("type".into(), Value::String((*name).into()));
    }
    payload.insert(
        "quantity".into(),
        Value::String(js_number_string(position.contracts)),
    );
    payload.insert(
        "callbackRate".into(),
        Value::String(js_number_string(percentage)),
    );
    payload.insert(
        "priceProtect".into(),
        Value::String(if portfolio { "true" } else { "TRUE" }.into()),
    );
    let id = uuid();
    if portfolio {
        payload.insert("newClientOrderId".into(), Value::String(id));
    } else {
        payload.insert(
            "clientAlgoId".into(),
            Value::String(format!("x-s3vFFRQp{id}")),
        );
    }
    Ok(vec![payload])
}
