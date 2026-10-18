//! REST reads over caller-owned asynchronous I/O and shared store snapshots.
use crate::{
    row_extraction::{extract_account_row, get_first_value},
    value_helpers::{is_api_trading_supported, parse_finite_number},
};
use safe_cex_num::{
    JsCoercionError, TableHit,
    get_key::{GetKeyObject, GetKeyValue, v},
    js_number, js_string, js_truthy, parse_float, parse_int,
};
use safe_cex_types::{
    Balance, Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    Position, PositionMode, Ticker,
};
use serde_json::{Value, json};
use std::future::Future;

fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .expect("Bitunix endpoint must exist")
        .1
}

fn error(message: &'static str) -> JsCoercionError {
    JsCoercionError {
        name: "TypeError",
        message,
    }
}

fn key<'a>(row: &'a Value, name: &str) -> Option<&'a Value> {
    match v(row.as_object().map(GetKeyObject::Json), name) {
        TableHit::Mapped(GetKeyValue::Json(value)) => Some(value),
        _ => None,
    }
}

fn first<'a>(row: Option<&'a Value>, fields: &[&str]) -> TableHit<&'a Value> {
    row.and_then(Value::as_object)
        .map_or(TableHit::Missing, |row| get_first_value(row, fields))
}

fn first_number(row: Option<&Value>, fields: &[&str]) -> Result<f64, JsCoercionError> {
    match first(row, fields) {
        TableHit::Mapped(value) => parse_finite_number(Some(value), 0.0),
        TableHit::Missing | TableHit::Inherited(_) => Ok(0.0),
    }
}

fn position_mode(account: Option<&Value>) -> Result<PositionMode, JsCoercionError> {
    let value = match first(account, &["positionMode", "posMode"]) {
        TableHit::Mapped(value) if js_truthy(Some(value)) => js_string(Some(value))?,
        TableHit::Inherited(member) => member.js_string().into(),
        _ => String::new(),
    };
    Ok(if value.to_uppercase() == "ONE_WAY" {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    })
}

fn precision(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let Some(value) = value else { return Ok(0.0) };
    let digits = parse_int(&js_string(Some(value))?, None);
    if digits == 0.0 {
        return Ok(1.0);
    }
    if digits.is_nan() || digits < 1.0 {
        return Ok(0.1);
    }
    if digits > 536_870_888.0 {
        return Err(JsCoercionError {
            name: "RangeError",
            message: "Invalid string length",
        });
    }
    Ok(parse_float(
        &safe_cex_num::ccxt_precision::precision_to_string(digits),
    ))
}

pub async fn fetch_markets<E, F>(
    cached: impl FnOnce() -> Vec<Market>,
    read: impl FnOnce(&'static str, Value) -> F,
    mut trace: impl FnMut(&E),
    mut refuse: impl FnMut(&str),
) -> Vec<Market>
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result: Result<(Vec<Market>, Vec<String>), E> = async {
        let response = read(endpoint("MARKETS"), json!({})).await?;
        let rows = response
            .as_ref()
            .and_then(|response| response.get("data"))
            .and_then(Value::as_array)
            .ok_or_else(|| E::from(error("Market response data must be an array")))?;
        let mut markets = Vec::new();
        let mut refusals = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if key(row, "symbolStatus").and_then(Value::as_str) != Some("OPEN")
                || key(row, "quote").and_then(Value::as_str) != Some("USDT")
                || !is_api_trading_supported(key(row, "isApiSupported"))
            {
                continue;
            }
            let id = format!("{}/USDT", js_string(key(row, "base"))?);
            let amount_precision = precision(key(row, "basePrecision"))?;
            let price_precision = precision(key(row, "quotePrecision"))?;
            let Some(base) = key(row, "base").and_then(Value::as_str) else {
                let value = key(row, "base").map_or_else(|| "undefined".into(), Value::to_string);
                let symbol = row.get("symbol").map_or_else(|| "undefined".into(), Value::to_string);
                refusals.push(format!("Refused Bitunix market row {index} symbol={symbol}: base={value}: expected string"));
                continue;
            };
            let Some(symbol) = row.get("symbol").and_then(Value::as_str) else {
                let value = row.get("symbol").map_or_else(|| "undefined".into(), Value::to_string);
                refusals.push(format!("Refused Bitunix market row {index}: symbol={value}: expected string"));
                continue;
            };
            let max_amount = if let Some(Value::String(value)) = key(row, "maxLimitOrderVolume") {
                safe_cex_types::NumericValue::Text(value.clone())
            } else {
                match js_number(key(row, "maxLimitOrderVolume")) {
                    Ok(value) => safe_cex_types::NumericValue::Number(value),
                    Err(cause) => {
                        let value = key(row, "maxLimitOrderVolume").map_or_else(|| "undefined".into(), Value::to_string);
                        refusals.push(format!("Refused Bitunix market row {index} symbol={symbol:?}: limits.amount.max={value}: {}", cause.message));
                        continue;
                    }
                }
            };
            markets.push(Market {
                runtime: Default::default(),
                id,
                symbol: symbol.into(),
                base: base.into(),
                quote: "USDT".into(),
                active: true,
                precision: MarketPrecision {
                    base: None,
                    quote: None,
                    amount: amount_precision,
                    price: price_precision,
                },
                limits: MarketLimits {
                    cost: None,
                    amount: MarketLimitsAmount {
                        min: 0.0,
                        max: max_amount,
                    },
                    leverage: MarketLimitsLeverage {
                        min: 1.0,
                        max: 100.0,
                    },
                    price: None,
                },
                pretty_symbol: None,
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                info: None,
                taker_fee: None,
                maker_fee: None,
                option: None,
            });
        }
        Ok((markets, refusals))
    }
    .await;
    match result {
        Ok((markets, refusals)) => {
            for message in refusals {
                refuse(&message);
            }
            markets
        }
        Err(error) => {
            trace(&error);
            cached()
        }
    }
}

