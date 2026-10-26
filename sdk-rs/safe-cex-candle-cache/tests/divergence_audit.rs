use safe_cex_candle_cache::calendar;
use safe_cex_types::{AnyTimeframe, ExtendedTimeframe};
use serde_json::Value;

#[test]
#[ignore = "requires root TypeScript dependencies"]
fn narrowed_calendar_matches_source_date_behavior() {
    let month = AnyTimeframe::Variant2(ExtendedTimeframe::MN1);
    let year_one = -62135596800.0;
    let year_hundred = -59011459200.0;
    let native = [
        (
            "start-year-one",
            calendar::start(&month, year_one),
            -2177452800.0,
        ),
        (
            "next-year-one",
            calendar::next(&month, year_one),
            -2174774400.0,
        ),
        (
            "close-year-one",
            calendar::close(&month, year_one),
            -2174774400.0,
        ),
        (
            "count-year-one-to-hundred",
            calendar::count(&month, year_one, year_hundred),
            -21611.0,
        ),
        ("start-NaN", calendar::start(&month, f64::NAN), f64::NAN),
        ("next-NaN", calendar::next(&month, f64::NAN), f64::NAN),
        (
            "count-NaN",
            calendar::count(&month, f64::NAN, 0.0),
            f64::NAN,
        ),
        ("start-epoch", calendar::start(&month, 0.0), 0.0),
        ("next-epoch", calendar::next(&month, 0.0), 2678400.0),
        ("close-epoch", calendar::close(&month, 0.0), 2678400.0),
        ("count-epoch", calendar::count(&month, 0.0, 2678400.0), 2.0),
    ];
    for (name, actual, expected) in &native {
        assert!(
            actual.is_nan() && expected.is_nan() || actual.to_bits() == f64::to_bits(*expected),
            "CORE-CALENDAR native value {name}"
        );
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence_audit_oracle.cjs"
        ))
        .output()
        .expect("CORE-CALENDAR source process");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> =
        serde_json::from_slice(&output.stdout).expect("CORE-CALENDAR source capture");
    assert_eq!(source.len(), 11, "CORE-CALENDAR independent native floor");
    let source_values = [
        -2177452800.0,
        -2174774400.0,
        -2174774400.0,
        -21611.0,
        f64::NAN,
        f64::NAN,
        f64::NAN,
        0.0,
        2678400.0,
        2678400.0,
        2.0,
    ];
    for (index, ((name, actual, _), row)) in native.iter().zip(&source).enumerate() {
        assert_eq!(
            row["name"].as_str(),
            Some(*name),
            "CORE-CALENDAR ordered identity"
        );
        let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        let expected = source_values[index];
        assert!(
            expected.is_nan() && f64::from_bits(bits).is_nan() || expected.to_bits() == bits,
            "CORE-CALENDAR source value {name}"
        );
        assert!(
            actual.is_nan() && f64::from_bits(bits).is_nan() || actual.to_bits() == bits,
            "CORE-CALENDAR repaired source parity {name}"
        );
    }
}
