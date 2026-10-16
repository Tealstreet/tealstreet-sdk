use safe_cex_types::*;
use serde_json::Value;
pub fn check(value: &Execution, input: &Value) {
    assert_eq!(
        value.id.is_some(),
        input.get("id").is_some(),
        "Execution.id native field identity presence"
    );
    if let Some(actual) = &value.id {
        assert_eq!(
            Some((actual).as_str()),
            (&input["id"]).as_str(),
            "Execution.id native field identity"
        );
    }
    if (&input["timestamp"]).is_null() {
        assert!(
            !(&value.timestamp).is_finite(),
            "Execution.timestamp native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.timestamp).to_bits(),
            (&input["timestamp"]).as_f64().unwrap().to_bits(),
            "Execution.timestamp native field identity"
        );
    }
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "Execution.symbol native field identity"
    );
    assert_eq!(
        value.order_id.is_some(),
        input.get("orderId").is_some(),
        "Execution.order_id native field identity presence"
    );
    if let Some(actual) = &value.order_id {
        assert_eq!(
            Some((actual).as_str()),
            (&input["orderId"]).as_str(),
            "Execution.order_id native field identity"
        );
    }
    if (&input["price"]).is_null() {
        assert!(
            !(&value.price).is_finite(),
            "Execution.price native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.price).to_bits(),
            (&input["price"]).as_f64().unwrap().to_bits(),
            "Execution.price native field identity"
        );
    }
    if (&input["qty"]).is_null() {
        assert!(
            !(&value.qty).is_finite(),
            "Execution.qty native field identity nonfinite"
        );
    } else {
        assert_eq!(
            (&value.qty).to_bits(),
            (&input["qty"]).as_f64().unwrap().to_bits(),
            "Execution.qty native field identity"
        );
    }
    assert_eq!(
        Some(match &value.side {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        }),
        (&input["side"]).as_str(),
        "Execution.side native field identity"
    );
    assert_eq!(
        value.commission.is_some(),
        input.get("commission").is_some(),
        "Execution.commission native field identity presence"
    );
    if let Some(actual) = &value.commission {
        if (&input["commission"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "Execution.commission native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["commission"]).as_f64().unwrap().to_bits(),
                "Execution.commission native field identity"
            );
        }
    }
    assert_eq!(
        value.commission_asset.is_some(),
        input.get("commissionAsset").is_some(),
        "Execution.commission_asset native field identity presence"
    );
    if let Some(actual) = &value.commission_asset {
        assert_eq!(
            Some((actual).as_str()),
            (&input["commissionAsset"]).as_str(),
            "Execution.commission_asset native field identity"
        );
    }
    assert_eq!(
        value.position_side.is_some(),
        input.get("positionSide").is_some(),
        "Execution.position_side native field identity presence"
    );
    if let Some(actual) = &value.position_side {
        assert_eq!(
            Some(match actual {
                ExecutionPositionSide::Side(PositionSide::Long) => "long",
                ExecutionPositionSide::Side(PositionSide::Short) => "short",
                ExecutionPositionSide::Both(BothPositionSide::Both) => "both",
            }),
            (&input["positionSide"]).as_str(),
            "Execution.position_side native field identity"
        );
    }
    assert_eq!(
        value.realized_pnl.is_some(),
        input.get("realizedPnl").is_some(),
        "Execution.realized_pnl native field identity presence"
    );
    if let Some(actual) = &value.realized_pnl {
        if (&input["realizedPnl"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "Execution.realized_pnl native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["realizedPnl"]).as_f64().unwrap().to_bits(),
                "Execution.realized_pnl native field identity"
            );
        }
    }
    assert_eq!(
        value.maker.is_some(),
        input.get("maker").is_some(),
        "Execution.maker native field identity presence"
    );
    if let Some(actual) = &value.maker {
        assert_eq!(
            Some(*(actual)),
            (&input["maker"]).as_bool(),
            "Execution.maker native field identity"
        );
    }
    assert_eq!(
        value.venue_reported_revenue.is_some(),
        input.get("venueReportedRevenue").is_some(),
        "Execution.venue_reported_revenue native field identity presence"
    );
    if let Some(actual) = &value.venue_reported_revenue {
        if (&input["venueReportedRevenue"]).is_null() {
            assert!(
                !(actual).is_finite(),
                "Execution.venue_reported_revenue native field identity nonfinite"
            );
        } else {
            assert_eq!(
                (actual).to_bits(),
                (&input["venueReportedRevenue"]).as_f64().unwrap().to_bits(),
                "Execution.venue_reported_revenue native field identity"
            );
        }
    }
    assert_eq!(
        value.venue_reported_revenue_asset.is_some(),
        input.get("venueReportedRevenueAsset").is_some(),
        "Execution.venue_reported_revenue_asset native field identity presence"
    );
    if let Some(actual) = &value.venue_reported_revenue_asset {
        assert_eq!(
            Some((actual).as_str()),
            (&input["venueReportedRevenueAsset"]).as_str(),
            "Execution.venue_reported_revenue_asset native field identity"
        );
    }
}
