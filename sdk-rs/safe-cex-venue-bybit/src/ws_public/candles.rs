use super::streams::Topics;
use safe_cex_exchange::{
    exchange::SubscribedCandle,
    js_runtime::value::{Js, Thrown, from_json},
    runtime::ExchangeError,
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_types::{Candle, ModelField, OHLCVSOptions};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn map_bulk(frame: &Value) -> Result<SubscribedCandle, Thrown> {
    let frame = from_json(frame);
    let data = frame.get("data")?;
    let raw = match &data {
        Js::Array(_) | Js::String(_) => data.get("0")?,
        _ => return Err(Thrown::error("TypeError", "object is not iterable")),
    };
    let topic = frame.get("topic")?;
    let second_dot = topic.call_method("indexOf", vec![Js::str("."), Js::Number(6.)])?;
    let wire_interval = topic.call_method("slice", vec![Js::Number(6.), second_dot.clone()])?;
    let symbol = topic.call_method("slice", vec![Js::Number(second_dot.to_number()? + 1.)])?;
    let key = String::from_utf16_lossy(&wire_interval.to_js_string()?);
    let interval = crate::types::INTERVAL
        .as_object()
        .and_then(|table| {
            table
                .iter()
                .find(|(_, wire)| wire.as_str() == Some(key.as_str()))
        })
        .map(|(interval, _)| {
            serde_json::from_value(Value::String(interval.clone()))
                .map(ModelField::Value)
                .map_err(|error| Thrown::error("TypeError", &error.to_string()))
        })
        .transpose()?
        .unwrap_or(ModelField::Undefined);
    Ok(SubscribedCandle {
        candle: Candle {
            timestamp: raw.get("start")?.to_number()? / 1000.,
            open: raw.get("open")?.parse_float()?,
            high: raw.get("high")?.parse_float()?,
            low: raw.get("low")?.parse_float()?,
            close: raw.get("close")?.parse_float()?,
            volume: raw.get("volume")?.parse_float()?,
            last_side: None,
        },
        symbol: String::from_utf16_lossy(&symbol.to_js_string()?),
        interval,
    })
}

fn connect_bulk(
    socket: Rc<BaseWebSocket>,
    topics: Rc<Vec<String>>,
    chunk_size: usize,
) -> Result<(), ExchangeError> {
    if socket.is_connected() {
        if !socket.is_disposed() {
            for chunk in topics.chunks(chunk_size) {
                socket.send(&json!({"op":"subscribe","args":chunk}).to_string())?;
            }
        }
    } else {
        let weak = Rc::downgrade(&socket);
        socket.start_timeout(100, move || {
            if let Some(socket) = weak.upgrade() {
                connect_bulk(socket, topics.clone(), chunk_size)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

pub(crate) fn listen_ohlcvs(
    socket: Rc<BaseWebSocket>,
    retained: Topics,
    options: OHLCVSOptions,
    chunk_size: usize,
    callback: Rc<dyn Fn(SubscribedCandle)>,
) -> Result<(), ExchangeError> {
    let interval = serde_json::to_value(&options.interval)
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    let wire_interval = crate::types::INTERVAL[interval.as_str().unwrap()]
        .as_str()
        .unwrap();
    let topics: Vec<String> = options
        .symbols
        .iter()
        .map(|symbol| format!("kline.{wire_interval}.{symbol}"))
        .collect();
    for topic in &topics {
        retained
            .borrow_mut()
            .insert(topic.clone(), Value::String(topic.clone()));
    }
    let parser: MessageHandler = Rc::new(move |frame| {
        callback(map_bulk(frame)?);
        Ok(())
    });
    socket
        .message_handlers
        .borrow_mut()
        .insert("kline".into(), Rc::new(RefCell::new(vec![parser])));
    connect_bulk(socket, Rc::new(topics), chunk_size)
}

#[cfg(test)]
mod tests {
    use super::map_bulk;
    use serde_json::{Value, json};
    #[test]
    #[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
    fn bulk_candle_callbacks_match_maintained_source() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/bulk-candle-fixtures.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cases.len(), 21);
        for (index, case) in cases.iter().enumerate() {
            let candle = map_bulk(&case["frame"]).unwrap();
            let mut actual = serde_json::to_value(&candle).unwrap();
            if candle.interval.is_undefined() {
                actual["interval"] = json!("$undefined");
            }
            assert_eq!(actual, case["emitted"], "maintained bulk case {index}");
        }
    }
}
