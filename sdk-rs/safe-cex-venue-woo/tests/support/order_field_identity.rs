use safe_cex_types::*;
use serde_json::Value;
fn present(input: &Value) -> bool {
    !input.is_null() && input.get("$type").and_then(Value::as_str) != Some("undefined")
}
pub fn check(value: &Order, input: &Value) {
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
    if let Some(actual) = &value.parent_id {
        assert_eq!(
            Some((actual).as_str()),
            (&input["parentId"]).as_str(),
            "Order.parent_id native field identity"
        );
    }
    assert_eq!(
        Some(match &value.status {
            OrderStatus::Open => "open",
            OrderStatus::Closed => "closed",
            OrderStatus::Canceled => "canceled",
            OrderStatus::Creating => "creating",
            OrderStatus::Updating => "updating",
            OrderStatus::Canceling => "canceling",
        }),
        (&input["status"]).as_str(),
        "Order.status native field identity"
    );
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "Order.symbol native field identity"
    );
    assert_eq!(
        Some(match &value.r#type {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Conditional => "conditional",
            OrderType::StopLoss => "stop_market",
            OrderType::TakeProfit => "take_profit_market",
            OrderType::TrailingStopLoss => "trailing_stop_market",
        }),
        (&input["type"]).as_str(),
        "Order.r#type native field identity"
    );
    assert_eq!(
        Some(match &value.side {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        }),
        (&input["side"]).as_str(),
        "Order.side native field identity"
    );
    assert_eq!(
        (&value.price).to_bits(),
        (&input["price"]).as_f64().unwrap().to_bits(),
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
    if let Some(actual) = &value.close {
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
    if let Some(actual) = &value.info {
        assert_eq!(actual, &input["info"], "Order.info native field identity");
    }
    assert_eq!(
        value.time_in_force.is_some(),
        present(&input["timeInForce"]),
        "Order.time_in_force native field identity presence"
    );
    if let Some(actual) = &value.time_in_force {
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
    if let Some(actual) = &value.trigger_type {
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
    if let Some(actual) = &value.tp_trigger_type {
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
    if let Some(actual) = &value.sl_trigger_type {
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
    if let Some(actual) = &value.take_profit {
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
    if let Some(actual) = &value.stop_loss {
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
    if let Some(actual) = &value.extra {
        panic!("unexpected populated Order.extra: {actual:?}");
    }
}
