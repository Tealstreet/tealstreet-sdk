//! GRVT public reads and account leverage against configured client hooks.
use indexmap::IndexMap;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{
    TableHit, js_number, js_number_string, js_truthy, math_max, math_min, parse_float, parse_int,
};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{Candle, FetchedLeverage, Market, OHLCVOptions, OrderBook, OrderBookOrders};
use serde::Serialize;
use serde_json::{Value, json};
use std::{borrow::Cow, future::Future};

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    safe_cex_types::js_coercion::js_string(value).map_err(|cause| type_error(cause.message))
}
fn number(value: Option<&Value>) -> Result<f64, ExchangeError> {
    js_number(value).map_err(|cause| type_error(cause.message))
}
fn parsed(value: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&text(value)?))
}
fn or<'a>(first: Option<&'a Value>, second: Option<&'a Value>) -> Option<&'a Value> {
    first.filter(|v| js_truthy(Some(v))).or(second)
}
fn default_parse(value: Option<&Value>, fallback: &str) -> Result<f64, ExchangeError> {
    if js_truthy(value) {
        parsed(value)
    } else {
        Ok(parse_float(fallback))
    }
}
fn numeric_truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn catch<T>(
    result: Result<T, ExchangeError>,
    prefix: &str,
    severity: LogSeverity,
    log: &mut impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
) -> Result<T, ExchangeError> {
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            log(&format!("{prefix}{}", cause.js_error_string()), severity)?;
            Err(cause)
        }
    }
}

enum MarketHit {
    Missing,
    Stored(Market),
    Inherited,
}
impl MarketHit {
    fn id(&self) -> Option<&str> {
        match self {
            Self::Stored(market) => Some(&market.id),
            _ => None,
        }
    }
}
fn market(store: &dyn Store, symbol: &str) -> Result<MarketHit, ExchangeError> {
    if let Some(market) = store.markets_by_symbol()?.get(symbol) {
        return Ok(MarketHit::Stored(market.clone()));
    }
    if matches!(
        safe_cex_num::js_table_lookup::<&str, ()>(&[], symbol),
        TableHit::Inherited(_)
    ) {
        Ok(MarketHit::Inherited)
    } else {
        Ok(MarketHit::Missing)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KlineRequest {
    #[serde(rename = "i", skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
    #[serde(rename = "i1")]
    pub interval: crate::types::GrvtCandlestickInterval,
    #[serde(rename = "t")]
    pub kind: &'static str,
    #[serde(rename = "st", skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(rename = "et", skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BookRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
}

/// ts: GrvtExchange.fetchOHLCV. Native request numbers reach the configured client unchanged.
pub fn fetch_ohlcv<'a, F: Future<Output = Result<Value, ExchangeError>> + 'a>(
    store: &dyn Store,
    opts: &'a OHLCVOptions,
    post: impl FnOnce(&str, KlineRequest) -> F,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<Vec<Candle>, ExchangeError>> + 'a {
    let begin = (|| {
        let market = market(store, &opts.symbol)?;
        if matches!(market, MarketHit::Missing) {
            log(
                &format!("Market {} not found", opts.symbol),
                LogSeverity::Warning,
            )?;
            return Ok(None);
        }
        let timeframe = serde_json::to_value(&opts.interval).expect("closed timeframe");
        let TableHit::Mapped(interval) =
            crate::order_semantics::convert_interval(timeframe.as_str().expect("timeframe string"))
        else {
            unreachable!("closed timeframe cannot be inherited");
        };
        let request = KlineRequest {
            instrument_id: market.id().map(str::to_owned),
            interval,
            kind: "TRADE",
            start: opts
                .from
                .filter(|value| numeric_truthy(*value))
                .map(|value| js_number_string(value * 1000000.0)),
            end: opts
                .to
                .filter(|value| numeric_truthy(*value))
                .map(|value| js_number_string(value * 1000000.0)),
            limit: opts.limit.filter(|value| numeric_truthy(*value)),
        };
        let interval = serde_json::to_value(interval).expect("closed interval");
        log(
            &format!(
                "Fetching OHLCV for {} with interval {}",
                opts.symbol,
                interval.as_str().expect("interval string")
            ),
            LogSeverity::Debug,
        )?;
        Ok(Some(post("/lite/v1/kline", request)))
    })();
    let begin = catch(
        begin,
        "Error fetching OHLCV: ",
        LogSeverity::Error,
        &mut log,
    );
    async move {
        let Some(request) = begin? else {
            return Ok(Vec::new());
        };
        let result = async {
            let data = request.await?;
            let rows = data.get("r");
            if !js_truthy(rows) {
                log("OHLCV API returned invalid data", LogSeverity::Warning)?;
                return Ok(Vec::new());
            }
            let rows = rows
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("response.data.r.map is not a function"))?;
            let mut candles = Vec::new();
            for row in rows {
                if row.is_null() {
                    return Err(type_error("Cannot read properties of null (reading 'ot')"));
                }
                let timestamp = or(row.get("ot"), row.get("open_time"));
                let timestamp = if let Some(Value::String(value)) = timestamp {
                    parse_int(value, None) / 1000000000.0
                } else {
                    number(timestamp)? / 1000.0
                };
                candles.push(Candle {
                    timestamp,
                    open: default_parse(or(row.get("o"), row.get("open")), "0")?,
                    high: default_parse(or(row.get("h"), row.get("high")), "0")?,
                    low: default_parse(or(row.get("l"), row.get("low")), "0")?,
                    close: default_parse(or(row.get("c"), row.get("close")), "0")?,
                    volume: default_parse(
                        or(or(row.get("vb"), row.get("volume_b")), row.get("volume")),
                        "0",
                    )?,
                    last_side: None,
                });
            }
            candles.reverse();
            log(
                &format!("Fetched {} candles for {}", candles.len(), opts.symbol),
                LogSeverity::Info,
            )?;
            Ok(candles)
        }
        .await;
        catch(
            result,
            "Error fetching OHLCV: ",
            LogSeverity::Error,
            &mut log,
        )
    }
}

fn book_side(data: &Value, key: &str) -> Result<Vec<OrderBookOrders>, ExchangeError> {
    let Some(rows) = data.get(key).filter(|value| js_truthy(Some(value))) else {
        return Ok(Vec::new());
    };
    let rows = rows
        .as_array()
        .ok_or_else(|| type_error(format!("(data.{key} || []).map is not a function")))?;
    let mut levels = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'price')",
            ));
        }
        let price = parsed(row.get("price"))?;
        let amount = parsed(row.get("size"))?;
        let mut total = amount;
        for prior in &rows[..index] {
            total += parsed(prior.get("size"))?;
        }
        levels.push(OrderBookOrders {
            price,
            amount,
            total,
        });
    }
    Ok(levels)
}

