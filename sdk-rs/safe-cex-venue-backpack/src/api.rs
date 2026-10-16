//! Pure helpers from backpack.api.ts. HTTP factory composition remains separate.
use crate::constants::INSTRUCTIONS;
use safe_cex_num::{JsJsonValue, js_own_keys};

// ts: resolveBackpackAuthWindow
pub fn resolve_backpack_auth_window(recv_window: Option<f64>) -> f64 {
    match recv_window {
        None => 60_000.0,
        Some(value) if !value.is_finite() || value <= 0.0 => 60_000.0,
        Some(value) => value.floor().min(60_000.0),
    }
}

// ts: isRecord
pub fn is_record(value: &JsJsonValue) -> bool {
    matches!(value, JsJsonValue::Object(_))
}

// ts: collectAuthParams
pub fn collect_auth_params(
    query_params: &JsJsonValue,
    request_data: &JsJsonValue,
) -> Result<JsJsonValue, &'static str> {
    let query = match query_params {
        JsJsonValue::Object(entries) => entries.as_slice(),
        _ => &[],
    };
    if let JsJsonValue::Array(rows) = request_data {
        if !query.is_empty() {
            return Err("Backpack batch request signing does not support query parameters");
        }
        if !rows.iter().all(is_record) {
            return Err("Backpack batch request body must contain only object rows");
        }
        // Owned snapshot boundary, witnessed alongside the source's array alias.
        return Ok(request_data.clone());
    }

    let body = match request_data {
        JsJsonValue::Object(entries) => entries.as_slice(),
        _ => &[],
    };
    let mut entries: Vec<(String, JsJsonValue)> = Vec::new();
    // Object spread: later values replace, while the first insertion owns order.
    for (key, value) in query.iter().chain(body) {
        if let Some((_, existing)) = entries.iter_mut().find(|(existing, _)| existing == key) {
            *existing = value.clone();
        } else {
            entries.push((key.clone(), value.clone()));
        }
    }
    let ordered = js_own_keys(entries.iter().map(|(key, _)| key.as_str()))
        .into_iter()
        .map(|key| {
            entries
                .iter()
                .find(|(candidate, _)| candidate == key)
                .unwrap()
                .clone()
        })
        .collect();
    Ok(JsJsonValue::Object(ordered))
}

// ts: isPublicEndpoint
pub fn is_public_endpoint(url: &str) -> bool {
    [
        "/api/v1/time",
        "/api/v1/status",
        "/api/v1/assets",
        "/api/v1/markets",
        "/api/v1/market",
        "/api/v1/ticker",
        "/api/v1/tickers",
        "/api/v1/depth",
        "/api/v1/klines",
        "/api/v1/trades",
        "/api/v1/trades/history",
        "/api/v1/markPrices",
        "/api/v1/openInterest",
        "/api/v1/fundingRates",
    ]
    .iter()
    .any(|endpoint| url.contains(endpoint))
}

fn instruction(key: &str) -> &'static str {
    INSTRUCTIONS
        .iter()
        .find(|(name, _)| *name == key)
        .expect("source instruction exists")
        .1
}

// ts: getInstruction
pub fn get_instruction(method: &str, url: &str) -> &'static str {
    if url.contains("/api/v1/order") && !url.contains("/api/v1/orders") {
        if method == "GET" {
            return instruction("ORDER_QUERY");
        }
        if method == "POST" {
            return instruction("ORDER_EXECUTE");
        }
        if method == "DELETE" {
            return instruction("ORDER_CANCEL");
        }
    }
    if url.contains("/api/v1/orders") {
        if method == "GET" {
            return instruction("ORDER_QUERY_ALL");
        }
        if method == "POST" {
            return instruction("ORDER_EXECUTE");
        }
        if method == "DELETE" {
            return instruction("ORDER_CANCEL_ALL");
        }
    }
    if url.contains("/api/v1/capital/collateral") {
        return instruction("COLLATERAL_QUERY");
    }
    if url.contains("/api/v1/capital") {
        return instruction("BALANCE_QUERY");
    }
    if url.contains("/api/v1/position") {
        return instruction("POSITION_QUERY");
    }
    if url.contains("/api/v1/account") {
        if method == "GET" {
            return instruction("ACCOUNT_QUERY");
        }
        if method == "PATCH" {
            return instruction("ACCOUNT_UPDATE");
        }
    }
    if url.contains("/history/fills") {
        return instruction("FILL_HISTORY_QUERY_ALL");
    }
    if url.contains("/history/orders") {
        return instruction("ORDER_HISTORY_QUERY_ALL");
    }
    if url.contains("/wapi/v1/capital/deposits") {
        return instruction("DEPOSIT_QUERY_ALL");
    }
    if url.contains("/wapi/v1/capital/withdrawals") {
        return instruction("WITHDRAWAL_QUERY_ALL");
    }
    if url.contains("/wapi/v1/capital/deposit/address") {
        return instruction("DEPOSIT_ADDRESS_QUERY");
    }
    "query"
}
