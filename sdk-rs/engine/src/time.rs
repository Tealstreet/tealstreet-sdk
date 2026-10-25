//! Proleptic Gregorian calendar math on Unix milliseconds (Howard Hinnant's
//! days-from-civil algorithms). Venues keep their own text grammars and use
//! these for the arithmetic.

pub const DAY_MS: i64 = 86_400_000;

/// Integer Unix milliseconds for outbound protocol fields. Apply calibration
/// first, then floor once; never round a timestamp into the next millisecond.
/// This is not a scheduling clock or input validator: nonfinite/out-of-range
/// values remain invalid to the caller's clock/receipt validation.
pub fn wire_stamp_ms(protocol_ms: f64) -> f64 {
    protocol_ms.floor()
}

pub fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days in `month` (1-12) of `year`; `None` for an invalid month.
pub fn days_in_month(year: i64, month: i64) -> Option<i64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if is_leap(year) { 29 } else { 28 }),
        _ => None,
    }
}

/// Days since 1970-01-01 for a civil date. The date is not validated.
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The civil `(year, month, day)` of a day count since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// `ms` moved back `count` calendar months, keeping the time of day and
/// clamping the day to the target month's length.
pub fn sub_months(ms: i64, count: u32) -> i64 {
    let (year, month, day) = civil_from_days(ms.div_euclid(DAY_MS));
    let months = year * 12 + month - 1 - i64::from(count);
    let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
    let last = days_in_month(year, month).unwrap_or(31);
    days_from_civil(year, month, day.min(last)) * DAY_MS + ms.rem_euclid(DAY_MS)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso_millis(ms: i64) -> String {
    let (year, month, day) = civil_from_days(ms.div_euclid(DAY_MS));
    let within = ms.rem_euclid(DAY_MS);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        within / 3_600_000,
        within / 60_000 % 60,
        within / 1000 % 60,
        within % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_stamp_never_rounds_or_validates_an_invalid_clock() {
        for (time, expected) in [
            (1000.125, 1000.),
            (1000.5, 1000.),
            (1000.999, 1000.),
            (1001., 1001.),
            (-0.125, -1.),
        ] {
            assert_eq!(wire_stamp_ms(time), expected);
        }
        assert!(wire_stamp_ms(f64::NAN).is_nan());
        assert_eq!(wire_stamp_ms(f64::INFINITY), f64::INFINITY);
        assert_eq!(wire_stamp_ms(f64::NEG_INFINITY), f64::NEG_INFINITY);
        assert!(
            crate::signing::SignReceipt {
                nonce: None,
                stamp_ms: Some(wire_stamp_ms(9_007_199_254_740_992.)),
                expiry_ms: None
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn civil_round_trips_across_eras_and_leap_days() {
        for days in (-800_000..800_000).step_by(997) {
            let (y, m, d) = civil_from_days(days);
            assert!(d >= 1 && d <= days_in_month(y, m).unwrap(), "{days}");
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(days_in_month(1900, 2), Some(28));
        assert_eq!(days_in_month(2000, 2), Some(29));
        assert_eq!(days_in_month(2024, 13), None);
    }

    #[test]
    fn formats_iso_milliseconds() {
        assert_eq!(iso_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_millis(1_709_209_849_123), "2024-02-29T12:30:49.123Z");
        assert_eq!(iso_millis(-1), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn month_subtraction_clamps_the_day() {
        let at = |y, m, d| days_from_civil(y, m, d) * DAY_MS + 5;
        assert_eq!(sub_months(at(2024, 3, 31), 1), at(2024, 2, 29));
        assert_eq!(sub_months(at(2023, 3, 31), 1), at(2023, 2, 28));
        assert_eq!(sub_months(at(2024, 1, 15), 13), at(2022, 12, 15));
        assert_eq!(sub_months(at(2024, 5, 31), 0), at(2024, 5, 31));
    }
}
