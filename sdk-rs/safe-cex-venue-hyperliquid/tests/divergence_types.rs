use safe_cex_venue_hyperliquid::types;

#[test]
fn audit_duplicate_root_01() {
    let value: types::Meta = serde_json::from_str(r####"{"universe":[{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"maxLeverage":17,"onlyIsolated":true,"isDelisted":true,"marginTableId":17,"marginMode":"strictIsolated","growthMode":"enabled","lastGrowthModeChangeTime":"wire-雪\u0000"}]}"####)
            .expect("HL TYPES public duplicate decode Meta");
    let selected = value.universe[0].name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Meta /universe/0/name"
    );
}

#[test]
fn audit_duplicate_root_02() {
    let value: types::ClearinghouseState = serde_json::from_str(r####"{"assetPositions":[{"position":{"coin":"\ud800","coin":"雪😀﻿","cumFunding":{"allTime":"wire-雪\u0000","sinceChange":"wire-雪\u0000","sinceOpen":"wire-雪\u0000"},"entryPx":"wire-雪\u0000","leverage":{"rawUsd":"wire-雪\u0000","type":"wire-雪\u0000","value":17},"liquidationPx":"wire-雪\u0000","marginUsed":"wire-雪\u0000","maxLeverage":17,"positionValue":"wire-雪\u0000","returnOnEquity":"wire-雪\u0000","szi":"wire-雪\u0000","unrealizedPnl":"wire-雪\u0000"},"type":"wire-雪\u0000"}],"crossMaintenanceMarginUsed":"wire-雪\u0000","crossMarginSummary":{"accountValue":"wire-雪\u0000","totalMarginUsed":"wire-雪\u0000","totalNtlPos":"wire-雪\u0000","totalRawUsd":"wire-雪\u0000"},"marginSummary":{"accountValue":"wire-雪\u0000","totalMarginUsed":"wire-雪\u0000","totalNtlPos":"wire-雪\u0000","totalRawUsd":"wire-雪\u0000"},"time":17,"withdrawable":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode ClearinghouseState");
    let selected = value.asset_positions[0].position.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten ClearinghouseState /assetPositions/0/position/coin"
    );
}

#[test]
fn audit_duplicate_root_03() {
    let value: types::UserFills = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode UserFills");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserFills /coin"
    );
}

#[test]
fn audit_duplicate_root_04() {
    let value: types::OrderResponseStatus =
        serde_json::from_str(r####"{"resting":{"oid":"\ud800","oid":"雪😀﻿"}}"####)
            .expect("HL TYPES public duplicate decode OrderResponseStatus");
    let selected = match &value {
        types::OrderResponseStatus::Case0(resting) => match &resting.resting.oid {
            types::OrderResponseStatusCase0RestingOid::Case1(text) => text.as_str(),
            _ => panic!("HL TYPES native resting oid must be string"),
        },
        _ => panic!("HL TYPES native status must be resting"),
    };
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten OrderResponseStatus /resting/oid"
    );
}

#[test]
fn audit_duplicate_root_05() {
    let value: types::OrderResponse = serde_json::from_str(
        r####"{"status":"\ud800","status":"雪😀﻿","response":"wire-雪\u0000"}"####,
    )
    .expect("HL TYPES public duplicate decode OrderResponse");
    let selected = value.status.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten OrderResponse /status"
    );
}

#[test]
fn audit_duplicate_root_06() {
    let value: types::WsTrade = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","side":"wire-雪\u0000","px":"wire-雪\u0000","sz":"wire-雪\u0000","hash":"wire-雪\u0000","time":17,"tid":17}"####)
            .expect("HL TYPES public duplicate decode WsTrade");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsTrade /coin"
    );
}

#[test]
fn audit_duplicate_root_07() {
    let value: types::WsBook = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","levels":[[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}],[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}]],"time":17}"####)
            .expect("HL TYPES public duplicate decode WsBook");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsBook /coin"
    );
}

