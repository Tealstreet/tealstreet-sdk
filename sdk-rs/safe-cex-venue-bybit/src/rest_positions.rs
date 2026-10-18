#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Position pagination awaiting class wiring")
)]
use safe_cex_num::{js_string, js_truthy};
use serde_json::{Value, json};
use std::{cell::Cell, future::Future};

fn same_cursor(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => {
            safe_cex_num::js_number(Some(a)).ok() == safe_cex_num::js_number(Some(b)).ok()
        }
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        // Each decoded response has fresh object/array identity, not structural equality.
        _ => false,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit dependencies replace class fields until exchange integration"
)]
pub(crate) async fn fetch_positions_helper<G, GF, C, M, D, N, T, E>(
    symbols: Option<&[String]>,
    category: Option<&str>,
    settle_coin: Option<&str>,
    needs_reinit: &Cell<bool>,
    mut get: G,
    mut category_for_symbol: C,
    mut map: M,
    mut emit: D,
    error: N,
) -> Result<Vec<T>, E>
where
    G: FnMut(&'static str, Value) -> GF,
    GF: Future<Output = Result<Value, E>>,
    C: FnMut(&str) -> Result<String, E>,
    M: FnMut(&Value) -> Result<T, E>,
    D: FnMut(Option<&Value>) -> Result<(), E>,
    N: Fn(&'static str, String) -> E,
{
    // This read owns no task or latch. Drop cancels only this invocation; a
    // completed rejection marks reinit before returning the original error.
    let scoped = symbols.is_some_and(|s| !s.is_empty());
    let symbols: Vec<Option<&str>> = if scoped {
        symbols
            .into_iter()
            .flatten()
            .map(|s| Some(s.as_str()))
            .collect()
    } else {
        vec![None]
    };
    let mut result = vec![];
    for symbol in symbols {
        let mut params = serde_json::Map::new();
        if let Some(symbol) = symbol {
            params.insert("symbol".into(), json!(symbol));
            params.insert("category".into(), json!(category_for_symbol(symbol)?));
        } else {
            if let Some(category) = category {
                params.insert("category".into(), json!(category));
            }
            if let Some(coin) = settle_coin {
                params.insert("settleCoin".into(), json!(coin));
            }
        }
        let mut cursor = json!("");
        let mut accumulated = vec![];
        loop {
            let mut page_params = params.clone();
            page_params.insert("limit".into(), json!(200));
            page_params.insert("cursor".into(), cursor.clone());
            let data = match get("/v5/position/list", Value::Object(page_params)).await {
                Ok(data) => data,
                Err(failure) => {
                    needs_reinit.set(true);
                    return Err(failure);
                }
            };
            if !data.is_null() && !data.is_object() && !data.is_array() {
                let text = js_string(Some(&data)).map_err(|e| error(e.name, e.message.into()))?;
                return Err(error(
                    "TypeError",
                    format!("Cannot use 'in' operator to search for 'retMsg' in {text}"),
                ));
            }
            let ret_msg = crate::json_field::field(&data, "retMsg")
                .map_err(|message| error("TypeError", message.into()))?;
            if ret_msg.and_then(Value::as_str) != Some("OK") {
                emit(ret_msg)?;
                if scoped {
                    let message = match ret_msg {
                        None => String::new(),
                        Some(value) => {
                            js_string(Some(value)).map_err(|e| error(e.name, e.message.into()))?
                        }
                    };
                    return Err(error("Error", message));
                }
                needs_reinit.set(true);
                accumulated.clear();
                break;
            }
            let page = data.get("result");
            if let Some(rows) = page.and_then(|p| p.get("list")).and_then(Value::as_array) {
                accumulated.extend(rows.iter().cloned());
            }
            let next = page.and_then(|p| p.get("nextPageCursor"));
            if let Some(next) = next.filter(|n| js_truthy(Some(n)) && !same_cursor(n, &cursor)) {
                cursor = next.clone();
            } else {
                break;
            }
        }
        for row in &accumulated {
            result.push(map(row)?);
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests;

// Keep closed-output refusals separate until all source work finishes.
#[expect(clippy::too_many_arguments, reason = "Explicit class dependencies")]
pub(crate) async fn fetch_inverse_positions<G, GF, C, D, N, E>(
    symbols: Option<&[String]>,
    category: Option<&str>,
    settle_coin: Option<&str>,
    needs_reinit: &Cell<bool>,
    get: G,
    category_for_symbol: C,
    emit: D,
    error: N,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<safe_cex_types::MarginType>,
    mut now: impl FnMut() -> f64,
    mut log: impl FnMut(String),
) -> Result<Vec<safe_cex_types::Position>, E>
where
    E: From<safe_cex_types::StoreReadError>,
    G: FnMut(&'static str, Value) -> GF,
    GF: Future<Output = Result<Value, E>>,
    C: FnMut(&str) -> Result<String, E>,
    D: FnMut(Option<&Value>) -> Result<(), E>,
    N: Fn(&'static str, String) -> E,
{
    let mapped = fetch_inverse_position_mappings(
        symbols,
        category,
        settle_coin,
        needs_reinit,
        get,
        category_for_symbol,
        emit,
        &error,
        store,
        margin_type,
        &mut now,
        &mut log,
    )
    .await?;
    finish_inverse_positions(mapped, error)
}

#[expect(clippy::too_many_arguments, reason = "Explicit class dependencies")]
pub(crate) async fn fetch_inverse_position_mappings<G, GF, C, D, N, E>(
    symbols: Option<&[String]>,
    category: Option<&str>,
    settle_coin: Option<&str>,
    needs_reinit: &Cell<bool>,
    get: G,
    category_for_symbol: C,
    emit: D,
    error: N,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<safe_cex_types::MarginType>,
    mut now: impl FnMut() -> f64,
    mut log: impl FnMut(String),
) -> Result<Vec<crate::inverse_position::PositionMapping>, E>
where
    E: From<safe_cex_types::StoreReadError>,
    G: FnMut(&'static str, Value) -> GF,
    GF: Future<Output = Result<Value, E>>,
    C: FnMut(&str) -> Result<String, E>,
    D: FnMut(Option<&Value>) -> Result<(), E>,
    N: Fn(&'static str, String) -> E,
{
    fetch_positions_helper(
        symbols,
        category,
        settle_coin,
        needs_reinit,
        get,
        category_for_symbol,
        |row| {
            crate::inverse_position::map_position_deferred(
                row,
                store,
                margin_type,
                &mut now,
                &mut log,
            )
            .map_err(|cause| match cause {
                crate::inverse_position::PositionMappingError::Source(message) => {
                    error("TypeError", message)
                }
                crate::inverse_position::PositionMappingError::Store(cause) => E::from(cause),
            })
        },
        emit,
        &error,
    )
    .await
}

fn finish_inverse_positions<E>(
    mapped: Vec<crate::inverse_position::PositionMapping>,
    error: impl Fn(&'static str, String) -> E,
) -> Result<Vec<safe_cex_types::Position>, E> {
    let mut positions = Vec::new();
    let mut refusal = None;
    for row in mapped {
        match row {
            crate::inverse_position::PositionMapping::Mapped(position) => positions.push(*position),
            crate::inverse_position::PositionMapping::Refused(message) => {
                if refusal.is_none() {
                    refusal = Some(message);
                }
            }
        }
    }
    match refusal {
        Some(message) => Err(error("RepresentationError", message)),
        None => Ok(positions),
    }
}

#[expect(clippy::too_many_arguments, reason = "Explicit class dependencies")]
pub(crate) async fn fetch_all_inverse_positions<G, GF, C, D, N, E>(
    symbols: Option<&[String]>,
    needs_reinit: &Cell<bool>,
    get: G,
    category_for_symbol: C,
    emit: D,
    error: N,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<safe_cex_types::MarginType>,
    now: impl FnMut() -> f64,
    log: impl FnMut(String),
) -> Result<Vec<safe_cex_types::Position>, E>
where
    E: From<safe_cex_types::StoreReadError>,
    G: FnMut(&'static str, Value) -> GF,
    GF: Future<Output = Result<Value, E>>,
    C: FnMut(&str) -> Result<String, E>,
    D: FnMut(Option<&Value>) -> Result<(), E>,
    N: Fn(&'static str, String) -> E,
{
    fetch_inverse_position_fanout(
        symbols,
        || Ok(vec!["inverse".into()]),
        |_| Ok(vec![None]),
        needs_reinit,
        get,
        category_for_symbol,
        emit,
        error,
        store,
        margin_type,
        now,
        log,
    )
    .await
}

#[expect(clippy::too_many_arguments, reason = "Explicit class dependencies")]
async fn fetch_inverse_position_fanout<G, GF, C, D, N, E>(
    symbols: Option<&[String]>,
    categories: impl FnOnce() -> Result<Vec<String>, E>,
    mut settle_coins: impl FnMut(&str) -> Result<Vec<Option<String>>, E>,
    needs_reinit: &Cell<bool>,
    mut get: G,
    mut category_for_symbol: C,
    mut emit: D,
    error: N,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<safe_cex_types::MarginType>,
    mut now: impl FnMut() -> f64,
    mut log: impl FnMut(String),
) -> Result<Vec<safe_cex_types::Position>, E>
where
    E: From<safe_cex_types::StoreReadError>,
    G: FnMut(&'static str, Value) -> GF,
    GF: Future<Output = Result<Value, E>>,
    C: FnMut(&str) -> Result<String, E>,
    D: FnMut(Option<&Value>) -> Result<(), E>,
    N: Fn(&'static str, String) -> E,
{
    let mut mapped = Vec::new();
    if symbols.is_some_and(|symbols| !symbols.is_empty()) {
        mapped = fetch_inverse_position_mappings(
            symbols,
            None,
            None,
            needs_reinit,
            &mut get,
            &mut category_for_symbol,
            &mut emit,
            &error,
            store,
            margin_type,
            &mut now,
            &mut log,
        )
        .await?;
    } else {
        for category in categories()? {
            for coin in settle_coins(&category)? {
                mapped.extend(
                    fetch_inverse_position_mappings(
                        None,
                        Some(&category),
                        coin.as_deref(),
                        needs_reinit,
                        &mut get,
                        &mut category_for_symbol,
                        &mut emit,
                        &error,
                        store,
                        margin_type,
                        &mut now,
                        &mut log,
                    )
                    .await?,
                );
            }
        }
    }
    finish_inverse_positions(mapped, error)
}
