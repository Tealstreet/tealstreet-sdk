use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType, PlaceOrderOpts};
use serde_json::{Map, Value};

// ts: BinanceSpotExchange.formatCreateOrder
pub fn format_spot_create_order(
    base: &BaseExchange,
    opts: &PlaceOrderOpts,
    mut uuid: impl FnMut() -> String,
) -> Result<Vec<Map<String, Value>>, ExchangeError> {
    let client_order_id = base.resolve_caller_client_order_id(opts)?;
    if opts.r#type == OrderType::TrailingStopLoss {
        return Err(ExchangeError::new(
            "Binance Spot trailing-stop placement requires trailingDelta and is not supported",
        ));
    }
    let market = base
        .store
        .markets_by_symbol()?
        .get(&opts.symbol)
        .cloned()
        .ok_or_else(|| ExchangeError::new(format!("Market {} not found", opts.symbol)))?;
    let amount = adjust(opts.amount, market.precision.amount);
    if !amount.is_finite() || amount <= 0.0 {
        return base.reject_preflight(format!(
            "Binance Spot orders require an explicit quantity greater than zero for {}",
            opts.symbol,
        ));
    }
    let price = opts
        .price
        .filter(|n| *n != 0.0 && !n.is_nan())
        .filter(|_| opts.r#type != OrderType::Market)
        .map(|p| adjust(p, market.precision.price));
    let tif = opts
        .time_in_force
        .unwrap_or(OrderTimeInForce::GoodTillCancel);
    let order_type = match opts.r#type {
        OrderType::Limit if tif == OrderTimeInForce::PostOnly => "LIMIT_MAKER",
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        OrderType::StopLoss => "STOP_LOSS",
        OrderType::TakeProfit => "TAKE_PROFIT",
        _ => {
            return Err(ExchangeError::new(
                "Binance Spot does not support conditional orders",
            ));
        }
    };
    let mut request = Map::new();
    insert(&mut request, "symbol", opts.symbol.clone());
    insert(&mut request, "side", side(opts.side));
    insert(&mut request, "type", order_type);
    insert(&mut request, "quantity", js_number_string(amount));
    let price_field = if matches!(opts.r#type, OrderType::StopLoss | OrderType::TakeProfit) {
        "stopPrice"
    } else {
        "price"
    };
    if let Some(price) = price.filter(|n| *n != 0.0 && !n.is_nan()) {
        insert(&mut request, price_field, js_number_string(price));
    }
    if opts.r#type == OrderType::Limit
        && tif != OrderTimeInForce::PostOnly
        && let Some((name, _)) = crate::binance_types::TIME_IN_FORCE
            .iter()
            .rfind(|(_, value)| *value == tif)
    {
        insert(&mut request, "timeInForce", *name);
    }
    let max = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
    let split = max > 0.0 && amount > max;
    let lots = if split { (amount / max).ceil() } else { 1.0 };
    let rest = if split {
        adjust(amount % max, market.precision.amount)
    } else {
        0.0
    };
    let lot_size = adjust((amount - rest) / lots, market.precision.amount);
    let mut payloads = Vec::new();
    // lodash times returns [] for nonfinite counts or counts above MAX_SAFE_INTEGER.
    if (1.0..=9_007_199_254_740_991.0).contains(&lots) {
        for _ in 0..lots as usize {
            let mut payload = request.clone();
            insert(&mut payload, "quantity", js_number_string(lot_size));
            payloads.push(payload);
        }
    }
    if rest != 0.0 && !rest.is_nan() {
        let mut payload = request.clone();
        insert(&mut payload, "quantity", js_number_string(rest));
        payloads.push(payload);
    }
    for (price, kind) in [
        (opts.stop_loss, "STOP_LOSS"),
        (opts.take_profit, "TAKE_PROFIT"),
    ] {
        if let Some(price) = price.filter(|n| *n != 0.0 && !n.is_nan()) {
            let mut payload = request.clone();
            payload.shift_remove("price");
            insert(
                &mut payload,
                "side",
                side(if opts.side == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                }),
            );
            insert(&mut payload, "type", kind);
            insert(
                &mut payload,
                "stopPrice",
                js_number_string(adjust(price, market.precision.price)),
            );
            payloads.push(payload);
        }
    }
    base.assert_single_native_client_order_id(client_order_id.as_deref(), payloads.len())?;
    for payload in &mut payloads {
        insert(
            payload,
            "newClientOrderId",
            client_order_id
                .clone()
                .unwrap_or_else(|| format!("x-s3vFFRQp{}", uuid())),
        );
    }
    Ok(payloads)
}
fn insert(payload: &mut Map<String, Value>, key: &str, value: impl Into<String>) {
    payload.insert(key.into(), Value::String(value.into()));
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
