#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::builder::get_eip712_domain;
use serde_json::Value;

#[test]
fn builder_domain_matches_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/builder-domain.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 2, "BUILDER-DOMAIN source count");
    let mut compared = 0;
    for (index, testnet) in [false, true].into_iter().enumerate() {
        assert_eq!(
            rows[index]["testnet"], testnet,
            "BUILDER-DOMAIN source network"
        );
        let actual = get_eip712_domain(testnet);
        let object = actual.as_object().unwrap();
        assert_eq!(object.len(), 3, "BUILDER-DOMAIN native field count");
        assert_eq!(
            serde_json::json!(object.keys().collect::<Vec<_>>()),
            rows[index]["keys"],
            "BUILDER-DOMAIN key order"
        );
        for key in ["name", "version", "chainId"] {
            assert_eq!(
                actual[key], rows[index]["value"][key],
                "BUILDER-DOMAIN native {key}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 6, "BUILDER-DOMAIN compared fields");
}

#[test]
fn domain_source_network_mutation_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "builder_domain_matches_actual_source",
            "--nocapture",
        ])
        .env("GRVT_DOMAIN_MUTATION", "network")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(log.contains("BUILDER-DOMAIN native chainId"), "{log}");
}