/// ts: GrvtExchange.fetchOrderBook. Cumulative totals start with this level, then prior levels.
pub fn fetch_order_book<'a, F: Future<Output = Result<Value, ExchangeError>> + 'a>(
    store: &dyn Store,
    symbol: &'a str,
    post: impl FnOnce(&str, BookRequest) -> F,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<OrderBook, ExchangeError>> + 'a {
    let begin = (|| {
        let market = market(store, symbol)?;
        if matches!(market, MarketHit::Missing) {
            log(&format!("Market {symbol} not found"), LogSeverity::Warning)?;
            return Ok(None);
        }
        let body = BookRequest {
            instrument_id: market.id().map(str::to_owned),
        };
        log(
            &format!("Fetching orderbook for {symbol}"),
            LogSeverity::Debug,
        )?;
        Ok(Some(post("/lite/v1/book", body)))
    })();
    let begin = catch(
        begin,
        "Error fetching orderbook: ",
        LogSeverity::Error,
        &mut log,
    );
    async move {
        let Some(request) = begin? else {
            return Ok(OrderBook {
                bids: Vec::new(),
                asks: Vec::new(),
            });
        };
        let result = async {
            let data = request.await?;
            let rows = data.get("r");
            if !js_truthy(rows) {
                log("Orderbook API returned invalid data", LogSeverity::Warning)?;
                return Ok(OrderBook {
                    bids: Vec::new(),
                    asks: Vec::new(),
                });
            }
            let rows = rows.expect("truthy row");
            let bids = book_side(rows, "bids")?;
            let asks = book_side(rows, "asks")?;
            log(
                &format!(
                    "Fetched orderbook for {symbol} - Bids: {}, Asks: {}",
                    bids.len(),
                    asks.len()
                ),
                LogSeverity::Info,
            )?;
            Ok(OrderBook { bids, asks })
        }
        .await;
        catch(
            result,
            "Error fetching orderbook: ",
            LogSeverity::Error,
            &mut log,
        )
    }
}

