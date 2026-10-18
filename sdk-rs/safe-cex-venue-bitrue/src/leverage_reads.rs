//! Leverage reads, composing the source's parsing and production Store update.
use crate::{
    read_helpers::{
        is_success_code, parse_max_leverage_from_bracket_response, set_market_max_leverage,
    },
    types::ENDPOINTS,
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::Store;
use serde_json::Value;
use std::future::Future;

pub fn fetch_and_sync_symbol_max_leverage<'a, F, Fut>(
    store: &'a dyn Store,
    symbol: &'a str,
    public_data_only: bool,
    get: F,
) -> impl Future<Output = Option<f64>> + 'a
where
    F: FnOnce(&'static str, Value) -> Result<Fut, ExchangeError> + 'a,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>> + 'a,
{
    // Source async arrows enter their body immediately, through the first await.
    let request = if symbol.is_empty() || public_data_only {
        None
    } else {
        let endpoint = ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "LEVERAGE_BRACKET")
            .expect("source endpoint")
            .1;
        get(
            endpoint,
            serde_json::json!({"params":{"contractName":symbol}}),
        )
        .ok()
    };
    async move {
        let request = request?;
        let attempt: Result<Option<f64>, ExchangeError> = async {
            let data = request.await?;
            if !is_success_code(data.as_ref().and_then(|data| data.get("code"))) {
                return Ok(None);
            }
            let maximum = parse_max_leverage_from_bracket_response(data.as_ref(), Some(symbol))
                .map_err(|error| {
                    let mut result = ExchangeError::new(error.message);
                    result.name = error.name.into();
                    result
                })?;
            if let Some(maximum) = maximum {
                set_market_max_leverage(store, symbol, maximum)?;
            }
            Ok(maximum)
        }
        .await;
        #[allow(
            clippy::manual_unwrap_or_default,
            reason = "Keep the source catch and its intentional post-commit error discard explicit"
        )]
        match attempt {
            Ok(maximum) => maximum,
            // TS-BUG: fetchAndSyncSymbolMaxLeverage's catch discards every failure,
            // including a subscriber failure AFTER the Store update committed.
            // Preserve that named source discard; do not move it into the setter.
            Err(_source_catch_discard) => None,
        }
    }
}

pub fn fetch_symbol_max_leverage<'a, F, Fut>(
    store: &'a dyn Store,
    symbol: &'a str,
    public_data_only: bool,
    get: F,
) -> impl Future<Output = Result<f64, ExchangeError>> + 'a
where
    F: FnOnce(&'static str, Value) -> Result<Fut, ExchangeError> + 'a,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>> + 'a,
{
    let request = fetch_and_sync_symbol_max_leverage(store, symbol, public_data_only, get);
    async move {
        if let Some(maximum) = request.await {
            return Ok(maximum);
        }
        let existing = store
            .markets_by_symbol()?
            .get(symbol)
            .map(|market| market.limits.leverage.max);
        Ok(match existing {
            Some(value) if value.is_finite() && value > 0.0 => value,
            _ => 1.0,
        })
    }
}

pub fn update_symbol_metadata<'a, F, Fut>(
    store: &'a dyn Store,
    symbol: &'a str,
    public_data_only: bool,
    get: F,
) -> impl Future<Output = ()> + 'a
where
    F: FnOnce(&'static str, Value) -> Result<Fut, ExchangeError> + 'a,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>> + 'a,
{
    let request = fetch_and_sync_symbol_max_leverage(store, symbol, public_data_only, get);
    async move {
        // Source deliberately ignores the optional maximum, after awaiting sync.
        request.await;
    }
}
