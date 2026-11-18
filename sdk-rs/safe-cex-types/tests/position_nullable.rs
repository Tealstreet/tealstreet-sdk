use safe_cex_types::Position;
use serde_json::json;

#[test]
fn liquidation_is_required_but_nullable() {
    let mut value = json!({"id":"p", "symbol":"BTC", "side":"long", "entryPrice":1,
        "notional":1, "leverage":1, "unrealizedPnl":0, "contracts":1, "liquidationPrice":null});
    let position: Position = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(position.liquidation_price, None);
    assert!(serde_json::to_value(position).unwrap()["liquidationPrice"].is_null());
    value["liquidationPrice"] = json!(123.5);
    let position: Position = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(position.liquidation_price, Some(123.5));
    value.as_object_mut().unwrap().remove("liquidationPrice");
    assert!(serde_json::from_value::<Position>(value).is_err());
}
