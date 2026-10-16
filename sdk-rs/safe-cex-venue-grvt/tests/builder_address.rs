#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::builder::{get_string_field, get_valid_address};
use serde_json::Value;

#[test]
fn builder_address_matches_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-address.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let addresses = corpus["addresses"].as_array().unwrap();
    assert_eq!(addresses.len(), 1343, "BUILDER-ADDRESS source total");
    let mut compared = 0;
    for row in addresses {
        let values: Vec<_> = row["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .collect();
        assert_eq!(
            get_valid_address(&values),
            row["expected"].as_str(),
            "BUILDER-ADDRESS native first address: {:?}",
            row["input"]
        );
        compared += 1;
    }
    assert_eq!(compared, 1343, "BUILDER-ADDRESS compared total");
    let fields = corpus["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 13, "BUILDER-FIELD source total");
    let mut compared = 0;
    for row in fields {
        let value = row["present"].as_bool().unwrap().then_some(&row["input"]);
        assert_eq!(
            get_string_field(value),
            row["expected"].as_str(),
            "BUILDER-FIELD native string identity: {row}"
        );
        compared += 1;
    }
    assert_eq!(compared, 13, "BUILDER-FIELD compared total");
    let surrogates = corpus["surrogates"].as_array().unwrap();
    assert_eq!(surrogates.len(), 3, "BUILDER-FIELD UTF16 count");
    for (row, units) in surrogates
        .iter()
        .zip([vec![0xd800], vec![0xdfff], vec![97, 0xd800, 98]])
    {
        assert_eq!(
            row["units"],
            serde_json::json!(units),
            "BUILDER-FIELD retained source units"
        );
        assert!(
            row["addressOnly"].is_null(),
            "BUILDER-ADDRESS surrogate rejected by source"
        );
        assert_eq!(
            row["beforeValid"], "0x52908400098527886E0F7030069857D2E4169EE7",
            "BUILDER-ADDRESS source skips surrogate candidate"
        );
        assert!(
            serde_json::from_str::<Value>(row["raw"].as_str().unwrap()).is_err(),
            "BUILDER-FIELD UTF8 boundary"
        );
    }
}

#[test]
fn source_last_address_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_address_matches_actual_source",
            "--nocapture",
        ])
        .env("GRVT_ADDRESS_MUTATION", "last")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("BUILDER-ADDRESS native first address"),
        "{log}"
    );
}
