use serde_json::Value;

fn oracle(args: &[&str]) -> std::process::Output {
    std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-store.mjs"
        ))
        .args(args)
        .output()
        .expect("Node production Store position-mode witness")
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_position_mode_store_listener_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(cases[2]["trace"][2][1], "subscriber failed");
    assert_eq!(cases[2]["isHedged"], true);
    assert_eq!(cases[2]["rejected"], false);
    assert_eq!(cases[3]["rejected"], true);
    // The native PM setter now compares all eight source cases, including
    // caught notification failures and emitter-only rejection.
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_position_mode_store_witness_controls() {
    for (arg, assertion) in [
        (
            "--bypass-store-write",
            "PM Store subscriber throw is caught after mutation and aborts later listeners",
        ),
        ("--missing-runtime-import", "MODULE_NOT_FOUND"),
    ] {
        let output = oracle(&[arg]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "control {arg} survived");
        assert!(stderr.contains(assertion), "control {arg}: {stderr}");
    }
}
