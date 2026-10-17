//! Persistence codecs keep Candle typed while restoring JSON's null numeric encoding.
use safe_cex_types::Candle;

pub(crate) fn decode_json_candle(value: &serde_json::Value) -> Option<Candle> {
    let row = value.as_object()?;
    let timestamp = row.get("timestamp")?.as_f64()?;
    if !timestamp.is_finite() {
        return None;
    }
    let number = |key| match row.get(key)? {
        serde_json::Value::Null => Some(f64::NAN),
        value => value.as_f64(),
    };
    Some(Candle {
        timestamp,
        open: number("open")?,
        high: number("high")?,
        low: number("low")?,
        close: number("close")?,
        volume: number("volume")?,
        last_side: match row.get("lastSide") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(serde_json::from_value(value.clone()).ok()?),
        },
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn decode_js_candle(value: &wasm_bindgen::JsValue) -> Result<Option<Candle>, String> {
    use wasm_bindgen::JsValue;
    if !value.is_object() || value.is_null() || js_sys::Array::is_array(value) {
        return Ok(None);
    }
    let get = |key| {
        js_sys::Reflect::get(value, &JsValue::from_str(key)).map_err(super::indexed_db::error)
    };
    let Some(timestamp) = get("timestamp")?.as_f64().filter(|n| n.is_finite()) else {
        return Ok(None);
    };
    let number = |key| -> Result<Option<f64>, String> {
        let value = get(key)?;
        Ok(if value.is_null() {
            Some(f64::NAN)
        } else {
            value.as_f64()
        })
    };
    let [Some(open), Some(high), Some(low), Some(close), Some(volume)] = [
        number("open")?,
        number("high")?,
        number("low")?,
        number("close")?,
        number("volume")?,
    ] else {
        return Ok(None);
    };
    let side = get("lastSide")?;
    let last_side = if side.is_null() || side.is_undefined() {
        None
    } else {
        match side.as_string().as_deref() {
            Some("buy") => Some(safe_cex_types::OrderSide::Buy),
            Some("sell") => Some(safe_cex_types::OrderSide::Sell),
            _ => return Ok(None),
        }
    };
    Ok(Some(Candle {
        timestamp,
        open,
        high,
        low,
        close,
        volume,
        last_side,
    }))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn encode_js_record(
    candles: Vec<Candle>,
    last_accessed: f64,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    use wasm_bindgen::JsValue;
    let rows = js_sys::Array::new();
    for Candle {
        timestamp,
        open,
        high,
        low,
        close,
        volume,
        last_side,
    } in candles
    {
        let row = js_sys::Object::new();
        for (name, number) in [
            ("timestamp", timestamp),
            ("open", open),
            ("high", high),
            ("low", low),
            ("close", close),
            ("volume", volume),
        ] {
            js_sys::Reflect::set(&row, &JsValue::from_str(name), &JsValue::from_f64(number))?;
        }
        if let Some(side) = last_side {
            let text = match side {
                safe_cex_types::OrderSide::Buy => "buy",
                safe_cex_types::OrderSide::Sell => "sell",
            };
            js_sys::Reflect::set(
                &row,
                &JsValue::from_str("lastSide"),
                &JsValue::from_str(text),
            )?;
        }
        rows.push(&row);
    }
    let record = js_sys::Object::new();
    js_sys::Reflect::set(&record, &JsValue::from_str("candles"), &rows)?;
    js_sys::Reflect::set(
        &record,
        &JsValue::from_str("lastAccessed"),
        &JsValue::from_f64(last_accessed),
    )?;
    Ok(record.into())
}
