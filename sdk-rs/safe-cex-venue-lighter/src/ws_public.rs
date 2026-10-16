//! Pure helpers from the Lighter public websocket implementation.

// ts: normalizeLighterFundingRate
pub fn normalize_lighter_funding_rate(funding_rate_percent_per_hour: f64) -> f64 {
    let eight_hour_percent = if funding_rate_percent_per_hour == 0.0012 {
        0.01
    } else {
        funding_rate_percent_per_hour * 8.0
    };
    eight_hour_percent / 100.0
}

/// Borrowed fields of the source helper's inline response object.
#[derive(Debug, PartialEq)]
pub struct LighterRestOrderBookLevels<'a> {
    pub bids: &'a [serde_json::Value],
    pub asks: &'a [serde_json::Value],
}

// ts: LighterPublicWebsocket.parseRestOrderBookLevels
pub fn parse_rest_order_book_levels(
    payload: Option<&serde_json::Value>,
) -> Option<LighterRestOrderBookLevels<'_>> {
    use serde_json::Value;
    let payload = payload?;
    let data = payload.get("data");
    let result = payload.get("result");
    let candidate = [
        payload.get("order_book"),
        data.and_then(|v| v.get("order_book")),
        result.and_then(|v| v.get("order_book")),
        payload.get("orderbook"),
        data.and_then(|v| v.get("orderbook")),
        result.and_then(|v| v.get("orderbook")),
        data,
        result,
        Some(payload),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.is_null())?;
    let candidate = candidate.as_object()?;
    let bids = candidate
        .get("bids")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let asks = candidate
        .get("asks")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if bids.is_empty() && asks.is_empty() {
        None
    } else {
        Some(LighterRestOrderBookLevels { bids, asks })
    }
}

// ts: LighterPublicWebsocket.parseRestTickerItems
pub fn parse_rest_ticker_items(payload: Option<&serde_json::Value>) -> Vec<&serde_json::Value> {
    use serde_json::Value;
    let Some(payload) = payload else {
        return Vec::new();
    };
    let data = payload.get("data");
    let result = payload.get("result");
    let candidates = [
        payload.get("order_book_stats"),
        data.and_then(|v| v.get("order_book_stats")),
        result.and_then(|v| v.get("order_book_stats")),
        payload.get("tickers"),
        data.and_then(|v| v.get("tickers")),
        result.and_then(|v| v.get("tickers")),
        payload.get("market_stats"),
        data.and_then(|v| v.get("market_stats")),
        result.and_then(|v| v.get("market_stats")),
        data,
        result,
        Some(payload),
    ];
    for candidate in candidates.into_iter().flatten() {
        match candidate {
            Value::Array(values) if !values.is_empty() => return values.iter().collect(),
            Value::Object(values) if !values.is_empty() => {
                return safe_cex_num::js_own_keys(values.keys().map(String::as_str))
                    .into_iter()
                    .filter_map(|key| values.get(key))
                    .collect();
            }
            _ => {}
        }
    }
    Vec::new()
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
            ticker.index,
            ticker.bid,
            ticker.ask,
        ]
        .into_iter()
        .any(|price| price.is_finite() && price > 0.0)
    })
}
