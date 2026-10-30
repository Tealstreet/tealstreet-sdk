//! The four market-data routes and `/health`, after NX
//! `candle-server/src/server.ts` (same parameters, bodies, status codes and the
//! unknown-symbol / empty-range classification).

use crate::calendar::{self, normalize_epoch_to_seconds};
use crate::json::{Json, Object};
use crate::routes::{Query, js_number};
use crate::{App, Reply, cache, pool};
use safe_cex_engine::model::Market;
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    cmp::Ordering,
    rc::Rc,
};

const MAX_LIMIT: f64 = 5000.;
const MAX_MARKET_LIMIT: f64 = 10_000.;
const MAX_SYMBOL_SAMPLE: usize = 50;

fn error(status: u16, message: impl Into<String>, code: &str) -> Reply {
    Reply::json(
        status,
        Object::new()
            .field("error", Json::str(message))
            .field("code", Json::str(code))
            .build(),
    )
}

fn bad_request(message: impl Into<String>) -> Reply {
    error(400, message, "bad-request")
}

fn unavailable() -> Reply {
    error(
        503,
        "Candle service temporarily unavailable",
        "service-unavailable",
    )
}

fn metadata_unavailable() -> Reply {
    error(
        503,
        "Market metadata temporarily unavailable",
        "service-unavailable",
    )
}

/// Alerts go to stderr; the deployment's log shipping is the record.
fn alert(key: &str, message: &str, detail: &[(&str, &str)]) {
    let detail: Vec<String> = detail.iter().map(|(k, v)| format!("{k}={v}")).collect();
    eprintln!("[candle-server] {message} {} key={key}", detail.join(" "));
}

