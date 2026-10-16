//! Remaining REST reads over retained JSON and host date/store projections.
use safe_cex_num::{JsJsonValue, js_number, js_string, js_truthy, parse_float};
use safe_cex_types::{Candle, MarginType};
use serde_json::Value;
use std::future::Future;

pub struct OhlcvReadOptions {
    pub symbol: String,
    pub interval: String,
    pub limit: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
}
fn endpoint(key: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == key)
        .unwrap()
        .1
}
fn truthy(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn float(value: Option<&Value>) -> Result<f64, String> {
    Ok(parse_float(
        &js_string(value).map_err(|e| e.message.to_owned())?,
    ))
}

pub async fn fetch_ohlcv<D, F: Future<Output = Result<Value, String>>>(
    options: OhlcvReadOptions,
    mut date: impl FnMut(Option<f64>) -> Result<D, String>,
    subtract: impl FnOnce(&D, f64, Option<&str>) -> Result<D, String>,
    mut value_of: impl FnMut(&D) -> Result<f64, String>,
    get: impl FnOnce(&'static str, JsJsonValue) -> F,
) -> Result<Vec<Candle>, String> {
    let requested = options.limit.filter(|v| truthy(*v)).unwrap_or(200.0);
    let limit = if requested.is_nan() {
        f64::NAN
    } else {
        requested.min(200.0)
    };
    let bytes = options.interval.as_bytes();
    let (amount, unit) = if let Some(start) = bytes.iter().position(u8::is_ascii_digit) {
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
        let unit_end = end
            + bytes[end..]
                .iter()
                .take_while(|b| !b.is_ascii_digit())
                .count();
        (
            parse_float(&options.interval[start..end]),
            Some(&options.interval[end..unit_end]),
        )
    } else {
        (f64::NAN, None)
    };
    let end = date(options.to.filter(|v| truthy(*v)))?;
    let start = if !options.limit.is_some_and(truthy) && options.from.is_some_and(truthy) {
        date(options.from)?
    } else {
        subtract(&end, amount * limit, unit)?
    };
    let mut params = vec![
        ("symbol".into(), JsJsonValue::String(options.symbol)),
        ("interval".into(), JsJsonValue::String(options.interval)),
        ("startTime".into(), JsJsonValue::Number(value_of(&start)?)),
        ("endTime".into(), JsJsonValue::Number(value_of(&end)?)),
    ];
    if options.limit.is_some_and(truthy) {
        params.push(("limit".into(), JsJsonValue::Number(limit)));
    }
    let data = get(
        endpoint("KLINE"),
        JsJsonValue::Object(vec![
            ("params".into(), JsJsonValue::Object(params)),
            ("timeout".into(), JsJsonValue::Number(300_000.0)),
        ]),
    )
    .await?;
    let Some(rows) = data.as_array() else {
        return Ok(vec![]);
    };
    rows.iter()
        .map(|row| {
            if row.is_null() {
                return Err("Cannot read candle fields from null".to_owned());
            }
            Ok(Candle {
                timestamp: js_number(row.get("time")).map_err(|e| e.message.to_owned())? / 1000.0,
                open: float(row.get("open"))?,
                high: float(row.get("high"))?,
                low: float(row.get("low"))?,
                close: float(row.get("close"))?,
                volume: float(row.get("quoteVol"))?,
                last_side: None,
            })
        })
        .collect()
}

pub async fn update_symbol_metadata<P, E, F: Future<Output = Result<Value, E>>>(
    symbol: &str,
    get: impl FnOnce(&'static str, JsJsonValue) -> F,
    set: impl FnOnce(&str, MarginType, Option<Value>) -> Result<(), E>,
    positions: impl FnOnce() -> Result<Vec<P>, E>,
    mut matches: impl FnMut(&P, &str) -> Result<bool, E>,
    mut overlay: impl FnMut(P, Option<Value>, MarginType) -> Result<P, E>,
    update: impl FnOnce(Vec<P>) -> Result<(), E>,
) -> Result<(), E> {
    let data = get(
        endpoint("LEVERAGE_MARGIN_MODE"),
        JsJsonValue::Object(vec![(
            "params".into(),
            JsJsonValue::Object(vec![
                ("symbol".into(), JsJsonValue::String(symbol.into())),
                ("marginCoin".into(), JsJsonValue::String("USDT".into())),
            ]),
        )]),
    )
    .await?;
    let margin = if data.get("marginMode").and_then(Value::as_str) == Some("ISOLATION") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    };
    let leverage = data.get("leverage").cloned();
    set(symbol, margin, leverage.clone())?;
    if js_truthy(Some(&data)) {
        let mut result = Vec::new();
        for position in positions()? {
            result.push(if matches(&position, symbol)? {
                overlay(position, leverage.clone(), margin)?
            } else {
                position
            });
        }
        update(result)?;
    }
    Ok(())
}

pub struct ValidationOutcome<B, E> {
    pub error: JsJsonValue,
    pub balance: Option<B>,
    pub includes_balance: bool,
    pub balance_error: Option<E>,
}

pub async fn validate_account<
    B,
    E,
    Get: Future<Output = Result<Value, E>>,
    Balance: Future<Output = Result<B, E>>,
>(
    get: impl FnOnce(&'static str, JsJsonValue) -> Get,
    extract: impl FnOnce(&Value) -> Result<(), E>,
    malformed: impl FnOnce(&'static str) -> E,
    message: impl FnOnce(&E) -> Result<JsJsonValue, E>,
    report: impl FnOnce(&JsJsonValue) -> Result<(), E>,
    balance: impl FnOnce() -> Balance,
) -> Result<ValidationOutcome<B, E>, E> {
    let validation = async {
        let data = get(
            endpoint("ACCOUNT"),
            JsJsonValue::Object(vec![(
                "params".into(),
                JsJsonValue::Object(vec![(
                    "marginCoin".into(),
                    JsJsonValue::String("USDT".into()),
                )]),
            )]),
        )
        .await?;
        if data.is_null() {
            return Err(malformed("Cannot read properties of null (reading 'code')"));
        }
        if data.get("code").and_then(Value::as_f64) != Some(0.0) {
            let error = if js_truthy(data.get("msg")) {
                JsJsonValue::from(data["msg"].clone())
            } else {
                JsJsonValue::String("Bitunix account validation failed".into())
            };
            return Ok(Some(error));
        }
        extract(&data)?;
        Ok(None)
    }
    .await;
    let error = match validation {
        Ok(Some(error)) => Some(error),
        Ok(None) => None,
        Err(error) => {
            let message = message(&error)?;
            report(&message)?;
            let present = match &message {
                JsJsonValue::Undefined | JsJsonValue::Null => false,
                JsJsonValue::Bool(v) => *v,
                JsJsonValue::Number(v) => truthy(*v),
                JsJsonValue::String(v) => !v.is_empty(),
                _ => true,
            };
            Some(if present {
                message
            } else {
                JsJsonValue::String("Invalid API key or secret".into())
            })
        }
    };
    if let Some(error) = error {
        return Ok(ValidationOutcome {
            error,
            balance: None,
            includes_balance: false,
            balance_error: None,
        });
    }
    let (balance, balance_error) = match balance().await {
        Ok(balance) => (Some(balance), None),
        Err(error) => (None, Some(error)),
    };
    Ok(ValidationOutcome {
        error: JsJsonValue::String(String::new()),
        balance,
        includes_balance: true,
        balance_error,
    })
}
