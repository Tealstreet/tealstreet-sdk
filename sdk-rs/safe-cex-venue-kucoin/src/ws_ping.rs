//! KuCoin heartbeat entrypoints composed with shared socket state and sending.
use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::rc::Rc;

fn ping_frame(timestamp: f64) -> String {
    let payload = JsJsonValue::Object(vec![
        ("id".into(), JsJsonValue::Number(timestamp)),
        ("type".into(), JsJsonValue::String("ping".into())),
    ]);
    js_json_stringify(&payload)
        .expect("constructed heartbeat is serializable")
        .expect("constructed heartbeat is an object")
}

// ts: KucoinPublicWebsocket.ping
pub fn ping_futures_public(
    socket: &Rc<BaseWebSocket>,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    let frame = ping_frame(date_now());
    socket.send(&frame)
}

// ts: KucoinPrivateWebsocket.ping
pub fn ping_futures_private(
    socket: &Rc<BaseWebSocket>,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    let frame = ping_frame(date_now());
    socket.send_with_log(&frame)
}

// ts: KucoinSpotPublicWebsocket.ping
pub fn ping_spot_public(
    socket: &Rc<BaseWebSocket>,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    let frame = ping_frame(date_now());
    socket.send(&frame)
}

// ts: KucoinSpotPrivateWebsocket.ping
pub fn ping_spot_private(
    socket: &Rc<BaseWebSocket>,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    let frame = ping_frame(date_now());
    socket.send_with_log(&frame)
}
