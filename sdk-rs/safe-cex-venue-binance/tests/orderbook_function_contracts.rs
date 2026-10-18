//! Keeps the accepted native depth observations tied to maintained TypeScript.
use serde_json::Value;

#[test]
fn orderbook_native_owner_source_cases_are_current() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/orderbook-function-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let contracts: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/contracts/safe-cex-venue-binance.json"
    )))
    .unwrap();
    assert_eq!(source.as_object().unwrap().len(), 4);
    let mut count = 0;
    for (name, expected) in [
        ("load_binance_order_book_snapshot", 36),
        ("get_buffered_binance_depth_updates", 25),
        ("apply_binance_depth_update", 36),
        ("depth_index", 26),
    ] {
        let cases = source[name].as_array().unwrap();
        assert_eq!(cases.len(), expected, "{name}");
        assert_eq!(
            source[name],
            contracts[format!("safe_cex_venue_binance::orderbook::{name}")]["cases"],
            "live maintained TypeScript orderbook source changed for {name}"
        );
        count += cases.len();
    }
    assert_eq!(count, 123);
}
