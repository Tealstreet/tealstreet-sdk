fn probe_BackpackAsset(v: &BackpackAsset) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("blockchain".into(), json!(&v.blockchain));
    out.insert("depositEnabled".into(), json!(&v.depositEnabled));
    out.insert("minimumDeposit".into(), json!(&v.minimumDeposit));
    out.insert("withdrawEnabled".into(), json!(&v.withdrawEnabled));
    out.insert("minimumWithdrawal".into(), json!(&v.minimumWithdrawal));
    out.insert("maximumWithdrawal".into(), json!(&v.maximumWithdrawal));
    out.insert("withdrawalFee".into(), json!(&v.withdrawalFee));
    Value::Object(out)
}
fn probe_BackpackMarketMarketType(v: &BackpackMarketMarketType) -> Value {
    json!(match v {
        BackpackMarketMarketType::Spot => "SPOT",
        BackpackMarketMarketType::Perp => "PERP",
    })
}
fn probe_BackpackMarketImfFunction(v: &BackpackMarketImfFunction) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("base".into(), json!(&v.base));
    out.insert("factor".into(), json!(&v.factor));
    out.insert("type".into(), json!(&v.r#type));
    Value::Object(out)
}
fn probe_BackpackMarketMmfFunction(v: &BackpackMarketMmfFunction) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("base".into(), json!(&v.base));
    out.insert("factor".into(), json!(&v.factor));
    out.insert("type".into(), json!(&v.r#type));
    Value::Object(out)
}
fn probe_BackpackMarketFiltersPrice(v: &BackpackMarketFiltersPrice) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.minPrice {
        out.insert("minPrice".into(), json!(v));
    }
    if let Some(v) = &v.maxPrice {
        out.insert("maxPrice".into(), json!(v));
    }
    if let Some(v) = &v.tickSize {
        out.insert("tickSize".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackMarketFiltersQuantity(v: &BackpackMarketFiltersQuantity) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.minQuantity {
        out.insert("minQuantity".into(), json!(v));
    }
    if let Some(v) = &v.maxQuantity {
        out.insert("maxQuantity".into(), json!(v));
    }
    if let Some(v) = &v.stepSize {
        out.insert("stepSize".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackMarketFiltersLeverage(v: &BackpackMarketFiltersLeverage) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.minLeverage {
        out.insert(
            "minLeverage".into(),
            json!(format!("{:016x}", (v).to_bits())),
        );
    }
    if let Some(v) = &v.maxLeverage {
        out.insert(
            "maxLeverage".into(),
            json!(format!("{:016x}", (v).to_bits())),
        );
    }
    Value::Object(out)
}
fn probe_BackpackMarketFilters(v: &BackpackMarketFilters) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.price {
        out.insert("price".into(), probe_BackpackMarketFiltersPrice(v));
    }
    if let Some(v) = &v.quantity {
        out.insert("quantity".into(), probe_BackpackMarketFiltersQuantity(v));
    }
    if let Some(v) = &v.leverage {
        out.insert("leverage".into(), probe_BackpackMarketFiltersLeverage(v));
    }
    Value::Object(out)
}
fn probe_BackpackMarket(v: &BackpackMarket) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("baseSymbol".into(), json!(&v.baseSymbol));
    out.insert("quoteSymbol".into(), json!(&v.quoteSymbol));
    out.insert(
        "marketType".into(),
        probe_BackpackMarketMarketType(&v.marketType),
    );
    out.insert("visible".into(), json!(&v.visible));
    out.insert("orderBookState".into(), json!(&v.orderBookState));
    out.insert("priceIncrement".into(), json!(&v.priceIncrement));
    out.insert("quantityIncrement".into(), json!(&v.quantityIncrement));
    out.insert("quoteIncrement".into(), json!(&v.quoteIncrement));
    out.insert("minNotional".into(), json!(&v.minNotional));
    out.insert("maxNotional".into(), json!(&v.maxNotional));
    out.insert("minQuantity".into(), json!(&v.minQuantity));
    out.insert("maxQuantity".into(), json!(&v.maxQuantity));
    if let Some(v) = &v.takerFeeRate {
        out.insert("takerFeeRate".into(), json!(v));
    }
    if let Some(v) = &v.makerFeeRate {
        out.insert("makerFeeRate".into(), json!(v));
    }
    if let Some(v) = &v.imfFunction {
        out.insert("imfFunction".into(), probe_BackpackMarketImfFunction(v));
    }
    if let Some(v) = &v.mmfFunction {
        out.insert("mmfFunction".into(), probe_BackpackMarketMmfFunction(v));
    }
    if let Some(v) = &v.filters {
        out.insert("filters".into(), probe_BackpackMarketFilters(v));
    }
    Value::Object(out)
}
fn probe_BackpackTicker(v: &BackpackTicker) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("firstPrice".into(), json!(&v.firstPrice));
    out.insert("lastPrice".into(), json!(&v.lastPrice));
    out.insert("priceChange".into(), json!(&v.priceChange));
    out.insert("priceChangePercent".into(), json!(&v.priceChangePercent));
    out.insert("high".into(), json!(&v.high));
    out.insert("low".into(), json!(&v.low));
    out.insert("volume".into(), json!(&v.volume));
    out.insert("quoteVolume".into(), json!(&v.quoteVolume));
    out.insert(
        "trades".into(),
        json!(format!("{:016x}", v.trades.to_bits())),
    );
    Value::Object(out)
}
fn probe_BackpackDepth(v: &BackpackDepth) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "lastUpdateId".into(),
        json!(format!("{:016x}", v.lastUpdateId.to_bits())),
    );
    out.insert(
        "bids".into(),
        Value::Array(
            v.bids
                .iter()
                .map(|v| json!([json!(&(v).0), json!(&(v).1)]))
                .collect(),
        ),
    );
    out.insert(
        "asks".into(),
        Value::Array(
            v.asks
                .iter()
                .map(|v| json!([json!(&(v).0), json!(&(v).1)]))
                .collect(),
        ),
    );
    Value::Object(out)
}
fn probe_BackpackTrade(v: &BackpackTrade) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".into(), json!(format!("{:016x}", v.id.to_bits())));
    out.insert("price".into(), json!(&v.price));
    out.insert("quantity".into(), json!(&v.quantity));
    out.insert("quoteQuantity".into(), json!(&v.quoteQuantity));
    out.insert(
        "timestamp".into(),
        json!(format!("{:016x}", v.timestamp.to_bits())),
    );
    out.insert("isBuyerMaker".into(), json!(&v.isBuyerMaker));
    Value::Object(out)
}
fn probe_BackpackKline(v: &BackpackKline) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("start".into(), json!(&v.start));
    out.insert("open".into(), json!(&v.open));
    out.insert("high".into(), json!(&v.high));
    out.insert("low".into(), json!(&v.low));
    out.insert("close".into(), json!(&v.close));
    out.insert("volume".into(), json!(&v.volume));
    out.insert("quoteVolume".into(), json!(&v.quoteVolume));
    out.insert("end".into(), json!(&v.end));
    out.insert("trades".into(), json!(&v.trades));
    Value::Object(out)
}
fn probe_BackpackMarkPrice(v: &BackpackMarkPrice) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("markPrice".into(), json!(&v.markPrice));
    out.insert("indexPrice".into(), json!(&v.indexPrice));
    out.insert("fundingRate".into(), json!(&v.fundingRate));
    out.insert(
        "nextFundingTimestamp".into(),
        json!(format!("{:016x}", v.nextFundingTimestamp.to_bits())),
    );
    Value::Object(out)
}
fn probe_BackpackBalance(v: &BackpackBalance) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.asset {
        out.insert("asset".into(), json!(v));
    }
    out.insert("available".into(), json!(&v.available));
    out.insert("locked".into(), json!(&v.locked));
    if let Some(v) = &v.staked {
        out.insert("staked".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackOrderSide(v: &BackpackOrderSide) -> Value {
    json!(match v {
        BackpackOrderSide::Bid => "Bid",
        BackpackOrderSide::Ask => "Ask",
    })
}
fn probe_BackpackOrderOrderType(v: &BackpackOrderOrderType) -> Value {
    json!(match v {
        BackpackOrderOrderType::Limit => "Limit",
        BackpackOrderOrderType::Market => "Market",
    })
}
fn probe_BackpackOrderTriggerBy(v: &BackpackOrderTriggerBy) -> Value {
    json!(match v {
        BackpackOrderTriggerBy::MarkPrice => "MarkPrice",
        BackpackOrderTriggerBy::IndexPrice => "IndexPrice",
        BackpackOrderTriggerBy::LastPrice => "LastPrice",
    })
}
fn probe_BackpackOrderTimeInForce(v: &BackpackOrderTimeInForce) -> Value {
    json!(match v {
        BackpackOrderTimeInForce::Gtc => "GTC",
        BackpackOrderTimeInForce::Ioc => "IOC",
        BackpackOrderTimeInForce::Fok => "FOK",
        BackpackOrderTimeInForce::Po => "PO",
    })
}
fn probe_BackpackOrderSelfTradePrevention(v: &BackpackOrderSelfTradePrevention) -> Value {
    json!(match v {
        BackpackOrderSelfTradePrevention::RejectTaker => "RejectTaker",
        BackpackOrderSelfTradePrevention::RejectMaker => "RejectMaker",
        BackpackOrderSelfTradePrevention::RejectBoth => "RejectBoth",
        BackpackOrderSelfTradePrevention::Allow => "Allow",
    })
}
fn probe_BackpackOrderStatus(v: &BackpackOrderStatus) -> Value {
    json!(match v {
        BackpackOrderStatus::New => "New",
        BackpackOrderStatus::PartiallyFilled => "PartiallyFilled",
        BackpackOrderStatus::Filled => "Filled",
        BackpackOrderStatus::Cancelled => "Cancelled",
        BackpackOrderStatus::Expired => "Expired",
        BackpackOrderStatus::TriggerPending => "TriggerPending",
    })
}
fn probe_BackpackOrder(v: &BackpackOrder) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("id".into(), json!(&v.id));
    if let Some(v) = &v.clientId {
        out.insert("clientId".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("side".into(), probe_BackpackOrderSide(&v.side));
    out.insert(
        "orderType".into(),
        probe_BackpackOrderOrderType(&v.orderType),
    );
    out.insert("price".into(), json!(&v.price));
    if let Some(v) = &v.triggerPrice {
        out.insert("triggerPrice".into(), json!(v));
    }
    if let Some(v) = &v.triggerBy {
        out.insert(
            "triggerBy".into(),
            (v).as_ref()
                .map_or(Value::Null, probe_BackpackOrderTriggerBy),
        );
    }
    out.insert("quantity".into(), json!(&v.quantity));
    if let Some(v) = &v.triggerQuantity {
        out.insert("triggerQuantity".into(), json!(v));
    }
    if let Some(v) = &v.quoteQuantity {
        out.insert("quoteQuantity".into(), json!(v));
    }
    out.insert(
        "timeInForce".into(),
        probe_BackpackOrderTimeInForce(&v.timeInForce),
    );
    out.insert(
        "selfTradePrevention".into(),
        probe_BackpackOrderSelfTradePrevention(&v.selfTradePrevention),
    );
    out.insert("status".into(), probe_BackpackOrderStatus(&v.status));
    out.insert(
        "createdAt".into(),
        json!(format!("{:016x}", v.createdAt.to_bits())),
    );
    if let Some(v) = &v.executedQuantity {
        out.insert("executedQuantity".into(), json!(v));
    }
    if let Some(v) = &v.executedQuoteQuantity {
        out.insert("executedQuoteQuantity".into(), json!(v));
    }
    if let Some(v) = &v.reduceOnly {
        out.insert("reduceOnly".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackOrderRequestSide(v: &BackpackOrderRequestSide) -> Value {
    json!(match v {
        BackpackOrderRequestSide::Bid => "Bid",
        BackpackOrderRequestSide::Ask => "Ask",
    })
}
fn probe_BackpackOrderRequestOrderType(v: &BackpackOrderRequestOrderType) -> Value {
    json!(match v {
        BackpackOrderRequestOrderType::Limit => "Limit",
        BackpackOrderRequestOrderType::Market => "Market",
    })
}
fn probe_BackpackOrderRequestTimeInForce(v: &BackpackOrderRequestTimeInForce) -> Value {
    json!(match v {
        BackpackOrderRequestTimeInForce::Gtc => "GTC",
        BackpackOrderRequestTimeInForce::Ioc => "IOC",
        BackpackOrderRequestTimeInForce::Fok => "FOK",
        BackpackOrderRequestTimeInForce::Po => "PO",
    })
}
fn probe_BackpackOrderRequestSelfTradePrevention(
    v: &BackpackOrderRequestSelfTradePrevention,
) -> Value {
    json!(match v {
        BackpackOrderRequestSelfTradePrevention::RejectTaker => "RejectTaker",
        BackpackOrderRequestSelfTradePrevention::RejectMaker => "RejectMaker",
        BackpackOrderRequestSelfTradePrevention::RejectBoth => "RejectBoth",
        BackpackOrderRequestSelfTradePrevention::Allow => "Allow",
    })
}
fn probe_BackpackOrderRequestTriggerBy(v: &BackpackOrderRequestTriggerBy) -> Value {
    json!(match v {
        BackpackOrderRequestTriggerBy::MarkPrice => "MarkPrice",
        BackpackOrderRequestTriggerBy::IndexPrice => "IndexPrice",
        BackpackOrderRequestTriggerBy::LastPrice => "LastPrice",
    })
}
fn probe_BackpackOrderRequestTakeProfitTriggerBy(
    v: &BackpackOrderRequestTakeProfitTriggerBy,
) -> Value {
    json!(match v {
        BackpackOrderRequestTakeProfitTriggerBy::MarkPrice => "MarkPrice",
        BackpackOrderRequestTakeProfitTriggerBy::IndexPrice => "IndexPrice",
        BackpackOrderRequestTakeProfitTriggerBy::LastPrice => "LastPrice",
    })
}
fn probe_BackpackOrderRequestStopLossTriggerBy(v: &BackpackOrderRequestStopLossTriggerBy) -> Value {
    json!(match v {
        BackpackOrderRequestStopLossTriggerBy::MarkPrice => "MarkPrice",
        BackpackOrderRequestStopLossTriggerBy::IndexPrice => "IndexPrice",
        BackpackOrderRequestStopLossTriggerBy::LastPrice => "LastPrice",
    })
}
fn probe_BackpackOrderRequest(v: &BackpackOrderRequest) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("side".into(), probe_BackpackOrderRequestSide(&v.side));
    out.insert(
        "orderType".into(),
        probe_BackpackOrderRequestOrderType(&v.orderType),
    );
    if let Some(v) = &v.price {
        out.insert("price".into(), json!(v));
    }
    if let Some(v) = &v.triggerPrice {
        out.insert("triggerPrice".into(), json!(v));
    }
    if let Some(v) = &v.quantity {
        out.insert("quantity".into(), json!(v));
    }
    if let Some(v) = &v.triggerQuantity {
        out.insert("triggerQuantity".into(), json!(v));
    }
    if let Some(v) = &v.quoteQuantity {
        out.insert("quoteQuantity".into(), json!(v));
    }
    if let Some(v) = &v.timeInForce {
        out.insert(
            "timeInForce".into(),
            probe_BackpackOrderRequestTimeInForce(v),
        );
    }
    if let Some(v) = &v.selfTradePrevention {
        out.insert(
            "selfTradePrevention".into(),
            probe_BackpackOrderRequestSelfTradePrevention(v),
        );
    }
    if let Some(v) = &v.clientId {
        out.insert("clientId".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    if let Some(v) = &v.postOnly {
        out.insert("postOnly".into(), json!(v));
    }
    if let Some(v) = &v.reduceOnly {
        out.insert("reduceOnly".into(), json!(v));
    }
    if let Some(v) = &v.triggerBy {
        out.insert("triggerBy".into(), probe_BackpackOrderRequestTriggerBy(v));
    }
    if let Some(v) = &v.takeProfitTriggerPrice {
        out.insert("takeProfitTriggerPrice".into(), json!(v));
    }
    if let Some(v) = &v.takeProfitLimitPrice {
        out.insert("takeProfitLimitPrice".into(), json!(v));
    }
    if let Some(v) = &v.takeProfitTriggerBy {
        out.insert(
            "takeProfitTriggerBy".into(),
            probe_BackpackOrderRequestTakeProfitTriggerBy(v),
        );
    }
    if let Some(v) = &v.stopLossTriggerPrice {
        out.insert("stopLossTriggerPrice".into(), json!(v));
    }
    if let Some(v) = &v.stopLossLimitPrice {
        out.insert("stopLossLimitPrice".into(), json!(v));
    }
    if let Some(v) = &v.stopLossTriggerBy {
        out.insert(
            "stopLossTriggerBy".into(),
            probe_BackpackOrderRequestStopLossTriggerBy(v),
        );
    }
    Value::Object(out)
}
fn probe_BackpackFillSide(v: &BackpackFillSide) -> Value {
    json!(match v {
        BackpackFillSide::Bid => "Bid",
        BackpackFillSide::Ask => "Ask",
    })
}
fn probe_BackpackFill(v: &BackpackFill) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.clientId {
        out.insert("clientId".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    out.insert("orderId".into(), json!(&v.orderId));
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("side".into(), probe_BackpackFillSide(&v.side));
    out.insert("price".into(), json!(&v.price));
    out.insert("quantity".into(), json!(&v.quantity));
    out.insert("fee".into(), json!(&v.fee));
    out.insert("feeSymbol".into(), json!(&v.feeSymbol));
    out.insert(
        "tradeId".into(),
        json!(format!("{:016x}", v.tradeId.to_bits())),
    );
    out.insert("timestamp".into(), json!(&v.timestamp));
    out.insert("isMaker".into(), json!(&v.isMaker));
    Value::Object(out)
}
fn probe_BackpackCollateralCollateralItem(v: &BackpackCollateralCollateralItem) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("assetMarkPrice".into(), json!(&v.assetMarkPrice));
    out.insert("availableQuantity".into(), json!(&v.availableQuantity));
    out.insert("balanceNotional".into(), json!(&v.balanceNotional));
    out.insert("collateralValue".into(), json!(&v.collateralValue));
    out.insert("collateralWeight".into(), json!(&v.collateralWeight));
    out.insert("lendQuantity".into(), json!(&v.lendQuantity));
    out.insert("openOrderQuantity".into(), json!(&v.openOrderQuantity));
    out.insert("totalQuantity".into(), json!(&v.totalQuantity));
    Value::Object(out)
}
fn probe_BackpackCollateral(v: &BackpackCollateral) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("assetsValue".into(), json!(&v.assetsValue));
    out.insert("borrowLiability".into(), json!(&v.borrowLiability));
    out.insert(
        "collateral".into(),
        Value::Array(
            v.collateral
                .iter()
                .map(probe_BackpackCollateralCollateralItem)
                .collect(),
        ),
    );
    out.insert("imf".into(), json!(&v.imf));
    out.insert("liabilitiesValue".into(), json!(&v.liabilitiesValue));
    out.insert("marginFraction".into(), json!(&v.marginFraction));
    out.insert("mmf".into(), json!(&v.mmf));
    out.insert("netEquity".into(), json!(&v.netEquity));
    out.insert("netEquityAvailable".into(), json!(&v.netEquityAvailable));
    out.insert("netEquityLocked".into(), json!(&v.netEquityLocked));
    out.insert("netExposureFutures".into(), json!(&v.netExposureFutures));
    out.insert("pnlUnrealized".into(), json!(&v.pnlUnrealized));
    out.insert("unsettledEquity".into(), json!(&v.unsettledEquity));
    Value::Object(out)
}
fn probe_BackpackPosition(v: &BackpackPosition) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), json!(&v.symbol));
    out.insert("positionId".into(), json!(&v.positionId));
    out.insert("netQuantity".into(), json!(&v.netQuantity));
    out.insert("netExposureQuantity".into(), json!(&v.netExposureQuantity));
    out.insert("netExposureNotional".into(), json!(&v.netExposureNotional));
    out.insert("netCost".into(), json!(&v.netCost));
    out.insert("entryPrice".into(), json!(&v.entryPrice));
    out.insert("markPrice".into(), json!(&v.markPrice));
    out.insert("breakEvenPrice".into(), json!(&v.breakEvenPrice));
    out.insert("estLiquidationPrice".into(), json!(&v.estLiquidationPrice));
    out.insert("pnlUnrealized".into(), json!(&v.pnlUnrealized));
    out.insert("pnlRealized".into(), json!(&v.pnlRealized));
    out.insert(
        "cumulativeFundingPayment".into(),
        json!(&v.cumulativeFundingPayment),
    );
    out.insert("cumulativeInterest".into(), json!(&v.cumulativeInterest));
    out.insert("imf".into(), json!(&v.imf));
    out.insert("mmf".into(), json!(&v.mmf));
    out.insert(
        "userId".into(),
        json!(format!("{:016x}", v.userId.to_bits())),
    );
    out.insert(
        "subaccountId".into(),
        v.subaccountId.as_ref().map_or(Value::Null, |v| json!(v)),
    );
    Value::Object(out)
}
fn probe_BackpackWsSubscribeMessageMethod(v: &BackpackWsSubscribeMessageMethod) -> Value {
    json!(match v {
        BackpackWsSubscribeMessageMethod::Subscribe => "SUBSCRIBE",
    })
}
fn probe_BackpackWsSubscribeMessage(v: &BackpackWsSubscribeMessage) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "method".into(),
        probe_BackpackWsSubscribeMessageMethod(&v.method),
    );
    out.insert(
        "params".into(),
        Value::Array(v.params.iter().map(|v| json!(v)).collect()),
    );
    if let Some(v) = &v.signature {
        out.insert("signature".into(), probe_BackpackWsSignature(v));
    }
    Value::Object(out)
}
fn probe_BackpackWsUnsubscribeMessageMethod(v: &BackpackWsUnsubscribeMessageMethod) -> Value {
    json!(match v {
        BackpackWsUnsubscribeMessageMethod::Unsubscribe => "UNSUBSCRIBE",
    })
}
fn probe_BackpackWsUnsubscribeMessage(v: &BackpackWsUnsubscribeMessage) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "method".into(),
        probe_BackpackWsUnsubscribeMessageMethod(&v.method),
    );
    out.insert(
        "params".into(),
        Value::Array(v.params.iter().map(|v| json!(v)).collect()),
    );
    Value::Object(out)
}
fn probe_BackpackWsSignature(v: &BackpackWsSignature) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("key".into(), json!(&v.key));
    out.insert("signature".into(), json!(&v.signature));
    out.insert(
        "timestamp".into(),
        json!(format!("{:016x}", v.timestamp.to_bits())),
    );
    out.insert(
        "window".into(),
        json!(format!("{:016x}", v.window.to_bits())),
    );
    Value::Object(out)
}
fn probe_BackpackWsTickerUpdateE(v: &BackpackWsTickerUpdateE) -> Value {
    json!(match v {
        BackpackWsTickerUpdateE::Ticker => "ticker",
    })
}
fn probe_BackpackWsTickerUpdate(v: &BackpackWsTickerUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), probe_BackpackWsTickerUpdateE(&v.e));
    out.insert("s".into(), json!(&v.s));
    out.insert("p".into(), json!(&v.p));
    out.insert("P".into(), json!(&v.P));
    out.insert("o".into(), json!(&v.o));
    out.insert("h".into(), json!(&v.h));
    out.insert("l".into(), json!(&v.l));
    out.insert("c".into(), json!(&v.c));
    out.insert("v".into(), json!(&v.v));
    out.insert("q".into(), json!(&v.q));
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    Value::Object(out)
}
fn probe_BackpackWsDepthUpdateE(v: &BackpackWsDepthUpdateE) -> Value {
    json!(match v {
        BackpackWsDepthUpdateE::Depth => "depth",
    })
}
fn probe_BackpackWsDepthUpdate(v: &BackpackWsDepthUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), probe_BackpackWsDepthUpdateE(&v.e));
    out.insert("s".into(), json!(&v.s));
    out.insert(
        "b".into(),
        Value::Array(
            v.b.iter()
                .map(|v| json!([json!(&(v).0), json!(&(v).1)]))
                .collect(),
        ),
    );
    out.insert(
        "a".into(),
        Value::Array(
            v.a.iter()
                .map(|v| json!([json!(&(v).0), json!(&(v).1)]))
                .collect(),
        ),
    );
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    if let Some(v) = &v.T {
        out.insert("T".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    if let Some(v) = &v.U {
        out.insert("U".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    if let Some(v) = &v.u {
        out.insert("u".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    Value::Object(out)
}
fn probe_BackpackWsTradeUpdateE(v: &BackpackWsTradeUpdateE) -> Value {
    json!(match v {
        BackpackWsTradeUpdateE::Trade => "trade",
    })
}
fn probe_BackpackWsTradeUpdate(v: &BackpackWsTradeUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), probe_BackpackWsTradeUpdateE(&v.e));
    out.insert("s".into(), json!(&v.s));
    out.insert("i".into(), json!(format!("{:016x}", v.i.to_bits())));
    out.insert("p".into(), json!(&v.p));
    out.insert("q".into(), json!(&v.q));
    out.insert("m".into(), json!(&v.m));
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    Value::Object(out)
}
fn probe_BackpackWsKlineUpdateE(v: &BackpackWsKlineUpdateE) -> Value {
    json!(match v {
        BackpackWsKlineUpdateE::Kline => "kline",
    })
}
fn probe_BackpackWsKlineUpdate(v: &BackpackWsKlineUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), probe_BackpackWsKlineUpdateE(&v.e));
    out.insert("s".into(), json!(&v.s));
    if let Some(v) = &v.i {
        out.insert("i".into(), json!(v));
    }
    out.insert("t".into(), json!(&v.t));
    out.insert("T".into(), json!(&v.T));
    out.insert("o".into(), v.o.as_ref().map_or(Value::Null, |v| json!(v)));
    out.insert("h".into(), v.h.as_ref().map_or(Value::Null, |v| json!(v)));
    out.insert("l".into(), v.l.as_ref().map_or(Value::Null, |v| json!(v)));
    out.insert("c".into(), v.c.as_ref().map_or(Value::Null, |v| json!(v)));
    out.insert("v".into(), v.v.as_ref().map_or(Value::Null, |v| json!(v)));
    out.insert("n".into(), json!(format!("{:016x}", v.n.to_bits())));
    out.insert("X".into(), json!(&v.X));
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    Value::Object(out)
}
fn probe_BackpackWsBookTickerUpdateE(v: &BackpackWsBookTickerUpdateE) -> Value {
    json!(match v {
        BackpackWsBookTickerUpdateE::BookTicker => "bookTicker",
    })
}
fn probe_BackpackWsBookTickerUpdate(v: &BackpackWsBookTickerUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), probe_BackpackWsBookTickerUpdateE(&v.e));
    out.insert("s".into(), json!(&v.s));
    out.insert("b".into(), json!(&v.b));
    out.insert("B".into(), json!(&v.B));
    out.insert("a".into(), json!(&v.a));
    out.insert("A".into(), json!(&v.A));
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    Value::Object(out)
}
fn probe_BackpackWsOrderUpdateS(v: &BackpackWsOrderUpdateS) -> Value {
    json!(match v {
        BackpackWsOrderUpdateS::Bid => "Bid",
        BackpackWsOrderUpdateS::Ask => "Ask",
    })
}
fn probe_BackpackWsOrderUpdate(v: &BackpackWsOrderUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("e".into(), json!(&v.e));
    out.insert("i".into(), json!(&v.i));
    if let Some(v) = &v.c {
        out.insert("c".into(), json!(v));
    }
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    out.insert("T".into(), json!(format!("{:016x}", v.T.to_bits())));
    out.insert("X".into(), json!(&v.X));
    out.insert("s".into(), json!(&v.s));
    out.insert("o".into(), json!(&v.o));
    out.insert("S".into(), probe_BackpackWsOrderUpdateS(&v.S));
    if let Some(v) = &v.p {
        out.insert("p".into(), json!(v));
    }
    if let Some(v) = &v.P {
        out.insert("P".into(), json!(v));
    }
    out.insert("q".into(), json!(&v.q));
    if let Some(v) = &v.Q {
        out.insert("Q".into(), json!(v));
    }
    if let Some(v) = &v.l {
        out.insert("l".into(), json!(v));
    }
    if let Some(v) = &v.L {
        out.insert("L".into(), json!(v));
    }
    out.insert("z".into(), json!(&v.z));
    if let Some(v) = &v.Z {
        out.insert("Z".into(), json!(v));
    }
    if let Some(v) = &v.N {
        out.insert("N".into(), json!(v));
    }
    if let Some(v) = &v.n {
        out.insert("n".into(), json!(v));
    }
    if let Some(v) = &v.f {
        out.insert("f".into(), json!(v));
    }
    if let Some(v) = &v.r {
        out.insert("r".into(), json!(v));
    }
    if let Some(v) = &v.t {
        out.insert(
            "t".into(),
            (v).as_ref()
                .map_or(Value::Null, |v| json!(format!("{:016x}", (v).to_bits()))),
        );
    }
    if let Some(v) = &v.m {
        out.insert("m".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackWsPositionUpdate(v: &BackpackWsPositionUpdate) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("i".into(), json!(format!("{:016x}", v.i.to_bits())));
    out.insert("s".into(), json!(&v.s));
    out.insert("b".into(), json!(&v.b));
    if let Some(v) = &v.B {
        out.insert("B".into(), json!(v));
    }
    out.insert("M".into(), json!(&v.M));
    out.insert("n".into(), json!(&v.n));
    out.insert("Q".into(), json!(&v.Q));
    out.insert("q".into(), json!(&v.q));
    out.insert("P".into(), json!(&v.P));
    out.insert("p".into(), json!(&v.p));
    out.insert("l".into(), json!(&v.l));
    out.insert("m".into(), json!(&v.m));
    out.insert("f".into(), json!(&v.f));
    out.insert("E".into(), json!(format!("{:016x}", v.E.to_bits())));
    if let Some(v) = &v.T {
        out.insert("T".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    Value::Object(out)
}
fn probe_BackpackError(v: &BackpackError) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("code".into(), json!(format!("{:016x}", v.code.to_bits())));
    out.insert("msg".into(), json!(&v.msg));
    Value::Object(out)
}
fn probe_BackpackApiResponse(v: &BackpackApiResponse<f64>) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(v) = &v.data {
        out.insert("data".into(), json!(format!("{:016x}", (v).to_bits())));
    }
    if let Some(v) = &v.error {
        out.insert("error".into(), probe_BackpackError(v));
    }
    Value::Object(out)
}
fn probe_BackpackTime(v: &BackpackTime) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "serverTime".into(),
        json!(format!("{:016x}", v.serverTime.to_bits())),
    );
    Value::Object(out)
}
fn probe_BackpackStatusStatus(v: &BackpackStatusStatus) -> Value {
    json!(match v {
        BackpackStatusStatus::Normal => "normal",
        BackpackStatusStatus::Maintenance => "maintenance",
    })
}
fn probe_BackpackStatus(v: &BackpackStatus) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("status".into(), probe_BackpackStatusStatus(&v.status));
    if let Some(v) = &v.msg {
        out.insert("msg".into(), json!(v));
    }
    Value::Object(out)
}
fn probe_BackpackAccount(v: &BackpackAccount) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "autoBorrowSettlements".into(),
        json!(&v.autoBorrowSettlements),
    );
    out.insert("autoLend".into(), json!(&v.autoLend));
    out.insert("autoRealizePnl".into(), json!(&v.autoRealizePnl));
    out.insert("autoRepayBorrows".into(), json!(&v.autoRepayBorrows));
    out.insert("borrowLimit".into(), json!(&v.borrowLimit));
    out.insert("futuresMakerFee".into(), json!(&v.futuresMakerFee));
    out.insert("futuresTakerFee".into(), json!(&v.futuresTakerFee));
    out.insert("leverageLimit".into(), json!(&v.leverageLimit));
    out.insert(
        "limitOrders".into(),
        json!(format!("{:016x}", v.limitOrders.to_bits())),
    );
    out.insert("liquidating".into(), json!(&v.liquidating));
    out.insert("positionLimit".into(), json!(&v.positionLimit));
    out.insert("spotMakerFee".into(), json!(&v.spotMakerFee));
    out.insert("spotTakerFee".into(), json!(&v.spotTakerFee));
    out.insert(
        "triggerOrders".into(),
        json!(format!("{:016x}", v.triggerOrders.to_bits())),
    );
    Value::Object(out)
}
