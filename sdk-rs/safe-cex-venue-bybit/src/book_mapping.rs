//! Shared book snapshot/delta state used by the public socket subscriptions.
use crate::{
    api::truthy,
    exchange_state::{number, parse_float},
};
use safe_cex_types::{OrderBook, OrderBookOrders};
use serde_json::{Value, json};

pub(crate) struct BookState {
    pub book: OrderBook,
    pub last_update: Value,
    pub resyncing: bool,
}
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Publish,
    Resync,
    Suppress,
}
impl BookState {
    pub(crate) fn new() -> Self {
        Self {
            book: OrderBook {
                bids: vec![],
                asks: vec![],
            },
            last_update: json!(0),
            resyncing: false,
        }
    }
    pub(crate) fn apply(&mut self, frame: &Value) -> Result<Outcome, String> {
        if frame.is_null() {
            return Err("Cannot read properties of null (reading 'data')".into());
        }
        let data = &frame["data"];
        let snapshot = frame["type"] == "snapshot";
        let delta = frame["type"] == "delta";
        if snapshot {
            self.book.bids.clear();
            self.book.asks.clear();
            self.last_update = update_id(data);
        }
        if delta {
            let update = update_id(data);
            if greater(&self.last_update, &json!(0))? && exceeds_next(&update, &self.last_update)? {
                if self.resyncing {
                    return Ok(Outcome::Suppress);
                }
                self.resyncing = true;
                self.last_update = json!(0);
                self.book.bids.clear();
                self.book.asks.clear();
                return Ok(Outcome::Resync);
            }
            self.last_update = update;
        }
        if snapshot || delta {
            for (key, bids) in [("a", false), ("b", true)] {
                let rows = &data[key];
                if !truthy(rows) {
                    continue;
                }
                let length = match rows {
                    Value::Array(values) => values.len() as f64,
                    Value::String(value) => value.encode_utf16().count() as f64,
                    _ => number(rows.get("length"))?,
                };
                let mut index = 0;
                while (index as f64) < length {
                    let row = indexed(rows, index).ok_or_else(|| {
                        "Cannot read properties of undefined (reading '0')".to_owned()
                    })?;
                    index += 1;
                    if row.is_null() {
                        return Err("Cannot read properties of null (reading '0')".into());
                    }
                    let price = parse_float(indexed(&row, 0).as_ref())?;
                    let amount = parse_float(indexed(&row, 1).as_ref())?;
                    if snapshot {
                        let levels = if bids {
                            &mut self.book.bids
                        } else {
                            &mut self.book.asks
                        };
                        levels.push(OrderBookOrders {
                            price,
                            amount,
                            total: 0.,
                        });
                    } else {
                        safe_cex_store::update_order_book::update_order_book_by_price(
                            &mut self.book,
                            bids,
                            price,
                            amount,
                            true,
                        );
                    }
                }
            }
        }
        // No sorting here: source trusts snapshot order and delta binary search.
        safe_cex_store::orderbook::calc_order_book_total(&mut self.book);
        Ok(Outcome::Publish)
    }
}
// JS property indexing on a level is distinct from array indexing. A string
// contributes UTF-16 code units. An isolated surrogate is necessarily NaN under
// parseFloat; the replacement scalar preserves that numeric observation only.
fn indexed(value: &Value, index: usize) -> Option<Value> {
    match value {
        Value::Array(values) => values.get(index).cloned(),
        Value::Object(values) => values.get(&index.to_string()).cloned(),
        Value::String(value) => value.encode_utf16().nth(index).map(|unit| {
            Value::String(
                char::from_u32(u32::from(unit))
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .to_string(),
            )
        }),
        _ => None,
    }
}
fn update_id(data: &Value) -> Value {
    let value = &data["u"];
    if truthy(value) {
        value.clone()
    } else {
        json!(0)
    }
}
fn primitive(value: &Value) -> Result<Value, String> {
    if value.is_object() || value.is_array() {
        Ok(Value::String(
            safe_cex_num::js_string(Some(value)).map_err(|e| e.message.to_owned())?,
        ))
    } else {
        Ok(value.clone())
    }
}
fn greater(a: &Value, b: &Value) -> Result<bool, String> {
    let a = primitive(a)?;
    let b = primitive(b)?;
    if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
        return Ok(a.encode_utf16().cmp(b.encode_utf16()).is_gt());
    }
    Ok(number(Some(&a))? > number(Some(&b))?)
}
fn exceeds_next(update: &Value, last: &Value) -> Result<bool, String> {
    let last = primitive(last)?;
    if let Some(last) = last.as_str() {
        greater(update, &Value::String(format!("{last}1")))
    } else {
        // Compare numerically directly so infinity never takes a JSON detour.
        Ok(number(Some(&primitive(update)?))? > number(Some(&last))? + 1.)
    }
}
#[cfg(test)]
mod tests;
