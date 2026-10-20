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
    const DAY_MS: i64 = 86_400_000;
    let ms = end.trunc() as i64;
    // Gregorian civil date from days since the Unix epoch (400-year eras).
    let day = ms.div_euclid(DAY_MS) + 719_468;
    let era = day.div_euclid(146_097);
    let doe = day - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    let months = year * 12 + month - 1 - i64::from(count);
    let year = months.div_euclid(12);
    let month = months.rem_euclid(12) + 1;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day.min(last) - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    (days * DAY_MS + ms.rem_euclid(DAY_MS)) as f64
}
