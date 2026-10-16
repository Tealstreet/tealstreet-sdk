//! Public-frame parsing from lighter.ws-public-parsers.ts.
use safe_cex_num::parse_float;
use serde_json::Value;

// ts: shouldDispatchLighterPublicMessage
pub fn should_dispatch_lighter_public_message(topic: &str, data: &str) -> bool {
    let has_matching_channel = [
        topic.to_owned(),
        topic.replace(':', "/"),
        topic.replace('/', ":"),
    ]
    .iter()
    .any(|channel| data.contains(&format!("\"channel\":\"{channel}\"")));
    let is_ticker_per_market_channel = topic == "ticker"
        && (data.contains("\"channel\":\"ticker:") || data.contains("\"channel\":\"ticker/"));
    has_matching_channel
        || is_ticker_per_market_channel
        || (topic == "pong" && data.contains("pong"))
}

// ts: parseLighterWsNumber
pub fn parse_lighter_ws_number(values: &[Value]) -> Option<f64> {
    for value in values {
        if let Some(number) = value.as_f64().filter(|n| n.is_finite()) {
            return Some(number);
        }
        if let Some(raw) = value.as_str().filter(|s| !s.is_empty()) {
            let parsed = parse_float(raw);
            if parsed.is_finite() {
                return Some(parsed);
            }
        }
    }
    None
}

// ts: parseLighterWsBookSidePrice
pub fn parse_lighter_ws_book_side_price(value: &Value) -> Option<f64> {
    if value.is_null()
        || value == &Value::Bool(false)
        || value.as_f64() == Some(0.0)
        || value.as_str() == Some("")
    {
        return None;
    }
    if let Some(values) = value.as_array()
        && !values.is_empty()
    {
        return parse_lighter_ws_number(&values[..1]);
    }
    let mut values = vec![value.clone()];
    for key in ["price", "p", "bestPrice", "best_price"] {
        values.push(value.get(key).cloned().unwrap_or(Value::Null));
    }
    parse_lighter_ws_number(&values)
}

// ts: extractLighterTickerMarketIdFromChannel
pub fn extract_lighter_ticker_market_id_from_channel(channel: &str) -> Option<&str> {
    for (start, _) in channel.match_indices("ticker") {
        let suffix = &channel[start + "ticker".len()..];
        if suffix.starts_with([':', '/']) {
            let suffix = &suffix[1..];
            let len = suffix.bytes().take_while(u8::is_ascii_digit).count();
            if len > 0 {
                return Some(&suffix[..len]);
            }
        }
    }
    None
}
