use safe_cex_types::*;
use serde_json::Value;
fn present(input: &Value) -> bool {
    !input.is_null() && input.get("$type").and_then(Value::as_str) != Some("undefined")
}
fn own_undefined(input: &Value) -> bool {
    input.get("$type").and_then(Value::as_str) == Some("undefined")
}
pub fn check(value: &Order, input: &Value) {
    for (name, field, key) in [
        ("status", value.status.is_undefined(), "status"),
        ("type", value.r#type.is_undefined(), "type"),
        ("side", value.side.is_undefined(), "side"),
        ("price", value.price.is_undefined(), "price"),
    ] {
        assert_eq!(
            field,
            own_undefined(&input[key]),
            "Order.{name} own undefined"
        );
    }
    assert_eq!(
        Some((&value.id).as_str()),
        (&input["id"]).as_str(),
        "Order.id native field identity"
    );
    assert_eq!(
        value.parent_id.is_some(),
        present(&input["parentId"]),
        "Order.parent_id native field identity presence"
    );
    if let ModelField::Value(actual) = &value.parent_id {
        assert_eq!(
            Some((actual).as_str()),
            (&input["parentId"]).as_str(),
            "Order.parent_id native field identity"
        );
    }
    assert_eq!(
        match &value.status {
            ModelField::Value(OrderStatus::Open) => Some("open"),
            ModelField::Value(OrderStatus::Closed) => Some("closed"),
            ModelField::Value(OrderStatus::Canceled) => Some("canceled"),
            ModelField::Value(OrderStatus::Creating) => Some("creating"),
            ModelField::Value(OrderStatus::Updating) => Some("updating"),
            ModelField::Value(OrderStatus::Canceling) => Some("canceling"),
            ModelField::Absent | ModelField::Undefined => None,
        },
        (&input["status"]).as_str(),
        "Order.status native field identity"
    );
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "Order.symbol native field identity"
    );
    assert_eq!(
        match &value.r#type {
            ModelField::Value(OrderType::Market) => Some("market"),
            ModelField::Value(OrderType::Limit) => Some("limit"),
            ModelField::Value(OrderType::Conditional) => Some("conditional"),
            ModelField::Value(OrderType::StopLoss) => Some("stop_market"),
            ModelField::Value(OrderType::TakeProfit) => Some("take_profit_market"),
            ModelField::Value(OrderType::TrailingStopLoss) => Some("trailing_stop_market"),
            ModelField::Absent | ModelField::Undefined => None,
        },
        (&input["type"]).as_str(),
        "Order.r#type native field identity"
    );
    assert_eq!(
        match &value.side {
            ModelField::Value(OrderSide::Buy) => Some("buy"),
            ModelField::Value(OrderSide::Sell) => Some("sell"),
            ModelField::Absent | ModelField::Undefined => None,
        },
        (&input["side"]).as_str(),
        "Order.side native field identity"
    );
    assert_eq!(
        value.price.value().map(|price| price.to_bits()),
        (&input["price"]).as_f64().map(f64::to_bits),
        "Order.price native field identity"
    );
    assert_eq!(
        (&value.amount).to_bits(),
        (&input["amount"]).as_f64().unwrap().to_bits(),
        "Order.amount native field identity"
    );
    assert_eq!(
        (&value.filled).to_bits(),
        (&input["filled"]).as_f64().unwrap().to_bits(),
        "Order.filled native field identity"
    );
    assert_eq!(
        (&value.remaining).to_bits(),
        (&input["remaining"]).as_f64().unwrap().to_bits(),
        "Order.remaining native field identity"
    );
    assert_eq!(
        Some(value.reduce_only),
        (&input["reduceOnly"]).as_bool(),
        "Order.reduce_only native field identity"
    );
    assert_eq!(
        value.close.is_some(),
        present(&input["close"]),
        "Order.close native field identity presence"
    );
    if let ModelField::Value(actual) = &value.close {
        assert_eq!(
            Some(*(actual)),
            (&input["close"]).as_bool(),
            "Order.close native field identity"
        );
    }
    assert_eq!(
        value.info.is_some(),
        present(&input["info"]),
        "Order.info native field identity presence"
    );
    if let ModelField::Value(actual) = &value.info {
        assert_eq!(actual, &input["info"], "Order.info native field identity");
    }
    assert_eq!(
        value.time_in_force.is_some(),
        present(&input["timeInForce"]),
        "Order.time_in_force native field identity presence"
    );
    if let ModelField::Value(actual) = &value.time_in_force {
        assert_eq!(
            Some(match actual {
                OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
                OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
                OrderTimeInForce::FillOrKill => "FillOrKill",
                OrderTimeInForce::PostOnly => "PostOnly",
            }),
            (&input["timeInForce"]).as_str(),
            "Order.time_in_force native field identity"
        );
    }
    assert_eq!(
        (&value.last_update).to_bits(),
        (&input["lastUpdate"]).as_f64().unwrap().to_bits(),
        "Order.last_update native field identity"
    );
    assert_eq!(
        value.trigger_type.is_some(),
        present(&input["triggerType"]),
        "Order.trigger_type native field identity presence"
    );
    if let ModelField::Value(actual) = &value.trigger_type {
        assert_eq!(
            Some(match actual {
                OrderStopTriggerType::MarkPrice => "mark_price",
                OrderStopTriggerType::LastPrice => "last_price",
                OrderStopTriggerType::IndexPrice => "index_price",
            }),
            (&input["triggerType"]).as_str(),
            "Order.trigger_type native field identity"
        );
    }
    assert_eq!(
        value.tp_trigger_type.is_some(),
        present(&input["tpTriggerType"]),
        "Order.tp_trigger_type native field identity presence"
    );
    if let ModelField::Value(actual) = &value.tp_trigger_type {
        assert_eq!(
            Some(match actual {
                OrderStopTriggerType::MarkPrice => "mark_price",
                OrderStopTriggerType::LastPrice => "last_price",
                OrderStopTriggerType::IndexPrice => "index_price",
            }),
            (&input["tpTriggerType"]).as_str(),
            "Order.tp_trigger_type native field identity"
        );
    }
    assert_eq!(
        value.sl_trigger_type.is_some(),
        present(&input["slTriggerType"]),
        "Order.sl_trigger_type native field identity presence"
    );
    if let ModelField::Value(actual) = &value.sl_trigger_type {
        assert_eq!(
            Some(match actual {
                OrderStopTriggerType::MarkPrice => "mark_price",
                OrderStopTriggerType::LastPrice => "last_price",
                OrderStopTriggerType::IndexPrice => "index_price",
            }),
            (&input["slTriggerType"]).as_str(),
            "Order.sl_trigger_type native field identity"
        );
    }
    assert_eq!(
        value.take_profit.is_some(),
        present(&input["takeProfit"]),
        "Order.take_profit native field identity presence"
    );
    if let ModelField::Value(actual) = &value.take_profit {
        assert_eq!(
            (actual).to_bits(),
            (&input["takeProfit"]).as_f64().unwrap().to_bits(),
            "Order.take_profit native field identity"
        );
    }
    assert_eq!(
        value.stop_loss.is_some(),
        present(&input["stopLoss"]),
        "Order.stop_loss native field identity presence"
    );
    if let ModelField::Value(actual) = &value.stop_loss {
        assert_eq!(
            (actual).to_bits(),
            (&input["stopLoss"]).as_f64().unwrap().to_bits(),
            "Order.stop_loss native field identity"
        );
    }
    assert_eq!(
        value.extra.is_some(),
        present(&input["extra"]),
        "Order.extra native field identity presence"
    );
    if let ModelField::Value(actual) = &value.extra {
        panic!("unexpected populated Order.extra: {actual:?}");
    }
}