pub async fn fetch_tickers<'a, E, F>(
    cached: impl FnOnce() -> Vec<Ticker>,
    mut market_by_symbol: impl FnMut(&str) -> Option<&'a Market>,
    read: impl FnOnce(&'static str, Value) -> F,
    mut trace: impl FnMut(&E),
) -> Vec<Ticker>
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result: Result<Vec<Ticker>, E> = async {
        let response = read(endpoint("TICKERS"), json!({})).await?;
        let rows = response
            .as_ref()
            .and_then(|response| response.get("data"))
            .and_then(Value::as_array)
            .ok_or_else(|| E::from(error("Ticker response data must be an array")))?;
        let mut tickers = Vec::new();
        for row in rows {
            if row.is_null() {
                return Err(error("Ticker row must not be null").into());
            }
            let symbol = js_string(row.get("symbol"))?;
            let Some(market) = market_by_symbol(&symbol) else {
                continue;
            };
            let last = parse_float(&js_string(key(row, "last"))?);
            let open = parse_float(&js_string(key(row, "open"))?);
            let mark = parse_float(&js_string(key(row, "markPrice"))?);
            tickers.push(Ticker {
                id: market.id.clone(),
                symbol: market.symbol.clone(),
                bid: last,
                ask: last,
                last,
                mark,
                index: mark,
                percentage: if open == 0.0 {
                    0.0
                } else {
                    ((last - open) / open) * 100.0
                },
                funding_rate: 0.0,
                funding_time: 0.0,
                volume: parse_float(&js_string(key(row, "baseVol"))?),
                quote_volume: parse_float(&js_string(key(row, "quoteVol"))?),
                open_interest: 0.0,
                mid_price: None,
                info: None,
                timestamp: None,
            });
        }
        Ok(tickers)
    }
    .await;
    match result {
        Ok(tickers) => tickers,
        Err(error) => {
            trace(&error);
            cached()
        }
    }
}

pub async fn fetch_balance<'a, E, F>(
    cached: impl FnOnce() -> Balance,
    positions: impl FnOnce() -> &'a [Position],
    account_position_mode: &mut PositionMode,
    read: impl FnOnce(&'static str, Value) -> F,
    mut report: impl FnMut(&E),
) -> (Balance, &'a [Position])
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result: Result<Balance, E> = async {
        let response = read(endpoint("ACCOUNT"), json!({"params":{"marginCoin":"USDT"}})).await?;
        let account = extract_account_row(response.as_ref(), "USDT")?;
        *account_position_mode = position_mode(account)?;
        let free = first_number(
            account,
            &["available", "availableBalance", "availableMargin"],
        )?;
        let cross = first(account, &["crossUnrealizedPNL", "crossUnrealizedPnl"]);
        let isolated = first(
            account,
            &["isolationUnrealizedPNL", "isolationUnrealizedPnl"],
        );
        let upnl = if cross.is_present() || isolated.is_present() {
            first_number(account, &["crossUnrealizedPNL", "crossUnrealizedPnl"])?
                + first_number(
                    account,
                    &["isolationUnrealizedPNL", "isolationUnrealizedPnl"],
                )?
        } else {
            first_number(account, &["unrealizedPNL", "unrealizedPnl", "upnl"])?
        };
        let used = first_number(account, &["margin", "used", "usedMargin"])?;
        let total = free + used + first_number(account, &["frozen", "locked"])?;
        Ok(Balance {
            total,
            free,
            used,
            upnl,
            info: None,
            currencies: None,
            coins: None,
        })
    }
    .await;
    let balance = match result {
        Ok(balance) => balance,
        Err(error) => {
            report(&error);
            cached()
        }
    };
    (balance, positions())
}

pub async fn fetch_position_mode<E, F>(
    read: impl FnOnce(&'static str, Value) -> F,
) -> Result<bool, E>
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let response = read(endpoint("ACCOUNT"), json!({"params":{"marginCoin":"USDT"}})).await?;
    let account = extract_account_row(response.as_ref(), "USDT")?;
    Ok(position_mode(account)? != PositionMode::OneWay)
}
