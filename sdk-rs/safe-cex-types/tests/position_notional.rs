use safe_cex_types::Position;
use safe_cex_types::store_model::{StoreData, initial_store_data};
use serde_json::{Value, json};

fn known_position(notional: f64) -> Position {
    serde_json::from_value(json!({
        "id":"p", "symbol":"PF_XBTUSD", "side":"long", "entryPrice":100.0,
        "notional":notional, "leverage":5.0, "unrealizedPnl":0.0,
        "contracts":2.0, "liquidationPrice":null
    }))
    .unwrap()
}

#[test]
fn unknown_position_notional_survives_json_and_typed_store_roundtrip() {
    for unknown in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut position = known_position(250.0);
        position.notional = unknown.into();
        let row = serde_json::to_value(position).unwrap();
        assert_eq!(row["notional"], Value::Null);
        let mut snapshot = serde_json::to_value(initial_store_data()).unwrap();
        snapshot["positions"] = json!([row.clone()]);
        let store: StoreData = serde_json::from_value(snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(store.positions[0].notional).unwrap(),
            Value::Null
        );
        assert_eq!(store.positions[0].contracts, 2.0);
        let decoded: Position = serde_json::from_value(row.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded.notional).unwrap(), Value::Null);
        assert_eq!(decoded.contracts, 2.0);
        assert_eq!(
            serde_json::to_value(store).unwrap()["positions"][0]["notional"],
            Value::Null
        );
    }
}

#[test]
fn position_notional_preserves_known_zero_and_values_and_remains_required() {
    for value in [0.0, 250.0, -250.0] {
        let row = serde_json::to_value(known_position(value)).unwrap();
        assert_eq!(row["notional"], value);
        let decoded: Position = serde_json::from_value(row.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(decoded.notional).unwrap(),
            json!(value)
        );
        let mut missing = row.clone();
        missing.as_object_mut().unwrap().remove("notional");
        assert!(serde_json::from_value::<Position>(missing).is_err());
        for invalid in [json!("unknown"), json!(false), json!([]), json!({})] {
            let mut malformed = row.clone();
            malformed["notional"] = invalid;
            assert!(serde_json::from_value::<Position>(malformed).is_err());
        }
    }
}
