//! Candle read orchestration; calendar formatting is supplied by the host.
use crate::types::{CandleResponse, INTERVAL};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    JsJsonValue as J, js_date_parse, js_json_stringify, js_truthy, numeric_ingress::JsNumberInput,
};
use safe_cex_transport::provider::{RestBody, RestRequest};
use safe_cex_types::{Candle, OHLCVOptions};
use serde_json::Value;
use std::future::Future;

pub fn fetch_ohlcv<S, A, R>(
    options: OHLCVOptions,
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    to_iso: &dyn Fn(f64) -> Result<String, ExchangeError>,
    local_to_utc: &dyn Fn(f64) -> f64,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Vec<Candle>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let pending = session();
    async move {
        let result: Result<Vec<Candle>, ExchangeError> = async {
            pending.await?;
            let account = account().await?;
            // Keep undefined and falsy numeric values until shared wire serialization.
            let date = |value: Option<f64>| -> Result<J, ExchangeError> {
                match value {
                    None => Ok(J::Undefined),
                    Some(n) if n == 0.0 || n.is_nan() => Ok(J::Number(n)),
                    Some(n) => to_iso(n).map(J::String),
                }
            };
            let interval = serde_json::to_value(options.interval).expect("string enum");
            let candle_type = INTERVAL
                .iter()
                .find(|(key, _)| Some(*key) == interval.as_str())
                .map_or(J::Undefined, |(_, value)| J::String((*value).into()));
            let body = J::Object(vec![
                (
                    "eventTypes".into(),
                    J::Array(vec![J::Object(vec![
                        ("type".into(), J::String("Candle".into())),
                        ("candleType".into(), candle_type),
                        ("fromTime".into(), date(options.from)?),
                        ("toTime".into(), date(options.to)?),
                    ])]),
                ),
                ("symbols".into(), J::Array(vec![J::String(options.symbol)])),
                ("account".into(), J::String(account)),
            ]);
            let encoded = js_json_stringify(&body)
                .map_err(|e| ExchangeError::new(e.to_string()))?
                .expect("object JSON")
                .into_bytes();
            let input = request(RestRequest {
                method: "POST".into(),
                url: "marketdata".into(),
                body: Some(RestBody::Encoded(encoded)),
                headers: [("Content-Type".into(), "application/json".into())]
                    .into_iter()
                    .collect(),
                ..RestRequest::default()
            })
            .await?;
            // CHOSEN: shared declared response codec rejects malformed records.
            let data: CandleResponse = input
                .decode()
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            let candles = data
                .events
                .iter()
                .map(|raw| Candle {
                    timestamp: js_date_parse(&raw.time, local_to_utc) / 1000.0,
                    open: raw.open,
                    high: raw.high,
                    low: raw.low,
                    close: raw.close,
                    volume: raw.volume,
                    last_side: None,
                })
                .collect();
            Ok(candles)
        }
        .await;
        if let Err(error) = &result {
            let message = error
                .response
                .as_ref()
                .and_then(|r| r.get("data"))
                .and_then(|d| d.get("msg"));
            logger(if js_truthy(message) {
                message.cloned().unwrap()
            } else {
                Value::String(error.message.clone())
            })?;
        }
        result
    }
}
