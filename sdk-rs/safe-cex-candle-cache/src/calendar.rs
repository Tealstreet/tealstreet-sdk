//! Typed calendar helpers used by the cache. All boundary arguments/results are seconds.
//! Source: packages/safe-cex/src/utils/calendarCandles.ts; narrowed APIs are manifest divergences.
use safe_cex_types::{AnyTimeframe, ExtendedTimeframe, SecondsTimeframe, Timeframe};

// TS: timeframeToSeconds; exhaustive union matching makes new intervals a compile error.
fn specification(interval: &AnyTimeframe) -> (&'static str, f64) {
    match interval {
        AnyTimeframe::Variant0(SecondsTimeframe::S1) => ("1s", 1.),
        AnyTimeframe::Variant0(SecondsTimeframe::S5) => ("5s", 5.),
        AnyTimeframe::Variant0(SecondsTimeframe::S15) => ("15s", 15.),
        AnyTimeframe::Variant0(SecondsTimeframe::S30) => ("30s", 30.),
        AnyTimeframe::Variant1(Timeframe::M1) => ("1m", 60.),
        AnyTimeframe::Variant1(Timeframe::M3) => ("3m", 180.),
        AnyTimeframe::Variant1(Timeframe::M5) => ("5m", 300.),
        AnyTimeframe::Variant1(Timeframe::M15) => ("15m", 900.),
        AnyTimeframe::Variant1(Timeframe::M30) => ("30m", 1800.),
        AnyTimeframe::Variant1(Timeframe::H1) => ("1h", 3600.),
        AnyTimeframe::Variant1(Timeframe::H2) => ("2h", 7200.),
        AnyTimeframe::Variant1(Timeframe::H4) => ("4h", 14400.),
        AnyTimeframe::Variant1(Timeframe::H6) => ("6h", 21600.),
        AnyTimeframe::Variant1(Timeframe::H12) => ("12h", 43200.),
        AnyTimeframe::Variant1(Timeframe::D1) => ("1d", 86400.),
        AnyTimeframe::Variant1(Timeframe::W1) => ("1w", 604800.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H3) => ("3h", 10800.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H5) => ("5h", 18000.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H7) => ("7h", 25200.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H8) => ("8h", 28800.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H9) => ("9h", 32400.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H10) => ("10h", 36000.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H11) => ("11h", 39600.),
        AnyTimeframe::Variant2(ExtendedTimeframe::H24) => ("24h", 86400.),
        AnyTimeframe::Variant2(ExtendedTimeframe::D2) => ("2d", 172800.),
        AnyTimeframe::Variant2(ExtendedTimeframe::D3) => ("3d", 259200.),
        AnyTimeframe::Variant2(ExtendedTimeframe::MN1) => ("1M", 2592000.),
    }
}
pub(crate) fn seconds(interval: &AnyTimeframe) -> f64 {
    specification(interval).1
}
pub(crate) fn name(interval: &AnyTimeframe) -> &'static str {
    specification(interval).0
}
pub(crate) fn month(interval: &AnyTimeframe) -> bool {
    matches!(interval, AnyTimeframe::Variant2(ExtendedTimeframe::MN1))
}
pub fn calendar(interval: &AnyTimeframe) -> bool {
    month(interval) || matches!(interval, AnyTimeframe::Variant1(Timeframe::W1))
}
pub fn normalize(epoch: f64) -> f64 {
    if epoch > 1_000_000_000_000. {
        (epoch / 1000.).floor()
    } else {
        epoch
    }
}
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}
pub fn start(interval: &AnyTimeframe, t: f64) -> f64 {
    if month(interval) {
        calendar_period_start_ms(CalendarPeriod::Month, t * 1000.0) / 1000.0
    } else if calendar(interval) {
        ((t - 345600.) / 604800.).floor() * 604800. + 345600.
    } else {
        (t / seconds(interval)).floor() * seconds(interval)
    }
}
pub fn close(interval: &AnyTimeframe, t: f64) -> f64 {
    if month(interval) {
        let clipped = safe_cex_num::js_time_clip(t * 1000.0);
        if !clipped.is_finite() {
            return f64::NAN;
        }
        let day = (clipped / 86400000.0).floor();
        let (mut y, m, d) = civil(day as i64);
        if (0..=99).contains(&y) {
            y += 1900;
        }
        let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let seconds = ((clipped - day * 86400000.0) / 1000.0).floor();
        safe_cex_num::js_time_clip((days(y, m, d) as f64 * 86400.0 + seconds) * 1000.0) / 1000.0
    } else {
        t + seconds(interval)
    }
}
pub fn next(interval: &AnyTimeframe, t: f64) -> f64 {
    if month(interval) {
        next_calendar_period_start_ms(CalendarPeriod::Month, t * 1000.0) / 1000.0
    } else {
        close(interval, start(interval, t))
    }
}
pub fn count(interval: &AnyTimeframe, from: f64, to: f64) -> f64 {
    if from > to {
        return 0.;
    }
    if month(interval) {
        let from = start(interval, from);
        let to = start(interval, to);
        if !from.is_finite() || !to.is_finite() {
            return f64::NAN;
        }
        let (fy, fm, _) = civil((from / 86400.).floor() as i64);
        let (ty, tm, _) = civil((to / 86400.).floor() as i64);
        ((ty - fy) * 12 + tm - fm + 1) as f64
    } else {
        ((start(interval, to) - start(interval, from)) / seconds(interval)).round() + 1.
    }
}

