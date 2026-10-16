use crate::task_types::TwapOptions;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::{OrderType, PlaceOrderOpts};

// ts: TwapInstance.sliceIntervalMs
pub fn slice_interval_ms(options: &TwapOptions) -> f64 {
    let orders_pace = options.duration / options.orders_count;
    orders_pace * 60.0 * 1000.0
}

// ts: TwapInstance.fireMarketRemainder
pub async fn fire_market_remainder(exchange: &dyn Exchange, options: &TwapOptions, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    let order = PlaceOrderOpts {
        symbol: options.active_market.symbol.clone(),
        side: options.side,
        r#type: OrderType::Market,
        amount,
        reduce_only: Some(options.reduce_only),
        price: None,
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
    };
    // The source awaits placement but swallows its rejection so later slices can run.
    let _ = exchange.base().place_order(exchange, order).await;
}

// ts: TwapInstance.createNewState
pub fn create_new_state(
    options: &TwapOptions,
    runtime: &dyn safe_cex_exchange::runtime::ExchangeRuntime,
) -> Result<crate::task_types::TwapState, safe_cex_exchange::runtime::ExchangeError> {
    use crate::task_types::{TwapOrigin, TwapState, TwapStatus};
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_num::{JsPrimitive, add, lodash_random, math_max, multiply, subtract};
    let array_length = math_max(0.0, (options.orders_count / 2.0).floor());
    let range_error = |message: &str| {
        let mut error = ExchangeError::new(message);
        error.name = "RangeError".into();
        error
    };
    if !array_length.is_finite() || array_length > f64::from(u32::MAX) {
        return Err(range_error("Invalid array length"));
    }
    let pairs = array_length as usize;
    let odd = options.orders_count % 2.0 != 0.0;
    let capacity = pairs
        .checked_mul(2)
        .and_then(|n| n.checked_add(usize::from(odd)))
        .ok_or_else(|| range_error("TWAP slice allocation exceeds host capacity"))?;
    let mut orders_sizes = Vec::new();
    orders_sizes
        .try_reserve_exact(capacity)
        .map_err(|_| range_error("TWAP slice allocation exceeds host capacity"))?;
    let order_size = options.size / options.orders_count;
    for _ in 0..pairs {
        let draw = lodash_random(
            &JsPrimitive::Number(0.0),
            &JsPrimitive::Number(options.randomness_percentage),
            &JsPrimitive::Undefined,
            || runtime.random(),
        )
        .map_err(|error| {
            let mut converted = ExchangeError::new(error.message);
            converted.name = error.name.into();
            converted
        })?;
        let size_mod = multiply(draw, 0.01);
        let diff = multiply(order_size, size_mod).abs();
        orders_sizes.push(add(order_size, diff));
        orders_sizes.push(subtract(order_size, diff));
    }
    if odd {
        orders_sizes.push(options.size / options.orders_count);
    }
    let size = if options.size == 0.0 || options.size.is_nan() {
        0.0
    } else {
        options.size
    };
    let remaining_size = size - orders_sizes.iter().fold(0.0, |sum, size| sum + size);
    // ts-bug: TwapInstance.createNewState writes a named "-1" property on empty arrays.
    // TwapState's Vec omits that property; the manifest retains the witnessed FINDING.
    if let Some(last) = orders_sizes.last_mut() {
        *last += remaining_size;
    }
    Ok(TwapState {
        account_name: options.account_name.clone(),
        active_market: options.active_market.clone(),
        side: options.side,
        size: options.size,
        status: TwapStatus::Idle,
        orders_sizes,
        orders_count: options.orders_count,
        orders_executed: 0.0,
        next_order_at: 0.0,
        origin: Some(options.origin.unwrap_or(TwapOrigin::Web)),
        created_at: Some(runtime.now_ms()),
        started_at: None,
        mark_for_cleanup: None,
    })
}
