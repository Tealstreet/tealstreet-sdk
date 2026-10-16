#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::builder::*;

#[test]
fn builder_constants_match_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-protocol.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "builder oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<(String, String)> = serde_json::from_slice(&output.stdout).unwrap();
    let actual = [
        (
            "TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET",
            TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET,
        ),
        (
            "TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET",
            TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET,
        ),
        (
            "TEALSTREET_GRVT_BUILDER_ADDRESS",
            TEALSTREET_GRVT_BUILDER_ADDRESS,
        ),
        (
            "DEFAULT_GRVT_BUILDER_FEE_RATE",
            DEFAULT_GRVT_BUILDER_FEE_RATE,
        ),
    ];
    assert_eq!(rows.len(), actual.len(), "builder constant census");
    for ((name, value), (expected_name, expected_value)) in actual.into_iter().zip(rows) {
        assert_eq!(name, expected_name);
        assert_eq!(value, expected_value, "GRVT builder constant {name}");
    }
}

#[test]
fn actual_builder_address_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_constants_match_actual_typescript",
            "--nocapture",
        ])
        .env("GRVT_BUILDER_PROTOCOL_MUTATION", "address")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("GRVT builder constant TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET"),
        "builder address mutation: {log}"
    );
}
