#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::{
    builder::GrvtBuilderUnauthorizedError, types::GrvtBuilderUnauthorizedReason,
};
fn number(value: &serde_json::Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
#[test]
fn builder_error_matches_actual_class_fields_message_and_string() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-error.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "builder error oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1632, "builder error corpus census");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["hasStack"], true, "source host stack witness");
        assert_eq!(
            row["errorPrototype"], "Error",
            "source Error prototype witness"
        );
        let reason: GrvtBuilderUnauthorizedReason =
            serde_json::from_value(row["reason"].clone()).unwrap();
        let actual = GrvtBuilderUnauthorizedError::new(
            reason,
            number(&row["required"]),
            row["authorized"]
                .as_str()
                .map(|_| number(&row["authorized"])),
            row["inputMessage"].as_str(),
        );
        assert_eq!(actual.name, row["name"].as_str().unwrap());
        assert_eq!(actual.code, row["code"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(actual.reason).unwrap(),
            row["reasonOut"]
        );
        assert_eq!(
            actual.required_raw.to_bits(),
            number(&row["requiredOut"]).to_bits(),
            "GRVT error required bits case {index}"
        );
        assert_eq!(
            actual.authorized_raw.map(f64::to_bits),
            row["authorizedOut"]
                .as_str()
                .map(|_| number(&row["authorizedOut"]).to_bits()),
            "GRVT error authorized bits case {index}"
        );
        assert_eq!(
            row["ownAuthorized"], true,
            "undefined is still an own source field"
        );
        assert_eq!(
            actual.message,
            row["message"].as_str().unwrap(),
            "GRVT error nullish cap message case {index}"
        );
        assert_eq!(
            actual.to_string(),
            row["string"].as_str().unwrap(),
            "GRVT error string rendering case {index}"
        );
    }
    let refused = corpus["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 3);
    for row in refused {
        assert!(
            serde_json::from_str::<String>(row["wire"].as_str().unwrap()).is_err(),
            "builder error UTF8 refusal"
        );
        assert!(
            row["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| (0xd800..=0xdfff).contains(&v.as_u64().unwrap())),
            "source surrogate message retained"
        );
    }
}
#[test]
fn source_nullish_cap_mutation_fails_named_assertion() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_error_matches_actual_class_fields_message_and_string",
            "--nocapture",
        ])
        .env("GRVT_BUILDER_ERROR_MUTATION", "nullish")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("GRVT error nullish cap message"),
        "source nullish mutation fails named message assertion"
    );
}