fn js_upper(text: &str) -> String {
    text.to_uppercase()
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn parse_exchange(query: &Query) -> Result<String, String> {
    match query.get("exchange") {
        Some(exchange) if !exchange.is_empty() && safe_cex_venues::supported(exchange) => {
            Ok(exchange.to_owned())
        }
        exchange => Err(format!("Invalid exchange {}", exchange.unwrap_or(""))
            .trim()
            .to_owned()),
    }
}

/// `parseNumber`: an absent or empty value is missing.
fn parse_number(value: Option<&str>, name: &str, required: bool) -> Result<f64, String> {
    match value.filter(|value| !value.is_empty()) {
        None if required => Err(format!("Missing {name}")),
        None => Ok(0.),
        Some(value) => {
            let parsed = js_number(value);
            if parsed.is_finite() {
                Ok(parsed)
            } else {
                Err(format!("Invalid {name}"))
            }
        }
    }
}

fn parse_timestamp(value: Option<&str>, name: &str) -> Result<f64, String> {
    let timestamp = parse_number(value, name, true)?;
    // Number.isSafeInteger, non-negative, and a valid Date.
    if timestamp.fract() != 0.
        || !(0. ..=9_007_199_254_740_991.).contains(&timestamp)
        || timestamp > 8.64e15
    {
        return Err(format!(
            "{name} must be a valid non-negative integer timestamp in milliseconds"
        ));
    }
    Ok(timestamp)
}

pub struct CandleRequest {
    pub exchange: String,
    pub symbol: String,
    pub interval: String,
    pub from_ms: f64,
    pub to_ms: f64,
    pub limit: f64,
}

pub fn parse_candle_request(query: &Query) -> Result<CandleRequest, String> {
    let exchange = parse_exchange(query)?;
    let symbol = match query.get("symbol") {
        Some(symbol) if !symbol.is_empty() => symbol.to_owned(),
        _ => return Err("Missing symbol".into()),
    };
    let interval = match query.get("interval") {
        Some(interval) if calendar::interval_seconds(interval).is_some() => interval.to_owned(),
        interval => {
            return Err(format!("Invalid interval {}", interval.unwrap_or(""))
                .trim()
                .to_owned());
        }
    };
    let from_ms = parse_timestamp(query.get("from"), "from")?;
    let to_ms = parse_timestamp(query.get("to"), "to")?;
    let requested = parse_number(query.get("limit"), "limit", false)?;
    let requested = if requested == 0. {
        MAX_LIMIT
    } else {
        requested
    };
    let limit = requested.floor().clamp(1., MAX_LIMIT);
    if from_ms > to_ms {
        return Err("from must be less than or equal to to".into());
    }
    let range = calendar::chart_candles_in_range(&interval, from_ms, to_ms);
    if range.fract() != 0. || !(1. ..=MAX_LIMIT).contains(&range) {
        return Err(format!(
            "Requested range must contain between 1 and {MAX_LIMIT} candles"
        ));
    }
    Ok(CandleRequest {
        exchange,
        symbol,
        interval,
        from_ms,
        to_ms,
        limit,
    })
}

fn finite_numbers(fields: &[(&'static str, Option<f64>)]) -> Option<Json> {
    let mut object = Object::new();
    let mut any = false;
    for (key, value) in fields {
        if let Some(value) = value.filter(|value| value.is_finite()) {
            object = object.field(key, Json::Num(value));
            any = true;
        }
    }
    any.then(|| object.build())
}

fn nested(fields: Vec<(&'static str, Option<Json>)>) -> Option<Json> {
    let mut object = Object::new();
    let mut any = false;
    for (key, value) in fields {
        if let Some(value) = value {
            object = object.field(key, value);
            any = true;
        }
    }
    any.then(|| object.build())
}

/// `projectMarket`: no raw venue info, only finite nested numbers.
pub fn project_market(market: &Market) -> Json {
    let limits = &market.limits;
    Object::new()
        .field("id", Json::str(&market.id))
        .field("symbol", Json::str(&market.symbol))
        .maybe(
            "prettySymbol",
            Json::opt_str(market.pretty_symbol.as_deref()),
        )
        .field("base", Json::str(&market.base))
        .field("quote", Json::str(&market.quote))
        .field("active", Json::Bool(market.active))
        .maybe("contractSize", market.contract_size.map(Json::Num))
        .maybe("isExpiring", market.is_expiring.map(Json::Bool))
        .maybe("isInverse", market.is_inverse.map(Json::Bool))
        .maybe("isSpot", market.is_spot.map(Json::Bool))
        .maybe("isReadOnly", market.is_read_only.map(Json::Bool))
        .maybe("takerFee", market.taker_fee.map(Json::Num))
        .maybe("makerFee", market.maker_fee.map(Json::Num))
        .maybe(
            "precision",
            finite_numbers(&[
                ("amount", Some(market.precision.amount)),
                ("price", Some(market.precision.price)),
            ]),
        )
        .maybe(
            "limits",
            nested(vec![
                (
                    "amount",
                    finite_numbers(&[
                        ("min", Some(limits.amount.min)),
                        ("max", Some(limits.amount.max)),
                    ]),
                ),
                (
                    "leverage",
                    finite_numbers(&[
                        ("min", Some(limits.leverage.min)),
                        ("max", Some(limits.leverage.max)),
                    ]),
                ),
                (
                    "price",
                    limits
                        .price
                        .and_then(|price| finite_numbers(&[("min", Some(price.min))])),
                ),
            ]),
        )
        .build()
}

/// An approximation of `String.prototype.localeCompare` (ICU root collation)
/// for market symbols: punctuation before digits before letters, letters
/// case-insensitive first, then lowercase before uppercase.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    fn primary(c: char) -> (u8, u32) {
        if c.is_alphabetic() {
            (2, c.to_lowercase().next().unwrap_or(c) as u32)
        } else if c.is_ascii_digit() {
            (1, c as u32)
        } else {
            // ICU root order for punctuation and symbols, then by code point.
            const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
            (
                0,
                PUNCTUATION
                    .find(c)
                    .map_or(0x1_0000 + c as u32, |at| at as u32),
            )
        }
    }
    let primary_order = a.chars().map(primary).cmp(b.chars().map(primary));
    primary_order
        .then_with(|| {
            a.chars()
                .map(|c| c.is_uppercase())
                .cmp(b.chars().map(|c| c.is_uppercase()))
        })
        .then_with(|| a.cmp(b))
}

pub async fn symbols(app: &App, query: &Query) -> Reply {
    let exchange = match parse_exchange(query) {
        Ok(exchange) => exchange,
        Err(message) => return bad_request(message),
    };
    let needle = js_upper(query.get("query").unwrap_or(""));
    let resident = match app.pool.hold(&exchange).await {
        Ok(resident) => resident,
        Err(reason) => {
            alert(
                &format!("symbols-error:{exchange}"),
                "symbol lookup failed",
                &[("exchange", &exchange), ("reason", &reason)],
            );
            return unavailable();
        }
    };
    let markets = resident.markets();
    let mut matches: Vec<(u8, &Market)> = markets
        .iter()
        .filter(|market| {
            needle.is_empty()
                || js_upper(&market.symbol).contains(&needle)
                || js_upper(&market.base) == needle
        })
        .map(|market| (u8::from(js_upper(&market.base) != needle), market))
        .collect();
    matches.sort_by_key(|(rank, _)| *rank);
    let sample = matches
        .into_iter()
        .take(MAX_SYMBOL_SAMPLE)
        .map(|(_, market)| {
            Object::new()
                .field("symbol", Json::str(&market.symbol))
                .field("base", Json::str(&market.base))
                .field("active", Json::Bool(market.active))
                .build()
        })
        .collect();
    Reply::json(
        200,
        Object::new()
            .field("exchange", Json::str(&exchange))
            .field("marketsLoaded", Json::Bool(resident.markets_loaded()))
            .field("marketCount", Json::Num(markets.len() as f64))
            .field("markets", Json::Arr(sample))
            .build(),
    )
}

fn market_matches(market: &Market, query: Option<&str>) -> bool {
    let Some(query) = query else { return true };
    [
        Some(market.id.as_str()),
        Some(market.symbol.as_str()),
        market.pretty_symbol.as_deref(),
        Some(market.base.as_str()),
        Some(market.quote.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|value| js_upper(value).contains(query))
}

fn market_rank(market: &Market, query: Option<&str>) -> u8 {
    let Some(query) = query else { return 1 };
    if js_upper(&market.symbol) == query || js_upper(&market.id) == query {
        0
    } else if js_upper(&market.base) == query {
        1
    } else if market.pretty_symbol.as_deref().map(js_upper).as_deref() == Some(query) {
        2
    } else {
        3
    }
}

pub async fn markets(app: &App, query: &Query) -> Reply {
    let parsed = (|| {
        let exchange = parse_exchange(query)?;
        let needle = match query.get("query") {
            None => None,
            Some(raw) => {
                let needle = js_upper(raw.trim());
                if needle.is_empty() {
                    None
                } else if utf16_len(&needle) > 64 {
                    return Err("query must be 64 characters or fewer".to_owned());
                } else {
                    Some(needle)
                }
            }
        };
        let limit = match query.get("limit").filter(|value| !value.is_empty()) {
            None => 100.,
            Some(value) => {
                let parsed = js_number(value);
                if !parsed.is_finite() {
                    return Err("Invalid limit".to_owned());
                }
                parsed.floor().clamp(1., MAX_MARKET_LIMIT)
            }
        };
        Ok((exchange, needle, limit))
    })();
    let (exchange, needle, limit) = match parsed {
        Ok(parsed) => parsed,
        Err(message) => return bad_request(message),
    };
    let resident = match app.pool.hold(&exchange).await {
        Ok(resident) => resident,
        Err(reason) => {
            alert(
                &format!("markets-error:{exchange}"),
                "market metadata lookup failed",
                &[("exchange", &exchange), ("reason", &reason)],
            );
            return unavailable();
        }
    };
    if !resident.markets_loaded() {
        return metadata_unavailable();
    }
    let markets = resident.markets();
    let needle = needle.as_deref();
    let mut matches: Vec<(u8, &Market)> = markets
        .iter()
        .filter(|market| market_matches(market, needle))
        .map(|market| (market_rank(market, needle), market))
        .collect();
    matches.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| locale_compare(&a.1.symbol, &b.1.symbol))
    });
    let projected: Vec<Json> = matches
        .iter()
        .take(limit as usize)
        .map(|(_, market)| project_market(market))
        .collect();
    Reply::json(
        200,
        Object::new()
            .field("exchange", Json::str(&exchange))
            .field("marketsLoaded", Json::Bool(true))
            .field("marketCount", Json::Num(markets.len() as f64))
            .field("returned", Json::Num(projected.len() as f64))
            .field("matching", Json::Num(matches.len() as f64))
            .field("limit", Json::Num(limit))
            .field("query", needle.map_or(Json::Null, Json::str))
            .field("refreshedAt", Json::Num(pool::now_ms().floor()))
            .field("markets", Json::Arr(projected))
            .build(),
    )
}

