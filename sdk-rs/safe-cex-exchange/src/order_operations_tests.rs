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
    assert!(!carries_client_id(&order, "123").unwrap());
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
        order.info = (Some(json!({field:123.0}).into())).into();
        assert!(
            carries_client_id(&order, "123").unwrap(),
            "mapper field {field}"
        );
        assert!(!carries_client_id(&order, "124").unwrap());
    }
    order.info = (None).into();
    order.parent_id = (Some("caller".into())).into();
    assert!(carries_client_id(&order, "caller").unwrap());
    order.id = "direct-caller".into();
    assert!(carries_client_id(&order, "direct-caller").unwrap());
}

#[test]
fn client_lookup_matches_the_top_level_client_order_id() {
    let mut order = order();
    order.client_order_id = (Some("0x0123456789abcdef0123456789abcdef".into())).into();
    assert!(carries_client_id(&order, "0x0123456789abcdef0123456789abcdef").unwrap());
    assert!(!carries_client_id(&order, "0x0123456789abcdef0123456789abcdee").unwrap());
    order.client_order_id = (Some("0".into())).into();
    assert!(!carries_client_id(&order, "0").unwrap());
    order.client_order_id = (None).into();
    let mut wire = serde_json::to_value(&order).unwrap();
    wire["clientOrderId"] = json!("0xfeed");
    let decoded: Order = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded.client_order_id.as_deref(), Some("0xfeed"));
    assert!(carries_client_id(&decoded, "0xfeed").unwrap());
    assert_eq!(
        serde_json::to_value(&decoded).unwrap()["clientOrderId"],
        "0xfeed"
    );
}

#[test]
fn replacement_ids_stay_an_id_list_and_carry_a_truthy_client_order_id() {
    use crate::client_order_id::{
        ReplacementOrderIds, replacement_client_order_id_of, with_replacement_client_order_id,
    };
    let ids = with_replacement_client_order_id(vec!["7".into()], "0xabc".into());
    assert_eq!(ids[0], "7");
    assert_eq!(ids.len(), 1);
    assert_eq!(replacement_client_order_id_of(&ids), Some("0xabc"));
    let empty = with_replacement_client_order_id(vec![], String::new());
    assert_eq!(replacement_client_order_id_of(&empty), None);
    assert_eq!(
        replacement_client_order_id_of(&ReplacementOrderIds::default()),
        None
    );
}

#[test]
fn client_lookup_rejects_sentinel_ids_even_if_venue_matches() {
    for sentinel in ["", "0", "undefined"] {
        let mut order = order();
        order.id = sentinel.into();
        order.parent_id = (Some(sentinel.into())).into();
        order.info = (Some(json!({"clientOrderId":sentinel}).into())).into();
        assert!(!carries_client_id(&order, sentinel).unwrap());
    }
    let mut order = order();
    order.info = (Some(json!({"clientOrderId":null}).into())).into();
    assert!(!carries_client_id(&order, "null").unwrap());
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
        assert!(!regular_shape(
            serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(value).unwrap()
        ));
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
