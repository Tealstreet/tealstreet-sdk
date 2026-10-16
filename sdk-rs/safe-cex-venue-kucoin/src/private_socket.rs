use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::cell::Cell;

fn subscribe_frame(topic: &str, timestamp: f64) -> String {
    let payload = JsJsonValue::Object(vec![
        ("id".into(), JsJsonValue::Number(timestamp)),
        ("type".into(), JsJsonValue::String("subscribe".into())),
        ("topic".into(), JsJsonValue::String(topic.into())),
        ("privateChannel".into(), JsJsonValue::Bool(true)),
        ("response".into(), JsJsonValue::Bool(true)),
    ]);
    js_json_stringify(&payload)
        .expect("constructed subscription frame is serializable")
        .expect("constructed subscription frame is an object")
}

/// Sender reports synchronous throws; it owns future/background effects.
pub fn subscribe_futures_private<E>(
    subscribed: &Cell<bool>,
    mut date_now: impl FnMut() -> f64,
    mut send_with_log: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    if subscribed.get() {
        return Ok(());
    }
    subscribed.set(true);
    for topic in [
        "/contractMarket/tradeOrders",
        "/contractMarket/advancedOrders",
        "/contract/positionAll",
        "/contractAccount/wallet",
        "/contract/marginMode",
        "/contract/crossLeverage",
    ] {
        send_with_log(&subscribe_frame(topic, date_now()))?;
    }
    Ok(())
}

/// Sender reports synchronous throws; it owns future/background effects.
pub fn subscribe_spot_private<E>(
    mut date_now: impl FnMut() -> f64,
    mut send_with_log: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    for topic in ["/spotMarket/tradeOrdersV2", "/account/balance"] {
        send_with_log(&subscribe_frame(topic, date_now()))?;
    }
    Ok(())
}

/// Snapshot initiation is synchronous; its completion remains host-owned.
pub fn on_open_futures_private<E>(
    disposed: bool,
    subscribed: &Cell<bool>,
    subscribe: impl FnOnce() -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
    reconnecting_enabled: bool,
    initiate_snapshot: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if disposed {
        return Ok(());
    }
    subscribed.set(false);
    subscribe()?;
    ping()?;
    if reconnecting_enabled {
        initiate_snapshot()?;
    }
    Ok(())
}

/// Snapshot initiation is synchronous; its completion remains host-owned.
pub fn on_open_spot_private<E>(
    disposed: bool,
    subscribe: impl FnOnce() -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
    reconnecting_enabled: bool,
    initiate_snapshot: impl FnOnce(u32) -> Result<(), E>,
) -> Result<(), E> {
    if disposed {
        return Ok(());
    }
    subscribe()?;
    ping()?;
    if reconnecting_enabled {
        initiate_snapshot(10)?;
    }
    Ok(())
}
