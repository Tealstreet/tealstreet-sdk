#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::builder::create_grvt_builder_info;

#[test]
fn builder_info_matches_actual_source_defaults_strings_and_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-info.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "builder info oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 326, "builder info corpus census");
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let actual = create_grvt_builder_info(
            input["testnet"].as_bool(),
            input["address"].as_str(),
            input["fee"].as_str(),
        );
        if let Some(bits) = row["bits"].as_str() {
            let actual = actual.unwrap();
            // Avoid dumping multi-megabyte scalar strings on a mismatch.
            assert!(
                actual.builder == row["builder"].as_str().unwrap(),
                "GRVT builder info lowercase/default address case {index}"
            );
            assert_eq!(
                actual.builder_fee,
                row["fee"].as_str().unwrap(),
                "GRVT builder info preserved REST fee case {index}"
            );
            assert_eq!(
                actual.builder_fee_int.to_bits(),
                u64::from_str_radix(bits, 16).unwrap(),
                "GRVT builder info signing bits case {index}"
            );
        } else {
            assert_eq!(row["error"]["name"], "Error");
            assert_eq!(
                actual.unwrap_err(),
                row["error"]["message"].as_str().unwrap(),
                "GRVT builder info exact error case {index}"
            );
        }
    }
    let refused = corpus["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 3);
    for row in refused {
        assert!(
            serde_json::from_str::<String>(row["wire"].as_str().unwrap()).is_err(),
            "GRVT builder info UTF8 refusal"
        );
        assert!(
            row["outputUnits"]
                .as_array()
                .unwrap()
                .iter()
                .any(|unit| (0xd800..=0xdfff).contains(&unit.as_u64().unwrap())),
            "source retains lone surrogate"
        );
    }
}

#[test]
fn source_lowercase_mutation_fails_named_assertion() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_info_matches_actual_source_defaults_strings_and_bits",
            "--nocapture",
        ])
        .env("GRVT_BUILDER_INFO_MUTATION", "lowercase")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("GRVT builder info lowercase/default address"),
        "source lowercase control must fail its named assertion"
    );
}
