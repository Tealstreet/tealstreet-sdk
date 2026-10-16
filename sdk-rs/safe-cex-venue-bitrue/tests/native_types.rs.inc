// Explicit native field access, independent of Serialize/Deserialize field names.
impl Native for BitrueOrderType {
    fn native(&self) -> Value {
        match self {
            Self::Limit => Value::String("LIMIT".into()),
            Self::Market => Value::String("MARKET".into()),
            Self::StopMarket => Value::String("STOP_MARKET".into()),
            Self::TakeProfitMarket => Value::String("TAKE_PROFIT_MARKET".into()),
        }
    }
}
impl Native for FetchAccountResponseDataAccountItemPositionVosItemPositionsItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("id".into(), self.id.native());
        out.insert("uid".into(), self.uid.native());
        out.insert("contractId".into(), self.contract_id.native());
        out.insert("positionType".into(), self.position_type.native());
        out.insert("side".into(), self.side.native());
        out.insert("volume".into(), self.volume.native());
        out.insert("openPrice".into(), self.open_price.native());
        out.insert("avgPrice".into(), self.avg_price.native());
        out.insert("closePrice".into(), self.close_price.native());
        out.insert("leverageLevel".into(), self.leverage_level.native());
        out.insert("openAmount".into(), self.open_amount.native());
        out.insert("holdAmount".into(), self.hold_amount.native());
        out.insert("closeVolume".into(), self.close_volume.native());
        out.insert(
            "pendingCloseVolume".into(),
            self.pending_close_volume.native(),
        );
        out.insert("realizedAmount".into(), self.realized_amount.native());
        out.insert(
            "historyRealizedAmount".into(),
            self.history_realized_amount.native(),
        );
        out.insert(
            "forceLiquidationVolume".into(),
            self.force_liquidation_volume.native(),
        );
        out.insert(
            "forceLiquidationPrice".into(),
            self.force_liquidation_price.native(),
        );
        out.insert("tradeFee".into(), self.trade_fee.native());
        out.insert("capitalFee".into(), self.capital_fee.native());
        out.insert("closeProfit".into(), self.close_profit.native());
        out.insert("shareAmount".into(), self.share_amount.native());
        out.insert("freezeLock".into(), self.freeze_lock.native());
        out.insert("status".into(), self.status.native());
        out.insert("ctime".into(), self.ctime.native());
        out.insert("mtime".into(), self.mtime.native());
        out.insert("brokerId".into(), self.broker_id.native());
        out.insert("usingAccountType".into(), self.using_account_type.native());
        out.insert("preShareAmount".into(), self.pre_share_amount.native());
        out.insert("marginRate".into(), self.margin_rate.native());
        out.insert("reducePrice".into(), self.reduce_price.native());
        out.insert("returnRate".into(), self.return_rate.native());
        out.insert("unRealizedAmount".into(), self.un_realized_amount.native());
        out.insert(
            "openRealizedAmount".into(),
            self.open_realized_amount.native(),
        );
        out.insert("positionBalance".into(), self.position_balance.native());
        out.insert("settleProfit".into(), self.settle_profit.native());
        out.insert("keepRate".into(), self.keep_rate.native());
        out.insert("maxFeeRate".into(), self.max_fee_rate.native());
        out.insert("version".into(), self.version.native());
        out.insert("indexPrice".into(), self.index_price.native());
        Value::Object(out)
    }
}
impl Native for FetchAccountResponseDataAccountItemPositionVosItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("contractId".into(), self.contract_id.native());
        out.insert("contractName".into(), self.contract_name.native());
        out.insert("contractSymbol".into(), self.contract_symbol.native());
        out.insert("adlEnabled".into(), self.adl_enabled.native());
        out.insert("positions".into(), self.positions.native());
        Value::Object(out)
    }
}
impl Native for FetchAccountResponseDataAccountItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("marginCoin".into(), self.margin_coin.native());
        out.insert("coinPrecious".into(), self.coin_precious.native());
        out.insert("exUsdtRate".into(), self.ex_usdt_rate.native());
        out.insert("exBtcRate".into(), self.ex_btc_rate.native());
        out.insert("accountNormal".into(), self.account_normal.native());
        out.insert("accountLock".into(), self.account_lock.native());
        out.insert(
            "partPositionNormal".into(),
            self.part_position_normal.native(),
        );
        out.insert(
            "totalPositionNormal".into(),
            self.total_position_normal.native(),
        );
        out.insert("achievedAmount".into(), self.achieved_amount.native());
        out.insert("unrealizedAmount".into(), self.unrealized_amount.native());
        out.insert("totalMarginRate".into(), self.total_margin_rate.native());
        out.insert("totalEquity".into(), self.total_equity.native());
        out.insert("partEquity".into(), self.part_equity.native());
        out.insert("totalCost".into(), self.total_cost.native());
        out.insert("sumMarginRate".into(), self.sum_margin_rate.native());
        out.insert(
            "sumOpenRealizedAmount".into(),
            self.sum_open_realized_amount.native(),
        );
        out.insert("canUseTrialFund".into(), self.can_use_trial_fund.native());
        out.insert(
            "sumMaintenanceMargin".into(),
            self.sum_maintenance_margin.native(),
        );
        out.insert("futureModel".into(), self.future_model.native());
        if let Some(value) = &self.position_vos {
            out.insert("positionVos".into(), value.native());
        }
        Value::Object(out)
    }
}
impl Native for FetchAccountResponseData {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("account".into(), self.account.native());
        Value::Object(out)
    }
}
impl Native for FetchAccountResponse {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("code".into(), self.code.native());
        out.insert("msg".into(), self.msg.native());
        out.insert("data".into(), self.data.native());
        Value::Object(out)
    }
}
impl Native for FetchContractsResponseItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("symbol".into(), self.symbol.native());
        out.insert("pricePrecision".into(), self.price_precision.native());
        out.insert("side".into(), self.side.native());
        out.insert("maxMarketVolume".into(), self.max_market_volume.native());
        out.insert("multiplier".into(), self.multiplier.native());
        out.insert("minOrderVolume".into(), self.min_order_volume.native());
        out.insert("maxMarketMoney".into(), self.max_market_money.native());
        out.insert("type".into(), self.r#type.native());
        out.insert("maxLimitVolume".into(), self.max_limit_volume.native());
        out.insert("maxValidOrder".into(), self.max_valid_order.native());
        out.insert("multiplierCoin".into(), self.multiplier_coin.native());
        out.insert("minOrderMoney".into(), self.min_order_money.native());
        out.insert("maxLimitMoney".into(), self.max_limit_money.native());
        out.insert("status".into(), self.status.native());
        Value::Object(out)
    }
}
impl Native for NumberOrString {
    fn native(&self) -> Value {
        match self {
            Self::Number(value) => value.native(),
            Self::String(value) => value.native(),
        }
    }
}
impl Native for BitrueLeverageBracket {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("bracket".into(), self.bracket.native());
        out.insert("initialLeverage".into(), self.initial_leverage.native());
        out.insert("maxPositionValue".into(), self.max_position_value.native());
        out.insert("minPositionValue".into(), self.min_position_value.native());
        out.insert("maintMarginRatio".into(), self.maint_margin_ratio.native());
        Value::Object(out)
    }
}
impl Native for BitrueLeverageBracketDataItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        if let Some(value) = &self.contract_name {
            out.insert("contractName".into(), value.native());
        }
        if let Some(value) = &self.brackets {
            out.insert("brackets".into(), value.native());
        }
        Value::Object(out)
    }
}
impl Native for FetchLeverageBracketResponseData {
    fn native(&self) -> Value {
        match self {
            Self::Object(value) => value.native(),
            Self::Array(value) => value.native(),
        }
    }
}
impl Native for FetchLeverageBracketResponse {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("code".into(), self.code.native());
        out.insert("msg".into(), self.msg.native());
        if let Some(value) = &self.data {
            out.insert("data".into(), value.native());
        }
        Value::Object(out)
    }
}
impl Native for FetchKlineResponseItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("high".into(), self.high.native());
        out.insert("vol".into(), self.vol.native());
        out.insert("last".into(), self.last.native());
        out.insert("low".into(), self.low.native());
        out.insert("rose".into(), self.rose.native());
        out.insert("time".into(), self.time.native());
        Value::Object(out)
    }
}
impl Native for OrderBookWsUpdate {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("asks".into(), self.asks.native());
        out.insert("buys".into(), self.buys.native());
        Value::Object(out)
    }
}
impl Native for TickerWsUpdate {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("amount".into(), self.amount.native());
        out.insert("close".into(), self.close.native());
        out.insert("data".into(), self.data.native());
        out.insert("high".into(), self.high.native());
        out.insert("low".into(), self.low.native());
        out.insert("open".into(), self.open.native());
        out.insert("rose".into(), self.rose.native());
        out.insert("vol".into(), self.vol.native());
        Value::Object(out)
    }
}
impl Native for FetchOrdersResponseDataItemTriggerPriceType {
    fn native(&self) -> Value {
        match self {
            Self::V1 => (1.0_f64).native(),
            Self::V2 => (2.0_f64).native(),
        }
    }
}
impl Native for FetchOrdersResponseDataItemTriggerType {
    fn native(&self) -> Value {
        match self {
            Self::V1 => (1.0_f64).native(),
            Self::V2 => (2.0_f64).native(),
        }
    }
}
impl Native for FetchOrdersResponseDataItemSide {
    fn native(&self) -> Value {
        match self {
            Self::Buy => Value::String("BUY".into()),
            Self::Sell => Value::String("SELL".into()),
        }
    }
}
impl Native for FetchOrdersResponseDataItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("triggerPrice".into(), self.trigger_price.native());
        if let Some(value) = &self.trigger_price_type {
            out.insert("triggerPriceType".into(), value.native());
        }
        if let Some(value) = &self.trigger_type {
            out.insert("triggerType".into(), value.native());
        }
        out.insert("orderId".into(), self.order_id.native());
        out.insert("clientOrderId".into(), self.client_order_id.native());
        out.insert("price".into(), self.price.native());
        out.insert("origQty".into(), self.orig_qty.native());
        out.insert("origAmount".into(), self.orig_amount.native());
        out.insert("executedQty".into(), self.executed_qty.native());
        out.insert("avgPrice".into(), self.avg_price.native());
        out.insert("status".into(), self.status.native());
        if let Some(value) = &self.r#type {
            out.insert("type".into(), value.native());
        }
        out.insert("side".into(), self.side.native());
        out.insert("action".into(), self.action.native());
        out.insert("transactTime".into(), self.transact_time.native());
        Value::Object(out)
    }
}
impl Native for FetchOrdersResponse {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("code".into(), self.code.native());
        out.insert("msg".into(), self.msg.native());
        out.insert("data".into(), self.data.native());
        Value::Object(out)
    }
}
impl Native for FetchMyTradesResponseDataItem {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("tradeId".into(), self.trade_id.native());
        out.insert("price".into(), self.price.native());
        out.insert("qty".into(), self.qty.native());
        out.insert("amount".into(), self.amount.native());
        out.insert("contractName".into(), self.contract_name.native());
        out.insert("side".into(), self.side.native());
        if let Some(value) = &self.fee {
            out.insert("fee".into(), value.native());
        }
        if let Some(value) = &self.bid_id {
            out.insert("bidId".into(), value.native());
        }
        if let Some(value) = &self.ask_id {
            out.insert("askId".into(), value.native());
        }
        if let Some(value) = &self.bid_user_id {
            out.insert("bidUserId".into(), value.native());
        }
        if let Some(value) = &self.ask_user_id {
            out.insert("askUserId".into(), value.native());
        }
        if let Some(value) = &self.is_buyer {
            out.insert("isBuyer".into(), value.native());
        }
        if let Some(value) = &self.is_maker {
            out.insert("isMaker".into(), value.native());
        }
        if let Some(value) = &self.ctime {
            out.insert("ctime".into(), value.native());
        }
        if let Some(value) = &self.c_time {
            out.insert("cTime".into(), value.native());
        }
        if let Some(value) = &self.time {
            out.insert("time".into(), value.native());
        }
        Value::Object(out)
    }
}
impl Native for FetchMyTradesResponse {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("code".into(), self.code.native());
        out.insert("msg".into(), self.msg.native());
        out.insert("data".into(), self.data.native());
        Value::Object(out)
    }
}
impl Native for WsOrderTradeUpdateO {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("L".into(), self.l_upper.native());
        out.insert("N".into(), self.n_upper.native());
        out.insert("R".into(), self.r_upper.native());
        out.insert("S".into(), self.s_upper.native());
        out.insert("T".into(), self.t_upper.native());
        out.insert("X".into(), self.x_upper.native());
        out.insert("ap".into(), self.ap.native());
        out.insert("c".into(), self.c.native());
        out.insert("i".into(), self.i.native());
        out.insert("l".into(), self.l.native());
        if let Some(value) = &self.volume {
            out.insert("volume".into(), value.native());
        }
        out.insert("m".into(), self.m.native());
        out.insert("n".into(), self.n.native());
        out.insert("o".into(), self.o.native());
        out.insert("p".into(), self.p.native());
        out.insert("q".into(), self.q.native());
        out.insert("s".into(), self.s.native());
        out.insert("t".into(), self.t.native());
        out.insert("x".into(), self.x.native());
        out.insert("z".into(), self.z.native());
        Value::Object(out)
    }
}
impl Native for WsOrderTradeUpdate {
    fn native(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("E".into(), self.e_upper.native());
        out.insert("e".into(), self.e.native());
        out.insert("o".into(), self.o.native());
        Value::Object(out)
    }
}
impl Native for OpaqueValue {
    fn native(&self) -> Value {
        self.value.native()
    }
}
