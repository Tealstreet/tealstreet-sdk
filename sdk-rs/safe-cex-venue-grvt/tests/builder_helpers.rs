#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::builder::{get_grvt_builder_address, grvt_builder_fee_rate_to_raw};

#[test]
fn builder_helpers_match_actual_typescript_bits_and_errors() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-helpers.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GRVT builder helpers oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["addresses"].as_array().unwrap().len(),
        2,
        "network corpus census"
    );
    assert_eq!(
        corpus["refused"].as_array().unwrap().len(),
        3,
        "UTF8 refusal census"
    );
    for row in corpus["addresses"].as_array().unwrap() {
        assert_eq!(
            get_grvt_builder_address(row[0].as_bool().unwrap()),
            row[1].as_str().unwrap(),
            "GRVT builder network address"
        );
    }
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 10_113, "GRVT fee corpus census");
    for row in rows {
        let text = row["text"].as_str().unwrap();
        let actual = grvt_builder_fee_rate_to_raw(text);
        if let Some(bits) = row["bits"].as_str() {
            assert_eq!(
                actual.unwrap().to_bits(),
                u64::from_str_radix(bits, 16).unwrap(),
                "GRVT builder fee bits {text:?}"
            );
        } else {
            assert_eq!(row["error"]["name"], "Error");
            assert_eq!(
                actual.unwrap_err(),
                row["error"]["message"].as_str().unwrap(),
                "GRVT builder fee error {text:?}"
            );
        }
    }
    for row in corpus["refused"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(row["wire"].as_str().unwrap()).is_err(),
            "GRVT fee shared UTF8 representation refusal"
        );
        assert!(
            row.get("bits").is_some() || row["error"]["messageUnits"].is_array(),
            "source surrogate result retained"
        );
    }
    eprintln!(
        "GRVT builder helpers: {} fee cases, 2 addresses, 3 UTF8 refusal witnesses",
        rows.len()
    );
}

#[test]
fn actual_fee_rounding_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_helpers_match_actual_typescript_bits_and_errors",
            "--nocapture",
        ])
        .env("GRVT_BUILDER_HELPER_MUTATION", "round")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("GRVT builder fee bits"),
        "actual TS rounding mutation: {log}"
    );
}

#[test]
fn actual_network_selection_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_helpers_match_actual_typescript_bits_and_errors",
            "--nocapture",
        ])
        .env("GRVT_BUILDER_HELPER_MUTATION", "network")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("GRVT builder network address"),
        "actual TS network mutation: {log}"
    );
}
