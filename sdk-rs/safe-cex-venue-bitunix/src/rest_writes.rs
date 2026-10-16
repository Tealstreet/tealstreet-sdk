//! Write orchestration with caller-owned transport, store effects and error handling.
use safe_cex_num::{JsJsonValue, math_max, math_min};
use safe_cex_types::{Market, Position};
use std::future::Future;

fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .expect("Bitunix endpoint must exist")
        .1
}

pub async fn change_position_mode<E, R, F>(
    hedged: bool,
    position_contracts: impl IntoIterator<Item = Result<f64, E>>,
    post: impl FnOnce(&'static str, JsJsonValue) -> F,
    set_setting: impl FnOnce(&'static str, bool) -> Result<(), E>,
    emit_literal: impl FnOnce(&'static str) -> Result<(), E>,
    report: impl FnOnce(&E) -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<R, E>>,
{
    let mut open_positions = 0;
    for contracts in position_contracts {
        if contracts? > 0.0 {
            open_positions += 1;
        }
    }
    if open_positions > 0 {
        emit_literal("Please close all positions before switching position mode")?;
        return Ok(());
    }
    let result = async {
        post(
            endpoint("HEDGE_MODE"),
            JsJsonValue::Object(vec![(
                "positionMode".into(),
                JsJsonValue::String(if hedged { "HEDGE" } else { "ONE_WAY" }.into()),
            )]),
        )
        .await?;
        set_setting("isHedged", hedged)
    }
    .await;
    if let Err(error) = result {
        report(&error)?;
    }
    Ok(())
}

// UTF-16 units retain every source string, including lone surrogates.
pub async fn change_margin_type<E, R, F>(
    isolated: bool,
    symbol: Option<&[u16]>,
    post: impl FnOnce(&'static str, &[u16], &'static str, &'static str) -> F,
    reject_missing: impl FnOnce(&'static str) -> Result<E, E>,
    reject_post: impl FnOnce(&E) -> Result<E, E>,
) -> Result<(), E>
where
    F: Future<Output = Result<R, E>>,
{
    let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty()) else {
        return Err(reject_missing("Symbol is required")?);
    };
    match post(
        endpoint("SET_MARGIN_TYPE"),
        symbol,
        if isolated { "ISOLATION" } else { "CROSS" },
        "USDT",
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(error) => Err(reject_post(&error)?),
    }
}

pub async fn set_leverage<'a, E, R, F>(
    symbol: &str,
    input_leverage: f64,
    (market_by_symbol, position_by_symbol): (
        impl FnOnce(&str) -> Result<Option<&'a Market>, E>,
        impl FnOnce(&str) -> Result<Option<&'a Position>, E>,
    ),
    post: impl FnOnce(&'static str, JsJsonValue) -> F,
    update_positions: impl FnOnce([(String, f64); 2]) -> Result<(), E>,
    report: impl FnOnce(&E) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<String>,
    F: Future<Output = Result<R, E>>,
{
    let market = market_by_symbol(symbol)?;
    let position = position_by_symbol(symbol)?;
    let market = market.ok_or_else(|| E::from(format!("Market {symbol} not found")))?;
    let position = position.ok_or_else(|| E::from(format!("Position {symbol} not found")))?;
    let leverage = math_min(
        math_max(input_leverage, market.limits.leverage.min),
        market.limits.leverage.max,
    );
    if position.leverage != leverage {
        let result = async {
            post(
                endpoint("SET_LEVERAGE"),
                JsJsonValue::Object(vec![
                    ("marginCoin".into(), JsJsonValue::String("USDT".into())),
                    ("symbol".into(), JsJsonValue::String(symbol.into())),
                    ("leverage".into(), JsJsonValue::Number(leverage)),
                ]),
            )
            .await?;
            update_positions([
                (format!("{symbol}:long"), leverage),
                (format!("{symbol}:short"), leverage),
            ])
        }
        .await;
        if let Err(error) = result {
            report(&error)?;
        }
    }
    Ok(())
}

pub async fn cancel_symbol_orders<S, O, E, R, F>(
    symbol: &S,
    length: impl FnOnce() -> Result<usize, E>,
    mut order_at: impl FnMut(usize) -> Result<Option<O>, E>,
    mut matches_symbol: impl FnMut(&O, &S) -> Result<bool, E>,
    cancel: impl FnOnce(Vec<O>) -> F,
) -> Result<R, E>
where
    F: Future<Output = Result<R, E>>,
{
    let mut selected = Vec::new();
    let limit = length()?;
    for index in 0..limit {
        if let Some(order) = order_at(index)? {
            if matches_symbol(&order, symbol)? {
                selected.push(order);
            }
        }
    }
    cancel(selected).await
}

pub async fn place_orders<O, P, E, R, H, B>(
    assert_caller_ids: impl FnOnce() -> Result<(), E>,
    mut length: impl FnMut() -> Result<usize, E>,
    mut order_at: impl FnMut(usize) -> Result<Option<O>, E>,
    mut hydrate: impl FnMut(Option<O>) -> H,
    mut format: impl FnMut(O) -> Result<Vec<P>, E>,
    batch: impl FnOnce(Vec<P>) -> B,
) -> Result<R, E>
where
    H: Future<Output = Result<(), E>>,
    B: Future<Output = Result<R, E>>,
{
    assert_caller_ids()?;
    let mut index = 0;
    while index < length()? {
        hydrate(order_at(index)?).await?;
        index += 1;
    }
    let mut requests = Vec::new();
    let limit = length()?;
    for index in 0..limit {
        if let Some(order) = order_at(index)? {
            requests.extend(format(order)?);
        }
    }
    batch(requests).await
}
