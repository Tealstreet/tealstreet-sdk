//! GRVT market reads against the configured HTTP client and shared Market model.
use indexmap::IndexMap;
use safe_cex_exchange::{
    runtime::{ExchangeError, LogSeverity},
    snapshots::MarketChanges,
};
use safe_cex_num::js_retained_parse_float;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    retained_json::RetainedJsonInput as Input,
};
use serde_json::{Value, json};
use std::future::Future;

fn error(name: &str, message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error
}

fn parsed(value: Option<&Input>, fallback: &str) -> Result<f64, ExchangeError> {
    let default = Input::Text(fallback.encode_utf16().collect());
    js_retained_parse_float(Some(
        value.filter(|value| value.is_truthy()).unwrap_or(&default),
    ))
    .map_err(|cause| error(cause.name, cause.message))
}

fn text(value: Option<&Input>) -> Option<String> {
    value
        .and_then(Input::as_text)
        .and_then(|units| String::from_utf16(units).ok())
}

// Source evaluation errors take precedence over a typed-output refusal in an earlier row.
fn project_instrument(inst: &Input) -> Result<Result<Market, ExchangeError>, ExchangeError> {
    if inst.is_null() {
        return Err(error(
            "TypeError",
            "Cannot read properties of null (reading 'k')",
        ));
    }
    let is_perpetual = inst
        .get("k")
        .is_some_and(|value| value.text_equals("PERPETUAL"));
    let min_size = parsed(inst.get("ms"), "0.001")?;
    let tick_size = parsed(inst.get("ts"), "0.1")?;
    let id = match inst.get("i") {
        Some(Input::Text(id)) => String::from_utf16(id)
            .map_err(|_| error("RepresentationError", "GRVT market id is not UTF-8"))?,
        None | Some(Input::Undefined) => {
            return Err(error(
                "TypeError",
                "Cannot read properties of undefined (reading 'toUpperCase')",
            ));
        }
        Some(Input::Null) => {
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
            .filter(|v| v.is_truthy())
            .or_else(|| inst.get("mxs")),
        "999999999",
    )?;
    let info = Input::Object(
        ["ih", "bd", "qd"]
            .into_iter()
            .map(|key| {
                (
                    key.encode_utf16().collect(),
                    inst.get(key).cloned().unwrap_or(Input::Undefined),
                )
            })
            .collect(),
    );
    let (Some(base), Some(quote)) = (text(inst.get("b")), text(inst.get("q"))) else {
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
        active: inst.get("ia") != Some(&Input::Bool(false)),
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
        info: Some(info),
        taker_fee: None,
        maker_fee: None,
        option: None,
    }))
}

/// ts: GrvtExchange.fetchMarkets. The post hook returns already transformed response.data.
pub fn fetch_markets<F: Future<Output = Result<Input, ExchangeError>>>(
    post: impl FnOnce(&str, Value) -> F,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Vec<Market>, ExchangeError>> {
    let request = post("/lite/v1/all_instruments", json!({}));
    async move {
        let source_result = async {
            let data = request.await?;
            let rows = data.get("r");
            if !rows.is_some_and(Input::is_truthy) {
                return Ok(Vec::new());
            }
            let rows = rows
                .and_then(Input::as_array)
                .ok_or_else(|| error("TypeError", "response.data.r.map is not a function"))?;
            let markets = rows
                .iter()
                .enumerate()
                .map(|(index, inst)| {
                    project_instrument(inst).map(|result| {
                        result.map_err(|cause| {
                            let field = |key| {
                                inst.get(key).map_or_else(
                                    || "undefined".into(),
                                    |value| {
                                        value
                                            .json_stringify()
                                            .ok()
                                            .flatten()
                                            .unwrap_or_else(|| "undefined".into())
                                    },
                                )
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
