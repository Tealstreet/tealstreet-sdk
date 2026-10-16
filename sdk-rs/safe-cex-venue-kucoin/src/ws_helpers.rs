//! Pure websocket wire-number helpers and the closed futures socket-role union.
use safe_cex_num::{js_number_string, js_string, parse_float, parse_int};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: KucoinPublicRole
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KucoinPublicRole {
    Detail,
    Snapshot,
}
// Private Serde companion; the public entry retains its admission checks.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "KucoinPublicRole")]
#[serde(rename_all = "lowercase")]
enum KucoinPublicRoleWire {
    Detail,
    Snapshot,
}

impl<'de> Deserialize<'de> for KucoinPublicRole {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        KucoinPublicRoleWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            text,
        ))
    }
}
// ts: parseNumber
pub fn parse_number(value: Option<&Value>) -> Result<Option<f64>, String> {
    let text = match value.filter(|v| !v.is_null()) {
        Some(v) => js_string(Some(v)).map_err(|e| e.to_string())?,
        None => String::new(),
    };
    let parsed = parse_float(&text);
    Ok(parsed.is_finite().then_some(parsed))
}
fn timestamp(value: Option<&Value>, now_ms: f64) -> Result<f64, String> {
    let text = match value.filter(|v| !v.is_null()) {
        Some(v) => js_string(Some(v)).map_err(|e| e.to_string())?,
        None => js_number_string(now_ms),
    };
    Ok(parse_int(&text, Some(10)))
}
// ts: normalizeTimestamp
pub fn normalize_timestamp(value: Option<&Value>, now_ms: f64) -> Result<f64, String> {
    let parsed = timestamp(value, now_ms)?;
    Ok(if parsed > 10_000_000_000.0 {
        (parsed / 1000.0).floor()
    } else {
        parsed
    })
}
// ts: normalizeMillis
pub fn normalize_millis(value: Option<&Value>, now_ms: f64) -> Result<f64, String> {
    let parsed = timestamp(value, now_ms)?;
    Ok(if parsed > 100_000_000_000_000_000.0 {
        (parsed / 1_000_000.0).floor()
    } else if parsed > 100_000_000_000_000.0 {
        (parsed / 1000.0).floor()
    } else if parsed < 10_000_000_000.0 {
        parsed * 1000.0
    } else {
        parsed
    })
}

// ts: KucoinPublicWebsocket.snapshotShardSymbols
pub fn snapshot_shard_symbols(
    store: &dyn safe_cex_store::Store,
    shard_index: f64,
    shard_count: f64,
) -> Vec<String> {
    store
        .markets()
        .iter()
        .map(|m| {
            if m.id.is_empty() {
                crate::sizing::to_api_symbol(Some(&m.symbol), store)
            } else {
                m.id.clone()
            }
        })
        .filter(|symbol| !symbol.is_empty())
        .enumerate()
        .filter_map(|(index, symbol)| {
            ((index as f64) % shard_count == shard_index).then_some(symbol)
        })
        .collect()
}
// ts: KucoinSpotPublicWebsocket.getCandleHandlerTopic
pub fn get_candle_handler_topic(topic: &str) -> Result<String, String> {
    let market = topic.split(':').nth(1).ok_or_else(|| {
        "TypeError: Cannot read properties of undefined (reading 'split')".to_owned()
    })?;
    let mut parts = market.split('_');
    let symbol = parts.next().unwrap_or_default();
    let interval = parts.next().unwrap_or("undefined");
    Ok(format!("candle_{symbol}_{interval}"))
}

// ts: KucoinPublicWebsocket.sendChannelOp
/// Send in source order; the host supplies Date.now and the core socket send.
/// A failed send stops the batch before reading another timestamp.
pub fn send_channel_op<E>(
    subscribe: bool,
    channels: &[String],
    symbols: &[String],
    mut now_ms: impl FnMut() -> f64,
    mut send: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    use safe_cex_num::{JsJsonValue, js_json_stringify};
    for symbols in symbols.chunks(100) {
        let chunk = symbols.join(",");
        if chunk.is_empty() {
            continue;
        }
        for channel in channels {
            let payload = JsJsonValue::Object(vec![
                ("id".into(), JsJsonValue::Number(now_ms())),
                (
                    "type".into(),
                    JsJsonValue::String(
                        if subscribe {
                            "subscribe"
                        } else {
                            "unsubscribe"
                        }
                        .into(),
                    ),
                ),
                (
                    "topic".into(),
                    JsJsonValue::String(format!("{channel}:{chunk}")),
                ),
                ("privateChannel".into(), JsJsonValue::Bool(false)),
                ("response".into(), JsJsonValue::Bool(false)),
            ]);
            // This constructed object contains no BigInt, hooks or cycles.
            let wire = js_json_stringify(&payload)
                .expect("constructed websocket payload is serializable")
                .expect("constructed websocket payload is an object");
            send(&wire)?;
        }
    }
    Ok(())
}
