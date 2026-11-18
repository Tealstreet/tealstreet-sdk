pub use safe_cex_types::{ExecutionPositionSide, OrderSide, strict_decode};
#[path = "../src/execution.rs"]
mod execution;

use execution::{Execution, OrderFillEvent, PublicTrade};
use serde_json::{Value, json};
use std::path::Path;
use ts_rs::{Config, TS};

#[test]
fn execution_bindings_are_generated_from_the_slice_model() {
    fn check<T: TS + 'static>(name: &str) {
        let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/raw");
        assert_eq!(
            T::export_to_string(&Config::new().with_out_dir(&raw)).unwrap(),
            std::fs::read_to_string(raw.join(format!("{name}.ts"))).unwrap(),
            "regenerate {name}"
        );
    }
    check::<OrderFillEvent>("OrderFillEvent");
    check::<Execution>("Execution");
    check::<PublicTrade>("PublicTrade");
}

#[test]
fn live_fills_preserve_execution_identity_and_optional_zeroes() {
    let row = json!({"id":"fill-2", "orderId":"order-1", "symbol":"BTC", "side":"buy",
        "price":100.0, "amount":0.25, "realizedPnl":0.0, "fee":0.0});
    let fill: OrderFillEvent = serde_json::from_value(row.clone()).unwrap();
    assert_eq!(fill.id, "fill-2");
    assert_eq!(fill.order_id.as_deref(), Some("order-1"));
    assert_eq!(serde_json::to_value(fill).unwrap(), row);
    let missing = json!({"id":"order-1", "symbol":"BTC", "side":"sell", "price":0.0, "amount":0.0});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<OrderFillEvent>(missing.clone()).unwrap())
            .unwrap(),
        missing
    );
    for field in ["orderId", "realizedPnl", "fee"] {
        let mut invalid = missing.clone();
        invalid[field] = Value::Null;
        assert!(
            serde_json::from_value::<OrderFillEvent>(invalid).is_err(),
            "{field}"
        );
    }
    assert!(serde_json::from_value::<OrderFillEvent>(json!(["id", 1, 1, "buy", "BTC"])).is_err());
}

#[test]
fn execution_seconds_position_legs_and_fee_revenue_are_distinct() {
    let base = json!({"id":"native", "orderId":"order", "timestamp":1700000000.125,
        "symbol":"BTC", "price":100.0, "qty":1.0, "side":"buy"});
    for side in [None, Some("long"), Some("short"), Some("both")] {
        let mut row = base.clone();
        if let Some(side) = side {
            row["positionSide"] = json!(side);
        }
        row["commission"] = json!(0.0);
        row["realizedPnl"] = json!(0.0);
        row["venueReportedRevenue"] = json!(0.0);
        let execution: Execution = serde_json::from_value(row.clone()).unwrap();
        assert_eq!(execution.timestamp, 1700000000.125);
        assert_eq!(serde_json::to_value(execution).unwrap(), row);
    }
    for field in [
        "id",
        "orderId",
        "commission",
        "commissionAsset",
        "positionSide",
        "realizedPnl",
        "maker",
        "venueReportedRevenue",
        "venueReportedRevenueAsset",
    ] {
        let mut row = base.clone();
        row[field] = Value::Null;
        assert!(serde_json::from_value::<Execution>(row).is_err(), "{field}");
    }
    let mut invalid = base;
    invalid["positionSide"] = json!("net");
    assert!(serde_json::from_value::<Execution>(invalid).is_err());
    assert!(serde_json::from_value::<Execution>(json!([])).is_err());
}

#[test]
fn public_trade_milliseconds_and_absent_quote_quantity_survive_round_trip() {
    let base = json!({"id":"trade", "symbol":"BTC", "price":0.0, "qty":0.0,
        "timestamp":1700000000125.0, "side":"sell"});
    for qty in [None, Some(0.0)] {
        let mut row = base.clone();
        if let Some(qty) = qty {
            row["quoteQty"] = json!(qty);
        }
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<PublicTrade>(row.clone()).unwrap())
                .unwrap(),
            row
        );
    }
    let mut invalid = base;
    invalid["quoteQty"] = Value::Null;
    assert!(serde_json::from_value::<PublicTrade>(invalid).is_err());
    assert!(serde_json::from_value::<PublicTrade>(json!([])).is_err());
}
