use safe_cex_venue_asterdex::exchange_helpers::map_timeframe;
use serde_json::Value;

#[test]
fn expanded_ohlcv_domain_preserves_source_one_hour_fallback() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ohlcv-timeframe-source.cjs"
        ))
        .output()
        .expect("execute maintained Aster timeframe mapper");
    assert!(
        output.status.success(),
        "OHLCV-ASTER source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(records.len(), 27, "OHLCV-ASTER complete source domain");
    for record in records {
        let interval = serde_json::from_value(record["interval"].clone()).unwrap();
        assert_eq!(
            map_timeframe(interval),
            record["result"].as_str().unwrap(),
            "OHLCV-ASTER source mapping {}",
            record["interval"]
        );
    }
}