pub fn find_market<'a>(markets: &'a [Market], symbol: &str) -> Option<&'a Market> {
    let needle = js_upper(symbol.trim());
    if needle.is_empty() {
        return None;
    }
    markets.iter().find(|market| {
        js_upper(&market.symbol) == needle
            || js_upper(&market.id) == needle
            || market.pretty_symbol.as_deref().map(js_upper).as_deref() == Some(needle.as_str())
    })
}

pub async fn market(app: &App, query: &Query) -> Reply {
    let exchange = match parse_exchange(query) {
        Ok(exchange) => exchange,
        Err(message) => return bad_request(message),
    };
    let symbol = query.get("symbol").map(str::trim).unwrap_or("");
    if symbol.is_empty() || utf16_len(symbol) > 96 {
        return bad_request("symbol is required and must be 96 characters or fewer");
    }
    let resident = match app.pool.hold(&exchange).await {
        Ok(resident) => resident,
        Err(reason) => {
            alert(
                &format!("market-error:{exchange}"),
                "single market metadata lookup failed",
                &[
                    ("exchange", &exchange),
                    ("symbol", symbol),
                    ("reason", &reason),
                ],
            );
            return unavailable();
        }
    };
    if !resident.markets_loaded() {
        return metadata_unavailable();
    }
    let markets = resident.markets();
    let Some(market) = find_market(&markets, symbol) else {
        return error(
            404,
            format!("Unknown symbol {symbol} on {exchange}"),
            "market-not-found",
        );
    };
    Reply::json(
        200,
        Object::new()
            .field("exchange", Json::str(&exchange))
            .field("symbol", Json::str(symbol))
            .field("marketsLoaded", Json::Bool(true))
            .field("marketCount", Json::Num(markets.len() as f64))
            .field("refreshedAt", Json::Num(pool::now_ms().floor()))
            .field("market", project_market(market))
            .build(),
    )
}

