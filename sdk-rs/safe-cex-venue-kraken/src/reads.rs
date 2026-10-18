//! Read method bodies at the parsed Axios data boundary; factories are separate rows.
use crate::{income, spot_types, types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    JsJsonValue, js_own_keys, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_from_value},
};
use safe_cex_store::Store;
use safe_cex_types::{IncomeRecord, MarginType, Position, PositionMode, PositionSide};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, future::Future};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountLogParams {
    start_time: f64,
    limit: f64,
}

#[derive(Deserialize)]
#[serde(remote = "AccountLogParams", rename_all = "camelCase")]
struct AccountLogParamsWire {
    start_time: f64,
    limit: f64,
}

impl<'de> Deserialize<'de> for AccountLogParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = <JsNumberInput as Deserialize<'de>>::deserialize(d)?;
        input
            .decode_with(|decoder| {
                AccountLogParamsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for AccountLogParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { start_time, limit });
    }
}

pub fn fetch_account_log_page<'a, F>(
    params: JsNumberInput,
    store: &'a dyn Store,
    history_read: impl FnOnce(&'static str, JsJsonValue) -> F,
    local_to_utc: &'a mut impl FnMut(f64) -> f64,
) -> impl Future<Output = Result<Vec<IncomeRecord>, Box<dyn Error>>> + 'a
where
    F: Future<Output = Result<Option<JsNumberInput>, Box<dyn Error>>> + 'a,
{
    // Invoke the read at function entry, matching the source's pre-await prefix.
    let response = params.decode::<AccountLogParams>().map(|params| {
        let endpoint = types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "ACCOUNT_LOG")
            .expect("account-log endpoint")
            .1;
        history_read(
            endpoint,
            JsJsonValue::Object(vec![
                ("since".into(), JsJsonValue::Number(params.start_time)),
                ("count".into(), JsJsonValue::Number(params.limit)),
            ]),
        )
    });
    async move {
        let data = response?.await?.map(wire::decode).transpose()?;
        if let Some(data) = data
            .as_ref()
            .filter(|data| data.is_object() || data.is_array())
            && !data.get("logs").is_some_and(Value::is_array)
        {
            let status = wire::text(data.get("status"))?;
            let reason = wire::text(data.get("reason"))?;
            let detail = if !status.is_empty() || !reason.is_empty() {
                format!(
                    " — {status}{}",
                    if reason.is_empty() {
                        String::new()
                    } else {
                        format!(": {reason}")
                    }
                )
            } else {
                let keys = match data {
                    Value::Object(values) => js_own_keys(values.keys().map(String::as_str))
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>(),
                    Value::Array(values) => (0..values.len()).map(|i| i.to_string()).collect(),
                    _ => unreachable!(),
                };
                format!(" (envelope: {})", keys.join(","))
            };
            return Err(Box::new(ExchangeError::new(format!(
                "kraken account-log did not return a log array{detail}"
            ))) as Box<dyn Error>);
        }
        let logs = data
            .as_ref()
            .and_then(|data| data.get("logs"))
            .filter(|v| v.is_array())
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let mut resolve = |contract: &str| {
            Ok(store
                .markets_by_symbol()
                .get(&contract.to_uppercase())
                .map(|market| market.symbol.clone()))
        };
        income::map_kraken_account_log(
            js_from_value::<JsNumberInput>(logs)?,
            &mut resolve,
            local_to_utc,
        )
    }
}

pub fn fetch_ws_token<F>(
    post: impl FnOnce(&'static str, Value) -> F,
) -> impl Future<Output = Result<Value, Box<dyn Error>>>
where
    F: Future<Output = Result<Option<JsNumberInput>, Box<dyn Error>>>,
{
    let endpoint = spot_types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == "WS_TOKEN")
        .expect("WS token endpoint")
        .1;
    let response = post(endpoint, serde_json::json!({}));
    async move {
        let data = response.await?.map(wire::decode).transpose()?;
        let token = data
            .as_ref()
            .and_then(|data| data.get("result"))
            .and_then(|result| result.get("token"));
        if !js_truthy(token) {
            return Err(
                Box::new(ExchangeError::new("Failed to get WebSocket token")) as Box<dyn Error>,
            );
        }
        Ok(token.expect("truthy token is present").clone())
    }
}

