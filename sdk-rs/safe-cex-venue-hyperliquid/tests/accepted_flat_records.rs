use safe_cex_num::numeric_ingress::JsNumberOutput;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    expected: &Value,
    project: fn(T) -> Value,
    label: &str,
) {
    let input: Value = serde_json::from_str(raw).unwrap();
    let routes = [
        serde_json::from_str::<T>(raw),
        T::deserialize(input.clone()),
        T::deserialize(&input),
        safe_cex_num::numeric_ingress::js_from_str::<T>(raw),
        safe_cex_num::numeric_ingress::js_from_value::<T>(input.clone()),
        safe_cex_num::numeric_ingress::js_from_value_ref::<T>(&input),
    ];
    for result in routes {
        let actual = match result {
            Ok(decoded) => project(decoded),
            Err(_) => json!({"ok":false}),
        };
        assert_eq!(&actual, expected, "{label}");
    }
}
#[test]
fn clearinghousestateassetpositionsitempositioncumfunding_native_fields_and_map_only() {
    fn project(
        decoded:safe_cex_venue_hyperliquid::types::ClearinghouseStateAssetPositionsItemPositionCumFunding,
    ) -> Value {
        json!({"ok":true,"fields":{"allTime":json!({"kind":"string","value":decoded.all_time}),"sinceChange":json!({"kind":"string","value":decoded.since_change}),"sinceOpen":json!({"kind":"string","value":decoded.since_open})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["ClearinghouseStateAssetPositionsItemPositionCumFunding"]
        .as_array()
        .unwrap()
    {
        check::<safe_cex_venue_hyperliquid::types::ClearinghouseStateAssetPositionsItemPositionCumFunding>(case["input"].as_str().unwrap(),&case["output"],project,case["label"].as_str().unwrap());
    }
}
#[test]
fn clearinghousestateassetpositionsitempositionleverage_native_fields_and_map_only() {
    fn project(
        decoded:safe_cex_venue_hyperliquid::types::ClearinghouseStateAssetPositionsItemPositionLeverage,
    ) -> Value {
        json!({"ok":true,"fields":{"rawUsd":json!({"kind":"string","value":decoded.raw_usd}),"type":json!({"kind":"string","value":decoded.r#type}),"value":json!({"kind":"number","bits":format!("{:016x}",decoded.value.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["ClearinghouseStateAssetPositionsItemPositionLeverage"]
        .as_array()
        .unwrap()
    {
        check::<
            safe_cex_venue_hyperliquid::types::ClearinghouseStateAssetPositionsItemPositionLeverage,
        >(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn clearinghousestatecrossmarginsummary_native_fields_and_map_only() {
    fn project(
        decoded: safe_cex_venue_hyperliquid::types::ClearinghouseStateCrossMarginSummary,
    ) -> Value {
        json!({"ok":true,"fields":{"accountValue":json!({"kind":"string","value":decoded.account_value}),"totalMarginUsed":json!({"kind":"string","value":decoded.total_margin_used}),"totalNtlPos":json!({"kind":"string","value":decoded.total_ntl_pos}),"totalRawUsd":json!({"kind":"string","value":decoded.total_raw_usd})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["ClearinghouseStateCrossMarginSummary"]
        .as_array()
        .unwrap()
    {
        check::<safe_cex_venue_hyperliquid::types::ClearinghouseStateCrossMarginSummary>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn clearinghousestatemarginsummary_native_fields_and_map_only() {
    fn project(
        decoded: safe_cex_venue_hyperliquid::types::ClearinghouseStateMarginSummary,
    ) -> Value {
        json!({"ok":true,"fields":{"accountValue":json!({"kind":"string","value":decoded.account_value}),"totalMarginUsed":json!({"kind":"string","value":decoded.total_margin_used}),"totalNtlPos":json!({"kind":"string","value":decoded.total_ntl_pos}),"totalRawUsd":json!({"kind":"string","value":decoded.total_raw_usd})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["ClearinghouseStateMarginSummary"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::ClearinghouseStateMarginSummary>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wstrade_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsTrade) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"side":json!({"kind":"string","value":decoded.side}),"px":json!({"kind":"string","value":decoded.px}),"sz":json!({"kind":"string","value":decoded.sz}),"hash":json!({"kind":"string","value":decoded.hash}),"time":json!({"kind":"number","bits":format!("{:016x}",decoded.time.to_bits())}),"tid":json!({"kind":"number","bits":format!("{:016x}",decoded.tid.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsTrade"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsTrade>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wslevel_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsLevel) -> Value {
        json!({"ok":true,"fields":{"px":json!({"kind":"string","value":decoded.px}),"sz":json!({"kind":"string","value":decoded.sz}),"n":json!({"kind":"number","bits":format!("{:016x}",decoded.n.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsLevel"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsLevel>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wsorderorder_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsOrderOrder) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"side":json!({"kind":"string","value":decoded.side}),"limitPx":json!({"kind":"string","value":decoded.limit_px}),"sz":json!({"kind":"string","value":decoded.sz}),"oid":json!({"kind":"number","bits":format!("{:016x}",decoded.oid.to_bits())}),"timestamp":json!({"kind":"number","bits":format!("{:016x}",decoded.timestamp.to_bits())}),"origSz":json!({"kind":"string","value":decoded.orig_sz})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsOrderOrder"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsOrderOrder>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wsuserfunding_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsUserFunding) -> Value {
        json!({"ok":true,"fields":{"time":json!({"kind":"number","bits":format!("{:016x}",decoded.time.to_bits())}),"coin":json!({"kind":"string","value":decoded.coin}),"usdc":json!({"kind":"string","value":decoded.usdc}),"szi":json!({"kind":"string","value":decoded.szi}),"fundingRate":json!({"kind":"string","value":decoded.funding_rate})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsUserFunding"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsUserFunding>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wsliquidation_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsLiquidation) -> Value {
        json!({"ok":true,"fields":{"lid":json!({"kind":"number","bits":format!("{:016x}",decoded.lid.to_bits())}),"liquidator":json!({"kind":"string","value":decoded.liquidator}),"liquidated_user":json!({"kind":"string","value":decoded.liquidated_user}),"liquidated_ntl_pos":json!({"kind":"string","value":decoded.liquidated_ntl_pos}),"liquidated_account_value":json!({"kind":"string","value":decoded.liquidated_account_value})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsLiquidation"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsLiquidation>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn wsnonusercancel_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::WsNonUserCancel) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"oid":json!({"kind":"number","bits":format!("{:016x}",decoded.oid.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["WsNonUserCancel"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::WsNonUserCancel>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn l2booklevelsitem0item_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::L2BookLevelsItem0Item) -> Value {
        json!({"ok":true,"fields":{"px":json!({"kind":"string","value":decoded.px}),"sz":json!({"kind":"string","value":decoded.sz}),"n":json!({"kind":"number","bits":format!("{:016x}",decoded.n.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["L2BookLevelsItem0Item"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::L2BookLevelsItem0Item>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn l2booklevelsitem1item_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::L2BookLevelsItem1Item) -> Value {
        json!({"ok":true,"fields":{"px":json!({"kind":"string","value":decoded.px}),"sz":json!({"kind":"string","value":decoded.sz}),"n":json!({"kind":"number","bits":format!("{:016x}",decoded.n.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["L2BookLevelsItem1Item"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::L2BookLevelsItem1Item>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn fundinghistoryentry_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::FundingHistoryEntry) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"fundingRate":json!({"kind":"string","value":decoded.funding_rate}),"premium":json!({"kind":"string","value":decoded.premium}),"time":json!({"kind":"number","bits":format!("{:016x}",decoded.time.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["FundingHistoryEntry"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::FundingHistoryEntry>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn useropenorder_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::UserOpenOrder) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"limitPx":json!({"kind":"string","value":decoded.limit_px}),"oid":json!({"kind":"number","bits":format!("{:016x}",decoded.oid.to_bits())}),"side":json!({"kind":"string","value":decoded.side}),"sz":json!({"kind":"string","value":decoded.sz}),"timestamp":json!({"kind":"number","bits":format!("{:016x}",decoded.timestamp.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["UserOpenOrder"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::UserOpenOrder>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn cancelorderrequest_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::CancelOrderRequest) -> Value {
        json!({"ok":true,"fields":{"coin":json!({"kind":"string","value":decoded.coin}),"o":json!({"kind":"number","bits":format!("{:016x}",decoded.o.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["CancelOrderRequest"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::CancelOrderRequest>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn cancelorderrequestsitem_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::CancelOrderRequestsItem) -> Value {
        json!({"ok":true,"fields":{"a":json!({"kind":"number","bits":format!("{:016x}",decoded.a.to_bits())}),"o":json!({"kind":"number","bits":format!("{:016x}",decoded.o.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["CancelOrderRequestsItem"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::CancelOrderRequestsItem>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn signature_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::Signature) -> Value {
        json!({"ok":true,"fields":{"r":json!({"kind":"string","value":decoded.r}),"s":json!({"kind":"string","value":decoded.s}),"v":json!({"kind":"number","bits":format!("{:016x}",decoded.v.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["Signature"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::Signature>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn notification_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::Notification) -> Value {
        json!({"ok":true,"fields":{"notification":json!({"kind":"string","value":decoded.notification}),"user":json!({"kind":"string","value":decoded.user})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["Notification"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::Notification>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn builderinfo_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::BuilderInfo) -> Value {
        json!({"ok":true,"fields":{"b":json!({"kind":"string","value":decoded.b}),"f":json!({"kind":"number","bits":format!("{:016x}",decoded.f.to_bits())})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["BuilderInfo"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::BuilderInfo>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
#[test]
fn builderfeeapprovalresult_native_fields_and_map_only() {
    fn project(decoded: safe_cex_venue_hyperliquid::types::BuilderFeeApprovalResult) -> Value {
        json!({"ok":true,"fields":{"success":json!({"kind":"boolean","value":decoded.success}),"message":json!({"kind":"string","value":decoded.message})}})
    }
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/accepted_flat_record_cases.json")).unwrap();
    for case in cases["BuilderFeeApprovalResult"].as_array().unwrap() {
        check::<safe_cex_venue_hyperliquid::types::BuilderFeeApprovalResult>(
            case["input"].as_str().unwrap(),
            &case["output"],
            project,
            case["label"].as_str().unwrap(),
        );
    }
}
