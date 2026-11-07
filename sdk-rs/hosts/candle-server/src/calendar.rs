//! Interval and range arithmetic from safe-cex `types.ts` (`timeframeToSeconds`),
//! `utils/calendarCandles.ts` (`getNumChartCandlesInRange`) and
//! `utils/normalizeEpochToSeconds.ts`, in the same f64 steps as the source.

/// Every `timeframeToSeconds` key, the TS server's candle interval allowlist.
pub const INTERVALS: [(&str, f64); 27] = [
    ("1s", 1.),
    ("5s", 5.),
    ("15s", 15.),
    ("30s", 30.),
    ("1m", 60.),
    ("3m", 180.),
    ("5m", 300.),
    ("15m", 900.),
    ("30m", 1800.),
    ("1h", 3600.),
    ("2h", 7200.),
    ("3h", 10800.),
    ("4h", 14400.),
    ("5h", 18000.),
    ("6h", 21600.),
    ("7h", 25200.),
    ("8h", 28800.),
    ("9h", 32400.),
    ("10h", 36000.),
    ("11h", 39600.),
    ("12h", 43200.),
    ("24h", 86400.),
    ("1d", 86400.),
    ("2d", 172800.),
    ("3d", 259200.),
    ("1w", 604800.),
    ("1M", 2592000.),
];

pub fn interval_seconds(interval: &str) -> Option<f64> {
    INTERVALS
        .iter()
        .find(|(name, _)| *name == interval)
        .map(|(_, seconds)| *seconds)
}

const DAY_MS: f64 = 86_400_000.;
const WEEK_MS: f64 = 7. * DAY_MS;
const FIRST_MONDAY_MS: f64 = 4. * DAY_MS;
const MS_THRESHOLD: f64 = 1_000_000_000_000.;

#[derive(Clone, Copy, PartialEq)]
enum Period {
    Week,
    Month,
}

fn calendar_period(interval: &str) -> Option<Period> {
    match interval {
        "1w" => Some(Period::Week),
        "1M" => Some(Period::Month),
        _ => None,
    }
}

pub fn is_calendar(interval: &str) -> bool {
    calendar_period(interval).is_some()
}

pub fn normalize_epoch_to_seconds(epoch: f64) -> f64 {
    if epoch > MS_THRESHOLD {
        (epoch / 1000.).floor()
    } else {
        epoch
    }
}

fn candle_start(seconds: f64, time: f64) -> f64 {
    let start = (normalize_epoch_to_seconds(time) / seconds).floor() * seconds;
    if time > MS_THRESHOLD {
        start * 1000.
    } else {
        start
    }
}

/// UTC (year, zero-based month) of an epoch-millisecond instant.
fn utc_year_month(ms: f64) -> (i64, i64) {
    let days = (ms / DAY_MS).floor() as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month - 1)
}

fn calendar_start_ms(period: Period, ts_ms: f64) -> (f64, i64) {
    match period {
        Period::Week => (
            ((ts_ms - FIRST_MONDAY_MS) / WEEK_MS).floor() * WEEK_MS + FIRST_MONDAY_MS,
            0,
        ),
        Period::Month => {
            let (year, month) = utc_year_month(ts_ms);
            (0., year * 12 + month)
        }
    }
}

/// `getNumChartCandlesInRange(interval, from, to)` for a supported interval.
pub fn chart_candles_in_range(interval: &str, from: f64, to: f64) -> f64 {
    let Some(period) = calendar_period(interval) else {
        if from > to {
            return 0.;
        }
        if from == to {
            return 1.;
        }
        let seconds = interval_seconds(interval).expect("a validated interval");
        // getNumCandlesInRange hands getCandleStartTime already-normalized seconds.
        let to_start = candle_start(seconds, normalize_epoch_to_seconds(to));
        let from_start = candle_start(seconds, normalize_epoch_to_seconds(from));
        return ((to_start - from_start) / seconds).floor() + 1.;
    };
    if from > to {
        return 0.;
    }
    let (from_ms, from_months) =
        calendar_start_ms(period, normalize_epoch_to_seconds(from) * 1000.);
    let (to_ms, to_months) = calendar_start_ms(period, normalize_epoch_to_seconds(to) * 1000.);
    match period {
        // Math.round: halves round towards +Infinity.
        Period::Week => ((to_ms - from_ms) / WEEK_MS + 0.5).floor() + 1.,
        Period::Month => (to_months - from_months + 1) as f64,
    }
}

/// `getCandleCacheKey(exchange, symbol, interval)` for a live (non-testnet) account.
pub fn cache_key(exchange: &str, symbol: &str, interval: &str) -> String {
    let suffix = if is_calendar(interval) { "-cal" } else { "" };
    format!("{exchange}-{symbol}-{interval}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAN_15_2025: f64 = 1_736_899_200_000.;

    #[test]
    fn epoch_intervals_count_inclusive_candle_starts() {
        assert_eq!(chart_candles_in_range("1m", JAN_15_2025, JAN_15_2025), 1.);
        assert_eq!(
            chart_candles_in_range("1m", JAN_15_2025, JAN_15_2025 + 59_999.),
            1.
        );
        assert_eq!(
            chart_candles_in_range("1m", JAN_15_2025, JAN_15_2025 + 60_000.),
            2.
        );
        assert_eq!(
            chart_candles_in_range("1s", JAN_15_2025, JAN_15_2025 + 86_399_000.),
            86_400.
        );
    }

    #[test]
    fn calendar_intervals_count_mondays_and_months() {
        // 2023-01-31 to 2023-03-01 spans three calendar months.
        assert_eq!(
            chart_candles_in_range("1M", 1_675_123_200_000., 1_677_628_800_000.),
            3.
        );
        // 2025-01-13 (a Monday) through 2025-01-26 is two weeks.
        assert_eq!(
            chart_candles_in_range("1w", 1_736_726_400_000., 1_737_849_600_000.),
            2.
        );
        assert_eq!(utc_year_month(0.), (1970, 0));
        assert_eq!(utc_year_month(951_782_400_000.), (2000, 1));
    }

    #[test]
    fn cache_keys_carry_the_calendar_suffix() {
        assert_eq!(cache_key("bybit", "BTCUSDT", "1m"), "bybit-BTCUSDT-1m");
        assert_eq!(cache_key("bybit", "BTCUSDT", "1M"), "bybit-BTCUSDT-1M-cal");
        assert_eq!(INTERVALS.len(), 27);
    }
}
