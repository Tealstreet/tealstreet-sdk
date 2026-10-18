//! Primary/inverse position-control bodies; host calls retain source sequencing.
//! JSON records exclude executable prototypes/getters. Futures begin at first poll.
use safe_cex_num::{js_number, js_number_string, js_string, js_truthy, math_max, math_min};
use safe_cex_types::MarginType;
use serde_json::{Map, Value, json};
use std::{future::Future, rc::Rc};

type Error = Rc<Value>;
/// Retained fetched batch: update callbacks receive the original allocation.
pub type Positions = Rc<Vec<Value>>;
fn error(name: &str, message: impl Into<String>) -> Error {
    Rc::new(json!({"name":name,"message":message.into()}))
}
fn field<'a>(data: &'a Value, key: &str) -> Result<Option<&'a Value>, Error> {
    crate::json_field::field(data, key).map_err(|m| error("TypeError", m))
}
fn message_error(message: Option<&Value>) -> Error {
    match message {
        None => error("Error", ""),
        Some(v) => match js_string(Some(v)) {
            Ok(s) => error("Error", s),
            Err(e) => error("TypeError", e.message),
        },
    }
}
fn endpoint(key: &str) -> &'static str {
    crate::types::ENDPOINTS[key].as_str().unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn set_leverage_impl<Post, Fetch>(
    symbol: &str,
    input_leverage: f64,
    market_limits: impl FnOnce(&str) -> Result<Option<(f64, f64)>, Error>,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    mut post: impl FnMut(&str, Map<String, Value>) -> Result<Post, Error>,
    mut fetch: impl FnMut(Option<Vec<String>>) -> Result<Fetch, Error>,
    mut upsert: impl FnMut(Positions) -> Result<(), Error>,
    mut emit: impl FnMut(Option<Value>) -> Result<(), Error>,
    accept_not_modified: bool,
) -> Result<(), Error>
where
    Post: Future<Output = Result<Rc<Value>, Error>>,
    Fetch: Future<Output = Result<Positions, Error>>,
{
    let Some((min, max)) = market_limits(symbol)? else {
        return Err(error("Error", format!("Market {symbol} not found")));
    };
    let mut found: Vec<Value> = positions()?
        .iter()
        .filter(|p| p.get("symbol").and_then(Value::as_str) == Some(symbol))
        .cloned()
        .collect();
    if found.is_empty() {
        let fetched = fetch(Some(vec![symbol.into()]))?.await?;
        upsert(Rc::clone(&fetched))?;
        found = fetched.as_ref().clone();
    }
    // Position.leverage is a declared number; missing/null defaults to one.
    let current = found
        .first()
        .and_then(|p| p.get("leverage"))
        .filter(|v| !v.is_null())
        .map(|v| {
            safe_cex_num::numeric_ingress::js_value_as_f64(v)
                .expect("Position.leverage is a number")
        })
        .unwrap_or(1.);
    let leverage = math_min(math_max(input_leverage, min), max);
    if current != leverage {
        let data = post(endpoint("SET_LEVERAGE"), json!({"category":category(symbol)?,"symbol":symbol,"buyLeverage":js_number_string(leverage),"sellLeverage":js_number_string(leverage)}).as_object().unwrap().clone())?.await?;
        if field(&data, "retMsg")?.and_then(Value::as_str) != Some("OK")
            && !(accept_not_modified
                && crate::failure_diagnostics::is_bybit_leverage_not_modified(data.as_object())
                    .map_err(|m| error("TypeError", m))?)
        {
            emit(field(&data, "retMsg")?.cloned())?;
            return Err(message_error(field(&data, "retMsg")?));
        }
        let refreshed = fetch(Some(vec![symbol.into()]))?.await?;
        upsert(refreshed)?;
    }
    Ok(())
}