pub fn fetch_spot_positions(store: &dyn Store) -> Vec<Position> {
    let mut positions = Vec::new();
    for market in store.markets().iter() {
        if !market.active {
            continue;
        }
        for (label, side) in [("long", PositionSide::Long), ("short", PositionSide::Short)] {
            positions.push(Position {
                id: format!("{}:{label}", market.symbol),
                symbol: market.symbol.clone(),
                side: side.into(),
                entry_price: 0.0,
                notional: 0.0,
                leverage: 1.0,
                unrealized_pnl: 0.0,
                contracts: 0.0,
                liquidation_price: 0.0,
                margin_type: (Some(MarginType::Isolated)).into(),
                position_mode: (Some(PositionMode::OneWay)).into(),
                info: (Some(serde_json::json!({}).into())).into(),
                realized_pnl: (None).into(),
                margin_size: (None).into(),
                extra: (None).into(),
                last_update: (None).into(),
                option: (None).into(),
            });
        }
    }
    positions
}

/// `KrakenExchange.fetchLeverageSettings`, with the exchange-owned Map operations supplied.
pub fn fetch_leverage_settings<'a, F>(
    read: impl FnOnce(&'static str) -> Result<F, ExchangeError>,
    clear: impl FnOnce() -> Result<(), ExchangeError> + 'a,
    mut set: impl FnMut(Value, Value) -> Result<(), ExchangeError> + 'a,
    console_error: impl FnOnce(&'static str, &str) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<(), ExchangeError>> + 'a
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>> + 'a,
{
    let mut console_error = Some(console_error);
    let endpoint = types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == "LEVERAGE_PREFERENCES")
        .expect("leverage preferences endpoint")
        .1;
    // A synchronous read throw is caught before the source async function returns.
    let response = match read(endpoint) {
        Ok(response) => Ok(Some(response)),
        Err(error) => console_error.take().expect("one console report")(
            "[Kraken] Failed to fetch leverage settings:",
            &error.message,
        )
        .map(|()| None),
    };
    async move {
        let Some(response) = response? else {
            return Ok(());
        };
        let outcome: Result<(), ExchangeError> = async {
            let data = response
                .await?
                .map(wire::decode)
                .transpose()
                .map_err(|error| ExchangeError::new(error.to_string()))?;
            let preferences = data
                .as_ref()
                .and_then(|data| data.get("leveragePreferences"));
            if data
                .as_ref()
                .and_then(|data| data.get("result"))
                .and_then(Value::as_str)
                == Some("success")
                && js_truthy(preferences)
            {
                clear()?;
                match preferences.expect("truthy preferences present") {
                    Value::Array(preferences) => {
                        for preference in preferences {
                            if preference.is_null() {
                                return Err(ExchangeError::new(
                                    "Cannot read properties of null (reading 'symbol')",
                                ));
                            }
                            let symbol = preference.get("symbol");
                            if js_truthy(symbol) {
                                let max_leverage = preference.get("maxLeverage");
                                if js_truthy(max_leverage) {
                                    set(
                                        symbol.expect("truthy symbol").clone(),
                                        max_leverage.expect("truthy leverage").clone(),
                                    )?;
                                }
                            }
                        }
                    }
                    // Each iterated string character has no symbol on its stock prototype.
                    Value::String(_) => {}
                    _ => {
                        return Err(ExchangeError::new(
                            "data.leveragePreferences is not iterable",
                        ));
                    }
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            // ts-bug: fetchLeverageSettings suppresses read/cache failures after console.error;
            // already-committed clear/set effects remain. A console failure still propagates.
            console_error.take().expect("one console report")(
                "[Kraken] Failed to fetch leverage settings:",
                &error.message,
            )?;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct HistoricalOptions {
    account: String,
    from: Option<f64>,
    to: Option<f64>,
}

#[derive(Deserialize)]
#[serde(remote = "HistoricalOptions")]
struct HistoricalOptionsWire {
    account: String,
    from: Option<f64>,
    to: Option<f64>,
}

impl<'de> Deserialize<'de> for HistoricalOptions {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = <JsNumberInput as Deserialize<'de>>::deserialize(d)?;
        input
            .decode_with(|decoder| {
                HistoricalOptionsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for HistoricalOptions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, from, to });
    }
}

/// Delegates to the shared journal operation; the venue does not fold records.
pub async fn fetch_historical_positions(
    venue: &dyn safe_cex_exchange::exchange::Exchange,
    folder: &dyn safe_cex_exchange::journal::JournalFolder,
    options: JsNumberInput,
) -> Result<Vec<Value>, ExchangeError> {
    let options = options
        .decode::<HistoricalOptions>()
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    safe_cex_exchange::journal::fold_closed_position_records(
        venue,
        folder,
        safe_cex_exchange::journal::HistoricalPositionOptions {
            account: options.account,
            from: options.from,
            to: options.to,
        },
        false,
    )
    .await
}

#[cfg(test)]
mod accepted_shape {
    use super::{AccountLogParams, HistoricalOptions};
    use crate::accepted_record_shapes_support as support;
    #[test]
    fn account_log_params_rejects_complete_positional_array() {
        support::positive::<AccountLogParams>(
            r#"{"startTime":-0,"limit":7.25,"ignored":true}"#,
            |d| {
                assert_eq!(d.start_time.to_bits(), (-0.0_f64).to_bits());
                assert_eq!(d.limit.to_bits(), 7.25_f64.to_bits());
            },
        );
        support::positive::<AccountLogParams>(
            r#"{"startTime":9007199254740993,"limit":1e400}"#,
            |d| {
                assert_eq!(
                    d.start_time.to_bits(),
                    9_007_199_254_740_992.0_f64.to_bits()
                );
                assert_eq!(d.limit.to_bits(), f64::INFINITY.to_bits());
            },
        );
        support::rejects::<AccountLogParams>(
            "[19.5,7.25]",
            &[
                "{}",
                r#"{"start_time":19.5,"limit":7.25}"#,
                r#"{"startTime":null,"limit":7.25}"#,
            ],
        );
    }
    #[test]
    fn historical_options_rejects_complete_positional_array() {
        support::positive::<HistoricalOptions>(
            r#"{"account":"account-A","from":-0,"to":93.5,"ignored":true}"#,
            |d| {
                assert_eq!(d.account, "account-A");
                assert_eq!(d.from.unwrap().to_bits(), (-0.0_f64).to_bits());
                assert_eq!(d.to.unwrap().to_bits(), 93.5_f64.to_bits());
            },
        );
        for raw in [
            r#"{"account":"account-B"}"#,
            r#"{"account":"account-B","from":null,"to":null}"#,
        ] {
            support::positive::<HistoricalOptions>(raw, |d| {
                assert_eq!(d.account, "account-B");
                assert!(d.from.is_none() && d.to.is_none());
            });
        }
        support::positive::<HistoricalOptions>(
            r#"{"account":"NUM","from":5e-324,"to":-1e400}"#,
            |d| {
                assert_eq!(d.account, "NUM");
                assert_eq!(d.from.unwrap().to_bits(), 1);
                assert_eq!(d.to.unwrap().to_bits(), f64::NEG_INFINITY.to_bits());
            },
        );
        support::rejects::<HistoricalOptions>(
            r#"["account-A",71.25,93.5]"#,
            &["{}", r#"{"account":15}"#],
        );
    }
}
