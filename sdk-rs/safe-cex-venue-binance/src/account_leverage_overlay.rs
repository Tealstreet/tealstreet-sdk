use crate::request_weight::{Failure, type_error};
use safe_cex_num::{
    get_key::{GetKeyUnknownValue, v_unknown},
    js_number,
};
use safe_cex_types::Market;
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin};

/// A string-symbol market record. Reference-carrying adapters preserve untouched
/// records and implement the source shallow copies of market/limits/leverage.
/// The production Market DTO adapter below preserves declared field values.
pub trait LeverageMarket: Clone {
    fn symbol(&self) -> &str;
    fn with_leverage_max(&self, max: f64) -> Self;
}
impl LeverageMarket for Market {
    fn symbol(&self) -> &str {
        &self.symbol
    }
    fn with_leverage_max(&self, max: f64) -> Self {
        let mut market = self.clone();
        market.limits.leverage.max = max;
        market
    }
}
pub struct LeverageOverlay {
    maxima: BTreeMap<String, f64>,
}
impl LeverageOverlay {
    pub fn apply<M: LeverageMarket>(&self, markets: Vec<M>) -> Vec<M> {
        markets
            .into_iter()
            .map(|market| match self.maxima.get(market.symbol()) {
                Some(max) if *max != 0.0 && !max.is_nan() => market.with_leverage_max(*max),
                _ => market,
            })
            .collect()
    }
}
// Stock decoded JSON and string-symbol markets are the data domain. Malformed
// string brackets starting with non-BMP characters produce lone UTF-16 error
// text in JS, outside this Rust UTF-8 error-text boundary. Hooks/undefined fields
// and non-string market symbols are also outside the adapter contract.
pub type LeverageFuture = Pin<Box<dyn Future<Output = Result<LeverageOverlay, Failure>>>>;
pub type LeverageLoader = Box<dyn FnMut() -> LeverageFuture>;

// ts: BinanceExchange#loadAccountMaxLeverage
/// Current xhr.get and loadAccountMarketOverlay are configured class bindings.
/// Loader invocation starts GET immediately; await continuation is polled by the
/// caller. The configured overlay method owns caching, disposal and error policy.
pub fn load_futures_account_max_leverage<G, F, R>(
    mut get: G,
    load_overlay: impl FnOnce(LeverageLoader) -> R,
) -> R
where
    G: FnMut(&'static str) -> Result<F, Failure> + 'static,
    F: Future<Output = Result<Option<Value>, Failure>> + 'static,
{
    let load: LeverageLoader = Box::new(move || {
        let future = get("/fapi/v1/leverageBracket");
        Box::pin(async move {
            let data = future?.await?;
            let rows = crate::response_shape::require_binance_array_response(
                "/fapi/v1/leverageBracket",
                data.as_ref(),
            )
            .map_err(|error| {
                let mut failure = safe_cex_exchange::runtime::ExchangeError::new(error.message);
                failure.name = error.name.into();
                std::rc::Rc::new(failure)
            })?;
            build_overlay(rows)
        })
    });
    load_overlay(load)
}

// ts: BinancePMExchange#loadAccountMaxLeverage
pub fn load_pm_account_max_leverage<G, F, R>(
    mut get: G,
    load_overlay: impl FnOnce(LeverageLoader) -> R,
) -> R
where
    G: FnMut(&'static str) -> Result<F, Failure> + 'static,
    F: Future<Output = Result<Option<Value>, Failure>> + 'static,
{
    let load: LeverageLoader = Box::new(move || {
        let future = get("/papi/v1/um/leverageBracket");
        Box::pin(async move {
            let data = future?.await?;
            let rows = crate::response_shape::require_binance_pm_array_response(
                "/papi/v1/um/leverageBracket",
                data.as_ref(),
            )
            .map_err(|error| {
                let mut failure = safe_cex_exchange::runtime::ExchangeError::new(error.message);
                failure.name = error.name.into();
                std::rc::Rc::new(failure)
            })?;
            build_overlay(rows)
        })
    });
    load_overlay(load)
}

fn build_overlay(rows: &[Value]) -> Result<LeverageOverlay, Failure> {
    let mut maxima = BTreeMap::new();
    for row in rows {
        if row.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'symbol')",
            ));
        }
        // Source reads symbol first, then b.brackets?.[0]. A string brackets
        // container indexes its first UTF-16 unit and v throws on the primitive.
        let first_owned;
        let first = match row.get("brackets") {
            Some(Value::Array(brackets)) => brackets.first(),
            Some(Value::Object(brackets)) => brackets.get("0"),
            Some(Value::String(brackets)) if !brackets.is_empty() => {
                first_owned = Value::String(brackets.chars().next().unwrap().to_string());
                Some(&first_owned)
            }
            _ => None,
        };
        let max =
            match v_unknown(first, "initialLeverage").map_err(|error| type_error(error.message))? {
                GetKeyUnknownValue::Json(value) => {
                    js_number(Some(value)).map_err(|error| type_error(error.message))?
                }
                _ => f64::NAN,
            };
        // Non-string keys cannot match this typed market domain, but coercion
        // above still executes and can fail before a key is excluded.
        if max > 0.0 {
            if let Some(symbol) = row.get("symbol").and_then(Value::as_str) {
                maxima.insert(symbol.to_owned(), max);
            }
        }
    }
    Ok(LeverageOverlay { maxima })
}
