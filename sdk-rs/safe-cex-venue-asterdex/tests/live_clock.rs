use safe_cex_venue_asterdex::clock_helpers::*;
use serde_json::Value;
fn number(value: &Value) -> f64 {
    match value["number"].as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => value.as_f64().unwrap(),
    }
}
fn same(actual: f64, expected: &Value, symbol: &str, case: &Value) {
    let expected = number(expected);
    assert!(
        actual.to_bits() == expected.to_bits() || actual.is_nan() && expected.is_nan(),
        "{symbol}: {case}: actual {actual:?}, expected {expected:?}"
    );
}
#[test]
#[ignore = "requires live isolated TypeScript and Dayjs oracle"]
fn clock_snapshot_calculations_match_dayjs() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/clock-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 7600);
    for c in cases {
        let server = (!c["server"].is_null()).then(|| number(&c["server"]));
        let client = (!c["client"].is_null()).then(|| number(&c["client"]));
        let now = number(&c["now"]);
        same(
            get_current_time(server, client, now),
            &c["current"],
            "getCurrentTime",
            &c,
        );
        same(
            get_current_time_ms(server, client, now),
            &c["ms"],
            "getCurrentTimeMs",
            &c,
        );
        same(
            get_time_offset(server, client),
            &c["offset"],
            "getTimeOffset",
            &c,
        );
        assert_eq!(
            needs_sync(server, client, now),
            c["needs"].as_bool().unwrap(),
            "needsSync: {c}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript and Dayjs oracle"]
fn clock_oracle_requires_real_dayjs() {
    let output = std::process::Command::new("node")
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/clock-oracle.cjs"),
            "--missing-package-control",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MODULE_NOT_FOUND") && stderr.contains("asterdex-missing-install-control"),
        "{stderr}"
    );
}
