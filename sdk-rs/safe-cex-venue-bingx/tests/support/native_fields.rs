use super::*;

impl Observe for BingxBalance {
    fn observe(&self) -> Value {
        json!({
            "userId": self.user_id.observe(),
            "asset": self.asset.observe(),
            "balance": self.balance.observe(),
            "equity": self.equity.observe(),
            "unrealizedProfit": self.unrealized_profit.observe(),
            "realisedProfit": self.realised_profit.observe(),
            "availableMargin": self.available_margin.observe(),
            "usedMargin": self.used_margin.observe(),
            "freezedMargin": self.freezed_margin.observe(),
        })
    }
}
impl Observe for FetchBalanceResponse {
    fn observe(&self) -> Value {
        json!({
            "code": self.code.observe(),
            "msg": self.msg.observe(),
            "data": self.data.observe(),
        })
    }
}
impl Observe for FetchLeverageResponseData {
    fn observe(&self) -> Value {
        json!({
            "longLeverage": self.long_leverage.observe(),
            "shortLeverage": self.short_leverage.observe(),
            "maxLongLeverage": self.max_long_leverage.observe(),
            "maxShortLeverage": self.max_short_leverage.observe(),
            "availableLongVol": self.available_long_vol.observe(),
            "availableShortVol": self.available_short_vol.observe(),
            "availableLongVal": self.available_long_val.observe(),
            "availableShortVal": self.available_short_val.observe(),
            "maxPositionLongVal": self.max_position_long_val.observe(),
            "maxPositionShortVal": self.max_position_short_val.observe(),
        })
    }
}
impl Observe for FetchLeverageResponse {
    fn observe(&self) -> Value {
        json!({
            "code": self.code.observe(),
            "msg": self.msg.observe(),
            "data": self.data.observe(),
        })
    }
}
impl Observe for FetchMarginModeResponseData {
    fn observe(&self) -> Value {
        json!({
            "marginType": self.margin_type.observe(),
        })
    }
}
impl Observe for FetchMarginModeResponse {
    fn observe(&self) -> Value {
        json!({
            "code": self.code.observe(),
            "msg": self.msg.observe(),
            "data": self.data.observe(),
        })
    }
}
impl Observe for FetchHedgeModeResponseData {
    fn observe(&self) -> Value {
        json!({
            "dualSidePosition": self.dual_side_position.observe(),
        })
    }
}
impl Observe for FetchHedgeModeResponse {
    fn observe(&self) -> Value {
        json!({
            "code": self.code.observe(),
            "msg": self.msg.observe(),
            "timeStamp": self.time_stamp.observe(),
            "data": self.data.observe(),
        })
    }
}
impl Observe for OrderTradeUpdateData {
    fn observe(&self) -> Value {
        let mut value = json!({
            "s": self.s.observe(),
            "i": self.i.observe(),
            "S": self.side.observe(),
            "o": self.o.observe(),
            "q": self.q.observe(),
            "p": self.p.observe(),
            "sp": self.sp.observe(),
            "ap": self.ap.observe(),
            "x": self.x.observe(),
            "X": self.status.observe(),
            "N": self.commission_asset.observe(),
            "n": self.n.observe(),
            "T": self.trade_time.observe(),
            "wt": self.wt.observe(),
            "ps": self.ps.observe(),
            "rp": self.rp.observe(),
            "z": self.z.observe(),
            "sg": self.sg.observe(),
            "ti": self.ti.observe(),
            "R": self.reduce_only.observe(),
        });
        if self.reduce_only.is_none() {
            value.as_object_mut().unwrap().remove("R");
        }
        value
    }
}
impl Observe for OrderTradeUpdate {
    fn observe(&self) -> Value {
        json!({
            "e": self.e.observe(),
            "E": self.event_time.observe(),
            "o": self.o.observe(),
        })
    }
}
impl Observe for FetchPositionsResponseData {
    fn observe(&self) -> Value {
        json!({
            "symbol": self.symbol.observe(),
            "positionId": self.position_id.observe(),
            "positionSide": self.position_side.observe(),
            "isolated": self.isolated.observe(),
            "positionAmt": self.position_amt.observe(),
            "availableAmt": self.available_amt.observe(),
            "unrealizedProfit": self.unrealized_profit.observe(),
            "realisedProfit": self.realised_profit.observe(),
            "initialMargin": self.initial_margin.observe(),
            "Margin": self.margin.observe(),
            "avgPrice": self.avg_price.observe(),
            "liquidationPrice": self.liquidation_price.observe(),
            "leverage": self.leverage.observe(),
            "positionValue": self.position_value.observe(),
            "markPrice": self.mark_price.observe(),
            "riskRate": self.risk_rate.observe(),
            "maxMarginReduction": self.max_margin_reduction.observe(),
            "pnlRatio": self.pnl_ratio.observe(),
            "updateTime": self.update_time.observe(),
        })
    }
}
impl Observe for FetchPositionsResponse {
    fn observe(&self) -> Value {
        json!({
            "code": self.code.observe(),
            "msg": self.msg.observe(),
            "data": self.data.observe(),
        })
    }
}
impl Observe for BingxOrderStatus {
    fn observe(&self) -> Value {
        match self {
            Self::Canceled => json!("CANCELED"),
            Self::Filled => json!("FILLED"),
            Self::New => json!("NEW"),
            Self::Open => json!("OPEN"),
            Self::PartiallyFilled => json!("PARTIALLY_FILLED"),
            Self::Pending => json!("PENDING"),
            Self::Working => json!("WORKING"),
            Self::Expired => json!("EXPIRED"),
        }
    }
}
impl Observe for BingxMarginType {
    fn observe(&self) -> Value {
        match self {
            Self::Crossed => json!("CROSSED"),
            Self::Isolated => json!("ISOLATED"),
        }
    }
}
impl Observe for BingxDualSidePosition {
    fn observe(&self) -> Value {
        match self {
            Self::True => json!("true"),
            Self::False => json!("false"),
        }
    }
}
impl Observe for OrderTradeUpdateEvent {
    fn observe(&self) -> Value {
        match self {
            Self::OrderTradeUpdate => json!("ORDER_TRADE_UPDATE"),
        }
    }
}
