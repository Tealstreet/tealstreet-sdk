//! Helpers from the Lighter public websocket implementation.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};

// ts: normalizeLighterFundingRate
pub fn normalize_lighter_funding_rate(funding_rate_percent_per_hour: f64) -> f64 {
    let eight_hour_percent = if funding_rate_percent_per_hour == 0.0012 {
        0.01
    } else {
        funding_rate_percent_per_hour * 8.0
    };
    eight_hour_percent / 100.0
}

fn optional_path(payload: &Js, path: &[&str]) -> Result<Js, Thrown> {
    let mut value = payload.clone();
    for key in path {
        value = if value.is_nullish() {
            Js::Undefined
        } else {
            value.get(key)?
        };
    }
    Ok(value)
}

// ts: LighterPublicWebsocket.parseRestOrderBookLevels
pub fn parse_rest_order_book_levels(payload: Option<&Js>) -> Result<Js, Thrown> {
    let payload = payload.unwrap_or(&Js::Undefined);
    let mut candidate = Js::Undefined;
    for path in [
        &["order_book"][..],
        &["data", "order_book"],
        &["result", "order_book"],
        &["orderbook"],
        &["data", "orderbook"],
        &["result", "orderbook"],
        &["data"],
        &["result"],
        &[],
    ] {
        candidate = optional_path(payload, path)?;
        if !candidate.is_nullish() {
            break;
        }
    }
    if !candidate.truthy() || candidate.type_of() != "object" {
        return Ok(Js::Undefined);
    }
    let bids = if candidate.get("bids")?.is_array() {
        candidate.get("bids")?
    } else {
        Js::array(vec![])
    };
    let asks = if candidate.get("asks")?.is_array() {
        candidate.get("asks")?
    } else {
        Js::array(vec![])
    };
    if !bids.get("length")?.truthy() && !asks.get("length")?.truthy() {
        Ok(Js::Undefined)
    } else {
        Ok(Js::object(vec![("bids", bids), ("asks", asks)]))
    }
}

// ts: LighterPublicWebsocket.parseRestTickerItems
pub fn parse_rest_ticker_items(payload: Option<&Js>) -> Result<Js, Thrown> {
    let payload = payload.unwrap_or(&Js::Undefined);
    let candidates = [
        &["order_book_stats"][..],
        &["data", "order_book_stats"],
        &["result", "order_book_stats"],
        &["tickers"],
        &["data", "tickers"],
        &["result", "tickers"],
        &["market_stats"],
        &["data", "market_stats"],
        &["result", "market_stats"],
        &["data"],
        &["result"],
        &[],
    ]
    .into_iter()
    .map(|path| optional_path(payload, path))
    .collect::<Result<Vec<_>, _>>()?;
    for candidate in candidates {
        if candidate.is_array() {
            if candidate.get("length")?.to_number()? > 0. {
                return Ok(candidate);
            }
        } else if candidate.truthy() && candidate.type_of() == "object" {
            let values = candidate
                .own_entries()?
                .into_iter()
                .map(|(_, value)| value)
                .collect::<Vec<_>>();
            if !values.is_empty() {
                return Ok(Js::array(values));
            }
        }
    }
    Ok(Js::array(vec![]))
}

// ts: LighterPublicWebsocket.shouldEnforcePongMissReconnect
pub fn should_enforce_pong_miss_reconnect() -> bool {
    false
}

// ts: LighterPublicWebsocket.getWatchdogStaleThresholdMs
pub fn get_watchdog_stale_threshold_ms() -> f64 {
    120_000.0
}

// ts: LighterPublicWebsocket.isTickerRestFallbackCoolingDown
pub fn is_ticker_rest_fallback_cooling_down(now_ms: f64, cooldown_until: f64) -> bool {
    now_ms < cooldown_until
}

// ts: LighterPublicWebsocket.hasHydratedTickerPrice
pub fn has_hydrated_ticker_price(tickers: &[safe_cex_types::Ticker]) -> bool {
    tickers.iter().any(|ticker| {
        [
            ticker.mark,
            ticker.last,
            ticker.index.unwrap_or(0.0),
            ticker.bid,
            ticker.ask,
        ]
        .into_iter()
        .any(|price| price.is_finite() && price > 0.0)
    })
}
