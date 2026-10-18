use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::Store;
use safe_cex_store::{StoreUpdate, returns::calculate_returns};
use safe_cex_types::{Order, Position};
use serde_json::Value;
#[test]
fn batch_threshold_and_returns_truthiness_are_preserved_from_actual_source() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/bucket_witnesses.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        if row["kind"] == "batch" {
            let store = DefaultStore::new();
            let existing: Vec<Order> = serde_json::from_value(row["existing"].clone()).unwrap();
            let incoming: Vec<Order> = serde_json::from_value(row["incoming"].clone()).unwrap();
            store
                .update_full(
                    StoreUpdate {
                        orders: Some(existing),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            store.add_or_update_orders(incoming).unwrap();
            let prices: Vec<_> = store.orders().iter().map(|o| o.price).collect();
            let expected: Vec<f64> = serde_json::from_value(row["prices"].clone()).unwrap();
            assert_eq!(prices, expected, "BUCKET-WITNESS batch duplicate threshold");
        } else {
            let position: Position = serde_json::from_value(row["position"].clone()).unwrap();
            let bits = u64::from_str_radix(row["overrideBits"].as_str().unwrap(), 16).unwrap();
            let value = f64::from_bits(bits);
            assert_eq!(
                if value.is_nan() {
                    0x7ff8000000000000
                } else {
                    value.to_bits()
                },
                bits,
                "BUCKET-WITNESS override bits before serialization"
            );
            let result = calculate_returns(&position, Some(value));
            assert_eq!(
                format!("{:016x}", result.roe.to_bits()),
                row["roeBits"],
                "BUCKET-WITNESS native roe"
            );
            assert_eq!(
                format!("{:016x}", result.roi.to_bits()),
                row["roiBits"],
                "BUCKET-WITNESS native roi"
            );
        }
    }
}
