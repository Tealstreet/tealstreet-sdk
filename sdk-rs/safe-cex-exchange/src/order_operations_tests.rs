use super::*;
use crate::client_order_id::{
    is_regular_client_order_shape as regular_shape,
    order_carries_client_order_id as carries_client_id, same_client_order_id,
};
use safe_cex_types::Order;
use serde_json::json;

fn order() -> Order {
    serde_json::from_value(json!({
        "id":"venue-id", "status":"open", "symbol":"BTCUSDT", "type":"limit", "side":"buy",
        "price":1, "amount":2, "filled":0, "remaining":2, "reduceOnly":false, "lastUpdate":0
    }))
    .unwrap()
}

#[test]
fn client_lookup_uses_every_mapper_field_and_js_numeric_coercion() {
    let mut order = order();
    assert!(!carries_client_id(&order, "123"));
    for field in [
        "clOrdId",
        "clientOrderId",
        "orderLinkId",
        "clientOid",
        "clOrdID",
        "c",
        "client_id",
        "client_order_id",
        "clientId",
    ] {
        order.info = Some(json!({field:123.0}));
        assert!(carries_client_id(&order, "123"), "mapper field {field}");
        assert!(!carries_client_id(&order, "124"));
    }
    order.info = None;
    order.parent_id = Some("caller".into());
    assert!(carries_client_id(&order, "caller"));
    order.id = "direct-caller".into();
    assert!(carries_client_id(&order, "direct-caller"));
}

#[test]
fn client_lookup_rejects_sentinel_ids_even_if_venue_matches() {
    for sentinel in ["", "0", "undefined"] {
        let mut order = order();
        order.id = sentinel.into();
        order.parent_id = Some(sentinel.into());
        order.info = Some(json!({"clientOrderId":sentinel}));
        assert!(!carries_client_id(&order, sentinel));
    }
    let mut order = order();
    order.info = Some(json!({"clientOrderId":null}));
    assert!(!carries_client_id(&order, "null"));
}

#[test]
fn regular_client_id_shape_checks_presence_not_numeric_truthiness() {
    let mut options = order_options("BTCUSDT".into(), OrderSide::Buy, OrderType::Limit, 2.0);
    assert!(regular_shape(&options));
    options.close = Some(true);
    assert!(regular_shape(&options));
    for key in ["triggerPrice", "stopLoss", "takeProfit"] {
        let mut value = serde_json::to_value(&options).unwrap();
        value[key] = json!(0);
        assert!(!regular_shape(&serde_json::from_value(value).unwrap()));
    }
    options.r#type = OrderType::StopLoss;
    assert!(!regular_shape(&options));
}

#[test]
fn js_id_coercion_preserves_array_and_object_behavior() {
    assert!(same_client_order_id(Some(&json!(-0.0)), "0"));
    assert!(same_client_order_id(Some(&json!(1e21)), "1e+21"));
    assert!(same_client_order_id(Some(&json!(1e-7)), "1e-7"));
    let overflow: serde_json::Value = serde_json::from_str("1e400").unwrap();
    assert!(same_client_order_id(Some(&overflow), "Infinity"));
    assert!(same_client_order_id(
        Some(&json!([123, null, "caller"])),
        "123,,caller"
    ));
    assert!(same_client_order_id(
        Some(&json!({"id":"caller"})),
        "[object Object]"
    ));
    assert!(same_client_order_id(Some(&json!(false)), "false"));
}

#[test]
fn nuke_cleanup_is_idempotent_even_if_a_new_operation_latches_between_drops() {
    let latch = std::cell::Cell::new(false);
    let mut guard = Some(super::NukeGuard::new(&latch));
    assert!(latch.get());
    guard.as_ref().unwrap().finish();
    assert!(!latch.get());
    guard.as_ref().unwrap().finish();
    latch.set(true);
    drop(guard.take());
    assert!(
        latch.get(),
        "the old guard must not clear the next operation's latch"
    );
    drop(guard.take());
    assert!(latch.get());
}
