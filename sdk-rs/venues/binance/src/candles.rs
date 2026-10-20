//! REST windows are trailing when a limit is requested; monthly bars use UTC
//! calendar months, preserving time of day and clamping to the target month.
use crate::sign::Query;

pub fn query(
    symbol: &str,
    interval: &str,
    limit: Option<u32>,
    from: Option<f64>,
    to: Option<f64>,
    now: f64,
) -> Result<Query, &'static str> {
    let limit = limit.filter(|n| *n != 0).map(|n| n.min(1_500));
    let end = to.filter(|n| *n != 0.0).unwrap_or(now);
    if !valid_time(end) {
        return Err("Binance candle end time is invalid");
    }
    let start = if limit.is_none() && from.is_some_and(|n| n != 0.0) {
        from.unwrap()
    } else {
        subtract(end, interval, limit.unwrap_or(500))
            .ok_or("Binance candle interval is unsupported")?
    };
    if !valid_time(start) {
        return Err("Binance candle start time is invalid");
    }
    Ok(Query::new()
        .str("symbol", symbol)
        .str("interval", interval)
        .num("startTime", start)
        .num("endTime", end)
        .opt_num("limit", limit.map(f64::from)))
}

fn valid_time(time: f64) -> bool {
    time.is_finite() && time.abs() <= 8_640_000_000_000_000.0
}

fn subtract(end: f64, interval: &str, count: u32) -> Option<f64> {
    let duration = match interval {
        "1s" => 1_000.0,
        "1m" => 60_000.0,
        "3m" => 180_000.0,
        "5m" => 300_000.0,
        "15m" => 900_000.0,
        "30m" => 1_800_000.0,
        "1h" => 3_600_000.0,
        "2h" => 7_200_000.0,
        "4h" => 14_400_000.0,
        "6h" => 21_600_000.0,
        "8h" => 28_800_000.0,
        "12h" => 43_200_000.0,
        "1d" => 86_400_000.0,
        "3d" => 259_200_000.0,
        "1w" => 604_800_000.0,
        "1M" => return Some(subtract_months(end, count)),
        _ => return None,
    };
    Some(end - duration * f64::from(count))
}

fn subtract_months(end: f64, count: u32) -> f64 {
    safe_cex_engine::time::sub_months(end.trunc() as i64, count) as f64
}
