//! Spot candle read at the shared Store and parsed-data host boundary.
use crate::{spot_types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    JsJsonValue, TableHit, js_own_keys, js_string, js_table_lookup, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_value_as_f64},
    parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::Candle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

#[derive(Serialize)]
struct Options {
    symbol: String,
    interval: String,
    from: Option<f64>,
}

#[derive(Deserialize)]
#[serde(remote = "Options")]
struct SpotOptionsWire {
    symbol: String,
    interval: String,
    from: Option<f64>,
}

impl<'de> Deserialize<'de> for Options {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = <JsNumberInput as Deserialize<'de>>::deserialize(d)?;
        input
            .decode_with(|decoder| {
                SpotOptionsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for Options {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, interval, from });
    }
}
fn field(value: &Value, index: usize) -> Option<&Value> {
    match value {
        Value::Array(rows) => rows.get(index),
        Value::Object(object) => object.get(&index.to_string()),
        _ => None,
    }
}

/// The request adapter retains inherited interval identities via shared TableHit.
pub async fn fetch_ohlcv<F>(
    options: JsNumberInput,
    store: &dyn Store,
    read: impl FnOnce(&'static str, JsJsonValue, TableHit<f64>, Option<f64>) -> Result<F, ExchangeError>,
    console_error: impl FnOnce(&'static str, &str) -> Result<(), ExchangeError>,
) -> Result<Vec<Candle>, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
{
    let outcome: Result<Vec<Candle>, ExchangeError> = async {
        let options = options
            .decode::<Options>()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let pair = store
            .markets_by_symbol()
            .get(&options.symbol)
            .and_then(|market| market.info.as_ref())
            .and_then(|info| info.get("pairName"))
            .filter(|pair| js_truthy(Some(pair)))
            .cloned()
            .unwrap_or_else(|| Value::String(options.symbol.clone()));
        let interval = match js_table_lookup(spot_types::INTERVAL, &options.interval) {
            TableHit::Mapped(value) => TableHit::Mapped(*value),
            TableHit::Inherited(member) => TableHit::Inherited(member),
            TableHit::Missing => TableHit::Mapped(1.0),
        };
        let since = options
            .from
            .filter(|value| *value != 0.0 && !value.is_nan())
            .map(|value| (value / 1000.0).floor());
        let endpoint = spot_types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "OHLC")
            .expect("OHLC endpoint")
            .1;
        let data = read(endpoint, pair.into(), interval, since)?
            .await?
            .map(wire::decode)
            .transpose()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let result = data
            .as_ref()
            .and_then(|data| data.get("result"))
            .filter(|value| js_truthy(Some(value)));
        let owned_keys: Vec<String> = match result {
            Some(Value::Object(object)) => js_own_keys(object.keys().map(String::as_str))
                .into_iter()
                .map(str::to_owned)
                .collect(),
            Some(Value::Array(array)) => (0..array.len()).map(|index| index.to_string()).collect(),
            Some(Value::String(string)) => (0..string.encode_utf16().count())
                .map(|index| index.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let Some(key) = owned_keys
            .iter()
            .find(|key| key.as_str() != "last")
            .filter(|key| !key.is_empty())
        else {
            return Ok(Vec::new());
        };
        let raw = match result {
            Some(Value::Object(object)) => object.get(key),
            Some(Value::Array(array)) => {
                key.parse::<usize>().ok().and_then(|index| array.get(index))
            }
            _ => None,
        };
        let rows = match raw {
            Some(Value::Array(rows)) => rows,
            Some(Value::Null) => {
                return Err(ExchangeError::new(
                    "Cannot read properties of null (reading 'map')",
                ));
            }
            _ => return Err(ExchangeError::new("rawCandles.map is not a function")),
        };
        let mut candles = Vec::with_capacity(rows.len());
        for row in rows {
            if row.is_null() {
                return Err(ExchangeError::new(
                    "Cannot read properties of null (reading '0')",
                ));
            }
            // CHOSEN typed Candle timestamp profile; source can retain non-number timestamps.
            let timestamp = field(row, 0).and_then(js_value_as_f64).ok_or_else(|| {
                ExchangeError::new("Kraken Spot candle timestamp must be a number")
            })?;
            let parse = |index| {
                js_string(field(row, index))
                    .map(|text| parse_float(&text))
                    .map_err(|error| ExchangeError::new(error.to_string()))
            };
            candles.push(Candle {
                timestamp,
                open: parse(1)?,
                high: parse(2)?,
                low: parse(3)?,
                close: parse(4)?,
                volume: parse(6)?,
                last_side: None,
            });
        }
        candles.sort_by(|left, right| {
            (left.timestamp - right.timestamp)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(candles)
    }
    .await;
    match outcome {
        Ok(candles) => Ok(candles),
        Err(error) => {
            // ts-bug compatibility: console succeeds, then the read error resolves as [].
            console_error("[KrakenSpot] fetchOHLCV error:", &error.message)?;
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod accepted_shape {
    use super::Options;
    use crate::accepted_record_shapes_support as support;
    #[test]
    fn spot_options_rejects_complete_positional_array() {
        support::positive::<Options>(
            r#"{"symbol":"XBT/USD","interval":"15m","from":-0,"ignored":true}"#,
            |d| {
                assert_eq!(d.symbol, "XBT/USD");
                assert_eq!(d.interval, "15m");
                assert_eq!(d.from.unwrap().to_bits(), (-0.0_f64).to_bits());
            },
        );
        for raw in [
            r#"{"symbol":"ETH/USD","interval":"5m"}"#,
            r#"{"symbol":"ETH/USD","interval":"5m","from":null}"#,
        ] {
            support::positive::<Options>(raw, |d| {
                assert_eq!(d.symbol, "ETH/USD");
                assert_eq!(d.interval, "5m");
                assert!(d.from.is_none());
            });
        }
        support::positive::<Options>(r#"{"symbol":"NUM","interval":"1m","from":1e400}"#, |d| {
            assert_eq!(d.symbol, "NUM");
            assert_eq!(d.interval, "1m");
            assert_eq!(d.from.unwrap().to_bits(), f64::INFINITY.to_bits());
        });
        support::rejects::<Options>(
            r#"["XBT/USD","15m",47.25]"#,
            &["{}", r#"{"symbol":"x","interval":"1m","from":true}"#],
        );
    }
}
