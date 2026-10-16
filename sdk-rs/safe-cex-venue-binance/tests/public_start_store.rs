use serde_json::Value;

fn oracle(args: &[&str]) -> std::process::Output {
    std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-start-store.mjs"
        ))
        .args(args)
        .output()
        .expect("Node production Store startup witness")
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_start_store_listener_finding() {
    let output = oracle(&[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 12);
    for chunk in cases.as_chunks::<4>().0 {
        assert_eq!(chunk[0]["rejected"], false);
        assert_eq!(chunk[0]["trace"].as_array().unwrap().len(), 3);
        assert_eq!(chunk[1]["rejected"], true);
        assert_eq!(chunk[1]["trace"].as_array().unwrap().len(), 2);
        assert_eq!(chunk[1]["total"], 0);
        assert_eq!(chunk[1]["loaded"]["orders"], true);
        assert_eq!(chunk[2]["total"], 3);
        assert_eq!(chunk[3]["rejected"], false);
    }
    // The native public_start test now pins this shared-contract witness:
    // committed state, skipped later subscribers, and retained error identity.
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_start_store_witness_controls() {
    for (arg, assertion) in [
        (
            "--bypass-store-write",
            "startup commits before notification and skips later subscribers after throw",
        ),
        ("--missing-runtime-import", "MODULE_NOT_FOUND"),
    ] {
        let output = oracle(&[arg]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "control {arg} survived");
        assert!(stderr.contains(assertion), "control {arg}: {stderr}");
    }
}
