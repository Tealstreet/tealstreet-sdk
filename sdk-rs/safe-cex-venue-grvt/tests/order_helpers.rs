#![cfg(not(target_arch = "wasm32"))]
use safe_cex_types::OrderStatus;
use safe_cex_venue_grvt::order_semantics::{get_order_creation_time_ms, map_order_status};
use serde_json::Value;

fn corpus() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-helpers.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ORDER HELPERS oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn order_status_native_identity() {
    let corpus = corpus();
    let rows = corpus["statusRows"].as_array().unwrap();
    assert_eq!(rows.len(), 47, "ORDER STATUS independent source count");
    let mut compared = 0;
    for row in rows {
        let expected = match row["expected"].as_str().unwrap() {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            other => panic!("unexpected source status {other}"),
        };
        let input = row["present"].as_bool().unwrap().then_some(&row["input"]);
        assert_eq!(
            map_order_status(input),
            expected,
            "ORDER STATUS native identity {row}"
        );
        compared += 1;
    }
    assert_eq!(compared, 47, "ORDER STATUS completed floor");
}

#[test]
fn creation_time_native_bits() {
    let corpus = corpus();
    let rows = corpus["creationRows"].as_array().unwrap();
    assert_eq!(rows.len(), 1450, "CREATION TIME independent source count");
    let mut compared = 0;
    for row in rows {
        let metadata: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
        let expected = row["expected"]
            .as_str()
            .map(|bits| u64::from_str_radix(bits, 16).unwrap());
        assert_eq!(
            get_order_creation_time_ms(Some(&metadata)).map(f64::to_bits),
            expected,
            "CREATION TIME native bits {row}"
        );
        compared += 1;
    }
    assert_eq!(compared, 1450, "CREATION TIME completed floor");
    assert_eq!(
        get_order_creation_time_ms(None),
        None,
        "CREATION TIME missing metadata"
    );
}

#[test]
fn order_helper_utf8_boundary() {
    let corpus = corpus();
    let rows = corpus["refusals"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "ORDER HELPERS surrogate floor");
    let mut compared = 0;
    for row in rows {
        assert_eq!(
            row["status"], "open",
            "ORDER HELPERS source surrogate status"
        );
        assert!(
            row["creation"].is_null(),
            "ORDER HELPERS source surrogate timestamp"
        );
        assert!(
            serde_json::from_str::<String>(row["raw"].as_str().unwrap()).is_err(),
            "ORDER HELPERS shared UTF8 refusal"
        );
        compared += 1;
    }
    assert_eq!(compared, 3, "ORDER HELPERS completed surrogate floor");
}
