//! Futures charts read; parsed-data host callbacks and shared Candle outputs.
use crate::wire;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    TableHit, js_number, js_string, js_table_lookup, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput},
    parse_float,
};
use safe_cex_types::Candle;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;

#[derive(Deserialize, Serialize)]
struct Options {
    symbol: String,
    interval: String,
    limit: Option<f64>,
    from: Option<f64>,
    to: Option<f64>,
}
impl JsNumberOutput for Options {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {symbol, interval, limit, from, to});
    }
}
fn truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

pub async fn fetch_ohlcv<F>(
    options: JsNumberInput,
    testnet: bool,
    now: impl FnOnce() -> Result<f64, ExchangeError>,
    read: impl FnOnce(String, Option<f64>, Option<f64>) -> Result<F, ExchangeError>,
    mut report: impl FnMut(
        &'static str,
        Option<&Value>,
        Option<&ExchangeError>,
    ) -> Result<(), ExchangeError>,
    emit_error: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<Candle>, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
{
    let outcome: Result<Vec<Candle>, ExchangeError> = async {
        let opts = options
            .decode::<Options>()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let intervals = [
            ("1m", 60000.0),
            ("5m", 300000.0),
            ("15m", 900000.0),
            ("30m", 1800000.0),
            ("1h", 3600000.0),
            ("4h", 14400000.0),
            ("12h", 43200000.0),
            ("1d", 86400000.0),
            ("1w", 604800000.0),
        ];
        let hit = js_table_lookup(&intervals, &opts.interval);
        let (resolution, interval_ms) = match hit {
            TableHit::Mapped(ms) => (opts.interval.as_str(), *ms),
            TableHit::Inherited(member) => (member.js_string(), f64::NAN),
            TableHit::Missing => ("1h", 3600000.0),
        };
        let raw_limit = opts.limit.filter(|v| truthy(*v)).unwrap_or(500.0);
        let limit = raw_limit.min(5000.0);
        let to = match opts.to.filter(|v| truthy(*v)) {
            Some(to) => to,
            None => now()?,
        };
        let mut from = opts.from;
        if !from.is_some_and(truthy) && opts.limit.is_some_and(truthy) {
            from = Some(to - interval_ms * limit);
        }
        let host = if testnet {
            "https://demo-futures.kraken.com"
        } else {
            "https://futures.kraken.com"
        };
        let url = format!("{host}/api/charts/v1/trade/{}/{resolution}", opts.symbol);
        let from = from.filter(|v| truthy(*v)).map(|v| (v / 1000.0).floor());
        let to = Some(to)
            .filter(|v| truthy(*v))
            .map(|v| (v / 1000.0).floor());
        let data = read(url, from, to)?
            .await?
            .map(wire::decode)
            .transpose()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let rows = data
            .as_ref()
            .and_then(|d| d.get("candles"))
            .filter(|v| js_truthy(Some(v)));
        let Some(rows) = rows else {
            report(
                "[Kraken] No candles in response, full response:",
                data.as_ref(),
                None,
            )?;
            return Err(ExchangeError::new("No candles data received"));
        };
        let rows = rows
            .as_array()
            .ok_or_else(|| type_error("data.candles.map is not a function"))?;
        let mut candles = Vec::with_capacity(rows.len());
        for row in rows {
            if row.is_null() {
                return Err(type_error(
                    "Cannot read properties of null (reading 'time')",
                ));
            }
            let timestamp = js_number(row.get("time")).map_err(|e| type_error(e.message))? / 1000.0;
            let parse = |key: &str| {
                js_string(row.get(key))
                    .map(|s| parse_float(&s))
                    .map_err(|e| type_error(e.message))
            };
            candles.push(Candle {
                timestamp,
                open: parse("open")?,
                high: parse("high")?,
                low: parse("low")?,
                close: parse("close")?,
                volume: parse("volume")?,
                last_side: None,
            });
        }
        if candles.iter().any(|c| c.timestamp.is_nan())
            && candles.iter().any(|c| !c.timestamp.is_nan())
        {
            return Err(type_error(
                "Kraken Futures mixed NaN timestamps are outside sortable timestamp profile",
            ));
        }
        candles.sort_by(|a, b| {
            (a.timestamp - b.timestamp)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(candles)
    }
    .await;
    match outcome {
        Ok(candles) => Ok(candles),
        Err(error) => {
            // ts-bug: catch logs/emits and resolves empty; failures of either callback propagate.
            report("[Kraken] fetchOHLCV error:", None, Some(&error))?;
            emit_error(if error.message.is_empty() {
                "Failed to fetch OHLCV"
            } else {
                &error.message
            })?;
            Ok(Vec::new())
        }
    }
}
