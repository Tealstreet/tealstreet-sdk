use safe_cex_candle_cache::calendar::*;
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    match v.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        v => v.parse().unwrap(),
    }
}
fn equal_number(actual: f64, expected: &Value, row: &Value) {
    let expected = number(expected);
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "{row}: actual {actual:?}"
    );
}
#[test]
#[ignore = "requires root TypeScript dependencies"]
fn current_source_calendar_utilities() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_calendar_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 2988, "CALENDAR-CARDINALITY source total");
    let mut operations = std::collections::BTreeMap::new();
    for row in &rows {
        *operations.entry(row["op"].as_str().unwrap()).or_insert(0) += 1;
    }
    assert_eq!(
        operations,
        std::collections::BTreeMap::from([
            ("calendarPeriodStartMs", 40),
            ("nextCalendarPeriodStartMs", 40),
            ("calendarPeriodForChartSeconds", 22),
            ("getCandleStartTime", 860),
            ("getCandleEndTime", 860),
            ("createLiveCandleStart", 860),
            ("getNumCandlesInRange", 258),
            ("splitTimeRange", 48),
        ]),
        "CALENDAR-CARDINALITY source operation counts"
    );
    let ranges = [
        ("0", "0"),
        ("0", "10"),
        ("5", "1"),
        ("-5", "12"),
        ("Infinity", "10"),
        ("0", "NaN"),
    ];
    let spans = ["-1", "0", "0.5", "1", "3", "10", "NaN", "Infinity"];
    let window_counts = [
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 7, 6, 3, 1, 1, 1],
        [0; 8],
        [1, 1, 12, 9, 5, 2, 1, 1],
        [0; 8],
        [0; 8],
    ];
    let mut split_cases = std::collections::BTreeMap::new();
    let mut live = None;
    let mut last_interval = String::new();
    let mut scalar_comparisons = 0;
    for row in &rows {
        let op = row["op"].as_str().unwrap();
        let interval = row["interval"].as_str().unwrap_or("");
        let expected = &row["expected"];
        match op {
            "calendarPeriodStartMs" | "nextCalendarPeriodStartMs" => {
                let period = serde_json::from_value(row["period"].clone()).unwrap();
                let time = number(&row["time"]);
                equal_number(
                    if op == "calendarPeriodStartMs" {
                        calendar_period_start_ms(period, time)
                    } else {
                        next_calendar_period_start_ms(period, time)
                    },
                    expected,
                    row,
                );
            }
            "calendarPeriodForChartSeconds" => assert_eq!(
                serde_json::to_value(calendar_period_for_chart_seconds(number(&row["time"])))
                    .unwrap(),
                *expected,
                "{row}"
            ),
            "getCandleStartTime" => equal_number(
                get_candle_start_time(interval, number(&row["time"])),
                expected,
                row,
            ),
            "getCandleEndTime" => equal_number(
                get_candle_end_time(interval, number(&row["time"])),
                expected,
                row,
            ),
            "createLiveCandleStart" => {
                if live.is_none() || last_interval != interval {
                    live = Some(create_live_candle_start(interval));
                    last_interval = interval.to_owned();
                }
                equal_number(live.as_mut().unwrap()(number(&row["time"])), expected, row);
            }
            "getNumCandlesInRange" => {
                match get_num_candles_in_range(interval, number(&row["from"]), number(&row["to"])) {
                    Ok(n) => equal_number(n, &expected["ok"], row),
                    Err(e) => assert_eq!(json!({"error":e}), *expected, "{row}"),
                }
            }
            "splitTimeRange" => {
                let range = ranges
                    .iter()
                    .position(|(from, to)| row["from"] == *from && row["to"] == *to)
                    .expect("CALENDAR-CARDINALITY known range");
                let span = spans
                    .iter()
                    .position(|span| row["span"] == *span)
                    .expect("CALENDAR-CARDINALITY known span");
                assert!(
                    split_cases.insert((range, span), row).is_none(),
                    "CALENDAR-CARDINALITY unique split case"
                );
                continue;
            }
            _ => unreachable!(),
        }
        scalar_comparisons += 1;
    }
    assert_eq!(
        scalar_comparisons, 2940,
        "CALENDAR-CARDINALITY scalar comparisons"
    );
    let mut native_windows = 0;
    let mut field_comparisons = 0;
    for (range, (from, to)) in ranges.iter().enumerate() {
        for (span, text) in spans.iter().enumerate() {
            let row = split_cases
                .get(&(range, span))
                .expect("CALENDAR-CARDINALITY every split input");
            let count = window_counts[range][span];
            let expected = row["expected"].as_array().unwrap();
            assert_eq!(
                expected.len(),
                count,
                "CALENDAR-CARDINALITY source windows per case"
            );
            let actual = split_time_range(
                number(&Value::String((*from).into())),
                number(&Value::String((*to).into())),
                number(&Value::String((*text).into())),
            );
            assert_eq!(
                actual.len(),
                count,
                "CALENDAR-CARDINALITY independent native windows"
            );
            assert_eq!(
                actual.len(),
                expected.len(),
                "CALENDAR-CARDINALITY expected/native windows"
            );
            native_windows += actual.len();
            for index in 0..count {
                for key in ["startTime", "endTime"] {
                    assert_eq!(
                        actual[index][key].as_f64().unwrap().to_bits(),
                        expected[index][key].as_f64().unwrap().to_bits(),
                        "{row}"
                    );
                    field_comparisons += 1;
                }
            }
        }
    }
    assert_eq!(
        native_windows, 61,
        "CALENDAR-CARDINALITY native total windows"
    );
    assert_eq!(
        field_comparisons, 122,
        "CALENDAR-CARDINALITY native field comparisons"
    );
}