fn results(response: &Value) -> Result<Option<&Value>, ExchangeError> {
    if response.is_null() {
        return Err(type_error(
            "Cannot read properties of null (reading 'results')",
        ));
    }
    Ok(or(response.get("results"), response.get("r")))
}
fn no_results(results: Option<&Value>) -> bool {
    !js_truthy(results)
        || results.is_some_and(|value| match value {
            Value::Array(rows) => rows.is_empty(),
            _ => value.get("length").and_then(Value::as_f64) == Some(0.0),
        })
}
fn instrument(item: &Value) -> Result<Option<&Value>, ExchangeError> {
    if item.is_null() {
        return Err(type_error(
            "Cannot read properties of null (reading 'instrument')",
        ));
    }
    Ok(or(item.get("instrument"), item.get("i")))
}
fn uppercase(value: Option<&Value>, message: &str) -> Result<Option<String>, ExchangeError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.to_uppercase())),
        _ => Err(type_error(message)),
    }
}
fn leverage(value: f64) -> FetchedLeverage {
    FetchedLeverage {
        buy_leverage: value,
        sell_leverage: value,
        margin_type: None,
        position_mode: None,
        is_fetching: None,
    }
}
fn cache(store: &dyn Store, symbol: &str, value: &FetchedLeverage) -> Result<(), ExchangeError> {
    let mut updated = store.fetched_leverage()?.clone();
    updated.insert(symbol.into(), value.clone());
    store.update(
        StoreUpdate {
            fetched_leverage: Some(updated),
            ..Default::default()
        },
        false,
    )?;
    Ok(())
}

fn default_cache(
    store: &dyn Store,
    symbol: &str,
    default: &impl Fn() -> f64,
) -> Result<Option<FetchedLeverage>, ExchangeError> {
    let mut value = leverage(default());
    value.sell_leverage = default();
    cache(store, symbol, &value)?;
    Ok(Some(value))
}

/// ts: GrvtExchange#updateMarketLeverageLimits. Unchanged and failed paths borrow the original input.
pub fn update_market_leverage_limits<
    'a,
    F: Future<Output = Result<Value, ExchangeError>> + 'a,
    G: Future<Output = Result<Value, ExchangeError>> + 'a,
