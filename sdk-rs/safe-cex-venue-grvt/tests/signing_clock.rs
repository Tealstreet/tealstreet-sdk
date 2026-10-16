#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::signing_clock::{generate_expiration, generate_nonce};
fn from_bits(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
#[test]
fn signing_clock_matches_actual_source_and_effects() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_clock.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 462, "clock corpus census");
    for row in rows {
        let now = from_bits(row["now"].as_str().unwrap());
        let hours = row["hours"].as_str().map(from_bits);
        let calls = std::cell::Cell::new(0);
        let clock = || {
            calls.set(calls.get() + 1);
            now
        };
        let nonce = generate_nonce(clock);
        let expected = from_bits(row["nonce"].as_str().unwrap());
        assert!(
            nonce.is_nan() && expected.is_nan() || nonce.to_bits() == expected.to_bits(),
            "SIGNING CLOCK nonce bits {row}"
        );
        assert_eq!(
            calls.get(),
            row["nonceCalls"].as_u64().unwrap(),
            "SIGNING CLOCK nonce reads"
        );
        calls.set(0);
        assert_eq!(
            generate_expiration(hours, clock),
            row["expiration"].as_str().unwrap(),
            "SIGNING CLOCK expiration text {row}"
        );
        assert_eq!(
            calls.get(),
            row["calls"].as_u64().unwrap(),
            "SIGNING CLOCK expiration reads"
        );
    }
}
#[test]
fn truncation_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "signing_clock_matches_actual_source_and_effects",
            "--nocapture",
        ])
        .env("GRVT_CLOCK_CONTROL", "truncated")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "source mutation must fail");
    assert!(
        text.contains("SIGNING CLOCK expiration text"),
        "named source control: {text}"
    );
}