#[test]
fn audit_duplicate_root_08() {
    let value: types::WsBbo = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","bbo":[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17},{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}],"time":17}"####)
            .expect("HL TYPES public duplicate decode WsBbo");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsBbo /coin"
    );
}

#[test]
fn audit_duplicate_root_09() {
    let value: types::WsLevel =
        serde_json::from_str(r####"{"px":"\ud800","px":"雪😀﻿","sz":"wire-雪\u0000","n":17}"####)
            .expect("HL TYPES public duplicate decode WsLevel");
    let selected = value.px.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsLevel /px"
    );
}

#[test]
fn audit_duplicate_root_10() {
    let value: types::WsOrder = serde_json::from_str(r####"{"order":{"coin":"\ud800","coin":"雪😀﻿","side":"wire-雪\u0000","limitPx":"wire-雪\u0000","sz":"wire-雪\u0000","oid":17,"timestamp":17,"origSz":"wire-雪\u0000"},"status":"wire-雪\u0000","statusTimestamp":17,"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsOrder");
    let selected = value.order.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsOrder /order/coin"
    );
}

#[test]
fn audit_duplicate_root_11() {
    let value: types::WsFill = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode WsFill");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsFill /coin"
    );
}

#[test]
fn audit_duplicate_root_12() {
    let value: types::WsUserFunding = serde_json::from_str(r####"{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","szi":"wire-雪\u0000","fundingRate":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFunding");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFunding /coin"
    );
}

#[test]
fn audit_duplicate_root_13() {
    let value: types::WsLiquidation = serde_json::from_str(r####"{"lid":17,"liquidator":"\ud800","liquidator":"雪😀﻿","liquidated_user":"wire-雪\u0000","liquidated_ntl_pos":"wire-雪\u0000","liquidated_account_value":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsLiquidation");
    let selected = value.liquidator.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsLiquidation /liquidator"
    );
}

#[test]
fn audit_duplicate_root_14() {
    let value: types::WsNonUserCancel =
        serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","oid":17}"####)
            .expect("HL TYPES public duplicate decode WsNonUserCancel");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsNonUserCancel /coin"
    );
}

#[test]
fn audit_duplicate_root_15() {
    let value: types::SpotClearinghouseBalance = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","token":17,"hold":"wire-雪\u0000","total":"wire-雪\u0000","entryNtl":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode SpotClearinghouseBalance");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotClearinghouseBalance /coin"
    );
}

#[test]
fn audit_duplicate_root_16() {
    let value: types::SpotClearinghouseState = serde_json::from_str(r####"{"balances":[{"coin":"\ud800","coin":"雪😀﻿","token":17,"hold":"wire-雪\u0000","total":"wire-雪\u0000","entryNtl":"wire-雪\u0000"}],"tokenToAvailableAfterMaintenance":[[17,"wire-雪\u0000"]]}"####)
            .expect("HL TYPES public duplicate decode SpotClearinghouseState");
    let selected = value.balances[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotClearinghouseState /balances/0/coin"
    );
}

#[test]
fn audit_duplicate_root_17() {
    let value: types::FrontendOpenOrders = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","isPositionTpsl":true,"isTrigger":true,"limitPx":"wire-雪\u0000","oid":17,"orderType":"wire-雪\u0000","origSz":"wire-雪\u0000","reduceOnly":true,"side":"wire-雪\u0000","sz":"wire-雪\u0000","timestamp":17,"triggerCondition":"wire-雪\u0000","triggerPx":"wire-雪\u0000","tif":"Alo"}"####)
            .expect("HL TYPES public duplicate decode FrontendOpenOrders");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten FrontendOpenOrders /coin"
    );
}

#[test]
fn audit_duplicate_root_18() {
    let value: types::L2Book = serde_json::from_str(r####"{"levels":[[{"px":"\ud800","px":"雪😀﻿","sz":"wire-雪\u0000","n":17}],[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}]]}"####)
            .expect("HL TYPES public duplicate decode L2Book");
    let selected = value.levels.0[0].px.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten L2Book /levels/0/0/px"
    );
}

