#![cfg(not(target_arch = "wasm32"))]

/// A pending-row source witness, not a Rust implementation of fetchBalance.
#[test]
fn actual_fetch_balance_inherited_referent_witness() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/balance-prototype.cjs"
        ))
        .output()
        .expect("run actual fetchBalance witness");
    assert!(
        output.status.success(),
        "GRVT inherited referent witness: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 13, "GRVT inherited referent witness census");
    let prototype = rows.iter().find(|row| row["key"] == "__proto__").unwrap();
    assert_eq!(
        prototype["prototypeOwnFree"], true,
        "GRVT global prototype free witness"
    );
    assert_eq!(
        prototype["emptyObjectFreeNaN"], true,
        "GRVT unrelated object observes pollution"
    );
}
