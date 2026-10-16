//! Futures payload formatting against production Store and core ID validation.
use crate::order_helpers::{
    get_kucoin_margin_mode, get_kucoin_position_side, get_kucoin_stop_direction,
    get_position_for_order, map_stop_price_type,
};
use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::{JsJsonValue as J, TableHit, adjust, js_number_string, js_table_lookup};
use safe_cex_types::{Market, OrderSide, OrderTimeInForce, OrderType, PlaceOrderOpts};
fn text(value: impl Into<String>) -> J {
    J::String(value.into())
}
fn optional(value: Option<&str>) -> J {
    value.map_or(J::Undefined, text)
}

// Own store entries shadow the shared standard prototype. No venue-local
// prototype list or setter is maintained here.
fn market_for(base: &BaseExchange, symbol: &str) -> Result<TableHit<Market>, ExchangeError> {
    let markets = base.store.markets_by_symbol();
    // Only the queried own entry matters; avoid scanning every market per order.
    let own = markets.get(symbol).map(|market| (symbol, market));
    let hit = js_table_lookup(own.as_slice(), symbol);
    if !hit.is_truthy_by(|_| true) {
        return Err(ExchangeError::new(format!("Market {symbol} not found")));
    }
    Ok(match hit {
        TableHit::Mapped(market) => TableHit::Mapped((**market).clone()),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    })
}
fn market_id(market: &TableHit<Market>) -> J {
    match market {
        TableHit::Mapped(market) => text(&market.id),
        _ => J::Undefined,
    }
}
fn precision(market: &TableHit<Market>, price: bool) -> Result<f64, ExchangeError> {
    match market {
        TableHit::Mapped(market) => Ok(if price {
            market.precision.price
        } else {
            market.precision.amount
        }),
        _ => Err(ExchangeError::new(format!(
            "Cannot read properties of undefined (reading '{}')",
            if price { "price" } else { "amount" }
        ))),
    }
}

// ts-bug: KucoinExchange.formatCreateOrder inherited market keys pass the TS guard.
// Shared TableHit retains the inherited hit without manufacturing a typed Market.
// ts: KucoinExchange.formatCreateOrder
pub fn format_create_order(
    base: &BaseExchange,
    opts: &PlaceOrderOpts,
) -> Result<Vec<J>, ExchangeError> {
    let market = market_for(base, &opts.symbol)?;
    let position = get_position_for_order(opts, &base.store.positions()).cloned();
    let leverage = position
        .as_ref()
        .map(|p| p.leverage)
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(1.0);
    // Source reads price precision even for market orders or zero amounts.
    let price_precision = precision(&market, true)?;
    let amount_precision = precision(&market, false)?;
    let amount = adjust(opts.amount, amount_precision);
    let price = opts
        .price
        .filter(|v| *v != 0.0 && !v.is_nan() && opts.r#type != OrderType::Market)
        .map(|v| adjust(v, price_precision));
    let buy = opts.side == OrderSide::Buy;
    let up = if buy {
        opts.take_profit
    } else {
        opts.stop_loss
    }
    .filter(|v| *v != 0.0 && !v.is_nan());
    let down = if buy {
        opts.stop_loss
    } else {
        opts.take_profit
    }
    .filter(|v| *v != 0.0 && !v.is_nan());
    let tif = match opts.time_in_force {
        Some(OrderTimeInForce::ImmediateOrCancel) => "IOC",
        Some(OrderTimeInForce::FillOrKill) => "FOK",
        _ => "GTC",
    };
    let mut payload = vec![
        ("symbol".into(), market_id(&market)),
        (
            "type".into(),
            text(if opts.r#type == OrderType::Market {
                "market"
            } else {
                "limit"
            }),
        ),
        ("side".into(), text(if buy { "buy" } else { "sell" })),
        (
            "price".into(),
            price
                .filter(|v| *v != 0.0 && !v.is_nan())
                .map_or(J::Undefined, |v| text(js_number_string(v))),
        ),
        (
            "size".into(),
            if amount != 0.0 && !amount.is_nan() {
                text(js_number_string(amount))
            } else {
                J::Undefined
            },
        ),
        ("leverage".into(), J::Number(leverage)),
        (
            "reduceOnly".into(),
            opts.reduce_only.map_or(J::Undefined, J::Bool),
        ),
        (
            "marginMode".into(),
            optional(get_kucoin_margin_mode(position.as_ref())),
        ),
        (
            "positionSide".into(),
            optional(get_kucoin_position_side(opts, position.as_ref())),
        ),
        (
            "timeInForce".into(),
            if opts.r#type == OrderType::Market {
                J::Undefined
            } else {
                text(tif)
            },
        ),
        (
            "clientOid".into(),
            text(
                base.resolve_caller_client_order_id(opts)?
                    .unwrap_or_else(|| base.runtime.unique_id()),
            ),
        ),
        (
            "triggerStopUpPrice".into(),
            up.map_or(J::Undefined, |v| {
                text(js_number_string(adjust(v, price_precision)))
            }),
        ),
        (
            "triggerStopDownPrice".into(),
            down.map_or(J::Undefined, |v| {
                text(js_number_string(adjust(v, price_precision)))
            }),
        ),
        (
            "stopPriceType".into(),
            if up.is_some() || down.is_some() {
                text(map_stop_price_type(
                    opts.tp_trigger_type.or(opts.sl_trigger_type),
                ))
            } else {
                J::Undefined
            },
        ),
        (
            "postOnly".into(),
            if opts.time_in_force == Some(OrderTimeInForce::PostOnly) {
                J::Bool(true)
            } else {
                J::Undefined
            },
        ),
    ];
    payload.retain(|(_, v)| !matches!(v, J::Undefined));
    Ok(vec![J::Object(payload)])
}

