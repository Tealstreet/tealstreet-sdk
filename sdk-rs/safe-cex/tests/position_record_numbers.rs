use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::PositionRecord;
use serde_json::Value;
fn number(record: &PositionRecord, name: &str) -> f64 {
    match name {
        "openedAt" => record.opened_at.unwrap(),
        "closedAt" => record.closed_at.unwrap(),
        "closedSize" => record.closed_size,
        "maxSize" => record.max_size.unwrap(),
        "leverage" => record.leverage.unwrap(),
        "closedNotional" => record.closed_notional.unwrap(),
        "avgEntryPrice" => record.avg_entry_price,
        "avgExitPrice" => record.avg_exit_price.unwrap(),
        "realizedPnl" => record.realized_pnl,
        "fees" => record.fees,
        "funding" => record.funding,
        "closeCount" => record.close_count,
        _ => panic!("new source numeric field needs native observer: {name}"),
    }
}
#[test]
fn canonical_record_numeric_ingress_retains_source_bits_on_all_entrypoints() {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position_record_numbers_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 84);
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let field = row["field"].as_str().unwrap();
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        for record in [
            js_from_str::<PositionRecord>(raw).unwrap(),
            js_from_value::<PositionRecord>(value.clone()).unwrap(),
            js_from_value_ref::<PositionRecord>(&value).unwrap(),
        ] {
            assert_eq!(
                number(&record, field).to_bits(),
                expected,
                "canonical journal Number bits {field}: {raw}"
            );
        }
        if expected == (-0.0_f64).to_bits() {
            let mut native = value;
            native[field] = serde_json::Number::from_f64(-0.0).unwrap().into();
            let record: PositionRecord = js_from_value(native).unwrap();
            assert_eq!(
                number(&record, field).to_bits(),
                expected,
                "native Value negative zero {field}"
            );
        }
    }
}