>(
    markets: &'a [Market],
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    authenticate: impl FnOnce() -> F,
    get: impl FnOnce(Value, Value) -> G + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<Cow<'a, [Market]>, ExchangeError>> + 'a {
    let begin: Result<_, ExchangeError> = (|| {
        if !has_client {
            return Ok(None);
        }
        let Some(account) = account().filter(|value| !value.is_empty()) else {
            log(
                "Skipping leverage limits fetch - no trading account ID provided",
                LogSeverity::Debug,
            )?;
            return Ok(None);
        };
        Ok(Some((json!({"sub_account_id":account}), authenticate())))
    })();
    let begin = match begin {
        Ok(value) => Ok(value),
        Err(cause) => match log(
            &format!(
                "Error fetching leverage limits: {}",
                cause.js_error_string()
            ),
            LogSeverity::Warning,
        ) {
            Ok(()) => Ok(None),
            Err(error) => Err(error),
        },
    };
    async move {
        let begin = begin?;
        let result: Result<_, ExchangeError> = async {
            let Some((payload, endpoint)) = begin else {
                return Ok(Cow::Borrowed(markets));
            };
            let response = get(payload, endpoint.await?).await?;
            let rows = results(&response)?;
            if no_results(rows) {
                log("No leverage data returned from API", LogSeverity::Debug)?;
                return Ok(Cow::Borrowed(markets));
            }
            let mut mapped = IndexMap::new();
            match rows.expect("truthy results") {
                Value::Array(rows) => {
                    for item in rows {
                        let instrument = uppercase(
                            instrument(item)?,
                            "(item.instrument || item.i)?.toUpperCase is not a function",
                        )?;
                        let max =
                            default_parse(or(item.get("max_leverage"), item.get("ml1")), "100")?;
                        if let Some(instrument) = instrument.filter(|value| !value.is_empty())
                            && max > 0.0
                        {
                            mapped.insert(instrument, max);
                        }
                    }
                }
                Value::String(_) => {}
                _ => return Err(type_error("results is not iterable")),
            }
            let updated = markets
                .iter()
                .map(|market| {
                    let mut market = market.clone();
                    if let Some(max) = mapped
                        .get(&market.id.to_uppercase())
                        .or_else(|| mapped.get(&market.symbol))
                    {
                        market.limits.leverage.max = *max;
                    }
                    market
                })
                .collect::<Vec<_>>();
            log(
                &format!("Updated leverage limits for {} instruments", mapped.len()),
                LogSeverity::Debug,
            )?;
            Ok(Cow::Owned(updated))
        }
        .await;
        match result {
            Ok(markets) => Ok(markets),
            Err(cause) => {
                log(
                    &format!(
                        "Error fetching leverage limits: {}",
                        cause.js_error_string()
                    ),
                    LogSeverity::Warning,
                )?;
                Ok(Cow::Borrowed(markets))
            }
        }
    }
}

/// ts: GrvtExchange.fetchLeverageForSymbol. Catch fallback updates the live cache again.
pub fn fetch_leverage_for_symbol<
    'a,
    F: Future<Output = Result<Value, ExchangeError>> + 'a,
    G: Future<Output = Result<Value, ExchangeError>> + 'a,
>(
    store: &'a dyn Store,
    symbol: &'a str,
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    default: impl Fn() -> f64 + 'a,
    authenticate: impl FnOnce() -> F,
    get: impl FnOnce(Value, Value) -> G + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<Option<FetchedLeverage>, ExchangeError>> + 'a {
    let begin = (|| {
        if !has_client {
            return Err(ExchangeError::new("GRVT SDK client not initialized"));
        }
        let market = market(store, symbol)?;
        if matches!(market, MarketHit::Missing) {
            log(&format!("Market {symbol} not found"), LogSeverity::Warning)?;
            return Ok(Err(None));
        }
        let Some(account) = account().filter(|value| !value.is_empty()) else {
            log(
                &format!("Skipping leverage fetch for {symbol} - no trading account ID provided"),
                LogSeverity::Debug,
            )?;
            return Ok(Err(default_cache(store, symbol, &default)?));
        };
        Ok(Ok((
            market,
            json!({"sub_account_id":account}),
            authenticate(),
        )))
    })();
    let begin = match begin {
        Ok(value) => Ok(value),
        Err(cause) => log(
            &format!(
                "Error fetching leverage for {symbol}: {}, using default {}",
                cause.message,
                js_number_string(default())
            ),
            LogSeverity::Warning,
        )
        .and_then(|_| default_cache(store, symbol, &default))
        .map(Err),
    };
    async move {
        let begin = begin?;
        let result: Result<Option<FetchedLeverage>, ExchangeError> = async {
            let (market, payload, endpoint) = match begin {
                Err(value) => return Ok(value),
                Ok(request) => request,
            };
            let response = get(payload, endpoint.await?).await?;
            let rows = results(&response)?;
            if no_results(rows) {
                log(
                    &format!(
                        "No leverage data found for {symbol}, using default {}",
                        js_number_string(default())
                    ),
                    LogSeverity::Debug,
                )?;
                return default_cache(store, symbol, &default);
            }
            let rows = rows
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("results.find is not a function"))?;
            let mut found = None;
            for row in rows {
                let id = instrument(row)?;
                if uppercase(id, "(r.instrument || r.i)?.toUpperCase is not a function")?.as_deref()
                    == Some(symbol)
                    || match market.id() {
                        Some(market_id) => id.and_then(Value::as_str) == Some(market_id),
                        None => id.is_none(),
                    }
                {
                    found = Some(row);
                    break;
                }
            }
            let Some(found) = found.filter(|row| js_truthy(Some(row))) else {
                log(
                    &format!(
                        "No leverage data for instrument {}, using default {}",
                        market.id().unwrap_or("undefined"),
                        js_number_string(default())
                    ),
                    LogSeverity::Debug,
                )?;
                return default_cache(store, symbol, &default);
            };
            let value = default_parse(or(found.get("leverage"), found.get("l")), "1")?;
            log(
                &format!("Fetched leverage for {symbol}: {}", js_number_string(value)),
                LogSeverity::Debug,
            )?;
            let value = leverage(value);
            cache(store, symbol, &value)?;
            Ok(Some(value))
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(cause) => {
                log(
                    &format!(
                        "Error fetching leverage for {symbol}: {}, using default {}",
                        cause.message,
                        js_number_string(default())
                    ),
                    LogSeverity::Warning,
                )?;
                default_cache(store, symbol, &default)
            }
        }
    }
}