#[test]
fn audit_duplicate_root_19() {
    let value: types::CandleSnapshot = serde_json::from_str(r####"{"T":17,"c":"\ud800","c":"雪😀﻿","h":"wire-雪\u0000","i":"wire-雪\u0000","l":"wire-雪\u0000","n":17,"o":"wire-雪\u0000","s":"wire-雪\u0000","t":17,"v":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode CandleSnapshot");
    let selected = value.c.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten CandleSnapshot /c"
    );
}

#[test]
fn audit_duplicate_root_20() {
    let value: types::AssetCtx = serde_json::from_str(r####"{"dayNtlVlm":"\ud800","dayNtlVlm":"雪😀﻿","dayBaseVlm":"wire-雪\u0000","funding":"wire-雪\u0000","impactPxs":["wire-雪\u0000","wire-雪\u0000"],"markPx":"wire-雪\u0000","midPx":"wire-雪\u0000","openInterest":"wire-雪\u0000","oraclePx":"wire-雪\u0000","premium":"wire-雪\u0000","prevDayPx":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode AssetCtx");
    let selected = value.day_ntl_vlm.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten AssetCtx /dayNtlVlm"
    );
}

#[test]
fn audit_duplicate_root_21() {
    let value: types::MetaAndAssetCtxs = serde_json::from_str(r####"{"meta":{"universe":[{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"maxLeverage":17,"onlyIsolated":true,"isDelisted":true,"marginTableId":17,"marginMode":"strictIsolated","growthMode":"enabled","lastGrowthModeChangeTime":"wire-雪\u0000"}]},"assetCtxs":[{"dayNtlVlm":"wire-雪\u0000","dayBaseVlm":"wire-雪\u0000","funding":"wire-雪\u0000","impactPxs":["wire-雪\u0000","wire-雪\u0000"],"markPx":"wire-雪\u0000","midPx":"wire-雪\u0000","openInterest":"wire-雪\u0000","oraclePx":"wire-雪\u0000","premium":"wire-雪\u0000","prevDayPx":"wire-雪\u0000"}]}"####)
            .expect("HL TYPES public duplicate decode MetaAndAssetCtxs");
    let selected = value.meta.universe[0].name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten MetaAndAssetCtxs /meta/universe/0/name"
    );
}

#[test]
fn audit_duplicate_root_22() {
    let value: types::UserFundingEntry = serde_json::from_str(r####"{"delta":{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","szi":"wire-雪\u0000","type":"funding","usdc":"wire-雪\u0000"},"hash":"wire-雪\u0000","time":17}"####)
            .expect("HL TYPES public duplicate decode UserFundingEntry");
    let selected = value.delta.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserFundingEntry /delta/coin"
    );
}

#[test]
fn audit_duplicate_root_23() {
    let value: types::UserFunding = serde_json::from_str(r####"[{"delta":{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","szi":"wire-雪\u0000","type":"funding","usdc":"wire-雪\u0000"},"hash":"wire-雪\u0000","time":17}]"####)
            .expect("HL TYPES public duplicate decode UserFunding");
    let selected = value[0].delta.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserFunding /0/delta/coin"
    );
}

#[test]
fn audit_duplicate_root_24() {
    let value: types::UserNonFundingLedgerEntry = serde_json::from_str(r####"{"delta":{"coin":"\ud800","coin":"雪😀﻿","type":"deposit","usdc":"wire-雪\u0000"},"hash":"wire-雪\u0000","time":17}"####)
            .expect("HL TYPES public duplicate decode UserNonFundingLedgerEntry");
    let selected = value.delta.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserNonFundingLedgerEntry /delta/coin"
    );
}

