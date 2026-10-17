//! GRVT market reads against the configured HTTP client and shared Market model.
use indexmap::IndexMap;
use safe_cex_exchange::{
    runtime::{ExchangeError, LogSeverity},
    snapshots::MarketChanges,
};
use safe_cex_num::{js_truthy, parse_float};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
};
use serde_json::{Map, Value, json};
use std::future::Future;

fn error(name: &str, message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error
}

fn parsed(value: Option<&Value>, fallback: &str) -> Result<f64, ExchangeError> {
    let text = if js_truthy(value) {
        safe_cex_types::js_coercion::js_string(value)
            .map_err(|cause| error(cause.name, cause.message))?
    } else {
        fallback.into()
    };
    Ok(parse_float(&text))
}

// Source evaluation errors take precedence over a typed-output refusal in an earlier row.
fn project_instrument(inst: &Value) -> Result<Result<Market, ExchangeError>, ExchangeError> {
    if inst.is_null() {
        return Err(error(
            "TypeError",
            "Cannot read properties of null (reading 'k')",
        ));
    }
    let is_perpetual = inst.get("k").and_then(Value::as_str) == Some("PERPETUAL");
    let min_size = parsed(inst.get("ms"), "0.001")?;
    let tick_size = parsed(inst.get("ts"), "0.1")?;
    let id = match inst.get("i") {
        Some(Value::String(id)) => id.clone(),
        None => {
            return Err(error(
                "TypeError",
                "Cannot read properties of undefined (reading 'toUpperCase')",
            ));
        }
        Some(Value::Null) => {
            return Err(error(
                "TypeError",
                "Cannot read properties of null (reading 'toUpperCase')",
            ));
        }
        _ => return Err(error("TypeError", "inst.i.toUpperCase is not a function")),
    };
    let symbol = id.to_uppercase();
    let max = parsed(
        inst.get("mp")
            .filter(|v| js_truthy(Some(v)))
            .or_else(|| inst.get("mxs")),
        "999999999",
    )?;
    let info: Map<_, _> = ["ih", "bd", "qd"]
        .into_iter()
        .filter_map(|key| inst.get(key).map(|value| (key.into(), value.clone())))
        .collect();
    let (Some(base), Some(quote)) = (
        inst.get("b").and_then(Value::as_str),
        inst.get("q").and_then(Value::as_str),
    ) else {
        return Ok(Err(error(
            "RepresentationError",
            "GRVT market base and quote must be strings for the shared Market model",
        )));
    };
    Ok(Ok(Market {
        id,
        symbol,
        pretty_symbol: None,
        base: base.into(),
        quote: quote.into(),
        active: inst.get("ia") != Some(&Value::Bool(false)),
        precision: MarketPrecision {
            amount: min_size,
            price: tick_size,
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount { min: min_size, max },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: Some(false),
        is_perpetual: Some(is_perpetual),
        is_read_only: None,
        info: Some(Value::Object(info)),
        taker_fee: None,
        maker_fee: None,
        option: None,
    }))
}

/// ts: GrvtExchange.fetchMarkets. The post hook returns already transformed response.data.
pub fn fetch_markets<F: Future<Output = Result<Value, ExchangeError>>>(
    post: impl FnOnce(&str, Value) -> F,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Vec<Market>, ExchangeError>> {
    let request = post("/lite/v1/all_instruments", json!({}));
    async move {
        let source_result = async {
            let data = request.await?;
            let rows = data.get("r");
            if !js_truthy(rows) {
                return Ok(Vec::new());
            }
            let rows = rows
                .and_then(Value::as_array)
                .ok_or_else(|| error("TypeError", "response.data.r.map is not a function"))?;
            let markets = rows
                .iter()
                .enumerate()
                .map(|(index, inst)| {
                    project_instrument(inst).map(|result| {
                        result.map_err(|cause| {
                            let field = |key| {
                                inst.get(key)
                                    .map_or_else(|| "undefined".into(), Value::to_string)
                            };
                            format!(
                                "Refused GRVT market row {index} id={}: base={} quote={}: {}",
                                field("i"),
                                field("b"),
                                field("q"),
                                cause.message
                            )
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            log(
                &format!("Fetched {} GRVT markets", markets.len()),
                LogSeverity::Debug,
            )?;
            Ok::<_, ExchangeError>(markets)
        }
        .await;
        match source_result {
            Ok(markets) => {
                let mut accepted = Vec::with_capacity(markets.len());
                for market in markets {
                    match market {
                        Ok(market) => accepted.push(market),
                        Err(message) => log(&message, LogSeverity::Warning)?,
                    }
                }
                Ok(accepted)
            }
            Err(cause) => {
                log(
                    &format!("Error fetching markets: {}", cause.js_error_string()),
                    LogSeverity::Error,
                )?;
                Err(cause)
            }
        }
    }
}

/// ts: GrvtExchange.refreshMarkets. Replace metadata even when membership is unchanged.
pub fn refresh_markets<'a, F: Future<Output = Result<Vec<Market>, ExchangeError>> + 'a>(
    store: &'a dyn Store,
    fetch: impl FnOnce() -> F,
    mut emit: impl FnMut(&str, &MarketChanges) -> Result<(), ExchangeError> + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<MarketChanges, ExchangeError>> + 'a {
    let old: IndexMap<_, _> = store
        .markets()
        .iter()
        .map(|market| (market.id.clone(), market.clone()))
        .collect();
    let request = fetch();
    async move {
        let new = request.await?;
        let changes = MarketChanges {
            added: new
                .iter()
                .filter(|market| !old.contains_key(&market.id))
                .cloned()
                .collect(),
            removed: old
                .into_values()
                .filter(|market| !new.iter().any(|new| new.id == market.id))
                .collect(),
        };
        let count = new.len();
        store.update(
            StoreUpdate {
                markets: Some(new),
                ..Default::default()
            },
            false,
        )?;
        if !changes.added.is_empty() || !changes.removed.is_empty() {
            emit("markets:changed", &changes)?;
            log(
                &format!(
                    "Markets refreshed: +{} -{} (total: {count})",
                    changes.added.len(),
                    changes.removed.len()
                ),
                LogSeverity::Info,
            )?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod accepted_contract_tests {
    #[test]
    fn accepted_market_error_factory() {
        for (name, message) in [
            ("TypeError", "protocol denied"),
            ("", ""),
            ("NativeDiagnostic", "café\n\0💠"),
        ] {
            let actual = super::error(name, message);
            assert_eq!(actual.name, name, "native diagnostic name");
            assert_eq!(actual.message, message, "native diagnostic text");
            assert_eq!(
                actual.metadata.as_ref(),
                &safe_cex_exchange::runtime::ExchangeErrorMetadata::default(),
                "fresh native metadata"
            );
        }
    }
}
