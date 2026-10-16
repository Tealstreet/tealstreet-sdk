use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::rc::Rc;

fn topic_frame(operation: &str, topic: &str, timestamp: f64) -> String {
    let payload = JsJsonValue::Object(vec![
        ("id".into(), JsJsonValue::Number(timestamp)),
        ("type".into(), JsJsonValue::String(operation.into())),
        ("topic".into(), JsJsonValue::String(topic.into())),
        ("privateChannel".into(), JsJsonValue::Bool(false)),
        ("response".into(), JsJsonValue::Bool(true)),
    ]);
    js_json_stringify(&payload)
        .expect("constructed topic frame is serializable")
        .expect("constructed topic frame is an object")
}

pub fn subscribe_spot_topic(
    socket: &Rc<BaseWebSocket>,
    topic: &str,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if !socket.is_connected() || socket.is_disposed() {
        return Ok(());
    }
    socket.send(&topic_frame("subscribe", topic, date_now()))
}

pub fn unsubscribe_spot_topic(
    socket: &Rc<BaseWebSocket>,
    topic: &str,
    date_now: impl FnOnce() -> f64,
) -> Result<(), ExchangeError> {
    if !socket.is_connected() || socket.is_disposed() {
        return Ok(());
    }
    socket.send(&topic_frame("unsubscribe", topic, date_now()))
}
