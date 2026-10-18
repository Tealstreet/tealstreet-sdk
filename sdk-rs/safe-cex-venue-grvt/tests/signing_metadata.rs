#![cfg(not(target_arch = "wasm32"))]
#[path = "../../safe-cex-num/tests/support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_venue_grvt::signing_metadata::get_instrument_signing_metadata;
#[test]
fn signing_metadata_matches_actual_source_native_fields() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_metadata.cjs"
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
    source_cardinality::for_each(
        corpus["refusals"].as_array().unwrap(),
        2,
        "grvt-metadata-refusals",
        |witness| {
            assert!(
                serde_json::from_str::<serde_json::Value>(witness["raw"].as_str().unwrap())
                    .is_err(),
                "SIGNING METADATA UTF8 refusal"
            );
            assert_eq!(witness["name"], "Error");
        },
    );
    assert_eq!(rows.len(), 3204, "signing metadata corpus count");
    for row in rows {
        let market: serde_json::Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
        let result = get_instrument_signing_metadata(
            market["id"].as_str().unwrap(),
            market.get("info").cloned().map(Into::into).as_ref(),
        );
        if let Some(error) = row.get("error") {
            assert_eq!(
                result.expect_err("SIGNING METADATA refused source"),
                error["message"].as_str().unwrap(),
                "SIGNING METADATA error identity {row}"
            );
            assert_eq!(error["name"], "Error");
        } else {
            let (hash, base, quote) = result.expect("SIGNING METADATA accepted source");
            assert_eq!(
                hash,
                row["hash"].as_str().unwrap(),
                "SIGNING METADATA hash identity"
            );
            assert_eq!(
                format!("{:016x}", base.to_bits()),
                row["base"],
                "SIGNING METADATA base identity {row}"
            );
            assert_eq!(
                format!("{:016x}", quote.to_bits()),
                row["quote"],
                "SIGNING METADATA quote identity {row}"
            );
        }
    }
}

#[test]
fn negative_exponent_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "signing_metadata_matches_actual_source_native_fields",
            "--nocapture",
        ])
        .env("GRVT_METADATA_CONTROL", "nonnegative")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "source control must fail");
    assert!(
        text.contains("SIGNING METADATA"),
        "named source control: {text}"
    );
}
