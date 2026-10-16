#![cfg(not(target_arch = "wasm32"))]

use safe_cex_venue_grvt::protocol::{
    BASE_URLS, ENDPOINTS, GRAFANA_ENDPOINTS_EVERY, GRAFANA_ENDPOINTS_PERIODIC, WSS_URLS,
};

#[test]
fn protocol_tables_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/protocol.cjs"))
        .output()
        .expect("run live TypeScript protocol oracle");
    assert!(
        output.status.success(),
        "protocol oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout).expect("UTF-8 protocol output");
    let mut actual = String::new();
    for (name, entries) in [
        ("BASE_URLS", BASE_URLS),
        ("WSS_URLS", WSS_URLS),
        ("ENDPOINTS", ENDPOINTS),
    ] {
        for (key, value) in entries {
            actual.push_str(&format!("{name}\t{key}\t{value}\n"));
        }
    }
    for (name, entries) in [
        ("GRAFANA_ENDPOINTS_EVERY", GRAFANA_ENDPOINTS_EVERY),
        ("GRAFANA_ENDPOINTS_PERIODIC", GRAFANA_ENDPOINTS_PERIODIC),
    ] {
        for (index, value) in entries.iter().enumerate() {
            actual.push_str(&format!("{name}\t{index}\t{value}\n"));
        }
    }
    assert_eq!(expected.lines().count(), 44, "protocol entry census");
    assert_eq!(actual, expected, "GRVT protocol table equivalence");
}
