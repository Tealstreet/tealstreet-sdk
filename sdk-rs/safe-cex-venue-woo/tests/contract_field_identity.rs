#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_venue_woo::v3_contracts::*;
use serde_json::Value;
fn check_woov3endpoint(value: &WooV3Endpoint, input: &Value, label: &str) {
    let wire = match value {
        WooV3Endpoint::PublicInstruments => "/v3/public/instruments",
        WooV3Endpoint::PublicFutures => "/v3/public/futures",
        WooV3Endpoint::PublicOrderbook => "/v3/public/orderbook",
        WooV3Endpoint::PublicKline => "/v3/public/kline",
        WooV3Endpoint::PublicKlineHistory => "/v3/public/klineHistory",
        WooV3Endpoint::TradeOrder => "/v3/trade/order",
        WooV3Endpoint::TradeOrders => "/v3/trade/orders",
        WooV3Endpoint::TradeAlgoOrder => "/v3/trade/algoOrder",
        WooV3Endpoint::TradeAlgoOrders => "/v3/trade/algoOrders",
        WooV3Endpoint::TradeAllOrders => "/v3/trade/allOrders",
        WooV3Endpoint::TradeCancelAllAfter => "/v3/trade/cancelAllAfter",
        WooV3Endpoint::TransactionHistory => "/v3/trade/transactionHistory",
        WooV3Endpoint::AccountInfo => "/v3/account/info",
        WooV3Endpoint::AccountListenKey => "/v3/account/listenKey",
        WooV3Endpoint::AssetBalances => "/v3/asset/balances",
        WooV3Endpoint::FuturesPositions => "/v3/futures/positions",
        WooV3Endpoint::FuturesPositionMode => "/v3/futures/positionMode",
        WooV3Endpoint::FuturesLeverage => "/v3/futures/leverage",
        WooV3Endpoint::SpotMarginLeverage => "/v3/spotMargin/leverage",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3httpmethod(value: &WooV3HttpMethod, input: &Value, label: &str) {
    let wire = match value {
        WooV3HttpMethod::Get => "GET",
        WooV3HttpMethod::Post => "POST",
        WooV3HttpMethod::Put => "PUT",
        WooV3HttpMethod::Delete => "DELETE",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3migrationgate(value: &WooV3MigrationGate, input: &Value, label: &str) {
    let wire = match value {
        WooV3MigrationGate::PublicReadParity => "publicReadParity",
        WooV3MigrationGate::PrivateReadParity => "privateReadParity",
        WooV3MigrationGate::RegularWriteDemoLifecycle => "regularWriteDemoLifecycle",
        WooV3MigrationGate::AlgoWriteDemoLifecycle => "algoWriteDemoLifecycle",
        WooV3MigrationGate::CancelAllAfterDemoLifecycle => "cancelAllAfterDemoLifecycle",
        WooV3MigrationGate::AccountWriteDemoLifecycle => "accountWriteDemoLifecycle",
        WooV3MigrationGate::PublicWebsocketParity => "publicWebsocketParity",
        WooV3MigrationGate::PrivateWebsocketParity => "privateWebsocketParity",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3migrationrouteruntimelifecycle(
    value: &WooV3MigrationRouteRuntimeLifecycle,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3MigrationRouteRuntimeLifecycle::AlreadyV3 => "already-v3",
        WooV3MigrationRouteRuntimeLifecycle::Ownerless => "ownerless",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3positionpositionside(value: &WooV3PositionPositionSide, input: &Value, label: &str) {
    let wire = match value {
        WooV3PositionPositionSide::Both => "BOTH",
        WooV3PositionPositionSide::Long => "LONG",
        WooV3PositionPositionSide::Short => "SHORT",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3accountinfopositionmode(
    value: &WooV3AccountInfoPositionMode,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3AccountInfoPositionMode::OneWay => "ONE_WAY",
        WooV3AccountInfoPositionMode::HedgeMode => "HEDGE_MODE",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3placeorderrequestside(
    value: &WooV3PlaceOrderRequestSide,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3PlaceOrderRequestSide::Buy => "BUY",
        WooV3PlaceOrderRequestSide::Sell => "SELL",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3placeorderrequesttype(
    value: &WooV3PlaceOrderRequestType,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3PlaceOrderRequestType::Limit => "LIMIT",
        WooV3PlaceOrderRequestType::Market => "MARKET",
        WooV3PlaceOrderRequestType::Ioc => "IOC",
        WooV3PlaceOrderRequestType::Fok => "FOK",
        WooV3PlaceOrderRequestType::PostOnly => "POST_ONLY",
        WooV3PlaceOrderRequestType::Ask => "ASK",
        WooV3PlaceOrderRequestType::Bid => "BID",
        WooV3PlaceOrderRequestType::Ac => "AC",
        WooV3PlaceOrderRequestType::Rpi => "RPI",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3placeorderrequestpositionside(
    value: &WooV3PlaceOrderRequestPositionSide,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3PlaceOrderRequestPositionSide::Both => "BOTH",
        WooV3PlaceOrderRequestPositionSide::Long => "LONG",
        WooV3PlaceOrderRequestPositionSide::Short => "SHORT",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3placeorderrequestmarginmode(
    value: &WooV3PlaceOrderRequestMarginMode,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3PlaceOrderRequestMarginMode::Cross => "CROSS",
        WooV3PlaceOrderRequestMarginMode::Isolated => "ISOLATED",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3placeorderrequestbidasklevel(
    value: &WooV3PlaceOrderRequestBidAskLevel,
    input: &Value,
    label: &str,
) {
    let wire = match value {
        WooV3PlaceOrderRequestBidAskLevel::Level1 => "1",
        WooV3PlaceOrderRequestBidAskLevel::Level2 => "2",
        WooV3PlaceOrderRequestBidAskLevel::Level3 => "3",
        WooV3PlaceOrderRequestBidAskLevel::Level4 => "4",
        WooV3PlaceOrderRequestBidAskLevel::Level5 => "5",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3listenkeyrequesttype(value: &WooV3ListenKeyRequestType, input: &Value, label: &str) {
    let wire = match value {
        WooV3ListenKeyRequestType::Websocket => "WEBSOCKET",
    };
    assert_eq!(Some(wire), input.as_str(), "{label} native field identity");
}
fn check_woov3instrumentbaseassetmultiplier(
    value: &WooV3InstrumentBaseAssetMultiplier,
    input: &Value,
    label: &str,
) {
    match value {
        WooV3InstrumentBaseAssetMultiplier::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentBaseAssetMultiplier::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentbasemin(value: &WooV3InstrumentBaseMin, input: &Value, label: &str) {
    match value {
        WooV3InstrumentBaseMin::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentBaseMin::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentbasemax(value: &WooV3InstrumentBaseMax, input: &Value, label: &str) {
    match value {
        WooV3InstrumentBaseMax::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentBaseMax::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentbasetick(value: &WooV3InstrumentBaseTick, input: &Value, label: &str) {
    match value {
        WooV3InstrumentBaseTick::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentBaseTick::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentquotetick(value: &WooV3InstrumentQuoteTick, input: &Value, label: &str) {
    match value {
        WooV3InstrumentQuoteTick::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentQuoteTick::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentminnotional(
    value: &WooV3InstrumentMinNotional,
    input: &Value,
    label: &str,
) {
    match value {
        WooV3InstrumentMinNotional::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentMinNotional::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3instrumentbaseimr(value: &WooV3InstrumentBaseIMR, input: &Value, label: &str) {
    match value {
        WooV3InstrumentBaseIMR::Decimal(value) => assert_eq!(
            Some(value.as_str()),
            input.as_str(),
            "{label} native field identity"
        ),
        WooV3InstrumentBaseIMR::Number(value) => assert_eq!(
            value.to_bits(),
            input.as_f64().unwrap().to_bits(),
            "{label} native field identity"
        ),
    }
}
fn check_woov3migrationroute(value: &WooV3MigrationRoute, input: &Value) {
    assert_eq!(
        Some((&value.id).as_str()),
        (&input["id"]).as_str(),
        "WooV3MigrationRoute.id native field identity"
    );
    check_woov3httpmethod(
        &value.legacy_method,
        &input["legacyMethod"],
        "WooV3MigrationRoute.legacy_method",
    );
    assert_eq!(
        Some((&value.legacy_path).as_str()),
        (&input["legacyPath"]).as_str(),
        "WooV3MigrationRoute.legacy_path native field identity"
    );
    check_woov3httpmethod(
        &value.v3_method,
        &input["v3Method"],
        "WooV3MigrationRoute.v3_method",
    );
    check_woov3endpoint(
        &value.v3_path,
        &input["v3Path"],
        "WooV3MigrationRoute.v3_path",
    );
    check_woov3migrationgate(&value.gate, &input["gate"], "WooV3MigrationRoute.gate");
    assert_eq!(
        Some((&value.consumer).as_str()),
        (&input["consumer"]).as_str(),
        "WooV3MigrationRoute.consumer native field identity"
    );
    assert_eq!(
        value.runtime_lifecycle.is_some(),
        input.get("runtimeLifecycle").is_some(),
        "WooV3MigrationRoute.runtime_lifecycle presence identity"
    );
    if let Some(actual) = &value.runtime_lifecycle {
        check_woov3migrationrouteruntimelifecycle(
            actual,
            &input["runtimeLifecycle"],
            "WooV3MigrationRoute.runtime_lifecycle",
        );
    }
}
fn check_woov3response(value: &WooV3Response<Vec<String>>, input: &Value) {
    assert_eq!(
        Some(value.success),
        (&input["success"]).as_bool(),
        "WooV3Response.success native field identity"
    );
    assert_eq!(
        &value.data,
        &serde_json::from_value::<Vec<String>>(input["data"].clone()).unwrap(),
        "WooV3Response.data native field identity"
    );
    assert_eq!(
        (&value.timestamp).to_bits(),
        (&input["timestamp"]).as_f64().unwrap().to_bits(),
        "WooV3Response.timestamp native field identity"
    );
}
fn check_woov3pagemeta(value: &WooV3PageMeta, input: &Value) {
    assert_eq!(
        (&value.total).to_bits(),
        (&input["total"]).as_f64().unwrap().to_bits(),
        "WooV3PageMeta.total native field identity"
    );
    assert_eq!(
        (&value.records_per_page).to_bits(),
        (&input["recordsPerPage"]).as_f64().unwrap().to_bits(),
        "WooV3PageMeta.records_per_page native field identity"
    );
    assert_eq!(
        (&value.current_page).to_bits(),
        (&input["currentPage"]).as_f64().unwrap().to_bits(),
        "WooV3PageMeta.current_page native field identity"
    );
}
fn check_woov3rows(value: &WooV3Rows<Vec<String>>, input: &Value) {
    assert_eq!(
        &value.rows,
        &serde_json::from_value::<Vec<Vec<String>>>(input["rows"].clone()).unwrap(),
        "WooV3Rows.rows native field identity"
    );
    assert_eq!(
        value.meta.is_some(),
        input.get("meta").is_some(),
        "WooV3Rows.meta presence identity"
    );
    if let Some(actual) = &value.meta {
        check_woov3pagemeta(actual, &input["meta"]);
    }
}
fn check_woov3instrument(value: &WooV3Instrument, input: &Value) {
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "WooV3Instrument.symbol native field identity"
    );
    assert_eq!(
        value.status.is_some(),
        input.get("status").is_some(),
        "WooV3Instrument.status presence identity"
    );
    if let Some(actual) = &value.status {
        assert_eq!(
            Some((actual).as_str()),
            (&input["status"]).as_str(),
            "WooV3Instrument.status native field identity"
        );
    }
    assert_eq!(
        Some((&value.base_asset).as_str()),
        (&input["baseAsset"]).as_str(),
        "WooV3Instrument.base_asset native field identity"
    );
    assert_eq!(
        Some((&value.quote_asset).as_str()),
        (&input["quoteAsset"]).as_str(),
        "WooV3Instrument.quote_asset native field identity"
    );
    assert_eq!(
        value.base_asset_multiplier.is_some(),
        input.get("baseAssetMultiplier").is_some(),
        "WooV3Instrument.base_asset_multiplier presence identity"
    );
    if let Some(actual) = &value.base_asset_multiplier {
        check_woov3instrumentbaseassetmultiplier(
            actual,
            &input["baseAssetMultiplier"],
            "WooV3Instrument.base_asset_multiplier",
        );
    }
    check_woov3instrumentbasemin(
        &value.base_min,
        &input["baseMin"],
        "WooV3Instrument.base_min",
    );
    check_woov3instrumentbasemax(
        &value.base_max,
        &input["baseMax"],
        "WooV3Instrument.base_max",
    );
    check_woov3instrumentbasetick(
        &value.base_tick,
        &input["baseTick"],
        "WooV3Instrument.base_tick",
    );
    check_woov3instrumentquotetick(
        &value.quote_tick,
        &input["quoteTick"],
        "WooV3Instrument.quote_tick",
    );
    check_woov3instrumentminnotional(
        &value.min_notional,
        &input["minNotional"],
        "WooV3Instrument.min_notional",
    );
    check_woov3instrumentbaseimr(
        &value.base_imr,
        &input["baseIMR"],
        "WooV3Instrument.base_imr",
    );
    assert_eq!(
        value.funding_interval_hours.is_some(),
        input.get("fundingIntervalHours").is_some(),
        "WooV3Instrument.funding_interval_hours presence identity"
    );
    if let Some(actual) = &value.funding_interval_hours {
        assert_eq!(
            (actual).to_bits(),
            (&input["fundingIntervalHours"]).as_f64().unwrap().to_bits(),
            "WooV3Instrument.funding_interval_hours native field identity"
        );
    }
    assert_eq!(
        value.is_allowed_rpi.is_some(),
        input.get("isAllowedRpi").is_some(),
        "WooV3Instrument.is_allowed_rpi presence identity"
    );
    if let Some(actual) = &value.is_allowed_rpi {
        assert_eq!(
            Some(*(actual)),
            (&input["isAllowedRpi"]).as_bool(),
            "WooV3Instrument.is_allowed_rpi native field identity"
        );
    }
}
fn check_woov3balance(value: &WooV3Balance, input: &Value) {
    assert_eq!(
        Some((&value.token).as_str()),
        (&input["token"]).as_str(),
        "WooV3Balance.token native field identity"
    );
    assert_eq!(
        Some((&value.holding).as_str()),
        (&input["holding"]).as_str(),
        "WooV3Balance.holding native field identity"
    );
    assert_eq!(
        Some((&value.frozen).as_str()),
        (&input["frozen"]).as_str(),
        "WooV3Balance.frozen native field identity"
    );
    assert_eq!(
        Some((&value.available_balance).as_str()),
        (&input["availableBalance"]).as_str(),
        "WooV3Balance.available_balance native field identity"
    );
    assert_eq!(
        Some((&value.pending_short_qty).as_str()),
        (&input["pendingShortQty"]).as_str(),
        "WooV3Balance.pending_short_qty native field identity"
    );
    assert_eq!(
        Some((&value.pending_long_qty).as_str()),
        (&input["pendingLongQty"]).as_str(),
        "WooV3Balance.pending_long_qty native field identity"
    );
    assert_eq!(
        Some((&value.average_open_price).as_str()),
        (&input["averageOpenPrice"]).as_str(),
        "WooV3Balance.average_open_price native field identity"
    );
    assert_eq!(
        Some((&value.mark_price).as_str()),
        (&input["markPrice"]).as_str(),
        "WooV3Balance.mark_price native field identity"
    );
    assert_eq!(
        (&value.timestamp).to_bits(),
        (&input["timestamp"]).as_f64().unwrap().to_bits(),
        "WooV3Balance.timestamp native field identity"
    );
}
fn check_woov3position(value: &WooV3Position, input: &Value) {
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "WooV3Position.symbol native field identity"
    );
    assert_eq!(
        Some((&value.holding).as_str()),
        (&input["holding"]).as_str(),
        "WooV3Position.holding native field identity"
    );
    check_woov3positionpositionside(
        &value.position_side,
        &input["positionSide"],
        "WooV3Position.position_side",
    );
    assert_eq!(
        Some((&value.pending_long_qty).as_str()),
        (&input["pendingLongQty"]).as_str(),
        "WooV3Position.pending_long_qty native field identity"
    );
    assert_eq!(
        Some((&value.pending_short_qty).as_str()),
        (&input["pendingShortQty"]).as_str(),
        "WooV3Position.pending_short_qty native field identity"
    );
    assert_eq!(
        Some((&value.settle_price).as_str()),
        (&input["settlePrice"]).as_str(),
        "WooV3Position.settle_price native field identity"
    );
    assert_eq!(
        Some((&value.average_open_price).as_str()),
        (&input["averageOpenPrice"]).as_str(),
        "WooV3Position.average_open_price native field identity"
    );
    assert_eq!(
        Some((&value.mark_price).as_str()),
        (&input["markPrice"]).as_str(),
        "WooV3Position.mark_price native field identity"
    );
    assert_eq!(
        Some((&value.est_liq_price).as_str()),
        (&input["estLiqPrice"]).as_str(),
        "WooV3Position.est_liq_price native field identity"
    );
    assert_eq!(
        (&value.timestamp).to_bits(),
        (&input["timestamp"]).as_f64().unwrap().to_bits(),
        "WooV3Position.timestamp native field identity"
    );
}
fn check_woov3accountinfo(value: &WooV3AccountInfo, input: &Value) {
    assert_eq!(
        Some((&value.application_id).as_str()),
        (&input["applicationId"]).as_str(),
        "WooV3AccountInfo.application_id native field identity"
    );
    assert_eq!(
        Some((&value.account).as_str()),
        (&input["account"]).as_str(),
        "WooV3AccountInfo.account native field identity"
    );
    assert_eq!(
        Some((&value.alias).as_str()),
        (&input["alias"]).as_str(),
        "WooV3AccountInfo.alias native field identity"
    );
    assert_eq!(
        Some((&value.account_mode).as_str()),
        (&input["accountMode"]).as_str(),
        "WooV3AccountInfo.account_mode native field identity"
    );
    check_woov3accountinfopositionmode(
        &value.position_mode,
        &input["positionMode"],
        "WooV3AccountInfo.position_mode",
    );
    assert_eq!(
        (&value.leverage).to_bits(),
        (&input["leverage"]).as_f64().unwrap().to_bits(),
        "WooV3AccountInfo.leverage native field identity"
    );
    assert_eq!(
        Some((&value.margin_ratio).as_str()),
        (&input["marginRatio"]).as_str(),
        "WooV3AccountInfo.margin_ratio native field identity"
    );
    assert_eq!(
        Some((&value.total_collateral).as_str()),
        (&input["totalCollateral"]).as_str(),
        "WooV3AccountInfo.total_collateral native field identity"
    );
    assert_eq!(
        Some((&value.total_account_value).as_str()),
        (&input["totalAccountValue"]).as_str(),
        "WooV3AccountInfo.total_account_value native field identity"
    );
    assert_eq!(
        Some((&value.account_type).as_str()),
        (&input["accountType"]).as_str(),
        "WooV3AccountInfo.account_type native field identity"
    );
}
fn check_woov3placeorderrequest(value: &WooV3PlaceOrderRequest, input: &Value) {
    assert_eq!(
        Some((&value.symbol).as_str()),
        (&input["symbol"]).as_str(),
        "WooV3PlaceOrderRequest.symbol native field identity"
    );
    check_woov3placeorderrequestside(&value.side, &input["side"], "WooV3PlaceOrderRequest.side");
    check_woov3placeorderrequesttype(
        &value.r#type,
        &input["type"],
        "WooV3PlaceOrderRequest.r#type",
    );
    assert_eq!(
        value.client_order_id.is_some(),
        input.get("clientOrderId").is_some(),
        "WooV3PlaceOrderRequest.client_order_id presence identity"
    );
    if let Some(actual) = &value.client_order_id {
        assert_eq!(
            (actual).to_bits(),
            (&input["clientOrderId"]).as_f64().unwrap().to_bits(),
            "WooV3PlaceOrderRequest.client_order_id native field identity"
        );
    }
    assert_eq!(
        value.order_tag.is_some(),
        input.get("orderTag").is_some(),
        "WooV3PlaceOrderRequest.order_tag presence identity"
    );
    if let Some(actual) = &value.order_tag {
        assert_eq!(
            Some((actual).as_str()),
            (&input["orderTag"]).as_str(),
            "WooV3PlaceOrderRequest.order_tag native field identity"
        );
    }
    assert_eq!(
        value.position_side.is_some(),
        input.get("positionSide").is_some(),
        "WooV3PlaceOrderRequest.position_side presence identity"
    );
    if let Some(actual) = &value.position_side {
        check_woov3placeorderrequestpositionside(
            actual,
            &input["positionSide"],
            "WooV3PlaceOrderRequest.position_side",
        );
    }
    assert_eq!(
        value.price.is_some(),
        input.get("price").is_some(),
        "WooV3PlaceOrderRequest.price presence identity"
    );
    if let Some(actual) = &value.price {
        assert_eq!(
            Some((actual).as_str()),
            (&input["price"]).as_str(),
            "WooV3PlaceOrderRequest.price native field identity"
        );
    }
    assert_eq!(
        value.quantity.is_some(),
        input.get("quantity").is_some(),
        "WooV3PlaceOrderRequest.quantity presence identity"
    );
    if let Some(actual) = &value.quantity {
        assert_eq!(
            Some((actual).as_str()),
            (&input["quantity"]).as_str(),
            "WooV3PlaceOrderRequest.quantity native field identity"
        );
    }
    assert_eq!(
        value.amount.is_some(),
        input.get("amount").is_some(),
        "WooV3PlaceOrderRequest.amount presence identity"
    );
    if let Some(actual) = &value.amount {
        assert_eq!(
            Some((actual).as_str()),
            (&input["amount"]).as_str(),
            "WooV3PlaceOrderRequest.amount native field identity"
        );
    }
    assert_eq!(
        value.reduce_only.is_some(),
        input.get("reduceOnly").is_some(),
        "WooV3PlaceOrderRequest.reduce_only presence identity"
    );
    if let Some(actual) = &value.reduce_only {
        assert_eq!(
            Some(*(actual)),
            (&input["reduceOnly"]).as_bool(),
            "WooV3PlaceOrderRequest.reduce_only native field identity"
        );
    }
    assert_eq!(
        value.visible_quantity.is_some(),
        input.get("visibleQuantity").is_some(),
        "WooV3PlaceOrderRequest.visible_quantity presence identity"
    );
    if let Some(actual) = &value.visible_quantity {
        assert_eq!(
            Some((actual).as_str()),
            (&input["visibleQuantity"]).as_str(),
            "WooV3PlaceOrderRequest.visible_quantity native field identity"
        );
    }
    assert_eq!(
        value.margin_mode.is_some(),
        input.get("marginMode").is_some(),
        "WooV3PlaceOrderRequest.margin_mode presence identity"
    );
    if let Some(actual) = &value.margin_mode {
        check_woov3placeorderrequestmarginmode(
            actual,
            &input["marginMode"],
            "WooV3PlaceOrderRequest.margin_mode",
        );
    }
    assert_eq!(
        value.bid_ask_level.is_some(),
        input.get("bidAskLevel").is_some(),
        "WooV3PlaceOrderRequest.bid_ask_level presence identity"
    );
    if let Some(actual) = &value.bid_ask_level {
        check_woov3placeorderrequestbidasklevel(
            actual,
            &input["bidAskLevel"],
            "WooV3PlaceOrderRequest.bid_ask_level",
        );
    }
    assert_eq!(
        value.post_only_adjusted.is_some(),
        input.get("postOnlyAdjusted").is_some(),
        "WooV3PlaceOrderRequest.post_only_adjusted presence identity"
    );
    if let Some(actual) = &value.post_only_adjusted {
        assert_eq!(
            Some(*(actual)),
            (&input["postOnlyAdjusted"]).as_bool(),
            "WooV3PlaceOrderRequest.post_only_adjusted native field identity"
        );
    }
}
fn check_woov3placeorderresult(value: &WooV3PlaceOrderResult, input: &Value) {
    assert_eq!(
        (&value.order_id).to_bits(),
        (&input["orderId"]).as_f64().unwrap().to_bits(),
        "WooV3PlaceOrderResult.order_id native field identity"
    );
    assert_eq!(
        Some((&value.r#type).as_str()),
        (&input["type"]).as_str(),
        "WooV3PlaceOrderResult.r#type native field identity"
    );
    assert_eq!(
        Some((&value.price).as_str()),
        (&input["price"]).as_str(),
        "WooV3PlaceOrderResult.price native field identity"
    );
    assert_eq!(
        Some((&value.quantity).as_str()),
        (&input["quantity"]).as_str(),
        "WooV3PlaceOrderResult.quantity native field identity"
    );
    assert_eq!(
        Some((&value.amount).as_str()),
        (&input["amount"]).as_str(),
        "WooV3PlaceOrderResult.amount native field identity"
    );
    assert_eq!(
        (&value.client_order_id).to_bits(),
        (&input["clientOrderId"]).as_f64().unwrap().to_bits(),
        "WooV3PlaceOrderResult.client_order_id native field identity"
    );
    assert_eq!(
        (&value.bid_ask_level).to_bits(),
        (&input["bidAskLevel"]).as_f64().unwrap().to_bits(),
        "WooV3PlaceOrderResult.bid_ask_level native field identity"
    );
}
fn check_woov3listenkeyrequest(value: &WooV3ListenKeyRequest, input: &Value) {
    check_woov3listenkeyrequesttype(
        &value.r#type,
        &input["type"],
        "WooV3ListenKeyRequest.r#type",
    );
}
fn check_woov3listenkeyresult(value: &WooV3ListenKeyResult, input: &Value) {
    assert_eq!(
        Some((&value.auth_key).as_str()),
        (&input["authKey"]).as_str(),
        "WooV3ListenKeyResult.auth_key native field identity"
    );
    assert_eq!(
        (&value.expired_time).to_bits(),
        (&input["expiredTime"]).as_f64().unwrap().to_bits(),
        "WooV3ListenKeyResult.expired_time native field identity"
    );
}
#[test]
fn contract_native_field_identity() {
    let fixtures: Value = serde_json::from_str(&identity::capture("contracts")).unwrap();
    let mut checked = std::collections::BTreeSet::new();
    for case in fixtures["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["accepted"] == true)
    {
        let input = &case["input"];
        let name = case["name"].as_str().unwrap();
        match name {
            "WooV3MigrationRoute" => {
                check_woov3migrationroute(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3Response" => {
                check_woov3response(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3PageMeta" => {
                check_woov3pagemeta(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3Rows" => check_woov3rows(&serde_json::from_value(input.clone()).unwrap(), input),
            "WooV3Instrument" => {
                check_woov3instrument(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3Balance" => {
                check_woov3balance(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3Position" => {
                check_woov3position(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3AccountInfo" => {
                check_woov3accountinfo(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3PlaceOrderRequest" => {
                check_woov3placeorderrequest(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3PlaceOrderResult" => {
                check_woov3placeorderresult(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3ListenKeyRequest" => {
                check_woov3listenkeyrequest(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            "WooV3ListenKeyResult" => {
                check_woov3listenkeyresult(&serde_json::from_value(input.clone()).unwrap(), input)
            }
            _ => continue,
        }
        checked.insert(name);
    }
    assert_eq!(
        checked.len(),
        12,
        "all contract records have native identity assertions"
    );
}
