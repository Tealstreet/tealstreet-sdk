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
    assert_eq!(cases.len(), 43, "RECORD-CARDINALITY source roots");
    let mut actual: Vec<_> = cases.iter().map(|case| case.name.as_str()).collect();
    actual.sort();
    let mut expected = vec![
        "LimitOrderType",
        "TriggerOrderType",
        "OrderType",
        "ClearinghouseState",
        "ClearinghouseStateAssetPositionsItem",
        "ClearinghouseStateAssetPositionsItemPosition",
        "ClearinghouseStateAssetPositionsItemPositionCumFunding",
        "ClearinghouseStateAssetPositionsItemPositionLeverage",
        "ClearinghouseStateCrossMarginSummary",
        "ClearinghouseStateMarginSummary",
        "UserFills",
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
        "L2Book",
        "L2BookLevelsItem0Item",
        "L2BookLevelsItem1Item",
        "FundingHistoryEntry",
        "UserOpenOrder",
        "CancelOrderRequest",
        "CancelOrderRequestsItem",
        "CancelByCloidRequest",
        "Signature",
        "Notification",
        "Candle",
        "HyperliquidCandle",
        "WsUserFill",
        "WsUserFills",
        "WsUserFundings",
        "WsUserNonFundingLedgerUpdate",
        "WsUserNonFundingLedgerUpdates",
        "BuilderInfo",
        "BuilderFeeApprovalResult",
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
    for index in 0..43 {
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
            "FundingHistoryEntry" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    FundingHistoryEntry::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<FundingHistoryEntry>(case)
            }
            "UserOpenOrder" => {
                let mut decoder = serde_json::Deserializer::from_str(&case.positional);
                assert!(
                    UserOpenOrder::deserialize(&mut decoder).is_err(),
                    "public qualified record decoder must refuse positional input"
                );
                check::<UserOpenOrder>(case)
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
    assert_eq!(comparisons, 301, "RECORD-CARDINALITY native receipt total");
}
#[test]
fn record_corpus_matches_live_typescript() {
    source_is_current();
}
