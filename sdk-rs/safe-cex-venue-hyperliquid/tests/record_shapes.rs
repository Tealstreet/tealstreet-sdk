use safe_cex_venue_hyperliquid::{
    signing::{CancelOrderResponse, CancelOrderResponseBody, CancelOrderResponseData},
    types::*,
};
use serde::Deserialize;
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Case {
    name: String,
    raw: String,
    positional: String,
}
fn check<T: for<'de> Deserialize<'de>>(case: &Case) -> [bool; 7] {
    [
        serde_json::from_str::<T>(&case.raw).is_ok(),
        serde_json::from_str::<T>(&case.positional).is_ok(),
        serde_json::from_str::<T>("[]").is_ok(),
        serde_json::from_str::<T>("null").is_ok(),
        serde_json::from_str::<T>("true").is_ok(),
        serde_json::from_str::<T>("123").is_ok(),
        serde_json::from_str::<T>("\"record\"").is_ok(),
    ]
}
fn source_is_current() {
    static SOURCE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    let result = SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/record-shapes.cjs"
            ))
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    });
    assert!(
        result.is_ok(),
        "RECORD-CONTENT actual source regeneration: {result:?}"
    );
}
#[test]
fn every_record_is_map_only() {
    source_is_current();
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/record-shapes.json")).unwrap();
    let admitted: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/record-shapes.json")).unwrap();
    assert_eq!(cases.len(), 77, "RECORD-CARDINALITY source roots");
    let mut actual: Vec<_> = cases.iter().map(|case| case.name.as_str()).collect();
    actual.sort();
    let mut expected = vec![
        "LimitOrderType",
        "TriggerOrderType",
        "OrderType",
        "Meta",
        "MetaUniverseItem",
        "ClearinghouseState",
        "ClearinghouseStateAssetPositionsItem",
        "ClearinghouseStateAssetPositionsItemPosition",
        "ClearinghouseStateAssetPositionsItemPositionCumFunding",
        "ClearinghouseStateAssetPositionsItemPositionLeverage",
        "ClearinghouseStateCrossMarginSummary",
        "ClearinghouseStateMarginSummary",
        "UserFills",
        "OrderResponseStatusCase0",
        "OrderResponseStatusCase0Resting",
        "OrderResponseStatusCase1",
        "OrderResponseStatusCase1Filled",
        "OrderResponseStatusCase2",
        "OrderResponse",
        "OrderResponseResponseCase1",
        "OrderResponseResponseCase1Data",
        "WsTrade",
        "WsBook",
        "WsBbo",
        "WsLevel",
        "WsOrder",
        "WsOrderOrder",
        "WsFill",
        "WsUserFunding",
        "WsLiquidation",
        "WsNonUserCancel",
        "SpotClearinghouseBalance",
        "SpotClearinghouseState",
        "FrontendOpenOrders",
        "L2Book",
        "L2BookLevelsItem0Item",
        "L2BookLevelsItem1Item",
        "CandleSnapshot",
        "AssetCtx",
        "MetaAndAssetCtxs",
        "UserFundingDelta",
        "UserFundingEntry",
        "UserNonFundingLedgerDelta",
        "UserNonFundingLedgerEntry",
        "FundingHistoryEntry",
        "SpotToken",
        "SpotTokenEvmContract",
        "SpotMarket",
        "SpotMeta",
        "SpotAssetCtx",
        "SpotMetaAndAssetCtxs",
        "UserOpenOrder",
        "OrderRequest",
        "OrderWire",
        "CancelOrderRequest",
        "CancelOrderRequestsItem",
        "CancelByCloidRequest",
        "ModifyRequest",
        "ModifyWire",
        "ScheduleCancelAction",
        "Signature",
        "Notification",
        "Candle",
        "HyperliquidCandle",
        "WsUserFill",
        "WsUserFills",
        "WsUserFundings",
        "WsUserNonFundingLedgerUpdate",
        "WsUserNonFundingLedgerUpdates",
        "PerpDex",
        "Hip3MarketInfo",
        "BuilderInfo",
        "BuilderFeeApprovalResult",
        "BuilderFeeCheckResult",
        "CancelOrderResponse",
        "CancelOrderResponseBody",
        "CancelOrderResponseData",
    ];
    expected.sort();
    assert_eq!(
        actual, expected,
        "exact record corpus coverage and uniqueness"
    );
    let mut comparisons = 0;
    for index in 0..77 {
        let case = cases
            .get(index)
            .expect("RECORD-CARDINALITY native index coverage");
        assert_eq!(
            case,
            admitted.get(index).unwrap(),
            "RECORD-CONTENT native input {index}"
        );
        let receipts = match case.name.as_str() {
            "LimitOrderType" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    LimitOrderType::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<LimitOrderType>(case)
            }
            "TriggerOrderType" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    TriggerOrderType::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<TriggerOrderType>(case)
            }
            "OrderType" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderType::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderType>(case)
            }
            "Meta" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    Meta::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<Meta>(case)
            }
            "MetaUniverseItem" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    MetaUniverseItem::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<MetaUniverseItem>(case)
            }
            "ClearinghouseState" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseState::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseState>(case)
            }
            "ClearinghouseStateAssetPositionsItem" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateAssetPositionsItem::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateAssetPositionsItem>(case)
            }
            "ClearinghouseStateAssetPositionsItemPosition" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateAssetPositionsItemPosition::deserialize(&mut decoder)
                        .is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateAssetPositionsItemPosition>(case)
            }
            "ClearinghouseStateAssetPositionsItemPositionCumFunding" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateAssetPositionsItemPositionCumFunding::deserialize(
                        &mut decoder
                    )
                    .is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateAssetPositionsItemPositionCumFunding>(case)
            }
            "ClearinghouseStateAssetPositionsItemPositionLeverage" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateAssetPositionsItemPositionLeverage::deserialize(&mut decoder)
                        .is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateAssetPositionsItemPositionLeverage>(case)
            }
            "ClearinghouseStateCrossMarginSummary" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateCrossMarginSummary::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateCrossMarginSummary>(case)
            }
            "ClearinghouseStateMarginSummary" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ClearinghouseStateMarginSummary::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ClearinghouseStateMarginSummary>(case)
            }
            "UserFills" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserFills::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserFills>(case)
            }
            "OrderResponseStatusCase0" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseStatusCase0::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseStatusCase0>(case)
            }
            "OrderResponseStatusCase0Resting" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseStatusCase0Resting::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseStatusCase0Resting>(case)
            }
            "OrderResponseStatusCase1" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseStatusCase1::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseStatusCase1>(case)
            }
            "OrderResponseStatusCase1Filled" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseStatusCase1Filled::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseStatusCase1Filled>(case)
            }
            "OrderResponseStatusCase2" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseStatusCase2::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseStatusCase2>(case)
            }
            "OrderResponse" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponse::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponse>(case)
            }
            "OrderResponseResponseCase1" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseResponseCase1::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseResponseCase1>(case)
            }
            "OrderResponseResponseCase1Data" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderResponseResponseCase1Data::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderResponseResponseCase1Data>(case)
            }
            "WsTrade" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsTrade::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsTrade>(case)
            }
            "WsBook" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsBook::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsBook>(case)
            }
            "WsBbo" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsBbo::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsBbo>(case)
            }
            "WsLevel" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsLevel::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsLevel>(case)
            }
            "WsOrder" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsOrder::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsOrder>(case)
            }
            "WsOrderOrder" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsOrderOrder::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsOrderOrder>(case)
            }
            "WsFill" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsFill::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsFill>(case)
            }
            "WsUserFunding" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserFunding::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserFunding>(case)
            }
            "WsLiquidation" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsLiquidation::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsLiquidation>(case)
            }
            "WsNonUserCancel" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsNonUserCancel::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsNonUserCancel>(case)
            }
            "SpotClearinghouseBalance" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotClearinghouseBalance::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotClearinghouseBalance>(case)
            }
            "SpotClearinghouseState" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotClearinghouseState::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotClearinghouseState>(case)
            }
            "FrontendOpenOrders" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    FrontendOpenOrders::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<FrontendOpenOrders>(case)
            }
            "L2Book" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    L2Book::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<L2Book>(case)
            }
            "L2BookLevelsItem0Item" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    L2BookLevelsItem0Item::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<L2BookLevelsItem0Item>(case)
            }
            "L2BookLevelsItem1Item" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    L2BookLevelsItem1Item::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<L2BookLevelsItem1Item>(case)
            }
            "CandleSnapshot" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CandleSnapshot::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CandleSnapshot>(case)
            }
            "AssetCtx" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    AssetCtx::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<AssetCtx>(case)
            }
            "MetaAndAssetCtxs" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    MetaAndAssetCtxs::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<MetaAndAssetCtxs>(case)
            }
            "UserFundingDelta" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserFundingDelta::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserFundingDelta>(case)
            }
            "UserFundingEntry" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserFundingEntry::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserFundingEntry>(case)
            }
            "UserNonFundingLedgerDelta" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserNonFundingLedgerDelta::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserNonFundingLedgerDelta>(case)
            }
            "UserNonFundingLedgerEntry" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserNonFundingLedgerEntry::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserNonFundingLedgerEntry>(case)
            }
            "FundingHistoryEntry" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    FundingHistoryEntry::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<FundingHistoryEntry>(case)
            }
            "SpotToken" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotToken::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotToken>(case)
            }
            "SpotTokenEvmContract" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotTokenEvmContract::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotTokenEvmContract>(case)
            }
            "SpotMarket" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotMarket::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotMarket>(case)
            }
            "SpotMeta" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotMeta::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotMeta>(case)
            }
            "SpotAssetCtx" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotAssetCtx::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotAssetCtx>(case)
            }
            "SpotMetaAndAssetCtxs" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    SpotMetaAndAssetCtxs::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<SpotMetaAndAssetCtxs>(case)
            }
            "UserOpenOrder" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserOpenOrder::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserOpenOrder>(case)
            }
            "OrderRequest" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderRequest::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderRequest>(case)
            }
            "OrderWire" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    OrderWire::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<OrderWire>(case)
            }
            "CancelOrderRequest" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelOrderRequest::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelOrderRequest>(case)
            }
            "CancelOrderRequestsItem" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelOrderRequestsItem::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelOrderRequestsItem>(case)
            }
            "CancelByCloidRequest" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelByCloidRequest::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelByCloidRequest>(case)
            }
            "ModifyRequest" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ModifyRequest::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ModifyRequest>(case)
            }
            "ModifyWire" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ModifyWire::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ModifyWire>(case)
            }
            "ScheduleCancelAction" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    ScheduleCancelAction::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<ScheduleCancelAction>(case)
            }
            "Signature" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    Signature::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<Signature>(case)
            }
            "Notification" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    Notification::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<Notification>(case)
            }
            "Candle" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    Candle::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<Candle>(case)
            }
            "HyperliquidCandle" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    HyperliquidCandle::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<HyperliquidCandle>(case)
            }
            "WsUserFill" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserFill::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserFill>(case)
            }
            "WsUserFills" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserFills::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserFills>(case)
            }
            "WsUserFundings" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserFundings::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserFundings>(case)
            }
            "WsUserNonFundingLedgerUpdate" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserNonFundingLedgerUpdate::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserNonFundingLedgerUpdate>(case)
            }
            "WsUserNonFundingLedgerUpdates" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    WsUserNonFundingLedgerUpdates::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<WsUserNonFundingLedgerUpdates>(case)
            }
            "PerpDex" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    PerpDex::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<PerpDex>(case)
            }
            "Hip3MarketInfo" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    Hip3MarketInfo::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<Hip3MarketInfo>(case)
            }
            "BuilderInfo" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    BuilderInfo::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<BuilderInfo>(case)
            }
            "BuilderFeeApprovalResult" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    BuilderFeeApprovalResult::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<BuilderFeeApprovalResult>(case)
            }
            "BuilderFeeCheckResult" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    BuilderFeeCheckResult::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<BuilderFeeCheckResult>(case)
            }
            "CancelOrderResponse" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelOrderResponse::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelOrderResponse>(case)
            }
            "CancelOrderResponseBody" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelOrderResponseBody::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelOrderResponseBody>(case)
            }
            "CancelOrderResponseData" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    CancelOrderResponseData::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<CancelOrderResponseData>(case)
            }
            other => panic!("unexamined record {other}"),
        };
        assert_eq!(
            receipts,
            [true, false, false, false, false, false, false],
            "RECORD-NATIVE decode receipts {}",
            case.name
        );
        comparisons += receipts.len();
    }
    assert_eq!(comparisons, 539, "RECORD-CARDINALITY native receipt total");
}
#[test]
fn record_corpus_matches_live_typescript() {
    source_is_current();
}
