use safe_cex_num::parse_float;
use safe_cex_types::{OrderBook, OrderBookOrders};

// ts: BinanceOrderBookSyncState
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BinanceOrderBookSyncState {
    pub snapshot_last_update_id: Option<f64>,
    pub last_applied_update_id: Option<f64>,
}

// ts: resetBinanceOrderBookSyncState
pub fn reset_binance_order_book_sync_state(state: &mut BinanceOrderBookSyncState) {
    state.snapshot_last_update_id = None;
    state.last_applied_update_id = None;
}

// ts: loadBinanceOrderBookSnapshot
pub fn load_binance_order_book_snapshot(
    order_book: &mut OrderBook,
    bids: &[Vec<String>],
    asks: &[Vec<String>],
    last_update_id: f64,
    state: &mut BinanceOrderBookSyncState,
) {
    let map = |rows: &[Vec<String>]| {
        rows.iter()
            .map(|row| OrderBookOrders {
                price: parse_float(row.first().map_or("undefined", String::as_str)),
                amount: parse_float(row.get(1).map_or("undefined", String::as_str)),
                total: 0.0,
            })
            .collect()
    };
    order_book.bids = map(bids);
    order_book.asks = map(asks);
    state.snapshot_last_update_id = Some(last_update_id);
    state.last_applied_update_id = None;
}

// ts: BinanceDepthUpdate
pub type BinanceDepthUpdate = serde_json::Map<String, serde_json::Value>;

// ts: getBufferedBinanceDepthUpdates
pub fn get_buffered_binance_depth_updates(
    updates: &[BinanceDepthUpdate],
    snapshot_last_update_id: f64,
) -> Result<Vec<&BinanceDepthUpdate>, safe_cex_num::JsCoercionError> {
    let mut buffered = Vec::new();
    for update in updates {
        if safe_cex_num::js_number(update.get("u"))? > snapshot_last_update_id {
            buffered.push(update);
        }
    }
    Ok(buffered)
}

// ts: applyBinanceDepthUpdate
pub fn apply_binance_depth_update(
    order_book: &mut OrderBook,
    update: &BinanceDepthUpdate,
    state: &mut BinanceOrderBookSyncState,
    require_previous_final_update_id: bool,
) -> Result<crate::protocol_types::BinanceOrderBookApplyResult, safe_cex_num::JsCoercionError> {
    use crate::protocol_types::BinanceOrderBookApplyResult::{Applied, Gap, Ignored};
    use safe_cex_num::{js_number, js_string, js_truthy};
    let final_id = js_number(update.get("u"))?;
    let first_id = js_number(update.get("U"))?;
    let Some(local_id) = state
        .last_applied_update_id
        .or(state.snapshot_last_update_id)
    else {
        return Ok(Gap);
    };
    if !final_id.is_finite() {
        return Ok(Gap);
    }
    if final_id <= local_id {
        return Ok(Ignored);
    }
    let contiguous = if let Some(previous) = state.last_applied_update_id {
        if require_previous_final_update_id && js_number(update.get("pu"))?.is_finite() {
            js_number(update.get("pu"))? == previous
        } else {
            first_id.is_finite()
                && first_id.partial_cmp(&(local_id + 1.0)) != Some(std::cmp::Ordering::Greater)
        }
    } else {
        first_id.is_finite()
            && first_id.partial_cmp(&(local_id + 1.0)) != Some(std::cmp::Ordering::Greater)
    };
    if !contiguous {
        return Ok(Gap);
    }
    for (key, bids) in [("b", true), ("a", false)] {
        let levels = update.get(key);
        if !js_truthy(levels) {
            continue;
        }
        let length = match levels {
            Some(serde_json::Value::Array(rows)) => rows.len() as f64,
            Some(serde_json::Value::String(text)) => text.encode_utf16().count() as f64,
            Some(serde_json::Value::Object(record)) => js_number(record.get("length"))?,
            _ => f64::NAN,
        };
        let mut index = 0.0;
        while index < length {
            let row = depth_index(levels, index);
            if row.as_ref().is_none_or(serde_json::Value::is_null) {
                return Err(safe_cex_num::JsCoercionError {
                    name: "TypeError",
                    message: if row.is_none() {
                        "Cannot read properties of undefined (reading '0')"
                    } else {
                        "Cannot read properties of null (reading '0')"
                    },
                });
            }
            let price = parse_float(&js_string(depth_index(row.as_ref(), 0.0).as_ref())?);
            let amount = parse_float(&js_string(depth_index(row.as_ref(), 1.0).as_ref())?);
            safe_cex_store::update_order_book::update_order_book(
                order_book,
                bids,
                OrderBookOrders {
                    price,
                    amount,
                    total: 0.0,
                },
                true,
            );
            index += 1.0;
        }
    }
    state.last_applied_update_id = Some(final_id);
    Ok(Applied)
}

fn depth_index(value: Option<&serde_json::Value>, index: f64) -> Option<serde_json::Value> {
    use serde_json::Value;
    match value {
        Some(Value::Array(rows)) => rows.get(index as usize).cloned(),
        Some(Value::Object(record)) => record.get(&safe_cex_num::js_number_string(index)).cloned(),
        // Each indexed string cell is consumed only by parseFloat; lone units parse as NaN.
        Some(Value::String(text)) => text
            .encode_utf16()
            .nth(index as usize)
            .map(|unit| Value::String(String::from_utf16_lossy(&[unit]))),
        _ => None,
    }
}
