#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{js_date_parse, js_date_parse_utf16, js_time_clip};
use serde_json::Value;
fn decode(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
fn same(actual: f64, expected: f64) -> bool {
    actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits()
}
#[test]
fn date_parser_and_time_clip_match_live_node() {
    for (timezone, offset) in [("UTC", 0.0), ("Etc/GMT-8", 28800000.0)] {
        let output = std::process::Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_date.cjs"))
            .env("TZ", timezone)
            .output()
            .unwrap();
        assert!(output.status.success());
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
        source_cardinality::for_each(
            rows["parse"].as_array().unwrap(),
            30161,
            "date-parse",
            |row| {
                let units: Vec<u16> = row["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_u64().unwrap() as u16)
                    .collect();
                let expected = decode(row["expected"].as_str().unwrap());
                let actual = js_date_parse_utf16(&units, |local| local - offset);
                assert!(
                    same(actual, expected),
                    "{timezone} {:?}: {actual} != {expected}",
                    String::from_utf16_lossy(&units)
                );
                if let Ok(input) = String::from_utf16(&units) {
                    assert!(
                        same(js_date_parse(&input, |local| local - offset), expected),
                        "str adapter {input:?}"
                    );
                }
            },
        );
        source_cardinality::for_each(
            rows["clip"].as_array().unwrap(),
            10013,
            "date-clip",
            |row| {
                let input = decode(row[0].as_str().unwrap());
                let expected = decode(row[1].as_str().unwrap());
                let actual = js_time_clip(input);
                assert!(
                    same(actual, expected),
                    "TimeClip {input:?}: {:016x} != {:016x}",
                    actual.to_bits(),
                    expected.to_bits()
                );
            },
        );
    }
}
#[test]
fn date_parser_calls_local_adapter_only_when_needed() {
    let calls = std::cell::Cell::new(0);
    let local = |ms| {
        calls.set(calls.get() + 1);
        assert_eq!(ms, 0.0);
        3600000.0
    };
    assert_eq!(js_date_parse("1970-01-01", local), 0.0);
    assert_eq!(js_date_parse("1970-01-01T00:00:00Z", local), 0.0);
    assert_eq!(calls.get(), 0);
    assert_eq!(js_date_parse("1970-01-01T00:00:00", local), 3600000.0);
    assert_eq!(calls.get(), 1);
    assert!(js_date_parse("invalid", local).is_nan());
    assert_eq!(calls.get(), 1);
    assert!(js_date_parse("+999999-01-01T00:00:00", local).is_nan());
    assert_eq!(calls.get(), 1);
    assert!(js_date_parse("1970-01-01T00:00:00", |_| f64::NAN).is_nan());
    assert!(js_date_parse("1970-01-01T00:00:00", |_| 8.64e15 + 1.0).is_nan());
}
