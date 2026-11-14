use safe_cex_types::{
    Order, OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
    OrderType, PlaceOrderOpts, UpdateOrderOpts,
};
use std::path::Path;
use ts_rs::{Config, TS};

#[test]
fn order_contract_bindings_are_generated_from_the_live_rust_model() {
    fn check<T: TS + 'static>(name: &str) {
        let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/raw");
        let config = Config::new().with_out_dir(&raw);
        let generated = T::export_to_string(&config).unwrap();
        assert_eq!(
            generated,
            std::fs::read_to_string(raw.join(format!("{name}.ts"))).unwrap(),
            "regenerate {name} from Rust"
        );
    }
    check::<Order>("Order");
    check::<PlaceOrderOpts>("PlaceOrderOpts");
    check::<UpdateOrderOpts>("UpdateOrderOpts");
    check::<OrderSide>("OrderSide");
    check::<OrderStatus>("OrderStatus");
    check::<OrderType>("OrderType");
    check::<OrderTimeInForce>("OrderTimeInForce");
    check::<OrderStopTriggerType>("OrderStopTriggerType");
    check::<OrderRejectReason>("OrderRejectReason");
}

#[test]
fn placement_optional_triggers_preserve_zero_and_refuse_null() {
    let minimal = serde_json::json!({"symbol":"BTC", "type":"limit", "side":"buy", "amount":1.});
    let absent: PlaceOrderOpts = serde_json::from_value(minimal.clone()).unwrap();
    assert!(absent.trigger_price.is_none());
    assert!(absent.stop_loss.is_none());
    assert!(absent.take_profit.is_none());
    for field in [
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "impliedVolatility",
    ] {
        let mut zero = minimal.clone();
        zero[field] = serde_json::json!(0);
        let parsed: PlaceOrderOpts = serde_json::from_value(zero).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap()[field], 0.);
        let mut null = minimal.clone();
        null[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<PlaceOrderOpts>(null).is_err(),
            "{field}"
        );
    }
}

#[test]
fn order_optional_identity_and_extras_survive_native_serialization() {
    let input = serde_json::json!({"id":"terminal", "parentId":"parent", "clientOrderId":"client",
        "status":"canceled", "symbol":"BTC", "type":"stop_market", "side":"sell", "price":0.,
        "amount":10., "filled":0.3, "remaining":0.7, "reduceOnly":true, "close":false,
        "lastUpdate":0., "stopLoss":0., "extra":{"remainingNative":7.,"filledInPlacementUnits":3.,
        "remainingNotional":700.,"conditionalOrderPrice":0.,"rejectReason":"PostOnly"}});
    let order: Order = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(serde_json::to_value(order).unwrap(), input);
}