/// Calendar period wire labels used by chart utilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CalendarPeriod {
    #[serde(rename = "1W")]
    Week,
    #[serde(rename = "1M")]
    Month,
}
pub fn calendar_period_for_chart_seconds(seconds: f64) -> Option<CalendarPeriod> {
    if seconds == 604800.0 {
        Some(CalendarPeriod::Week)
    } else if seconds == 2592000.0 {
        Some(CalendarPeriod::Month)
    } else {
        None
    }
}
pub fn calendar_period_start_ms(period: CalendarPeriod, ts_ms: f64) -> f64 {
    if period == CalendarPeriod::Week {
        return ((ts_ms - 345600000.0) / 604800000.0).floor() * 604800000.0 + 345600000.0;
    }
    // ECMAScript Date TimeClip truncates sub-ms input and rejects out-of-range dates.
    if !ts_ms.is_finite() || ts_ms.abs() > 8640000000000000.0 {
        return f64::NAN;
    }
    let (mut year, month, _) = civil((ts_ms.trunc() / 86400000.0).floor() as i64);
    // Date.UTC treats years 0..99 as 1900..1999 (including this source quirk).
    if (0..=99).contains(&year) {
        year += 1900;
    }
    let result = days(year, month, 1) as f64 * 86400000.0;
    if result.abs() > 8640000000000000.0 {
        f64::NAN
    } else {
        result
    }
}
pub fn next_calendar_period_start_ms(period: CalendarPeriod, ts_ms: f64) -> f64 {
    let start = calendar_period_start_ms(period, ts_ms);
    if period == CalendarPeriod::Week {
        return start + 604800000.0;
    }
    if !start.is_finite() {
        return f64::NAN;
    }
    let (mut year, mut month, _) = civil((start / 86400000.0).floor() as i64);
    // Date.UTC remaps its original year before normalizing the month overflow.
    if (0..=99).contains(&year) {
        year += 1900;
    }
    month += 1;
    if month == 13 {
        month = 1;
        year += 1;
    }
    let result = days(year, month, 1) as f64 * 86400000.0;
    if result.abs() > 8640000000000000.0 {
        f64::NAN
    } else {
        result
    }
}
pub fn create_live_candle_start(interval: &str) -> impl FnMut(f64) -> f64 + use<> {
    let period = match interval {
        "1w" | "1W" | "W" => Some(CalendarPeriod::Week),
        "1M" | "M" => Some(CalendarPeriod::Month),
        _ => None,
    };
    let width = safe_cex_types::extra_model::TIMEFRAME_TO_SECONDS
        .get(interval)
        .copied()
        .unwrap_or(f64::NAN);
    let mut start = f64::NAN;
    let mut end = f64::NAN;
    move |t| {
        let Some(period) = period else {
            return (t / width).floor() * width;
        };
        if t >= start && t < end {
            return start;
        }
        start = calendar_period_start_ms(period, t * 1000.0) / 1000.0;
        end = next_calendar_period_start_ms(period, t * 1000.0) / 1000.0;
        start
    }
}
fn epoch_width(interval: &str) -> f64 {
    safe_cex_types::extra_model::TIMEFRAME_TO_SECONDS
        .get(interval)
        .copied()
        .unwrap_or(f64::NAN)
}
pub fn get_candle_start_time(interval: &str, time: f64) -> f64 {
    let width = epoch_width(interval);
    let result = (normalize(time) / width).floor() * width;
    if time > 1000000000000.0 {
        result * 1000.0
    } else {
        result
    }
}
pub fn get_candle_end_time(interval: &str, time: f64) -> f64 {
    let epoch = normalize(time);
    let width = epoch_width(interval);
    let result = if epoch % width == 0.0 {
        epoch + width
    } else {
        (epoch / width).ceil() * width
    };
    if time > 1000000000000.0 {
        result * 1000.0
    } else {
        result
    }
}
pub fn get_num_candles_in_range(interval: &str, from: f64, to: f64) -> Result<f64, String> {
    if from > to {
        eprintln!(
            "[getNumCandlesInRange] Invalid range: from ({}) must be less than to ({})",
            safe_cex_num::js_number_string(from),
            safe_cex_num::js_number_string(to)
        );
        return Ok(0.0);
    }
    if from == to {
        return Ok(1.0);
    }
    let width = epoch_width(interval);
    // ts-bug: getNumCandlesInRange — inherited Object.prototype properties
    // pass the table's truthiness guard, then numeric coercion yields NaN.
    let inherited = matches!(
        interval,
        "constructor"
            | "__proto__"
            | "toString"
            | "toLocaleString"
            | "valueOf"
            | "hasOwnProperty"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "__defineGetter__"
            | "__defineSetter__"
            | "__lookupGetter__"
            | "__lookupSetter__"
    );
    if width == 0.0 || (width.is_nan() && !inherited) {
        return Err(format!("Unsupported timeframe: {interval}"));
    }
    Ok(((get_candle_start_time(interval, normalize(to))
        - get_candle_start_time(interval, normalize(from)))
        / width)
        .floor()
        + 1.0)
}
pub fn split_time_range(from: f64, to: f64, max_span_ms: f64) -> Vec<serde_json::Value> {
    if !from.is_finite() || !to.is_finite() || to < from {
        return vec![];
    }
    if !max_span_ms.is_finite() || max_span_ms <= 0.0 {
        return vec![serde_json::json!({"startTime":from,"endTime":to})];
    }
    let mut windows = vec![];
    let mut start = from;
    while start <= to {
        let end = (start + max_span_ms).min(to);
        windows.push(serde_json::json!({"startTime":start,"endTime":end}));
        if end >= to {
            break;
        }
        start = end + 1.0;
    }
    windows
}
