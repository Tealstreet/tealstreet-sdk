//! Native request preflight and multi-order placement from BackpackExchange.
use crate::{
    order_utils::{is_ordinary_limit_order, map_time_in_force},
    types::{
        BackpackOrderRequest, BackpackOrderRequestOrderType, BackpackOrderRequestSide,
        BackpackOrderRequestTimeInForce,
    },
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderTimeInForce, PlaceOrderOpts};

#[derive(Clone)]
pub struct BatchMarket {
    pub id: String,
    pub active: bool,
    pub read_only: bool,
    pub amount_min: f64,
    pub amount_max: f64,
    pub price_precision: f64,
    pub amount_precision: f64,
}
fn format_value(
    value: f64,
    precision: f64,
    field: &str,
    symbol: &str,
) -> Result<String, ExchangeError> {
    if !precision.is_finite() || precision <= 0.0 {
        return Err(ExchangeError::new(format!(
            "Invalid {field} precision for {symbol}: {}",
            safe_cex_num::js_number_string(precision)
        )));
    }
    let factor = 1.0 / precision;
    let rounded = safe_cex_num::math_round(value * factor) / factor;
    let text = safe_cex_num::js_number_string(precision);
    let decimals = if precision >= 1.0 {
        0
    } else {
        text.find('.').map_or(0, |i| text.len() - i - 1)
    };
    let formatted = if decimals > 0 {
        safe_cex_num::js_to_fixed(rounded, decimals as f64).map_err(|message| {
            let mut e = ExchangeError::new(message);
            e.name = "RangeError".into();
            e
        })?
    } else {
        safe_cex_num::js_number_string(safe_cex_num::math_round(rounded))
    };
    let number = safe_cex_num::number_from_string(&formatted);
    if !number.is_finite() || number <= 0.0 {
        return Err(ExchangeError::new(format!(
            "Invalid formatted {field} for {symbol}: {formatted}"
        )));
    }
    Ok(formatted)
}
// ts: BackpackExchange#buildBatchLimitOrderRequest
pub fn build_batch_limit_order_request(
    opts: &PlaceOrderOpts,
    client_id: f64,
    lookup: impl FnOnce(&str) -> Result<Option<BatchMarket>, ExchangeError>,
) -> Result<BackpackOrderRequest, ExchangeError> {
    if !is_ordinary_limit_order(opts) {
        return Err(ExchangeError::new(
            "Backpack native batches only support ordinary LIMIT orders",
        ));
    }
    if !client_id.is_finite()
        || client_id.fract() != 0.0
        || client_id <= 0.0
        || client_id > f64::from(u32::MAX)
    {
        return Err(ExchangeError::new(format!(
            "Invalid Backpack batch clientId: {}",
            safe_cex_num::js_number_string(client_id)
        )));
    }
    let market = lookup(&opts.symbol)?
        .ok_or_else(|| ExchangeError::new(format!("Market not found: {}", opts.symbol)))?;
    if !market.active || market.read_only {
        return Err(ExchangeError::new(format!(
            "Market is not independently placeable: {}",
            opts.symbol
        )));
    }
    if !opts.amount.is_finite() || opts.amount <= 0.0 {
        return Err(ExchangeError::new(format!(
            "Invalid order amount: {}",
            safe_cex_num::js_number_string(opts.amount)
        )));
    }
    if opts.amount < market.amount_min || opts.amount > market.amount_max {
        return Err(ExchangeError::new(format!(
            "Order amount {} is outside {} bounds {}-{}",
            safe_cex_num::js_number_string(opts.amount),
            opts.symbol,
            safe_cex_num::js_number_string(market.amount_min),
            safe_cex_num::js_number_string(market.amount_max)
        )));
    }
    let price = match opts.price {
        Some(v) if v.is_finite() && v > 0.0 => v,
        v => {
            return Err(ExchangeError::new(format!(
                "Invalid limit order price: {}",
                v.map_or("undefined".into(), safe_cex_num::js_number_string)
            )));
        }
    };
    let post_only = opts.time_in_force == Some(OrderTimeInForce::PostOnly);
    let tif = match map_time_in_force(opts.time_in_force) {
        "IOC" => BackpackOrderRequestTimeInForce::Ioc,
        "FOK" => BackpackOrderRequestTimeInForce::Fok,
        _ => BackpackOrderRequestTimeInForce::Gtc,
    };
    Ok(BackpackOrderRequest {
        symbol: market.id,
        side: if opts.side == OrderSide::Buy {
            BackpackOrderRequestSide::Bid
        } else {
            BackpackOrderRequestSide::Ask
        },
        orderType: BackpackOrderRequestOrderType::Limit,
        price: Some(format_value(
            price,
            market.price_precision,
            "price",
            &opts.symbol,
        )?),
        quantity: Some(format_value(
            opts.amount,
            market.amount_precision,
            "amount",
            &opts.symbol,
        )?),
        timeInForce: Some(if post_only {
            BackpackOrderRequestTimeInForce::Gtc
        } else {
            tif
        }),
        postOnly: Some(post_only),
        clientId: Some(client_id),
        reduceOnly: opts.reduce_only.filter(|v| *v),
        triggerPrice: None,
        triggerQuantity: None,
        quoteQuantity: None,
        selfTradePrevention: None,
        triggerBy: None,
        takeProfitTriggerPrice: None,
        takeProfitLimitPrice: None,
        takeProfitTriggerBy: None,
        stopLossTriggerPrice: None,
        stopLossLimitPrice: None,
        stopLossTriggerBy: None,
    })
}