// ts-bug: KucoinExchange.formatAlgoOrder inherited market keys consume a UUID.
// Shared TableHit retains the inherited hit without manufacturing a typed Market.
// ts: KucoinExchange.formatAlgoOrder
pub fn format_algo_order(base: &BaseExchange, opts: &PlaceOrderOpts) -> Result<J, ExchangeError> {
    let market = market_for(base, &opts.symbol)?;
    let mut payload = vec![
        ("symbol".into(), market_id(&market)),
        ("clientOid".into(), text(base.runtime.unique_id())),
        (
            "side".into(),
            text(if opts.side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            }),
        ),
        ("type".into(), text("market")),
        (
            "reduceOnly".into(),
            J::Bool(opts.reduce_only.unwrap_or(true)),
        ),
    ];
    if opts.amount == 0.0 || opts.amount.is_nan() {
        payload.push(("closeOrder".into(), J::Bool(true)));
    } else {
        payload.push((
            "size".into(),
            text(js_number_string(adjust(
                opts.amount,
                precision(&market, false)?,
            ))),
        ));
    }
    if matches!(opts.r#type, OrderType::TakeProfit | OrderType::StopLoss)
        && let Some(price) = opts.price.filter(|v| *v != 0.0 && !v.is_nan())
    {
        let price = adjust(price, precision(&market, true)?);
        let position = get_position_for_order(opts, &base.store.positions()).cloned();
        payload.extend([
            (
                "stop".into(),
                text(get_kucoin_stop_direction(opts).map_err(ExchangeError::new)?),
            ),
            ("stopPrice".into(), text(js_number_string(price))),
            (
                "stopPriceType".into(),
                text(map_stop_price_type(
                    if opts.r#type == OrderType::TakeProfit {
                        opts.tp_trigger_type
                    } else {
                        opts.sl_trigger_type
                    },
                )),
            ),
            (
                "marginMode".into(),
                optional(get_kucoin_margin_mode(position.as_ref())),
            ),
            (
                "positionSide".into(),
                optional(get_kucoin_position_side(opts, position.as_ref())),
            ),
        ]);
    }
    if opts.r#type == OrderType::TrailingStopLoss {
        return Err(ExchangeError::new(
            "KuCoin Futures trailing stop placement is not exposed in the current REST documentation",
        ));
    }
    Ok(J::Object(payload))
}