/// A cache candle; persisted non-finite numbers arrive as `null`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedCandle {
    timestamp: Option<f64>,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    volume: Option<f64>,
    #[serde(default)]
    last_side: Option<String>,
}

/// `cleanCandles`: first candle per timestamp, then ascending (stable).
fn clean_candles(mut candles: Vec<CachedCandle>) -> Vec<CachedCandle> {
    let mut seen = std::collections::HashSet::new();
    candles.retain(|candle| seen.insert(candle.timestamp.map(f64::to_bits)));
    candles.sort_by(|a, b| {
        let (a, b) = (
            a.timestamp.unwrap_or(f64::NAN),
            b.timestamp.unwrap_or(f64::NAN),
        );
        a.partial_cmp(&b).unwrap_or(Ordering::Equal)
    });
    candles
}

fn render_candle(candle: &CachedCandle) -> Json {
    let num = |value: Option<f64>| Json::Num(value.unwrap_or(f64::NAN));
    Object::new()
        .field("timestamp", num(candle.timestamp))
        .field("open", num(candle.open))
        .field("high", num(candle.high))
        .field("low", num(candle.low))
        .field("close", num(candle.close))
        .field("volume", num(candle.volume))
        .maybe("lastSide", Json::opt_str(candle.last_side.as_deref()))
        .build()
}

enum Fetched {
    Candles(Vec<CachedCandle>),
    Upstream(String),
    Service(String),
}

async fn fetch_once(
    app: &App,
    request: &CandleRequest,
    fetcher: cache::Fetcher,
    upstream: &RefCell<Option<String>>,
) -> Fetched {
    let result = app
        .cache
        .fetch(
            cache::FetchRequest {
                cache_key: calendar::cache_key(
                    &request.exchange,
                    &request.symbol,
                    &request.interval,
                ),
                interval: request.interval.clone(),
                from_ms: request.from_ms,
                to_ms: request.to_ms,
                limit: request.limit,
                now_ms: pool::now_ms().floor(),
            },
            fetcher,
        )
        .await;
    match result {
        Ok(candles) => match serde_json::from_value::<Vec<CachedCandle>>(candles) {
            Ok(candles) => Fetched::Candles(candles),
            Err(error) => Fetched::Service(format!("cache returned malformed candles: {error}")),
        },
        Err(message) if upstream.borrow().as_deref() == Some(message.as_str()) => {
            Fetched::Upstream(message)
        }
        Err(message) => Fetched::Service(message),
    }
}

