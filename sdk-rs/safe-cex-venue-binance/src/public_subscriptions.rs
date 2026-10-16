use safe_cex_num::{js_number_string, js_own_keys};
use serde_json::Value;

// ts: BinanceMarketWebsocket.subscribe
pub fn subscribe_binance_market<E>(
    keys: impl IntoIterator<Item = String>,
    count: impl FnMut(&str) -> Option<f64>,
    send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(keys, count, send)
}
// ts: BinancePublicWebsocket.subscribe
pub fn subscribe_binance_public<E>(
    keys: impl IntoIterator<Item = String>,
    count: impl FnMut(&str) -> Option<f64>,
    send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(keys, count, send)
}
// ts: BinancePMMarketWebsocket.subscribe
pub fn subscribe_binance_pm_market<E>(
    keys: impl IntoIterator<Item = String>,
    count: impl FnMut(&str) -> Option<f64>,
    send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(keys, count, send)
}
// ts: BinancePMPublicWebsocket.subscribe
pub fn subscribe_binance_pm_public<E>(
    keys: impl IntoIterator<Item = String>,
    count: impl FnMut(&str) -> Option<f64>,
    send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(keys, count, send)
}
// ts: BinancePublicWebsocket.subscribe (binancespot.ws-public.ts)
pub fn subscribe_binance_spot_public<E>(
    keys: impl IntoIterator<Item = String>,
    count: impl FnMut(&str) -> Option<f64>,
    send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    subscribe(keys, count, send)
}

fn subscribe<E>(
    keys: impl IntoIterator<Item = String>,
    mut count: impl FnMut(&str) -> Option<f64>,
    mut send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    let keys: Vec<String> = keys.into_iter().collect();
    let keys = js_own_keys(keys.iter().map(String::as_str));
    let mut id = 10.0;
    for topic in keys {
        let enabled = count(topic).is_some_and(|count| count.is_finite() && count > 0.0);
        if !enabled {
            continue;
        }
        let frame = format!(
            "{{\"method\":\"SUBSCRIBE\",\"params\":[{}],\"id\":{}}}",
            Value::String(topic.to_owned()),
            js_number_string(id)
        );
        id += 1.0;
        send(&frame)?;
    }
    Ok(())
}
