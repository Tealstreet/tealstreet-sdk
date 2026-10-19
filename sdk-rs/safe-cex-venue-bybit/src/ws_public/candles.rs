use super::streams::Topics;
use safe_cex_exchange::{
    exchange::SubscribedCandle,
    js_runtime::value::{Js, Thrown},
    runtime::ExchangeError,
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_types::{Candle, ModelField, OHLCVSOptions};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub(super) fn first_candle(data: &Js) -> Result<Js, Thrown> {
    if matches!(data, Js::Array(_) | Js::String(_)) {
        let iterator = data.iterator()?;
        let step = iterator.call_method("next", vec![])?;
        let value = step.get("value")?;
        if !step.get("done")?.truthy() {
            let close = iterator.get("return")?;
            if !close.is_nullish() {
                close.call(iterator, vec![])?;
            }
        }
        return Ok(value);
    }
    let operand = match data {
        Js::Null => "object null".to_owned(),
        Js::Bool(_) | Js::Number(_) => format!("{} {}", data.type_of(), data.to_display()),
        _ => data.type_of().to_owned(),
    };
    let message = if data.is_nullish() {
        format!(
            "Cannot destructure property 'Symbol(Symbol.iterator)' of '{operand}' as it is {}.",
            data.to_display()
        )
    } else {
        format!("{operand} is not iterable (cannot read property Symbol(Symbol.iterator))")
    };
    Err(Thrown::type_error(&message))
}

fn map_bulk(frame: &Js) -> Result<SubscribedCandle, Thrown> {
    let raw = first_candle(&frame.get("data")?)?;
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

fn connect_bulk(socket: Rc<BaseWebSocket>, topics: Rc<Vec<String>>) -> Result<(), ExchangeError> {
    if socket.is_connected() {
        if !socket.is_disposed() {
            for chunk in topics.chunks(50) {
                socket.send(&json!({"op":"subscribe","args":chunk}).to_string())?;
            }
        }
    } else {
        let weak = Rc::downgrade(&socket);
        socket.start_timeout(100, move || {
            if let Some(socket) = weak.upgrade() {
                connect_bulk(socket, topics.clone())?;
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
    callback: Rc<dyn Fn(SubscribedCandle)>,
) -> Result<(), ExchangeError> {
    let interval = serde_json::to_value(options.interval)
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
    connect_bulk(socket, Rc::new(topics))
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
        assert_eq!(cases.len(), 36);
        for (index, case) in cases.iter().enumerate() {
            if !case["error"].is_null() {
                let error = map_bulk(&safe_cex_exchange::js_runtime::value::from_json(
                    &case["frame"],
                ))
                .unwrap_err()
                .to_exchange_error();
                assert_eq!(
                    json!({"name":error.name,"message":error.message}),
                    case["error"],
                    "maintained bulk error {index}"
                );
                continue;
            }
            let candle = map_bulk(&safe_cex_exchange::js_runtime::value::from_json(
                &case["frame"],
            ))
            .unwrap();
            let mut actual = serde_json::to_value(&candle).unwrap();
            if candle.interval.is_undefined() {
                actual["interval"] = json!("$undefined");
            }
            let mut expected = case["emitted"].clone();
            for field in ["timestamp", "open", "high", "low", "close", "volume"] {
                if let Some(value) = expected[field].as_f64() {
                    expected[field] = json!(value);
                }
            }
            assert_eq!(actual, expected, "maintained bulk case {index}");
        }
    }
}
