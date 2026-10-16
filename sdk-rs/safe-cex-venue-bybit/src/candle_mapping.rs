#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Candle mapper awaits subscription lifecycle")
)]
//! Candle-state slice; socket subscription/timer ownership remains pending.
use crate::exchange_state::{number, parse_float};
use safe_cex_types::Candle;
use serde_json::Value;

pub(crate) fn apply_kline(
    last: &mut Option<Candle>,
    raw: &Value,
) -> Result<Option<Candle>, &'static str> {
    if raw.is_null() {
        return Err("Cannot read properties of null (reading 'start')");
    }
    let timestamp = number(raw.get("start"))? / 1000.;
    if last.as_ref().is_some_and(|last| last.timestamp > timestamp) {
        return Ok(None);
    }
    let high = parse_float(raw.get("high"))?;
    let low = parse_float(raw.get("low"))?;
    let close = parse_float(raw.get("close"))?;
    let volume = parse_float(raw.get("volume"))?;
    let next = match last.as_ref() {
        Some(previous)
            if previous.timestamp.partial_cmp(&timestamp) != Some(std::cmp::Ordering::Less) =>
        {
            Candle {
                // Deliberately use source comparisons, not f64::max/min: NaN differs.
                high: if previous.high > high {
                    previous.high
                } else {
                    high
                },
                low: if previous.low < low {
                    previous.low
                } else {
                    low
                },
                close,
                volume,
                ..previous.clone()
            }
        }
        _ => Candle {
            timestamp,
            open: parse_float(raw.get("open"))?,
            high,
            low,
            close,
            volume,
            last_side: None,
        },
    };
    *last = Some(next.clone());
    Ok(Some(next))
}

#[cfg(test)]
mod tests;
