// Fixed Vec<Candle> instantiation of the actual SDK bridge; terminal I/O only.
#[path = "exchange-native-runtime.rs"]
pub mod native;
use crate::exchange_classes::{
    BinanceExchange, BinancePMExchange, BinanceSpotExchange, ExchangeOwner,
};
use crate::request_weight::Failure;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, from_typed},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{Candle, OHLCVOptions};
use serde_json::{Value, json};
use std::rc::Rc;

pub type Call =
    for<'a> fn(&'a Rc<ExchangeOwner>, Result<Vec<Js>, Failure>) -> ExchangeFuture<'a, Vec<Candle>>;

enum Subject {
    Futures(BinanceExchange),
    PM(BinancePMExchange),
    Spot(BinanceSpotExchange),
}
impl Subject {
    fn owner(&self) -> &Rc<ExchangeOwner> {
        match self {
            Self::Futures(value) => &value.0,
            Self::PM(value) => &value.0,
            Self::Spot(value) => &value.0,
        }
    }
    fn candles(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        match self {
            Self::Futures(value) => value.fetch_ohlcv(options),
            Self::PM(value) => value.fetch_ohlcv(options),
            Self::Spot(value) => value.fetch_ohlcv(options),
        }
    }
    fn dispose(&self) {
        match self {
            Self::Futures(value) => value.dispose().unwrap(),
            Self::PM(value) => value.dispose().unwrap(),
            Self::Spot(value) => value.dispose().unwrap(),
        }
    }
}
fn subject(input: &Value) -> (native::Parts, Subject) {
    let mut parts = native::parts(input);
    let (_, name) = native::family(input);
    let options = parts.options.clone();
    let store = parts.store.clone();
    let deps = parts.deps.take().unwrap();
    let subject = match input["family"].as_str().unwrap() {
        "futures" => Subject::Futures(BinanceExchange::new(name, options, store, deps).unwrap()),
        "pm" => Subject::PM(BinancePMExchange::new(name, options, store, deps).unwrap()),
        "spot" => Subject::Spot(BinanceSpotExchange::new(name, options, store, deps).unwrap()),
        _ => panic!("unreviewed SDK family"),
    };
    if input.get("argumentError").is_none() {
        parts.http.ok(
            "GET",
            if input["family"] == "spot" {
                "/api/v3/klines"
            } else {
                "/fapi/v1/klines"
            },
            input["data"].clone(),
        );
    }
    (parts, subject)
}
fn number(value: f64) -> Value {
    if value.is_nan() {
        json!({"kind":"NaN"})
    } else {
        json!({"bits":format!("{:016x}",value.to_bits())})
    }
}
fn result(value: Result<Vec<Candle>, ExchangeError>) -> Value {
    match value {
        Ok(rows) => json!({"candles": rows.iter().map(|row| {
            json!({"timestamp":number(row.timestamp),"open":number(row.open),"high":number(row.high),"low":number(row.low),"close":number(row.close),"volume":number(row.volume),"lastSide":row.last_side})
        }).collect::<Vec<_>>()}),
        Err(error) => {
            json!({"error":error.name,"message":error.message,"code":error.code.as_ref().map(native::raw),"context":error.context})
        }
    }
}
fn finish(
    parts: &native::Parts,
    subject: &Subject,
    value: Result<Vec<Candle>, ExchangeError>,
) -> Value {
    let result = result(value);
    let requests = native::wire(&parts.http);
    let reports = parts.runtime.reports.borrow().len();
    subject.dispose();
    json!({"result":result,"requests":requests,"reports":reports})
}
pub fn observe(input: &Value, call: Call) -> Value {
    let (parts, subject) = subject(input);
    let args = if let Some(message) = input["argumentError"].as_str() {
        let mut error = ExchangeError::new(message);
        error.name = "ArgumentFailure".into();
        error.metadata.code = Some(Js::Number(if input["negativeZero"] == true {
            -0.0
        } else {
            17.0
        }));
        error.metadata.context = Some(json!({"stage":"argument"}));
        Err(Rc::new(error))
    } else {
        let options: OHLCVOptions = serde_json::from_value(input["options"].clone()).unwrap();
        Ok(vec![from_typed(&options).unwrap()])
    };
    let value = native::drive(&parts.runtime, call(subject.owner(), args));
    finish(&parts, &subject, value)
}
// Independent actual public SDK consumer for the maintained-source comparison.
pub fn public_sdk(input: &Value) -> Value {
    assert!(input.get("argumentError").is_none());
    let (parts, subject) = subject(input);
    let options: OHLCVOptions = serde_json::from_value(input["options"].clone()).unwrap();
    let value = native::drive(&parts.runtime, subject.candles(options));
    finish(&parts, &subject, value)
}
pub fn inputs() -> Vec<Value> {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/sdk-call-inputs.json"
    )))
    .unwrap()
}
