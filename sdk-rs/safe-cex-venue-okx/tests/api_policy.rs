use safe_cex_venue_okx::api::is_public_endpoint;
use serde_json::Value;

#[test]
#[ignore = "requires Node and the isolated TypeScript compiler; mandatory in CI"]
fn maintained_typescript_public_endpoint_policy() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api-policy-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the live TypeScript oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        assert_eq!(
            is_public_endpoint(case["url"].as_str()),
            case["expected"].as_bool().unwrap(),
            "{case}"
        );
    }
}
