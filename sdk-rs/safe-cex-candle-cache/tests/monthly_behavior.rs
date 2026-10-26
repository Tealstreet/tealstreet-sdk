use safe_cex_candle_cache::calendar;
use safe_cex_types::{AnyTimeframe, ExtendedTimeframe};

fn equal(actual: f64, expected: f64, label: &str) {
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "MONTHLY native {label}: {actual:?} != {expected:?}"
    );
}
fn check(op: &str, implementation: fn(&AnyTimeframe, f64) -> f64, expected: [f64; 8]) {
    let interval = AnyTimeframe::Variant2(ExtendedTimeframe::MN1);
    let times = [
        -62135596800.0,
        -59012928000.0,
        -59011459200.0,
        1706659200.0,
        -0.0001,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for i in 0..8 {
        equal(
            implementation(&interval, times[i]),
            expected[i],
            &format!("{op}/{i}"),
        );
    }
    source(op, &expected);
}
fn source(op: &str, expected: &[f64]) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/monthly-behavior-source.cjs"
        ))
        .arg(op)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MONTHLY actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "MONTHLY source independent operation floor"
    );
    for i in 0..expected.len() {
        equal(
            actual[i].parse().unwrap(),
            expected[i],
            &format!("source {op}/{i}"),
        );
    }
}
#[test]
fn monthly_start_preserves_date_utc() {
    check(
        "chartCandleStartTime",
        calendar::start,
        [
            -2177452800.0,
            944006400.0,
            -59011459200.0,
            1704067200.0,
            0.0,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        ],
    );
}
#[test]
fn monthly_next_preserves_date_utc() {
    check(
        "nextChartCandleStartTime",
        calendar::next,
        [
            -2174774400.0,
            946684800.0,
            -59008780800.0,
            1706745600.0,
            2678400.0,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        ],
    );
}
#[test]
fn monthly_close_preserves_date_utc() {
    check(
        "chartCandleCloseTime",
        calendar::close,
        [
            -2174774400.0,
            947894400.0,
            -59008780800.0,
            1709337600.0,
            2678400.0,
            f64::NAN,
            f64::NAN,
            f64::NAN,
        ],
    );
}
#[test]
fn monthly_count_uses_remapped_starts() {
    let interval = AnyTimeframe::Variant2(ExtendedTimeframe::MN1);
    let cases = [
        (-62135596800.0, -59011459200.0, -21611.0),
        (0.0, 0.0, 1.0),
        (1706659200.0, 1706745600.0, 2.0),
        (f64::NAN, 0.0, f64::NAN),
        (0.0, f64::NAN, f64::NAN),
        (f64::INFINITY, 0.0, 0.0),
        (0.0, f64::INFINITY, f64::NAN),
        (f64::NEG_INFINITY, 0.0, f64::NAN),
        (f64::INFINITY, f64::NEG_INFINITY, 0.0),
        (-0.0001, 0.0, 1.0),
    ];
    for (i, (from, to, expected)) in cases.iter().enumerate() {
        equal(
            calendar::count(&interval, *from, *to),
            *expected,
            &format!("count/{i}"),
        );
    }
    source(
        "getNumChartCandlesInRange",
        &cases.map(|(_, _, value)| value),
    );
}

#[test]
fn monthly_negative_year_rollover_keeps_original_utc_year() {
    equal(
        calendar::next_calendar_period_start_ms(calendar::CalendarPeriod::Month, -62169897600000.0),
        -62167219200000.0,
        "negative-year helper rollover",
    );
    let month = AnyTimeframe::Variant2(ExtendedTimeframe::MN1);
    equal(
        calendar::next(&month, -62169897600.0),
        -62167219200.0,
        "negative-year typed next",
    );
    let mut live = calendar::create_live_candle_start("1M");
    equal(
        live(-62169897600.0),
        -62169897600.0,
        "negative-year live December",
    );
    equal(
        live(-62167219200.0),
        -2208988800.0,
        "negative-year live January remap",
    );
    source(
        "negativeYearRollover",
        &[-62167219200000.0, -62169897600.0, -2208988800.0],
    );
}