/// ts: GrvtExchange.setLeverage. Cache notification precedes a fresh first-position lookup.
pub fn set_leverage<
    'a,
    F: Future<Output = Result<Value, ExchangeError>> + 'a,
    G: Future<Output = Result<Value, ExchangeError>> + 'a,
>(
    store: &'a dyn Store,
    symbol: &'a str,
    input: f64,
    has_client: bool,
    account: impl Fn() -> Option<String>,
    authenticate: impl FnOnce() -> F,
    set: impl FnOnce(Value, Value) -> G + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<(), ExchangeError>> + 'a {
    let begin = (|| {
        if !has_client {
            return Err(ExchangeError::new("GRVT SDK client not initialized"));
        }
        account()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ExchangeError::new("Trading account ID required to set leverage"))?;
        let market = market(store, symbol)?;
        let value = match &market {
            MarketHit::Missing => {
                return Err(ExchangeError::new(format!("Market {symbol} not found")));
            }
            MarketHit::Inherited => {
                return Err(type_error(
                    "Cannot read properties of undefined (reading 'leverage')",
                ));
            }
            MarketHit::Stored(market) => math_min(
                math_max(input, market.limits.leverage.min),
                market.limits.leverage.max,
            ),
        };
        log(
            &format!(
                "Setting leverage for {symbol} to {}",
                js_number_string(value)
            ),
            LogSeverity::Info,
        )?;
        let mut payload = serde_json::Map::new();
        if let Some(account) = account() {
            payload.insert("sub_account_id".into(), json!(account));
        }
        payload.insert(
            "instrument".into(),
            json!(market.id().expect("stored market")),
        );
        payload.insert("leverage".into(), json!(js_number_string(value)));
        let payload = Value::Object(payload);
        Ok((value, payload, authenticate()))
    })();
    let begin = match begin {
        Ok(request) => Ok(request),
        Err(cause) => {
            match log(
                &format!("Error setting leverage for {symbol}: {}", cause.message),
                LogSeverity::Error,
            ) {
                Ok(()) => Err(cause),
                Err(replacement) => Err(replacement),
            }
        }
    };
    async move {
        let (value, payload, endpoint) = begin?;
        let result = async {
            let response = set(payload, endpoint.await?).await?;
            if response.is_null() {
                return Err(type_error(
                    "Cannot read properties of null (reading 'success')",
                ));
            }
            if !js_truthy(or(response.get("success"), response.get("s"))) {
                let message = or(
                    response.get("error").and_then(|value| value.get("message")),
                    response.get("e").and_then(|value| value.get("m")),
                );
                return Err(ExchangeError::new(if js_truthy(message) {
                    text(message)?
                } else {
                    "Failed to set leverage".into()
                }));
            }
            log(
                &format!(
                    "Leverage set successfully for {symbol}: {}",
                    js_number_string(value)
                ),
                LogSeverity::Info,
            )?;
            cache(store, symbol, &leverage(value))?;
            let position = store
                .positions()?
                .iter()
                .find(|position| position.symbol == symbol)
                .cloned();
            if let Some(position) = position {
                let mut patch = ModelPatch::default();
                patch.set_number("leverage", value);
                store.update_position(&position.id, &patch)?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(cause) => {
                log(
                    &format!("Error setting leverage for {symbol}: {}", cause.message),
                    LogSeverity::Error,
                )?;
                Err(cause)
            }
        }
    }
}

#[cfg(test)]
mod accepted_contract_tests {
    #[test]
    fn accepted_data_read_error_factory() {
        for message in ["", "protocol denied", "café\n\0💠"] {
            let actual = super::type_error(message);
            assert_eq!(actual.name, "TypeError", "native diagnostic name");
            assert_eq!(actual.message, message, "native diagnostic text");
            assert_eq!(
                actual.metadata.as_ref(),
                &safe_cex_exchange::runtime::ExchangeErrorMetadata::default(),
                "fresh native metadata"
            );
        }
    }
}
