use super::streams::Topics;
use safe_cex_exchange::{
    exchange::SubscribedCandle,
    js_runtime::value::{Js, Thrown, from_json},
    runtime::ExchangeError,
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_types::{Candle, OHLCVSOptions};
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
        .map(|(interval, _)| interval)
        .ok_or_else(|| Thrown::error("TypeError", "Unknown Bybit candle interval"))?;
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
        interval: serde_json::from_value(Value::String(interval.clone()))
            .map_err(|error| Thrown::error("TypeError", &error.to_string()))?,
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