#[test]
fn audit_duplicate_root_25() {
    let value: types::UserNonFundingLedgerUpdates = serde_json::from_str(r####"[{"delta":{"coin":"\ud800","coin":"雪😀﻿","type":"deposit","usdc":"wire-雪\u0000"},"hash":"wire-雪\u0000","time":17}]"####)
            .expect("HL TYPES public duplicate decode UserNonFundingLedgerUpdates");
    let selected = value[0].delta.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserNonFundingLedgerUpdates /0/delta/coin"
    );
}

#[test]
fn audit_duplicate_root_26() {
    let value: types::FundingHistoryEntry = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","premium":"wire-雪\u0000","time":17}"####)
            .expect("HL TYPES public duplicate decode FundingHistoryEntry");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten FundingHistoryEntry /coin"
    );
}

#[test]
fn audit_duplicate_root_27() {
    let value: types::FundingHistory = serde_json::from_str(r####"[{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","premium":"wire-雪\u0000","time":17}]"####)
            .expect("HL TYPES public duplicate decode FundingHistory");
    let selected = value[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten FundingHistory /0/coin"
    );
}

#[test]
fn audit_duplicate_root_28() {
    let value: types::SpotToken = serde_json::from_str(r####"{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"weiDecimals":17,"index":17,"tokenId":"wire-雪\u0000","isCanonical":true,"fullName":"wire-雪\u0000","evmContract":{"address":"wire-雪\u0000","evm_extra_wei_decimals":17},"deployerTradingFeeShare":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode SpotToken");
    let selected = value.name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotToken /name"
    );
}

#[test]
fn audit_duplicate_root_29() {
    let value: types::SpotMarket = serde_json::from_str(
        r####"{"name":"\ud800","name":"雪😀﻿","tokens":[17,17],"index":17,"isCanonical":true}"####,
    )
    .expect("HL TYPES public duplicate decode SpotMarket");
    let selected = value.name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotMarket /name"
    );
}

#[test]
fn audit_duplicate_root_30() {
    let value: types::SpotMeta = serde_json::from_str(r####"{"tokens":[{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"weiDecimals":17,"index":17,"tokenId":"wire-雪\u0000","isCanonical":true,"fullName":"wire-雪\u0000","evmContract":{"address":"wire-雪\u0000","evm_extra_wei_decimals":17},"deployerTradingFeeShare":"wire-雪\u0000"}],"universe":[{"name":"wire-雪\u0000","tokens":[17,17],"index":17,"isCanonical":true}]}"####)
            .expect("HL TYPES public duplicate decode SpotMeta");
    let selected = value.tokens[0].name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotMeta /tokens/0/name"
    );
}

#[test]
fn audit_duplicate_root_31() {
    let value: types::SpotAssetCtx = serde_json::from_str(r####"{"dayNtlVlm":"\ud800","dayNtlVlm":"雪😀﻿","markPx":"wire-雪\u0000","midPx":"wire-雪\u0000","prevDayPx":"wire-雪\u0000","coin":"wire-雪\u0000","circulatingSupply":"wire-雪\u0000","totalSupply":"wire-雪\u0000","dayBaseVlm":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode SpotAssetCtx");
    let selected = value.day_ntl_vlm.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotAssetCtx /dayNtlVlm"
    );
}

#[test]
fn audit_duplicate_root_32() {
    let value: types::SpotMetaAndAssetCtxs = serde_json::from_str(r####"{"meta":{"tokens":[{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"weiDecimals":17,"index":17,"tokenId":"wire-雪\u0000","isCanonical":true,"fullName":"wire-雪\u0000","evmContract":{"address":"wire-雪\u0000","evm_extra_wei_decimals":17},"deployerTradingFeeShare":"wire-雪\u0000"}],"universe":[{"name":"wire-雪\u0000","tokens":[17,17],"index":17,"isCanonical":true}]},"assetCtxs":[{"dayNtlVlm":"wire-雪\u0000","markPx":"wire-雪\u0000","midPx":"wire-雪\u0000","prevDayPx":"wire-雪\u0000","coin":"wire-雪\u0000","circulatingSupply":"wire-雪\u0000","totalSupply":"wire-雪\u0000","dayBaseVlm":"wire-雪\u0000"}]}"####)
            .expect("HL TYPES public duplicate decode SpotMetaAndAssetCtxs");
    let selected = value.meta.tokens[0].name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten SpotMetaAndAssetCtxs /meta/tokens/0/name"
    );
}

