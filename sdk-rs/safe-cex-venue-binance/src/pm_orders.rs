use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{
    OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PlaceOrderOpts,
};
use serde_json::{Map, Value};

// ts: BinancePMExchange.formatCreateOrder
pub fn format_pm_create_order(
    base: &BaseExchange,
    opts: &PlaceOrderOpts,
    mut uuid: impl FnMut() -> String,
) -> Result<Vec<Map<String, Value>>, ExchangeError> {
    let client_order_id = base.resolve_caller_client_order_id(opts)?;
    let market = base.store.markets_by_symbol().get(&opts.symbol).cloned();
    let is_hedged = base.store.options().is_hedged;
    if opts.r#type == OrderType::TrailingStopLoss {
        let ticker = base.store.tickers_by_symbol().get(&opts.symbol).cloned();
        let positions = base.store.positions().to_vec();
        return crate::trailing_orders::format_pm_create_trailing_stop_loss_order(
            opts,
            market.as_ref(),
            ticker.as_ref(),
            &positions,
            is_hedged,
            uuid,
        )
        .map_err(ExchangeError::new);
    }
    let market =
        market.ok_or_else(|| ExchangeError::new(format!("Market {} not found", opts.symbol)))?;
    let stop = matches!(opts.r#type, OrderType::StopLoss | OrderType::TakeProfit);
    let amount = adjust(opts.amount, market.precision.amount);
    let price = opts
        .price
        .filter(|n| *n != 0.0 && !n.is_nan())
        .filter(|_| opts.r#type != OrderType::Market)
        .map(|price| adjust(price, market.precision.price));
    let reduce_only = !is_hedged && opts.reduce_only == Some(true);
    let mut request = Map::new();
    insert(&mut request, "symbol", opts.symbol.clone());
    insert(
        &mut request,
        "positionSide",
        crate::pm_order_logic::get_order_position_side(is_hedged, opts),
    );
    insert(&mut request, "side", side(opts.side));
    if let Some((name, _)) = crate::binancepm_types::ORDER_TYPE
        .iter()
        .rfind(|(_, value)| *value == opts.r#type)
    {
        insert(&mut request, "type", *name);
    }
    if amount != 0.0 && !amount.is_nan() {
        insert(&mut request, "quantity", js_number_string(amount));
    }
    if let Some(price) = price.filter(|n| *n != 0.0 && !n.is_nan()) {
        insert(
            &mut request,
            if stop { "stopPrice" } else { "price" },
            js_number_string(price),
        );
    }
    if opts.r#type == OrderType::Limit
        && let Some((name, _)) = crate::binancepm_types::TIME_IN_FORCE
            .iter()
            .rfind(|(_, value)| {
                *value
                    == opts
                        .time_in_force
                        .unwrap_or(OrderTimeInForce::GoodTillCancel)
            })
    {
        insert(&mut request, "timeInForce", *name);
    }
    if stop && (amount == 0.0 || amount.is_nan()) {
        insert(&mut request, "closePosition", "true");
    }
    if reduce_only && (!stop || (amount != 0.0 && !amount.is_nan())) {
        insert(&mut request, "reduceOnly", "true");
    }
    if stop {
        insert(
            &mut request,
            "workingType",
            working_type(opts, opts.r#type == OrderType::TakeProfit),
        );
    }
    let max = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
    let lots = if amount > max {
        (amount / max).ceil()
    } else {
        1.0
    };
    let rest = if amount > max {
        adjust(amount % max, market.precision.amount)
    } else {
        0.0
    };
    let lot_size = adjust((amount - rest) / lots, market.precision.amount);
    let mut payloads = Vec::new();
    // ts-bug: BinancePMExchange.formatCreateOrder retains quantity alongside closePosition.
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
        (opts.stop_loss, OrderType::StopLoss),
        (opts.take_profit, OrderType::TakeProfit),
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
            if let Some((name, _)) = crate::binancepm_types::ORDER_TYPE
                .iter()
                .rfind(|(_, value)| *value == kind)
            {
                insert(&mut payload, "type", *name);
            }
            insert(
                &mut payload,
                "stopPrice",
                js_number_string(adjust(price, market.precision.price)),
            );
            insert(&mut payload, "timeInForce", "GTC");
            insert(&mut payload, "closePosition", "true");
            insert(
                &mut payload,
                "workingType",
                working_type(opts, kind == OrderType::TakeProfit),
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
fn working_type(opts: &PlaceOrderOpts, take_profit: bool) -> &'static str {
    if if take_profit {
        opts.tp_trigger_type == Some(OrderStopTriggerType::MarkPrice)
    } else {
        opts.sl_trigger_type != Some(OrderStopTriggerType::LastPrice)
    } {
        "MARK_PRICE"
    } else {
        "CONTRACT_PRICE"
    }
}
