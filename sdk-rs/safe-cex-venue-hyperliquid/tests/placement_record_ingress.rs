use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType};
use safe_cex_venue_hyperliquid::cloid::HyperliquidPlacementRecord;
use serde::Deserialize;
use serde_json::{Value, json};

fn complete() -> Value {
    json!({"type":"limit","price":1.25,"side":"buy","amount":2.5,
        "reduceOnly":true,"timeInForce":"PostOnly","symbol":"BTCUSD","placedAt":9001.5})
}

fn assert_complete(record: HyperliquidPlacementRecord) {
    assert_eq!(record.r#type, OrderType::Limit);
    assert_eq!(record.price.to_bits(), 0x3ff4000000000000);
    assert_eq!(record.side, OrderSide::Buy);
    assert_eq!(record.amount.to_bits(), 0x4004000000000000);
    assert!(record.reduce_only);
    assert_eq!(record.time_in_force, Some(OrderTimeInForce::PostOnly));
    assert_eq!(record.symbol, "BTCUSD");
    assert_eq!(record.placed_at.to_bits(), 0x40c194c000000000);
}

#[test]
fn placement_record_native_fields_and_presence() {
    let value = complete();
    assert_complete(HyperliquidPlacementRecord::deserialize(value.clone()).unwrap());
    assert_complete(HyperliquidPlacementRecord::deserialize(&value).unwrap());
    assert_complete(serde_json::from_value(value.clone()).unwrap());
    assert_complete(serde_json::from_str(&value.to_string()).unwrap());
    assert_complete(serde_json::from_slice(value.to_string().as_bytes()).unwrap());
    assert_complete(safe_cex_num::numeric_ingress::js_from_value(value.clone()).unwrap());
    assert_complete(safe_cex_num::numeric_ingress::js_from_value_ref(&value).unwrap());
    assert_complete(safe_cex_num::numeric_ingress::js_from_str(&value.to_string()).unwrap());
    let text = r#"{"type":"market","price":-0,"side":"sell","amount":0.125,"reduceOnly":false,"symbol":"雪/ETH","placedAt":0}"#;
    let record: HyperliquidPlacementRecord = serde_json::from_str(text).unwrap();
    assert_eq!(record.r#type, OrderType::Market);
    assert_eq!(record.price.to_bits(), 0x8000000000000000);
    assert_eq!(record.side, OrderSide::Sell);
    assert_eq!(record.amount.to_bits(), 0x3fc0000000000000);
    assert!(!record.reduce_only);
    assert_eq!(record.time_in_force, None);
    assert_eq!(record.symbol, "雪/ETH");
    assert_eq!(record.placed_at.to_bits(), 0);
}

#[test]
fn placement_record_rejects_complete_positional_array() {
    // Complete and valid in the original Serde declaration order, including the optional field.
    let value = json!([
        "limit", 1.25, "buy", 2.5, true, "PostOnly", "BTCUSD", 9001.5
    ]);
    assert!(
        HyperliquidPlacementRecord::deserialize(value.clone()).is_err(),
        "owned complete positional array accepted"
    );
    assert!(
        HyperliquidPlacementRecord::deserialize(&value).is_err(),
        "borrowed complete positional array accepted"
    );
    assert!(
        serde_json::from_value::<HyperliquidPlacementRecord>(value.clone()).is_err(),
        "qualified complete positional array accepted"
    );
    assert!(
        serde_json::from_str::<HyperliquidPlacementRecord>(&value.to_string()).is_err(),
        "text complete positional array accepted"
    );
}

#[test]
fn placement_record_optional_time_in_force_rejects_null() {
    let mut value = complete();
    value["timeInForce"] = Value::Null;
    assert!(
        serde_json::from_value::<HyperliquidPlacementRecord>(value.clone()).is_err(),
        "present null timeInForce accepted"
    );
    assert!(
        serde_json::from_str::<HyperliquidPlacementRecord>(&value.to_string()).is_err(),
        "text null timeInForce accepted"
    );
}

#[test]
fn placement_record_captures_binary64_before_serde_json() {
    let text = r#"{"type":"market","price":1e400,"side":"sell","amount":-0,"reduceOnly":false,"symbol":"BTCUSD","placedAt":9007199254740993}"#;
    let record: HyperliquidPlacementRecord =
        serde_json::from_str(text).expect("raw binary64 capture must admit overflow");
    assert_eq!(record.r#type, OrderType::Market);
    assert_eq!(record.price.to_bits(), 0x7ff0000000000000);
    assert_eq!(record.side, OrderSide::Sell);
    assert_eq!(record.amount.to_bits(), 0x8000000000000000);
    assert!(!record.reduce_only);
    assert_eq!(record.time_in_force, None);
    assert_eq!(record.symbol, "BTCUSD");
    assert_eq!(record.placed_at.to_bits(), 0x4340000000000000);
    let record: HyperliquidPlacementRecord = serde_json::from_slice(text.as_bytes()).unwrap();
    assert_eq!(record.price.to_bits(), 0x7ff0000000000000);
    assert_eq!(record.amount.to_bits(), 0x8000000000000000);
    assert_eq!(record.placed_at.to_bits(), 0x4340000000000000);
    let record: HyperliquidPlacementRecord =
        safe_cex_num::numeric_ingress::js_from_str(text).unwrap();
    assert_eq!(record.price.to_bits(), 0x7ff0000000000000);
    assert_eq!(record.amount.to_bits(), 0x8000000000000000);
    assert_eq!(record.placed_at.to_bits(), 0x4340000000000000);
}

#[test]
fn placement_record_required_field_types_and_outer_shapes() {
    for field in [
        "type",
        "price",
        "side",
        "amount",
        "reduceOnly",
        "symbol",
        "placedAt",
    ] {
        let mut value = complete();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<HyperliquidPlacementRecord>(value).is_err(),
            "missing {field}"
        );
        let mut value = complete();
        value[field] = Value::Null;
        assert!(
            serde_json::from_value::<HyperliquidPlacementRecord>(value).is_err(),
            "null {field}"
        );
    }
    for value in [
        Value::Null,
        json!([]),
        json!(true),
        json!("record"),
        json!(7),
    ] {
        assert!(serde_json::from_value::<HyperliquidPlacementRecord>(value).is_err());
    }
    for field in ["price", "amount", "placedAt"] {
        let mut value = complete();
        value[field] = json!("1.25");
        assert!(
            serde_json::from_value::<HyperliquidPlacementRecord>(value).is_err(),
            "numeric string {field}"
        );
    }
}