pub async fn candles(app: &App, query: &Query) -> Reply {
    let request = match parse_candle_request(query) {
        Ok(request) => request,
        Err(message) => return bad_request(message),
    };
    let exchange = request.exchange.clone();
    let service_error = |reason: &str| {
        alert(
            &format!("service-error:{exchange}"),
            "candle request failed inside the service",
            &[
                ("exchange", &exchange),
                ("symbol", &request.symbol),
                ("reason", reason),
            ],
        );
        unavailable()
    };
    let upstream_error = |reason: String| {
        alert(
            &format!("upstream-error:{exchange}"),
            "upstream candle fetch failed",
            &[
                ("exchange", &exchange),
                ("symbol", &request.symbol),
                ("reason", &reason),
            ],
        );
        error(502, reason, "upstream-error")
    };
    let resident = match app.pool.hold(&request.exchange).await {
        Ok(resident) => resident,
        Err(reason) => return service_error(&reason),
    };
    let engine = resident.engine();
    // Only a request that reached the venue may be judged by its error latch.
    let fetcher_ran = Rc::new(Cell::new(false));
    let upstream = Rc::new(RefCell::new(None::<String>));
    let fetcher: cache::Fetcher = {
        let (ran, upstream, symbol) = (
            fetcher_ran.clone(),
            upstream.clone(),
            request.symbol.clone(),
        );
        Rc::new(move |options| {
            ran.set(true);
            let (engine, upstream, symbol) = (engine.clone(), upstream.clone(), symbol.clone());
            Box::pin(async move {
                let result = engine.fetch_candles(&symbol, &options).await;
                if let Err(message) = &result {
                    *upstream.borrow_mut() = Some(message.clone());
                }
                result
            })
        })
    };
    let mut errors_before = resident.error_seq();
    let mut candles = match fetch_once(app, &request, fetcher.clone(), &upstream).await {
        Fetched::Candles(candles) => candles,
        Fetched::Upstream(reason) => return upstream_error(reason),
        Fetched::Service(reason) => return service_error(&reason),
    };
    // An unknown symbol is an error only when the fetch also came back empty:
    // many venues resolve symbols outside their fetched catalogue.
    if candles.is_empty() && resident.markets_loaded() && !resident.knows_symbol(&request.symbol) {
        let markets_current = resident.reload_markets().await;
        if !resident.knows_symbol(&request.symbol) {
            if markets_current {
                return error(
                    404,
                    format!("Unknown symbol {} on {}", request.symbol, request.exchange),
                    "market-not-found",
                );
            }
        } else {
            errors_before = resident.error_seq();
            candles = match fetch_once(app, &request, fetcher, &upstream).await {
                Fetched::Candles(candles) => candles,
                Fetched::Upstream(reason) => return upstream_error(reason),
                Fetched::Service(reason) => return service_error(&reason),
            };
        }
    }
    if candles.is_empty() && fetcher_ran.get() && resident.error_seq() != errors_before {
        return upstream_error(
            resident
                .last_error()
                .unwrap_or_else(|| "exchange reported an error".into()),
        );
    }
    let normalized = clean_candles(
        candles
            .into_iter()
            .map(|mut candle| {
                candle.timestamp = candle
                    .timestamp
                    .map(|timestamp| normalize_epoch_to_seconds(timestamp) * 1000.);
                candle
            })
            .collect(),
    );
    Reply::json(
        200,
        Object::new()
            .field("exchange", Json::str(&request.exchange))
            .field("symbol", Json::str(&request.symbol))
            .field("interval", Json::str(&request.interval))
            .field(
                "candles",
                Json::Arr(normalized.iter().map(render_candle).collect()),
            )
            .build(),
    )
}

