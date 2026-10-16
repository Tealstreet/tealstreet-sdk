use safe_cex_venue_bitunix::websocket_public::{listen_ohlcv, listen_order_book};
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn actual_typescript_public_listener_errors() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/public-listener-errors-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PUBLIC listeners actual source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 6, "PUBLIC listener independent floor");
    for row in rows {
        let native = if row["name"] == "listenOHLCV" {
            listen_ohlcv::<()>()
        } else {
            listen_order_book::<()>()
        };
        let error = native.expect_err("PUBLIC listener must reject");
        assert_eq!(
            json!({"name":error.name,"message":error.message}),
            row["error"],
            "PUBLIC listener native rejection {}",
            row["name"]
        );
    }
}
