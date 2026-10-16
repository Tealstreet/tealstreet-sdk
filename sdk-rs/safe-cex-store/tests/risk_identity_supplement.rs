use safe_cex_store::risk::calculate_orders_risk;
use safe_cex_types::{Market, Order, Position, Ticker};
use serde_json::Value;

#[test]
fn actual_source_cumulative_risk_fields_have_distinct_native_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/risk_identity_supplement.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let input = &source["input"];
    let markets: Vec<Market> = serde_json::from_value(input["markets"].clone()).unwrap();
    let tickers: Vec<Ticker> = serde_json::from_value(input["tickers"].clone()).unwrap();
    let positions: Vec<Position> = serde_json::from_value(input["positions"].clone()).unwrap();
    let orders: Vec<Order> = serde_json::from_value(input["orders"].clone()).unwrap();
    let actual = calculate_orders_risk(&markets, &tickers, &positions, &orders, None).unwrap();
    let expected = source["risk"].as_object().unwrap();
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "RISK-IDENTITY complete source key set"
    );
    for (id, profiles) in &actual {
        assert_eq!(
            profiles.len(),
            expected[id].as_array().unwrap().len(),
            "RISK-IDENTITY complete profile count for {id}"
        );
        for (index, profile) in profiles.iter().enumerate() {
            for (field, value) in [
                ("orderPnl", profile.order_pnl),
                ("totalRealizedPnl", profile.total_realized_pnl),
                ("rollingUnrealizedPnl", profile.rolling_unrealized_pnl),
                ("positionSize", profile.position_size),
            ] {
                assert_eq!(
                    value.to_bits(),
                    source["risk"][id][index][field].as_f64().unwrap().to_bits(),
                    "RISK-IDENTITY native {field} for {id}"
                );
            }
        }
    }
}