#[test]
fn audit_duplicate_root_33() {
    let value: types::UserOpenOrder = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","limitPx":"wire-雪\u0000","oid":17,"side":"wire-雪\u0000","sz":"wire-雪\u0000","timestamp":17}"####)
            .expect("HL TYPES public duplicate decode UserOpenOrder");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserOpenOrder /coin"
    );
}

#[test]
fn audit_duplicate_root_34() {
    let value: types::UserOpenOrders = serde_json::from_str(r####"[{"coin":"\ud800","coin":"雪😀﻿","limitPx":"wire-雪\u0000","oid":17,"side":"wire-雪\u0000","sz":"wire-雪\u0000","timestamp":17}]"####)
            .expect("HL TYPES public duplicate decode UserOpenOrders");
    let selected = value[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserOpenOrders /0/coin"
    );
}

#[test]
fn audit_duplicate_root_35() {
    let value: types::OrderRequest = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","is_buy":true,"sz":17,"limit_px":17,"order_type":{"limit":{"tif":"Alo"},"trigger":{"triggerPx":17,"isMarket":true,"tpsl":"tp"}},"reduce_only":true,"cloid":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode OrderRequest");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten OrderRequest /coin"
    );
}

#[test]
fn audit_duplicate_root_36() {
    let value: types::OrderWire = serde_json::from_str(r####"{"a":17,"b":true,"p":"\ud800","p":"雪😀﻿","s":"wire-雪\u0000","r":true,"t":{"limit":{"tif":"Alo"},"trigger":{"triggerPx":17,"isMarket":true,"tpsl":"tp"}},"c":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode OrderWire");
    let selected = value.p.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten OrderWire /p"
    );
}

#[test]
fn audit_duplicate_root_37() {
    let value: types::CancelOrderRequest =
        serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","o":17}"####)
            .expect("HL TYPES public duplicate decode CancelOrderRequest");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten CancelOrderRequest /coin"
    );
}

#[test]
fn audit_duplicate_root_38() {
    let value: types::ModifyRequest = serde_json::from_str(r####"{"oid":17,"order":{"coin":"\ud800","coin":"雪😀﻿","is_buy":true,"sz":17,"limit_px":17,"order_type":{"limit":{"tif":"Alo"},"trigger":{"triggerPx":17,"isMarket":true,"tpsl":"tp"}},"reduce_only":true,"cloid":"wire-雪\u0000"}}"####)
            .expect("HL TYPES public duplicate decode ModifyRequest");
    let selected = value.order.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten ModifyRequest /order/coin"
    );
}

#[test]
fn audit_duplicate_root_39() {
    let value: types::ModifyWire = serde_json::from_str(r####"{"oid":17,"order":{"a":17,"b":true,"p":"\ud800","p":"雪😀﻿","s":"wire-雪\u0000","r":true,"t":{"limit":{"tif":"Alo"},"trigger":{"triggerPx":17,"isMarket":true,"tpsl":"tp"}},"c":"wire-雪\u0000"}}"####)
            .expect("HL TYPES public duplicate decode ModifyWire");
    let selected = value.order.p.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten ModifyWire /order/p"
    );
}

#[test]
fn audit_duplicate_root_40() {
    let value: types::Signature =
        serde_json::from_str(r####"{"r":"\ud800","r":"雪😀﻿","s":"wire-雪\u0000","v":17}"####)
            .expect("HL TYPES public duplicate decode Signature");
    let selected = value.r.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Signature /r"
    );
}

