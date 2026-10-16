use safe_cex_venue_backpack::constants::*;
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn actual_typescript_tables_match_per_key_and_order() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/constants_oracle.cjs"
        ))
        .output()
        .expect("execute actual TypeScript constants oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).expect("oracle JSON");
    for key in ["node", "arch", "platform"] {
        assert!(
            !corpus["runtime"][key]
                .as_str()
                .expect("runtime metadata")
                .is_empty()
        );
    }
    eprintln!("TypeScript oracle runtime: {}", corpus["runtime"]);
    let actual = &corpus["tables"];
    let rust = json!({
        "BASE_URLS": BASE_URLS,
        "WSS_URLS": WSS_URLS,
        "ENDPOINTS": ENDPOINTS,
        "INSTRUCTIONS": INSTRUCTIONS,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
    });
    assert_eq!(actual.as_object().unwrap().len(), 6, "export census");
    assert_eq!(
        rust.as_object().unwrap().len(),
        6,
        "TABLE-CARDINALITY native tables"
    );
    for (name, count) in [
        ("BASE_URLS", 2),
        ("WSS_URLS", 2),
        ("ENDPOINTS", 25),
        ("INSTRUCTIONS", 16),
        ("GRAFANA_ENDPOINTS_EVERY", 2),
        ("GRAFANA_ENDPOINTS_PERIODIC", 2),
    ] {
        assert_eq!(
            actual[name].as_array().unwrap().len(),
            count,
            "TABLE-CARDINALITY source {name}"
        );
        assert_eq!(
            rust[name].as_array().unwrap().len(),
            actual[name].as_array().unwrap().len(),
            "TABLE-CARDINALITY native {name}"
        );
    }
    let mut compared = 0;
    for (name, table) in rust.as_object().unwrap() {
        assert_eq!(
            table, &actual[name],
            "source table {name}: per-key value and order"
        );
        compared += 1;
    }
    assert_eq!(compared, 6, "TABLE-CARDINALITY completed comparisons");
}
