//! Candle state consumed by the concrete public kline listener.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::Candle;

pub(crate) fn apply_kline(last: &mut Option<Candle>, raw: &Js) -> Result<Option<Candle>, Thrown> {
    let timestamp = raw.get("start")?.to_number()? / 1000.;
    if last.as_ref().is_some_and(|last| last.timestamp > timestamp) {
        return Ok(None);
    }
    let high = raw.get("high")?.parse_float()?;
    let low = raw.get("low")?.parse_float()?;
    let close = raw.get("close")?.parse_float()?;
    let volume = raw.get("volume")?.parse_float()?;
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
            open: raw.get("open")?.parse_float()?,
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