#[test]
fn audit_duplicate_root_41() {
    let value: types::Candle = serde_json::from_str(r####"{"t":17,"T":17,"s":"\ud800","s":"雪😀﻿","i":"wire-雪\u0000","o":"wire-雪\u0000","c":"wire-雪\u0000","h":"wire-雪\u0000","l":"wire-雪\u0000","v":"wire-雪\u0000","n":17,"coin":"wire-雪\u0000","interval":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode Candle");
    let selected = value.s.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Candle /s"
    );
}

#[test]
fn audit_duplicate_root_42() {
    let value: types::HyperliquidCandle = serde_json::from_str(r####"{"t":17,"T":17,"s":"\ud800","s":"雪😀﻿","i":"wire-雪\u0000","o":"wire-雪\u0000","c":"wire-雪\u0000","h":"wire-雪\u0000","l":"wire-雪\u0000","v":"wire-雪\u0000","n":17,"coin":"wire-雪\u0000","interval":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode HyperliquidCandle");
    let selected = value.s.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten HyperliquidCandle /s"
    );
}

#[test]
fn audit_duplicate_root_43() {
    let value: types::WsUserFill = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode WsUserFill");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFill /coin"
    );
}

#[test]
fn audit_duplicate_root_44() {
    let value: types::WsUserFills = serde_json::from_str(r####"{"isSnapshot":true,"fills":[{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFills");
    let selected = value.fills[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFills /fills/0/coin"
    );
}

#[test]
fn audit_duplicate_root_45() {
    let value: types::WsUserFundings = serde_json::from_str(r####"{"isSnapshot":true,"fundings":[{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","szi":"wire-雪\u0000","fundingRate":"wire-雪\u0000"}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFundings");
    let selected = value.fundings[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFundings /fundings/0/coin"
    );
}

#[test]
fn audit_duplicate_root_46() {
    let value: types::WsUserNonFundingLedgerUpdate = serde_json::from_str(r####"{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","type":"deposit"}"####)
            .expect("HL TYPES public duplicate decode WsUserNonFundingLedgerUpdate");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserNonFundingLedgerUpdate /coin"
    );
}

#[test]
fn audit_duplicate_root_47() {
    let value: types::WsUserNonFundingLedgerUpdates = serde_json::from_str(r####"{"isSnapshot":true,"updates":[{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","type":"deposit"}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserNonFundingLedgerUpdates");
    let selected = value.updates[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserNonFundingLedgerUpdates /updates/0/coin"
    );
}

#[test]
fn audit_duplicate_root_48() {
    let value: types::Hip3MarketInfo = serde_json::from_str(r####"{"name":"\ud800","name":"雪😀﻿","szDecimals":17,"maxLeverage":17,"onlyIsolated":true,"marginTableId":17,"marginMode":"strictIsolated","growthMode":"enabled","isDelisted":true,"lastGrowthModeChangeTime":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode Hip3MarketInfo");
    let selected = value.name.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Hip3MarketInfo /name"
    );
}

#[test]
fn audit_duplicate_root_49() {
    let value: types::BuilderInfo =
        serde_json::from_str(r####"{"b":"\ud800","b":"雪😀﻿","f":17}"####)
            .expect("HL TYPES public duplicate decode BuilderInfo");
    let selected = value.b.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten BuilderInfo /b"
    );
}

#[test]
fn audit_duplicate_root_50() {
    let value: types::BuilderFeeCheckResult = serde_json::from_str(r####"{"approved":true,"maxFeeRate":"\ud800","maxFeeRate":"雪😀﻿","maxFeeRateTenthsBps":17,"error":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode BuilderFeeCheckResult");
    let selected = value.max_fee_rate.as_ref().unwrap().as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten BuilderFeeCheckResult /maxFeeRate"
    );
}

#[test]
fn audit_duplicate_selection_retains_last_value_validation_and_key_position() {
    use safe_cex_types::retained_json::RetainedJsonInput;
    let raw = r#"{"z":"\ud800","a":1,"z":"kept"}"#;
    let value = serde_json::from_str::<RetainedJsonInput>(raw)
        .unwrap()
        .into_value()
        .unwrap();
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        ["z", "a"],
        "AUDIT native first insertion order survives last replacement"
    );
    assert_eq!(value["z"].as_str(), Some("kept"));
    for raw in [
        r#"{"z":"kept","z":"\ud800"}"#,
        r#"{"z":invalid,"z":"kept"}"#,
    ] {
        assert!(
            serde_json::from_str::<RetainedJsonInput>(raw).is_err(),
            "AUDIT surviving invalid string or invalid syntax still refused"
        );
    }
    let overwritten = format!(
        "{{\"z\":{}0{},\"z\":\"kept\"}}",
        "[".repeat(140),
        "]".repeat(140)
    );
    let value = serde_json::from_str::<RetainedJsonInput>(&overwritten)
        .unwrap()
        .into_value()
        .unwrap();
    assert_eq!(value["z"].as_str(), Some("kept"));
    let surviving = format!(
        "{{\"z\":\"kept\",\"z\":{}0{}}}",
        "[".repeat(140),
        "]".repeat(140)
    );
    assert!(
        serde_json::from_str::<RetainedJsonInput>(&surviving).is_err(),
        "AUDIT surviving depth budget remains enforced"
    );
}

#[test]
fn audit_scalar_cloid_observes_native_text_before_any_wire_projection() {
    let scalar: types::OidOrCloid = serde_json::from_str("\"雪😀\\ufeff\"").unwrap();
    let types::OidOrCloid::Case1(text) = scalar else {
        panic!("AUDIT native scalar cloid string alternative");
    };
    assert_eq!(
        text, "雪😀\u{feff}",
        "AUDIT native scalar cloid code points"
    );
    for raw in [r#""\ud800""#, r#""\udfff""#] {
        assert!(
            serde_json::from_str::<types::OidOrCloid>(raw).is_err(),
            "AUDIT native scalar cloid whole-root refusal"
        );
    }
}

#[test]
fn audit_any_indexed_roots_preserve_utf16_and_array_native_alternatives() {
    use safe_cex_venue_hyperliquid::json_values::{JsonContainer, JsonValue};
    let raw = r#"{"bad":"\ud800","literal":{"$serde_json::private::Number":"120"}}"#;
    for value in [
        serde_json::from_str::<types::UserRateLimit>(raw).unwrap(),
        serde_json::from_str::<types::OrderStatus>(raw).unwrap(),
    ] {
        let JsonContainer::Object(object) = value else {
            panic!("AUDIT native any-indexed object");
        };
        let Some(JsonValue::String(text)) = object.get(&"bad".encode_utf16().collect::<Vec<_>>())
        else {
            panic!("AUDIT native any-indexed retained string");
        };
        assert_eq!(text.0, [0xd800], "AUDIT native any-indexed lone unit");
        let Some(JsonValue::Object(marker)) =
            object.get(&"literal".encode_utf16().collect::<Vec<_>>())
        else {
            panic!("AUDIT native any-indexed marker stays object");
        };
        let Some(JsonValue::String(text)) = marker.get(
            &"$serde_json::private::Number"
                .encode_utf16()
                .collect::<Vec<_>>(),
        ) else {
            panic!("AUDIT native any-indexed own marker field");
        };
        assert_eq!(text.0, "120".encode_utf16().collect::<Vec<_>>());
    }
    for value in [
        serde_json::from_str::<types::UserRateLimit>(r#"["\udfff"]"#).unwrap(),
        serde_json::from_str::<types::OrderStatus>(r#"["\udfff"]"#).unwrap(),
    ] {
        let JsonContainer::Array(values) = value else {
            panic!("AUDIT native any-indexed array alternative");
        };
        assert_eq!(values.len(), 1);
        let JsonValue::String(text) = &values[0] else {
            panic!("AUDIT native any-indexed array string");
        };
        assert_eq!(text.0, [0xdfff]);
    }
}