enum Placement<B, S> {
    Pair(B),
    Single(S),
    Done,
}
fn log_single<E>(
    error: E,
    format: &mut impl FnMut(&E) -> Result<Vec<u16>, E>,
    log: &mut impl FnMut(&[u16], &str) -> Result<(), E>,
) -> Result<(), E> {
    let mut message = "Error placing order in batch: "
        .encode_utf16()
        .collect::<Vec<_>>();
    message.extend(format(&error)?);
    log(&message, "error")
}
fn next_placement<T, E, B, S>(
    index: &mut usize,
    length: &mut impl FnMut() -> Result<usize, E>,
    element: &mut impl FnMut(usize) -> Result<Option<T>, E>,
    can_batch: &mut impl FnMut(&Option<T>) -> Result<bool, E>,
    pair: &mut impl FnMut([Option<T>; 2]) -> Result<B, E>,
    single: &mut impl FnMut(Option<T>) -> Result<S, E>,
    report: &mut impl FnMut(E, &str) -> Result<(), E>,
    format: &mut impl FnMut(&E) -> Result<Vec<u16>, E>,
    log: &mut impl FnMut(&[u16], &str) -> Result<(), E>,
) -> Result<Placement<B, S>, E> {
    while *index < length()? {
        let first = element(*index)?;
        let second = element(*index + 1)?;
        if second.is_some() && can_batch(&first)? && can_batch(&second)? {
            match pair([first, second]) {
                Ok(future) => return Ok(Placement::Pair(future)),
                Err(error) => report(error, "Failed to place Backpack native order pair")?,
            }
            *index += 2;
        } else {
            match single(first) {
                Ok(future) => return Ok(Placement::Single(future)),
                Err(error) => log_single(error, format, log)?,
            }
            *index += 1;
        }
    }
    Ok(Placement::Done)
}
// ts: BackpackExchange.placeOrders
pub fn place_orders<'a, T: 'a, I: 'a, E: 'a, B, S>(
    assert_ids: impl FnOnce() -> Result<(), E>,
    mut length: impl FnMut() -> Result<usize, E> + 'a,
    mut element: impl FnMut(usize) -> Result<Option<T>, E> + 'a,
    mut can_batch: impl FnMut(&Option<T>) -> Result<bool, E> + 'a,
    mut pair: impl FnMut([Option<T>; 2]) -> Result<B, E> + 'a,
    mut single: impl FnMut(Option<T>) -> Result<S, E> + 'a,
    mut report: impl FnMut(E, &str) -> Result<(), E> + 'a,
    mut format: impl FnMut(&E) -> Result<Vec<u16>, E> + 'a,
    mut log: impl FnMut(&[u16], &str) -> Result<(), E> + 'a,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<I>, E>> + 'a>>
where
    B: std::future::Future<Output = Result<Vec<I>, E>> + 'a,
    S: std::future::Future<Output = Result<Vec<I>, E>> + 'a,
{
    let mut index = 0;
    let first = assert_ids().and_then(|()| {
        next_placement(
            &mut index,
            &mut length,
            &mut element,
            &mut can_batch,
            &mut pair,
            &mut single,
            &mut report,
            &mut format,
            &mut log,
        )
    });
    Box::pin(async move {
        let mut current = first?;
        let mut ids = Vec::new();
        loop {
            match current {
                Placement::Done => return Ok(ids),
                Placement::Pair(response) => {
                    match response.await {
                        Ok(next) => ids.extend(next),
                        Err(error) => report(error, "Failed to place Backpack native order pair")?,
                    }
                    index += 2;
                }
                Placement::Single(response) => {
                    match response.await {
                        Ok(next) => ids.extend(next),
                        Err(error) => log_single(error, &mut format, &mut log)?,
                    }
                    index += 1;
                }
            }
            current = next_placement(
                &mut index,
                &mut length,
                &mut element,
                &mut can_batch,
                &mut pair,
                &mut single,
                &mut report,
                &mut format,
                &mut log,
            )?;
        }
    })
}
