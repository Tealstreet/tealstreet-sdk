#![cfg(not(target_arch = "wasm32"))]
#[path = "../../safe-cex-num/tests/support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_venue_grvt::order_semantics::symbol_to_instrument_id;
#[test]
fn instrument_symbol_matches_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/instrument_symbol.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1128, "instrument symbol corpus count");
    for row in rows {
        assert_eq!(
            symbol_to_instrument_id(row["input"].as_str().unwrap()),
            row["expected"].as_str().unwrap(),
            "INSTRUMENT SYMBOL suffix identity {row}"
        );
    }
    source_cardinality::for_each(
        corpus["refusals"].as_array().unwrap(),
        2,
        "grvt-symbol-refusals",
        |witness| {
            assert!(
                serde_json::from_str::<String>(witness["raw"].as_str().unwrap()).is_err(),
                "instrument symbol UTF8 ingress refusal"
            );
            assert!(
                serde_json::from_str::<String>(witness["expected"].as_str().unwrap()).is_err(),
                "source retains observable lone surrogate"
            );
        },
    );
}
#[test]
fn unanchored_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "instrument_symbol_matches_actual_source",
            "--nocapture",
        ])
        .env("GRVT_SYMBOL_CONTROL", "unanchored")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "source control must fail");
    assert!(
        text.contains("INSTRUMENT SYMBOL suffix identity"),
        "named control: {text}"
    );
}
