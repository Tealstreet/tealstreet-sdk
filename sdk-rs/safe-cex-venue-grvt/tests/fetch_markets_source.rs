#![cfg(not(target_arch = "wasm32"))]
use serde_json::Value;

const INSTRUMENT: &str = r#"{"i":"btc_perp","b":"BTC","q":"USDT","k":"PERPETUAL","ms":"0.013","ts":"0.2","mp":"27.5","bd":-0,"qd":1e400}"#;

#[test]
fn whole_axios_markets_source_witness() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch-markets-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MARKETS source oracle execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture.get("rows").and_then(Value::as_array).unwrap();
    assert_eq!(rows.len(), 11, "MARKETS source total");
    let one = format!("{{\"r\":[{INSTRUMENT}]}}");
    let quoted = serde_json::to_string(&one).unwrap();
    let inputs: [(&str, String); 11] = [
        ("missing", "{}".into()),
        ("null", "null".into()),
        ("empty", r#"{"r":[]}"#.into()),
        ("quoted", quoted),
        ("one", one.clone()),
        (
            "two",
            format!(
                r#"{{"r":[{INSTRUMENT},{{"i":"eth_perp","b":"ETH","q":"USD","k":"FUTURE","ms":"0.07","ts":"1.5","ia":false,"mxs":"18.25"}}]}}"#
            ),
        ),
        (
            "later-null",
            r#"{"r":[{"i":"x","b":{},"q":"USD"},null]}"#.into(),
        ),
        ("non-array", r#"{"r":{}}"#.into()),
        (
            "defaults",
            r#"{"r":[{"i":"zero","b":"B","q":"Q","ms":0,"ts":false,"ia":0,"mp":0,"mxs":"19"}]}"#
                .into(),
        ),
        ("success-log-throws", one.clone()),
        ("catch-log-throws", one),
    ];
    let mut compared = 0;
    for (index, (name, raw)) in inputs.into_iter().enumerate() {
        let row = &rows[index];
        assert_eq!(
            row.get("name").and_then(Value::as_str),
            Some(name),
            "MARKETS source input order"
        );
        assert_eq!(
            row.get("raw").and_then(Value::as_str),
            Some(raw.as_str()),
            "MARKETS source input content"
        );
        assert!(row.get("error").is_some(), "MARKETS explicit error outcome");
        assert!(
            row.get("result").is_some(),
            "MARKETS explicit result outcome"
        );
        compared += 1;
    }
    assert_eq!(compared, 11, "MARKETS compared source inputs");
    assert_eq!(
        rows[4]["result"][0]["id"], "btc_perp",
        "MARKETS captured native id"
    );
    assert_eq!(
        rows[4]["result"][0]["base"], "BTC",
        "MARKETS captured native base"
    );
    assert_eq!(
        rows[4]["result"][0]["quote"], "USDT",
        "MARKETS captured native quote"
    );
    assert_eq!(
        rows[5]["result"][1]["id"], "eth_perp",
        "MARKETS captured second id"
    );
}

#[test]
fn source_witness_controls_fail_by_name() {
    let controls: [(&str, &str, &str); 7] = [
        (
            "GRVT_MARKETS_SOURCE_CONTROL",
            "double-parse",
            "MARKETS single Axios transform",
        ),
        (
            "GRVT_MARKETS_SOURCE_CONTROL",
            "swapped-steps",
            "MARKETS distinct native steps",
        ),
        (
            "GRVT_MARKETS_SOURCE_CONTROL",
            "skip-success-log",
            "MARKETS success-log-throws rejected outcome",
        ),
        (
            "GRVT_MARKETS_CAPTURE_CONTROL",
            "substitute-symbol",
            "MARKETS source input content",
        ),
        (
            "GRVT_MARKETS_CAPTURE_CONTROL",
            "reverse",
            "MARKETS source input order",
        ),
        (
            "GRVT_MARKETS_SOURCE_CONTROL",
            "non-array-fallback",
            "MARKETS non-array error",
        ),
        (
            "GRVT_MARKETS_SOURCE_CONTROL",
            "missing-log",
            "MARKETS missing effects",
        ),
    ];
    for (variable, value, assertion) in controls {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "whole_axios_markets_source_witness",
                "--nocapture",
            ])
            .env(variable, value)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(101), "{value}");
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(log.contains(assertion), "{value}: {log}");
    }
}