// ts: BybitExchange#refuseOptionSymbol
fn refuse_option_symbol(symbol: Option<&str>, what: &str) -> Result<(), Error> {
    match symbol {
        Some(symbol) if !symbol.is_empty() && crate::options::is_bybit_option_symbol(symbol) => {
            Err(error(
                "Error",
                format!("Bybit has no {what} for options ({symbol})"),
            ))
        }
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn set_leverage<Post, Fetch>(
    symbol: &str,
    input_leverage: f64,
    market_limits: impl FnOnce(&str) -> Result<Option<(f64, f64)>, Error>,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    post: impl FnMut(&str, Map<String, Value>) -> Result<Post, Error>,
    fetch: impl FnMut(Option<Vec<String>>) -> Result<Fetch, Error>,
    upsert: impl FnMut(Positions) -> Result<(), Error>,
    emit: impl FnMut(Option<Value>) -> Result<(), Error>,
) -> Result<(), Error>
where
    Post: Future<Output = Result<Rc<Value>, Error>>,
    Fetch: Future<Output = Result<Positions, Error>>,
{
    refuse_option_symbol(Some(symbol), "leverage")?;
    set_leverage_impl(
        symbol,
        input_leverage,
        market_limits,
        positions,
        category,
        post,
        fetch,
        upsert,
        emit,
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn set_inverse_leverage<Post, Fetch>(
    symbol: &str,
    input_leverage: f64,
    market_limits: impl FnOnce(&str) -> Result<Option<(f64, f64)>, Error>,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    post: impl FnMut(&str, Map<String, Value>) -> Result<Post, Error>,
    fetch: impl FnMut(Option<Vec<String>>) -> Result<Fetch, Error>,
    upsert: impl FnMut(Positions) -> Result<(), Error>,
    emit: impl FnMut(Option<Value>) -> Result<(), Error>,
) -> Result<(), Error>
where
    Post: Future<Output = Result<Rc<Value>, Error>>,
    Fetch: Future<Output = Result<Positions, Error>>,
{
    set_leverage_impl(
        symbol,
        input_leverage,
        market_limits,
        positions,
        category,
        post,
        fetch,
        upsert,
        emit,
        false,
    )
    .await
}

/// The bybit connection refuses option and inverse symbols before the shared body.
pub async fn change_primary_position_mode<F: Future<Output = Result<(), Error>>>(
    hedged: bool,
    symbol: Option<&str>,
    helper: impl FnOnce(bool, &str, &str) -> Result<F, Error>,
) -> Result<(), Error> {
    refuse_option_symbol(symbol, "position mode")?;
    if let Some(symbol) = symbol.filter(|s| crate::inverse_symbol::is_bybit_inverse_symbol(s)) {
        return Err(error(
            "Error",
            format!("Bybit position mode can only be changed on linear markets here ({symbol})"),
        ));
    }
    change_position_mode(hedged, symbol, helper).await
}

/// Source explicitly clears its symbol argument, including the inverse variant.
pub async fn change_position_mode<F: Future<Output = Result<(), E>>, E>(
    hedged: bool,
    _symbol: Option<&str>,
    helper: impl FnOnce(bool, &str, &str) -> Result<F, E>,
) -> Result<(), E> {
    helper(hedged, "linear", "USDT")?.await
}

#[allow(clippy::too_many_arguments)]
pub async fn change_position_mode_helper<Post, Fetch, P>(
    hedged: bool,
    category: Option<&str>,
    coin: Option<&str>,
    symbol: Option<&str>,
    post: impl FnOnce(&str, Map<String, Value>) -> Result<Post, Error>,
    mut fetch: impl FnMut(Option<Vec<String>>) -> Result<Fetch, Error>,
    update: impl FnOnce(P) -> Result<(), Error>,
    upsert: impl FnOnce(P) -> Result<(), Error>,
    emit: impl FnOnce(Option<Value>) -> Result<(), Error>,
) -> Result<(), Error>
where
    Post: Future<Output = Result<Rc<Value>, Error>>,
    Fetch: Future<Output = Result<P, Error>>,
{
    let mut payload = Map::new();
    for (key, value) in [("category", category), ("coin", coin), ("symbol", symbol)] {
        if let Some(value) = value {
            payload.insert(key.into(), value.into());
        }
    }
    payload.insert("mode".into(), Value::from(if hedged { 3 } else { 0 }));
    let data = post(endpoint("SET_POSITION_MODE"), payload)?.await?;
    let positions = fetch(None)?.await?;
    update(positions)?;
    if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
        let positions = fetch(Some(vec![symbol.into()]))?.await?;
        upsert(positions)?;
    }
    let message = data.get("retMsg");
    if message.and_then(Value::as_str) == Some("All symbols switched successfully.") {
        return Ok(());
    }
    emit(message.cloned())?;
    if message.and_then(Value::as_str)
        != Some(
            "Partial symbols switched successfully, excluding symbols with open orders or positions.",
        )
    {
        return Err(message_error(message));
    }
    Ok(())
}

pub async fn change_margin_type<Post, Fetch, P>(
    isolated: bool,
    _symbol: Option<&str>,
    post: impl FnOnce(&str, Map<String, Value>) -> Result<Post, Error>,
    emit: impl FnOnce(Option<Value>) -> Result<(), Error>,
    set_margin_type: impl FnOnce(MarginType) -> Result<(), Error>,
    fetch: impl FnOnce(Option<Vec<String>>) -> Result<Fetch, Error>,
    update: impl FnOnce(P) -> Result<(), Error>,
) -> Result<(), Error>
where
    Post: Future<Output = Result<Rc<Value>, Error>>,
    Fetch: Future<Output = Result<P, Error>>,
{
    let data = post(
        endpoint("SET_MARGIN_MODE"),
        json!({"setMarginMode":if isolated {"ISOLATED_MARGIN"} else {"REGULAR_MARGIN"}})
            .as_object()
            .unwrap()
            .clone(),
    )?
    .await?;
    let reasons = data
        .get("result")
        .and_then(|r| r.get("reasons"))
        .and_then(Value::as_array);
    // Success is retCode 0 with no reasons; this endpoint answers "Request accepted", not "OK".
    if js_number(field(&data, "retCode")?).map_err(|e| error("TypeError", e.message))? != 0.
        || reasons.is_some_and(|r| !r.is_empty())
    {
        let parts = reasons
            .into_iter()
            .flatten()
            .filter_map(|r| r.get("reasonMsg"))
            .filter(|v| js_truthy(Some(v)))
            .map(|v| js_string(Some(v)).map_err(|e| error("TypeError", e.message)))
            .collect::<Result<Vec<_>, _>>()?;
        let joined = parts.join(", ");
        let message = if !joined.is_empty() {
            Value::String(joined)
        } else {
            field(&data, "retMsg")?
                .filter(|v| js_truthy(Some(v)))
                .cloned()
                .unwrap_or_else(|| Value::String("Failed to switch margin type".into()))
        };
        emit(Some(message.clone()))?;
        return Err(message_error(Some(&message)));
    }
    set_margin_type(if isolated {
        MarginType::Isolated
    } else {
        MarginType::Cross
    })?;
    let positions = fetch(None)?.await?;
    update(positions)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn fetch_position_mode_impl<Get>(
    symbol: &str,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    get: impl FnOnce(&str, Map<String, Value>) -> Result<Get, Error>,
    mut map_position: impl FnMut(&Value, usize) -> Result<Value, Error>,
    upsert: impl FnOnce(Option<Positions>) -> Result<(), Error>,
    emit: impl FnOnce(Option<Value>, Option<&str>, Option<Map<String, Value>>) -> Result<(), Error>,
    primary: bool,
) -> Result<Option<Value>, Error>
where
    Get: Future<Output = Result<Rc<Value>, Error>>,
{
    let stored = positions()?;
    if let Some(mode) = stored
        .iter()
        .find(|p| p.get("symbol").and_then(Value::as_str) == Some(symbol))
        .and_then(|p| p.get("positionMode"))
        .filter(|m| js_truthy(Some(m)))
    {
        return Ok(Some(mode.clone()));
    }
    let data = get(
        endpoint("POSITIONS"),
        json!({"params":{"category":category(symbol)?,"symbol":symbol}})
            .as_object()
            .unwrap()
            .clone(),
    )?
    .await?;
    let list = data.get("result").and_then(|r| r.get("list"));
    if primary {
        let meta = crate::response::get_bybit_response_meta(&data.as_ref().clone().into())
            .map_err(|fault| error(fault.name, fault.message))?;
        if !meta.is_success && list.and_then(Value::as_array).is_none_or(Vec::is_empty) {
            let fallback = meta
                .ret_msg
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    meta.ret_code
                        .map(|code| {
                            format!(
                                "Bybit positions request failed with code {}",
                                js_number_string(code)
                            )
                        })
                        .unwrap_or_else(|| {
                            "Bybit positions request returned malformed status payload".into()
                        })
                });
            let mut extra = json!({"endpoint":endpoint("POSITIONS"),"symbol":symbol,"hasExplicitStatus":meta.has_explicit_status}).as_object().unwrap().clone();
            if let Some(code) = meta.ret_code {
                extra.insert("retCode".into(), Value::from(code));
            }
            if let Some(message) = meta.ret_msg {
                extra.insert("retMsg".into(), Value::String(message));
            }
            emit(
                Some(Value::String(fallback)),
                Some("Bybit fetchPositionMode failed"),
                Some(extra),
            )?;
            return Ok(Some(Value::String("oneway".into())));
        }
    } else if field(&data, "retMsg")?.and_then(Value::as_str) != Some("OK") {
        emit(field(&data, "retMsg")?.cloned(), None, None)?;
        return Ok(Some(Value::String("oneway".into())));
    }
    let formatted = if let Some(rows) = list.and_then(Value::as_array) {
        Some(Rc::new(
            rows.iter()
                .enumerate()
                .map(|(i, p)| map_position(p, i))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    } else if primary {
        Some(Rc::new(vec![]))
    } else if list.is_none_or(Value::is_null) {
        None
    } else {
        return Err(error(
            "TypeError",
            "data?.result?.list?.map is not a function",
        ));
    };
    upsert(formatted.clone())?;
    Ok(match formatted.as_deref().and_then(|v| v.first()) {
        Some(first) => first.get("positionMode").cloned(),
        None => Some(Value::String("oneway".into())),
    })
}

pub async fn fetch_position_mode<Get>(
    symbol: &str,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    get: impl FnOnce(&str, Map<String, Value>) -> Result<Get, Error>,
    map_position: impl FnMut(&Value, usize) -> Result<Value, Error>,
    upsert: impl FnOnce(Option<Positions>) -> Result<(), Error>,
    emit: impl FnOnce(Option<Value>, Option<&str>, Option<Map<String, Value>>) -> Result<(), Error>,
) -> Result<Option<Value>, Error>
where
    Get: Future<Output = Result<Rc<Value>, Error>>,
{
    fetch_position_mode_impl(
        symbol,
        positions,
        category,
        get,
        map_position,
        upsert,
        emit,
        true,
    )
    .await
}

pub async fn fetch_inverse_position_mode<Get>(
    symbol: &str,
    positions: impl FnOnce() -> Result<Positions, Error>,
    category: impl FnOnce(&str) -> Result<String, Error>,
    get: impl FnOnce(&str, Map<String, Value>) -> Result<Get, Error>,
    map_position: impl FnMut(&Value, usize) -> Result<Value, Error>,
    upsert: impl FnOnce(Option<Positions>) -> Result<(), Error>,
    emit: impl FnOnce(Option<Value>, Option<&str>, Option<Map<String, Value>>) -> Result<(), Error>,
) -> Result<Option<Value>, Error>
where
    Get: Future<Output = Result<Rc<Value>, Error>>,
{
    fetch_position_mode_impl(
        symbol,
        positions,
        category,
        get,
        map_position,
        upsert,
        emit,
        false,
    )
    .await
}
