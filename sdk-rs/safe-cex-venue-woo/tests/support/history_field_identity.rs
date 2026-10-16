use safe_cex_types::*;
use serde_json::Value;
pub fn check(value: &HistoricalOrder, input: &Value) {
    assert_eq!(
        Some((&value.id).as_str()),
        (&input["id"]).as_str(),
        "HistoricalOrder.id native field identity"
    );
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "HistoricalOrder.symbol native field identity"
    );
    assert_eq!(
        Some(match &value.side {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        }),
        (&input["side"]).as_str(),
        "HistoricalOrder.side native field identity"
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
        "HistoricalOrder.r#type native field identity"
    );
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
        "HistoricalOrder.status native field identity"
    );
    assert_eq!(
        value.price.is_some(),
        input.get("price").is_some(),
        "HistoricalOrder.price native field identity presence"
    );
    if let Some(actual) = &value.price {
        if (&input["price"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.price native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["price"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.price native field identity"
            );
        }
    }
    assert_eq!(
        value.average_price.is_some(),
        input.get("averagePrice").is_some(),
        "HistoricalOrder.average_price native field identity presence"
    );
    if let Some(actual) = &value.average_price {
        if (&input["averagePrice"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.average_price native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["averagePrice"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.average_price native field identity"
            );
        }
    }
    if (&input["amount"]).is_null() {
        assert!(
            !(&value.amount).is_finite(),
            "HistoricalOrder.amount native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.amount).to_bits(),
            (&input["amount"]).as_f64().unwrap().to_bits(),
            "HistoricalOrder.amount native field identity"
        );
    }
    if (&input["filled"]).is_null() {
        assert!(
            !(&value.filled).is_finite(),
            "HistoricalOrder.filled native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.filled).to_bits(),
            (&input["filled"]).as_f64().unwrap().to_bits(),
            "HistoricalOrder.filled native field identity"
        );
    }
    assert_eq!(
        value.reduce_only.is_some(),
        input.get("reduceOnly").is_some(),
        "HistoricalOrder.reduce_only native field identity presence"
    );
    if let Some(actual) = &value.reduce_only {
        assert_eq!(
            Some(*(actual)),
            (&input["reduceOnly"]).as_bool(),
            "HistoricalOrder.reduce_only native field identity"
        );
    }
    assert_eq!(
        value.position_side.is_some(),
        input.get("positionSide").is_some(),
        "HistoricalOrder.position_side native field identity presence"
    );
    if let Some(actual) = &value.position_side {
        assert_eq!(
            Some(match actual {
                ExecutionPositionSide::Side(PositionSide::Long) => "long",
                ExecutionPositionSide::Side(PositionSide::Short) => "short",
                ExecutionPositionSide::Both(BothPositionSide::Both) => "both",
            }),
            (&input["positionSide"]).as_str(),
            "HistoricalOrder.position_side native field identity"
        );
    }
    if (&input["createdTime"]).is_null() {
        assert!(
            !(&value.created_time).is_finite(),
            "HistoricalOrder.created_time native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.created_time).to_bits(),
            (&input["createdTime"]).as_f64().unwrap().to_bits(),
            "HistoricalOrder.created_time native field identity"
        );
    }
    assert_eq!(
        value.updated_time.is_some(),
        input.get("updatedTime").is_some(),
        "HistoricalOrder.updated_time native field identity presence"
    );
    if let Some(actual) = &value.updated_time {
        if (&input["updatedTime"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.updated_time native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["updatedTime"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.updated_time native field identity"
            );
        }
    }
    assert_eq!(
        value.realized_pnl.is_some(),
        input.get("realizedPnl").is_some(),
        "HistoricalOrder.realized_pnl native field identity presence"
    );
    if let Some(actual) = &value.realized_pnl {
        if (&input["realizedPnl"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.realized_pnl native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["realizedPnl"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.realized_pnl native field identity"
            );
        }
    }
    assert_eq!(
        value.fees.is_some(),
        input.get("fees").is_some(),
        "HistoricalOrder.fees native field identity presence"
    );
    if let Some(actual) = &value.fees {
        if (&input["fees"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.fees native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["fees"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.fees native field identity"
            );
        }
    }
    assert_eq!(
        value.fee_asset.is_some(),
        input.get("feeAsset").is_some(),
        "HistoricalOrder.fee_asset native field identity presence"
    );
    if let Some(actual) = &value.fee_asset {
        assert_eq!(
            Some((actual).as_str()),
            (&input["feeAsset"]).as_str(),
            "HistoricalOrder.fee_asset native field identity"
        );
    }
    assert_eq!(
        value.rebate.is_some(),
        input.get("rebate").is_some(),
        "HistoricalOrder.rebate native field identity presence"
    );
    if let Some(actual) = &value.rebate {
        if (&input["rebate"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "HistoricalOrder.rebate native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["rebate"]).as_f64().unwrap().to_bits(),
                "HistoricalOrder.rebate native field identity"
            );
        }
    }
    assert_eq!(
        value.rebate_asset.is_some(),
        input.get("rebateAsset").is_some(),
        "HistoricalOrder.rebate_asset native field identity presence"
    );
    if let Some(actual) = &value.rebate_asset {
        assert_eq!(
            Some((actual).as_str()),
            (&input["rebateAsset"]).as_str(),
            "HistoricalOrder.rebate_asset native field identity"
        );
    }
}
