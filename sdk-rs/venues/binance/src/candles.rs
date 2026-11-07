//! REST windows are trailing when a limit is requested; monthly bars use UTC
//! calendar months, preserving time of day and clamping to the target month.
use crate::sign::Query;
use safe_cex_engine::model::Interval;

pub fn query(
    symbol: &str,
    interval: Interval,
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
    };
    if !valid_time(start) {
        return Err("Binance candle start time is invalid");
    }
    Ok(Query::new()
        .str("symbol", symbol)
        .str("interval", interval.as_str())
        .num("startTime", start)
        .num("endTime", end)
        .opt_num("limit", limit.map(f64::from)))
}

fn valid_time(time: f64) -> bool {
    time.is_finite() && time.abs() <= 8_640_000_000_000_000.0
}

fn subtract(end: f64, interval: Interval, count: u32) -> f64 {
    match interval {
        Interval::Mo1 => subtract_months(end, count),
        _ => end - interval.ms() * f64::from(count),
    }
}

fn subtract_months(end: f64, count: u32) -> f64 {
    safe_cex_engine::time::sub_months(end.trunc() as i64, count) as f64
}