pub async fn health(app: &App) -> Reply {
    let redis = app.redis.probe().await;
    let rss = crate::health::rss_mb();
    let body = Object::new()
        .field("ok", Json::Bool(true))
        .field("memoryMB", Json::Num(rss))
        .field("heapMB", Json::Num(rss))
        .field(
            "uptime",
            Json::Num(app.started.elapsed().as_secs_f64().round()),
        )
        .field("publicExchanges", Json::Num(app.pool.size() as f64))
        .field(
            "limits",
            Object::new()
                .field("maxResident", Json::Num(app.pool.max_resident as f64))
                .field("idleTtlMs", Json::Num(app.pool.idle_ttl_ms))
                .build(),
        )
        .field("redis", Json::Bool(redis.connected))
        .field("redisConfigured", Json::Bool(redis.configured))
        .field("redisReachable", Json::Bool(redis.reachable))
        .maybe("redisErrorKind", redis.kind.map(Json::str))
        .maybe("redisError", redis.error.map(Json::str))
        .build();
    Reply::json(200, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> Query {
        Query::parse(Some(text))
    }

    #[test]
    fn length_limits_count_ts_string_units() {
        // Golden lengths from node's `String.prototype.length`.
        for (text, units) in [("BTCUSDT", 7), ("é", 1), ("😀", 2), ("é😀A", 4)] {
            assert_eq!(utf16_len(text), units, "{text}");
        }
        assert_eq!(utf16_len(&"😀".repeat(32)), 64);
        assert_eq!(utf16_len(&"😀".repeat(33)), 66);
    }

    #[test]
    fn candle_requests_validate_like_the_ts_server() {
        let base = "exchange=bybit&symbol=BTCUSDT&interval=1m&from=1736899200000&to=1736899260000";
        let ok = parse_candle_request(&query(base)).unwrap();
        assert_eq!(
            (ok.limit, ok.from_ms, ok.to_ms),
            (5000., 1_736_899_200_000., 1_736_899_260_000.)
        );
        assert_eq!(
            parse_candle_request(&query(&format!("{base}&limit=-3")))
                .unwrap()
                .limit,
            1.
        );
        assert_eq!(
            parse_candle_request(&query(&format!("{base}&limit=9999")))
                .unwrap()
                .limit,
            5000.
        );
        let message = |text: &str| parse_candle_request(&query(text)).err().unwrap();
        assert_eq!(message("exchange=nope"), "Invalid exchange nope");
        assert_eq!(message(""), "Invalid exchange");
        assert_eq!(message("exchange=bybit"), "Missing symbol");
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=toString"),
            "Invalid interval toString"
        );
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=1m"),
            "Missing from"
        );
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=1m&from=abc"),
            "Invalid from"
        );
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=1m&from=1.5&to=2"),
            "from must be a valid non-negative integer timestamp in milliseconds"
        );
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=1m&from=5&to=2"),
            "from must be less than or equal to to"
        );
        assert_eq!(
            message("exchange=bybit&symbol=X&interval=1m&from=1736899200000&to=1737899200000"),
            "Requested range must contain between 1 and 5000 candles"
        );
        assert_eq!(message(&format!("{base}&limit=x")), "Invalid limit");
    }

    #[test]
    fn candles_dedupe_first_and_sort_ascending() {
        let candle = |timestamp: f64, open: f64| CachedCandle {
            timestamp: Some(timestamp),
            open: Some(open),
            high: None,
            low: None,
            close: None,
            volume: None,
            last_side: None,
        };
        let cleaned = clean_candles(vec![candle(3., 1.), candle(1., 2.), candle(3., 9.)]);
        let rendered: Vec<String> = cleaned.iter().map(|c| render_candle(c).render()).collect();
        assert_eq!(
            rendered,
            [
                r#"{"timestamp":1,"open":2,"high":null,"low":null,"close":null,"volume":null}"#,
                r#"{"timestamp":3,"open":1,"high":null,"low":null,"close":null,"volume":null}"#,
            ]
        );
    }

    #[test]
    fn symbols_sort_like_locale_compare() {
        let mut symbols = vec![
            "btcusdt",
            "BTCUSDT",
            "BTC_USDT",
            "1000PEPEUSDT",
            "ETHUSDT",
            "BTC-USDT",
        ];
        symbols.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(
            symbols,
            [
                "1000PEPEUSDT",
                "BTC_USDT",
                "BTC-USDT",
                "btcusdt",
                "BTCUSDT",
                "ETHUSDT"
            ]
        );
    }
}
