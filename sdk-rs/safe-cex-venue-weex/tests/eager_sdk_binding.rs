//! The public SDK call must reach the actual JS member before it returns a waiter.
#[allow(dead_code)]
#[path = "support/binding_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::Value;

#[test]
fn empty_market_mode_change_updates_the_store_in_its_call_prefix() {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/eager_sdk_source.cjs"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["prefix"], true);
    let host = support::harness(support::options(), |_| {});
    assert!(!host.store.options().is_hedged);
    let pending = host.exchange.change_position_mode(true, None);
    assert_eq!(
        host.store.options().is_hedged,
        source["prefix"].as_bool().unwrap(),
        "WEEX-SDK-PREFIX actual member executes before caller polling"
    );
    host.drive(pending).unwrap();
    assert_eq!(
        host.store.options().is_hedged,
        source["settled"].as_bool().unwrap()
    );
    assert_eq!(host.http.sent.lock().unwrap().len(), 0);
    host.exchange.dispose().unwrap();
}
