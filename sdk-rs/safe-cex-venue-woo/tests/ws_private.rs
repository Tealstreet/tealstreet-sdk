use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_venue_woo::{
    v3_ws_private::*,
    v3_ws_public::WooV3WsPublicPayload,
    v3_ws_state::{WooV3WsBlocker, WooV3WsBlockerCode, WsBlockerRequirement},
    v3_ws_topics::{
        PlainKind, PrivateChannel, PublicChannel, WooV3WsPrivateTopic, WooV3WsTopicDescriptor,
    },
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-private-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WS-PRIVATE source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value) -> Option<JsNumberInput> {
    row["input"]
        .as_str()
        .map(|s| serde_json::from_str(s).unwrap())
}
fn number(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        expected["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{label}"
    );
}
fn error<T: std::fmt::Debug>(actual: Result<T, String>, expected: &Value) {
    assert_eq!(
        actual.unwrap_err(),
        expected["error"]["message"].as_str().unwrap(),
        "WS-PRIVATE native first validation error"
    );
    assert_eq!(expected["error"]["name"], "Error");
}
fn private_kind(s: &str) -> WooV3WsPrivateTopic {
    match s {
        "account" => WooV3WsPrivateTopic::Account,
        "balance" => WooV3WsPrivateTopic::Balance,
        "position" => WooV3WsPrivateTopic::Position,
        "executionreport" => WooV3WsPrivateTopic::Executionreport,
        "algoexecutionreport" => WooV3WsPrivateTopic::Algoexecutionreport,
        _ => panic!("unknown expected private kind"),
    }
}
fn account(actual: &WooV3WsAccount, expected: &Value) {
    match (expected["mode"].as_str().unwrap(), actual) {
        (
            "PURE_SPOT",
            WooV3WsAccount::PureSpot {
                mode,
                total_account_value,
            },
        ) => {
            assert_eq!(
                mode,
                expected["mode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount mode"
            );
            assert_eq!(
                total_account_value,
                expected["totalAccountValue"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount totalAccountValue"
            );
        }
        (
            "MARGIN",
            WooV3WsAccount::Margin {
                mode,
                leverage,
                margin_ratio,
                open_margin_ratio,
                initial_margin_ratio,
                maintenance_margin_ratio,
                total_collateral,
                free_collateral,
                total_account_value,
            },
        ) => {
            assert_eq!(
                mode,
                expected["mode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount mode"
            );
            number(
                *leverage,
                &expected["leverage"],
                "WS-PRIVATE native WooV3WsAccount leverage",
            );
            assert_eq!(
                margin_ratio,
                expected["marginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount marginRatio"
            );
            assert_eq!(
                open_margin_ratio,
                expected["openMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount openMarginRatio"
            );
            assert_eq!(
                initial_margin_ratio,
                expected["initialMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount initialMarginRatio"
            );
            assert_eq!(
                maintenance_margin_ratio,
                expected["maintenanceMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount maintenanceMarginRatio"
            );
            assert_eq!(
                total_collateral,
                expected["totalCollateral"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount totalCollateral"
            );
            assert_eq!(
                free_collateral,
                expected["freeCollateral"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount freeCollateral"
            );
            assert_eq!(
                total_account_value,
                expected["totalAccountValue"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount totalAccountValue"
            );
        }
        (
            "FUTURE",
            WooV3WsAccount::Future {
                mode,
                default_margin_mode,
                position_mode,
                margin_ratio,
                open_margin_ratio,
                initial_margin_ratio,
                maintenance_margin_ratio,
                total_collateral,
                free_collateral,
                total_account_value,
            },
        ) => {
            assert_eq!(
                mode,
                expected["mode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount mode"
            );
            assert_eq!(
                default_margin_mode,
                expected["defaultMarginMode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount defaultMarginMode"
            );
            assert_eq!(
                position_mode,
                expected["positionMode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount positionMode"
            );
            assert_eq!(
                margin_ratio,
                expected["marginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount marginRatio"
            );
            assert_eq!(
                open_margin_ratio,
                expected["openMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount openMarginRatio"
            );
            assert_eq!(
                initial_margin_ratio,
                expected["initialMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount initialMarginRatio"
            );
            assert_eq!(
                maintenance_margin_ratio,
                expected["maintenanceMarginRatio"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount maintenanceMarginRatio"
            );
            assert_eq!(
                total_collateral,
                expected["totalCollateral"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount totalCollateral"
            );
            assert_eq!(
                free_collateral,
                expected["freeCollateral"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount freeCollateral"
            );
            assert_eq!(
                total_account_value,
                expected["totalAccountValue"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsAccount totalAccountValue"
            );
        }
        _ => panic!("WS-PRIVATE native WooV3WsAccount variant"),
    }
}
fn balance_row(actual: &WooV3WsBalanceRow, expected: &Value) {
    assert_eq!(
        actual.token,
        expected["token"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow token"
    );
    assert_eq!(
        actual.holding,
        expected["holding"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow holding"
    );
    assert_eq!(
        actual.frozen,
        expected["frozen"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow frozen"
    );
    assert_eq!(
        actual.interest,
        expected["interest"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow interest"
    );
    assert_eq!(
        actual.pending_short_quantity,
        expected["pendingShortQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow pendingShortQuantity"
    );
    assert_eq!(
        actual.pending_long_quantity,
        expected["pendingLongQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow pendingLongQuantity"
    );
    assert_eq!(
        actual.staked,
        expected["staked"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow staked"
    );
    assert_eq!(
        actual.unbonding,
        expected["unbonding"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow unbonding"
    );
    assert_eq!(
        actual.vault,
        expected["vault"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow vault"
    );
    assert_eq!(
        actual.launchpad_vault,
        expected["launchpadVault"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow launchpadVault"
    );
    assert_eq!(
        actual.earn,
        expected["earn"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow earn"
    );
    assert_eq!(
        actual.average_open_price,
        expected["averageOpenPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow averageOpenPrice"
    );
    assert_eq!(
        actual.pnl24h,
        expected["pnl24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow pnl24h"
    );
    assert_eq!(
        actual.roi24h,
        expected["roi24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow roi24h"
    );
    assert_eq!(
        actual.fee24h,
        expected["fee24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow fee24h"
    );
    assert_eq!(
        actual.mark_price,
        expected["markPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow markPrice"
    );
    number(
        actual.version,
        &expected["version"],
        "WS-PRIVATE native WooV3WsBalanceRow version",
    );
    assert_eq!(
        actual.trial_fund_bonus,
        expected["trialFundBonus"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsBalanceRow trialFundBonus"
    );
    number(
        actual.loss_cover_ratio,
        &expected["lossCoverRatio"],
        "WS-PRIVATE native WooV3WsBalanceRow lossCoverRatio",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PRIVATE native WooV3WsBalanceRow timestamp",
    );
}
fn position_row(actual: &WooV3WsPositionRow, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow symbol"
    );
    assert_eq!(
        actual.holding,
        expected["holding"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow holding"
    );
    assert_eq!(
        actual.pending_long_quantity,
        expected["pendingLongQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow pendingLongQuantity"
    );
    assert_eq!(
        actual.pending_short_quantity,
        expected["pendingShortQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow pendingShortQuantity"
    );
    assert_eq!(
        actual.average_open_price,
        expected["averageOpenPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow averageOpenPrice"
    );
    assert_eq!(
        actual.pnl24h,
        expected["pnl24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow pnl24h"
    );
    assert_eq!(
        actual.roi24h,
        expected["roi24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow roi24h"
    );
    assert_eq!(
        actual.fee24h,
        expected["fee24h"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow fee24h"
    );
    assert_eq!(
        actual.settle_price,
        expected["settlePrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow settlePrice"
    );
    assert_eq!(
        actual.mark_price,
        expected["markPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow markPrice"
    );
    number(
        actual.opening_timestamp,
        &expected["openingTimestamp"],
        "WS-PRIVATE native WooV3WsPositionRow openingTimestamp",
    );
    number(
        actual.adl_quantile,
        &expected["adlQuantile"],
        "WS-PRIVATE native WooV3WsPositionRow adlQuantile",
    );
    number(
        actual.leverage,
        &expected["leverage"],
        "WS-PRIVATE native WooV3WsPositionRow leverage",
    );
    assert_eq!(
        actual.margin_mode,
        expected["marginMode"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow marginMode"
    );
    assert_eq!(
        actual.position_side,
        expected["positionSide"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow positionSide"
    );
    assert_eq!(
        actual.isolated_token,
        expected["isolatedToken"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow isolatedToken"
    );
    assert_eq!(
        actual.isolated_amount,
        expected["isolatedAmount"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow isolatedAmount"
    );
    assert_eq!(
        actual.isolated_frozen_long,
        expected["isolatedFrozenLong"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow isolatedFrozenLong"
    );
    assert_eq!(
        actual.isolated_frozen_short,
        expected["isolatedFrozenShort"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsPositionRow isolatedFrozenShort"
    );
    number(
        actual.version,
        &expected["version"],
        "WS-PRIVATE native WooV3WsPositionRow version",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PRIVATE native WooV3WsPositionRow timestamp",
    );
}
fn execution_report(actual: &WooV3WsExecutionReport, expected: &Value) {
    let mt = expected["messageType"]["$number"]
        .as_str()
        .unwrap()
        .parse::<f64>()
        .unwrap();
    let kind = if mt == 0.0 {
        0
    } else if mt == 1.0 || mt == 2.0 {
        1
    } else {
        2
    };
    match (kind, actual) {
        (
            0,
            WooV3WsExecutionReport::Fill {
                message_type,
                symbol,
                client_order_id,
                order_id,
                order_type,
                side,
                position_side,
                quantity,
                price,
                trade_id,
                executed_quantity,
                executed_price,
                fee,
                fee_asset,
                total_executed_quantity,
                average_executed_price,
                status,
                reason,
                order_tag,
                total_fee,
                fee_currency,
                total_rebate,
                rebate_currency,
                visible_quantity,
                timestamp,
                reduce_only,
                maker,
                leverage,
                matched_rpi,
                margin_mode,
            },
        ) => {
            number(
                *message_type,
                &expected["messageType"],
                "WS-PRIVATE native WooV3WsExecutionReport messageType",
            );
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport symbol"
            );
            number(
                *client_order_id,
                &expected["clientOrderId"],
                "WS-PRIVATE native WooV3WsExecutionReport clientOrderId",
            );
            number(
                *order_id,
                &expected["orderId"],
                "WS-PRIVATE native WooV3WsExecutionReport orderId",
            );
            assert_eq!(
                order_type,
                expected["orderType"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport orderType"
            );
            assert_eq!(
                side,
                expected["side"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport side"
            );
            assert_eq!(
                position_side,
                expected["positionSide"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport positionSide"
            );
            assert_eq!(
                quantity,
                expected["quantity"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport quantity"
            );
            assert_eq!(
                price,
                expected["price"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport price"
            );
            number(
                *trade_id,
                &expected["tradeId"],
                "WS-PRIVATE native WooV3WsExecutionReport tradeId",
            );
            assert_eq!(
                executed_quantity,
                expected["executedQuantity"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport executedQuantity"
            );
            assert_eq!(
                executed_price,
                expected["executedPrice"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport executedPrice"
            );
            assert_eq!(
                fee,
                expected["fee"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport fee"
            );
            assert_eq!(
                fee_asset,
                expected["feeAsset"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport feeAsset"
            );
            assert_eq!(
                total_executed_quantity,
                expected["totalExecutedQuantity"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport totalExecutedQuantity"
            );
            assert_eq!(
                average_executed_price,
                expected["averageExecutedPrice"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport averageExecutedPrice"
            );
            assert_eq!(
                status,
                expected["status"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport status"
            );
            assert_eq!(
                reason,
                expected["reason"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport reason"
            );
            assert_eq!(
                order_tag,
                expected["orderTag"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport orderTag"
            );
            assert_eq!(
                total_fee,
                expected["totalFee"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport totalFee"
            );
            assert_eq!(
                fee_currency,
                expected["feeCurrency"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport feeCurrency"
            );
            assert_eq!(
                total_rebate,
                expected["totalRebate"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport totalRebate"
            );
            assert_eq!(
                rebate_currency,
                expected["rebateCurrency"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport rebateCurrency"
            );
            assert_eq!(
                visible_quantity,
                expected["visibleQuantity"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport visibleQuantity"
            );
            number(
                *timestamp,
                &expected["timestamp"],
                "WS-PRIVATE native WooV3WsExecutionReport timestamp",
            );
            assert_eq!(
                *reduce_only,
                expected["reduceOnly"].as_bool().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport reduceOnly"
            );
            assert_eq!(
                *maker,
                expected["maker"].as_bool().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport maker"
            );
            number(
                *leverage,
                &expected["leverage"],
                "WS-PRIVATE native WooV3WsExecutionReport leverage",
            );
            assert_eq!(
                *matched_rpi,
                expected["matchedRpi"].as_bool().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport matchedRpi"
            );
            assert_eq!(
                margin_mode,
                expected["marginMode"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport marginMode"
            );
        }
        (
            1,
            WooV3WsExecutionReport::Reject {
                message_type,
                symbol,
                order_id,
                client_order_id,
                reason,
            },
        ) => {
            number(
                *message_type,
                &expected["messageType"],
                "WS-PRIVATE native WooV3WsExecutionReport messageType",
            );
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport symbol"
            );
            number(
                *order_id,
                &expected["orderId"],
                "WS-PRIVATE native WooV3WsExecutionReport orderId",
            );
            number(
                *client_order_id,
                &expected["clientOrderId"],
                "WS-PRIVATE native WooV3WsExecutionReport clientOrderId",
            );
            assert_eq!(
                reason,
                expected["reason"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport reason"
            );
        }
        (
            2,
            WooV3WsExecutionReport::Cancel {
                message_type,
                symbol,
                reason,
            },
        ) => {
            number(
                *message_type,
                &expected["messageType"],
                "WS-PRIVATE native WooV3WsExecutionReport messageType",
            );
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport symbol"
            );
            assert_eq!(
                reason,
                expected["reason"].as_str().unwrap(),
                "WS-PRIVATE native WooV3WsExecutionReport reason"
            );
        }
        _ => panic!("WS-PRIVATE native WooV3WsExecutionReport variant"),
    }
}
fn algo_execution_row(actual: &WooV3WsAlgoExecutionRow, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow symbol"
    );
    number(
        actual.root_algo_order_id,
        &expected["rootAlgoOrderId"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow rootAlgoOrderId",
    );
    number(
        actual.parent_algo_order_id,
        &expected["parentAlgoOrderId"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow parentAlgoOrderId",
    );
    number(
        actual.algo_order_id,
        &expected["algoOrderId"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow algoOrderId",
    );
    number(
        actual.client_algo_order_id,
        &expected["clientAlgoOrderId"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow clientAlgoOrderId",
    );
    assert_eq!(
        actual.order_tag,
        expected["orderTag"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow orderTag"
    );
    assert_eq!(
        actual.status,
        expected["status"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow status"
    );
    assert_eq!(
        actual.algo_type,
        expected["algoType"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow algoType"
    );
    assert_eq!(
        actual.side,
        expected["side"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow side"
    );
    assert_eq!(
        actual.position_side,
        expected["positionSide"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow positionSide"
    );
    assert_eq!(
        actual.quantity,
        expected["quantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow quantity"
    );
    assert_eq!(
        actual.trigger_price,
        expected["triggerPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggerPrice"
    );
    assert_eq!(
        actual.trigger_price_type,
        expected["triggerPriceType"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggerPriceType"
    );
    assert_eq!(
        actual.trigger_status,
        expected["triggerStatus"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggerStatus"
    );
    assert_eq!(
        actual.price,
        expected["price"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow price"
    );
    assert_eq!(
        actual.order_type,
        expected["orderType"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow orderType"
    );
    assert_eq!(
        actual.trigger_execution_price,
        expected["triggerExecutionPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggerExecutionPrice"
    );
    number(
        actual.trigger_timestamp,
        &expected["triggerTimestamp"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggerTimestamp",
    );
    number(
        actual.trade_id,
        &expected["tradeId"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow tradeId",
    );
    assert_eq!(
        actual.executed_price,
        expected["executedPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow executedPrice"
    );
    assert_eq!(
        actual.executed_quantity,
        expected["executedQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow executedQuantity"
    );
    assert_eq!(
        actual.fee,
        expected["fee"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow fee"
    );
    assert_eq!(
        actual.fee_asset,
        expected["feeAsset"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow feeAsset"
    );
    assert_eq!(
        actual.reason,
        expected["reason"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow reason"
    );
    assert_eq!(
        actual.total_executed_quantity,
        expected["totalExecutedQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow totalExecutedQuantity"
    );
    assert_eq!(
        actual.average_executed_price,
        expected["averageExecutedPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow averageExecutedPrice"
    );
    assert_eq!(
        actual.total_fee,
        expected["totalFee"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow totalFee"
    );
    assert_eq!(
        actual.fee_currency,
        expected["feeCurrency"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow feeCurrency"
    );
    assert_eq!(
        actual.total_rebate,
        expected["totalRebate"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow totalRebate"
    );
    assert_eq!(
        actual.rebate_currency,
        expected["rebateCurrency"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow rebateCurrency"
    );
    assert_eq!(
        actual.visible_quantity,
        expected["visibleQuantity"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow visibleQuantity"
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow timestamp",
    );
    assert_eq!(
        actual.reduce_only,
        expected["reduceOnly"].as_bool().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow reduceOnly"
    );
    assert_eq!(
        actual.activated_price,
        expected["activatedPrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow activatedPrice"
    );
    assert_eq!(
        actual.callback_ratio,
        expected["callbackRatio"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow callbackRatio"
    );
    assert_eq!(
        actual.callback_value,
        expected["callbackValue"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow callbackValue"
    );
    assert_eq!(
        actual.extreme_price,
        expected["extremePrice"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow extremePrice"
    );
    assert_eq!(
        actual.triggered,
        expected["triggered"].as_bool().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow triggered"
    );
    assert_eq!(
        actual.activated,
        expected["activated"].as_bool().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow activated"
    );
    assert_eq!(
        actual.maker,
        expected["maker"].as_bool().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow maker"
    );
    assert_eq!(
        actual.root_algo_status,
        expected["rootAlgoStatus"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow rootAlgoStatus"
    );
    assert_eq!(
        actual.algo_status,
        expected["algoStatus"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow algoStatus"
    );
    number(
        actual.leverage,
        &expected["leverage"],
        "WS-PRIVATE native WooV3WsAlgoExecutionRow leverage",
    );
    assert_eq!(
        actual.margin_mode,
        expected["marginMode"].as_str().unwrap(),
        "WS-PRIVATE native WooV3WsAlgoExecutionRow marginMode"
    );
}
fn rows<T>(actual: &[T], expected: &Value, check: fn(&T, &Value)) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "WS-PRIVATE native row count versus source"
    );
    for (index, row) in actual.iter().enumerate() {
        check(row, &expected[index]);
    }
}
fn payload(actual: &WooV3WsPrivatePayload, topic: &str, expected: &Value) {
    match (topic, actual) {
        ("account", WooV3WsPrivatePayload::Account(row)) => account(row, expected),
        ("balance", WooV3WsPrivatePayload::Balance(value)) => rows(value, expected, balance_row),
        ("position", WooV3WsPrivatePayload::Position(value)) => rows(value, expected, position_row),
        ("executionreport", WooV3WsPrivatePayload::Execution(row)) => {
            execution_report(row, expected)
        }
        ("algoexecutionreport", WooV3WsPrivatePayload::AlgoExecution(value)) => {
            rows(value, expected, algo_execution_row)
        }
        _ => panic!("WS-PRIVATE native payload variant for {topic}"),
    }
}
fn blockers(actual: &[WooV3WsBlocker], topic: &str, expected: &Value) {
    assert_eq!(actual.len(), 1, "WS-PRIVATE native blocker count");
    assert_eq!(
        expected.as_array().unwrap().len(),
        1,
        "WS-PRIVATE source blocker count"
    );
    let code = match topic {
        "account" => WooV3WsBlockerCode::PrivateAccountReplacementSemanticsUndocumented,
        "balance" => WooV3WsBlockerCode::PrivateBalanceReplacementSemanticsUndocumented,
        "position" => WooV3WsBlockerCode::PrivatePositionReplacementSemanticsUndocumented,
        "executionreport" => WooV3WsBlockerCode::ExecutionStatusLifecycleIncomplete,
        "algoexecutionreport" => WooV3WsBlockerCode::AlgoShapeAndLifecycleIncomplete,
        _ => panic!("unknown blocker topic"),
    };
    assert_eq!(actual[0].code, code, "WS-PRIVATE native blocker code");
    assert_eq!(
        actual[0].code.as_str(),
        expected[0]["code"].as_str().unwrap()
    );
    assert_eq!(
        actual[0].reason,
        expected[0]["reason"].as_str().unwrap(),
        "WS-PRIVATE native blocker reason"
    );
    assert_eq!(
        actual[0].requires,
        WsBlockerRequirement::CredentialedDemo,
        "WS-PRIVATE native blocker requirement"
    );
    assert_eq!(expected[0]["requires"], "credentialed-demo");
}
fn frame(actual: &WooV3WsDecodedFrame, expected: &Value, canonical_empty: bool) {
    match (expected["channel"].as_str().unwrap(), actual) {
        (
            "private",
            WooV3WsDecodedFrame::Private {
                channel,
                topic,
                sent_timestamp,
                payload: value,
                blockers: list,
            },
        ) => {
            assert_eq!(
                *channel,
                PrivateChannel::Private,
                "WS-PRIVATE native frame private channel"
            );
            let kind = expected["topic"]["kind"].as_str().unwrap();
            match topic {
                WooV3WsTopicDescriptor::Private {
                    channel,
                    kind: native,
                    topic,
                } => {
                    assert_eq!(*channel, PrivateChannel::Private);
                    assert_eq!(
                        *native,
                        private_kind(kind),
                        "WS-PRIVATE native frame topic kind"
                    );
                    assert_eq!(
                        *topic,
                        private_kind(expected["topic"]["topic"].as_str().unwrap()),
                        "WS-PRIVATE native frame topic text"
                    );
                }
                _ => panic!("WS-PRIVATE native private topic variant"),
            };
            number(
                *sent_timestamp,
                &expected["sentTimestamp"],
                "WS-PRIVATE native sent timestamp",
            );
            if canonical_empty && expected["payload"].as_array().is_some_and(|a| a.is_empty()) {
                match value {
                    WooV3WsPrivatePayload::Balance(value) => assert!(
                        value.is_empty(),
                        "WS-PRIVATE canonical empty declaration union"
                    ),
                    _ => panic!("WS-PRIVATE canonical empty declaration variant"),
                }
            } else {
                payload(value, kind, &expected["payload"]);
            }
            blockers(list, kind, &expected["blockers"]);
        }
        (
            "public",
            WooV3WsDecodedFrame::Public {
                channel,
                topic,
                sent_timestamp,
                payload,
                blockers,
            },
        ) => {
            assert_eq!(
                *channel,
                PublicChannel::Public,
                "WS-PRIVATE native frame public channel"
            );
            let kind = expected["topic"]["kind"].as_str().unwrap();
            match topic {
                WooV3WsTopicDescriptor::Public {
                    channel,
                    kind: native,
                    symbol,
                    topic,
                } => {
                    assert_eq!(*channel, PublicChannel::Public);
                    assert_eq!(
                        *native,
                        match kind {
                            "bbo" => PlainKind::Bbo,
                            "trade" => PlainKind::Trade,
                            _ => panic!("unexpected public fixture"),
                        },
                        "WS-PRIVATE native public topic kind"
                    );
                    assert_eq!(symbol, expected["topic"]["symbol"].as_str().unwrap());
                    assert_eq!(topic, expected["topic"]["topic"].as_str().unwrap());
                }
                _ => panic!("WS-PRIVATE native public topic variant"),
            };
            number(
                *sent_timestamp,
                &expected["sentTimestamp"],
                "WS-PRIVATE native sent timestamp",
            );
            let row = &expected["payload"];
            match (kind, payload) {
                ("bbo", WooV3WsPublicPayload::Bbo(value)) => {
                    assert_eq!(value.symbol, row["symbol"].as_str().unwrap());
                    assert_eq!(value.ask_price, row["askPrice"].as_str().unwrap());
                    assert_eq!(value.ask_quantity, row["askQuantity"].as_str().unwrap());
                    assert_eq!(value.bid_price, row["bidPrice"].as_str().unwrap());
                    assert_eq!(value.bid_quantity, row["bidQuantity"].as_str().unwrap());
                    number(
                        value.timestamp,
                        &row["timestamp"],
                        "WS-PRIVATE public BBO timestamp",
                    );
                }
                ("trade", WooV3WsPublicPayload::Trade(value)) => {
                    assert_eq!(value.symbol, row["symbol"].as_str().unwrap());
                    assert_eq!(value.price, row["price"].as_str().unwrap());
                    assert_eq!(value.quantity, row["quantity"].as_str().unwrap());
                    assert_eq!(value.side, row["side"].as_str().unwrap());
                    number(
                        value.source,
                        &row["source"],
                        "WS-PRIVATE public trade source",
                    );
                    assert_eq!(value.matched_rpi, row["matchedRpi"].as_bool().unwrap());
                    number(
                        value.timestamp,
                        &row["timestamp"],
                        "WS-PRIVATE public trade timestamp",
                    );
                }
                _ => panic!("WS-PRIVATE native public payload variant"),
            };
            assert_eq!(blockers.len(), 0, "WS-PRIVATE native public blocker count");
            assert_eq!(
                expected["blockers"].as_array().unwrap().len(),
                0,
                "WS-PRIVATE source public blocker count"
            );
        }
        _ => panic!("WS-PRIVATE native frame channel variant"),
    }
}
#[test]
fn decoded_native_private_declarations() {
    let data = source();
    assert_eq!(data["declarations"].as_array().unwrap().len(), 9);
    let names = [
        "WooV3WsAccount",
        "WooV3WsAccount",
        "WooV3WsAccount",
        "WooV3WsBalanceRow",
        "WooV3WsPositionRow",
        "WooV3WsExecutionReport",
        "WooV3WsExecutionReport",
        "WooV3WsExecutionReport",
        "WooV3WsAlgoExecutionRow",
    ];
    for index in 0..9 {
        let row = &data["declarations"][index];
        assert_eq!(
            row["name"], names[index],
            "WS-PRIVATE declaration fixture floor"
        );
        let raw = row["input"].as_str().unwrap();
        let expected = &row["expected"];
        match names[index] {
            "WooV3WsAccount" => account(&js_from_str::<WooV3WsAccount>(raw).unwrap(), expected),
            "WooV3WsBalanceRow" => {
                balance_row(&js_from_str::<WooV3WsBalanceRow>(raw).unwrap(), expected)
            }
            "WooV3WsPositionRow" => {
                position_row(&js_from_str::<WooV3WsPositionRow>(raw).unwrap(), expected)
            }
            "WooV3WsExecutionReport" => execution_report(
                &js_from_str::<WooV3WsExecutionReport>(raw).unwrap(),
                expected,
            ),
            "WooV3WsAlgoExecutionRow" => algo_execution_row(
                &js_from_str::<WooV3WsAlgoExecutionRow>(raw).unwrap(),
                expected,
            ),
            _ => unreachable!(),
        }
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsAccount>(raw).is_err(),
            "WS-PRIVATE strict WooV3WsAccount record {raw}"
        );
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsBalanceRow>(raw).is_err(),
            "WS-PRIVATE strict WooV3WsBalanceRow record {raw}"
        );
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsPositionRow>(raw).is_err(),
            "WS-PRIVATE strict WooV3WsPositionRow record {raw}"
        );
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsExecutionReport>(raw).is_err(),
            "WS-PRIVATE strict WooV3WsExecutionReport record {raw}"
        );
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsAlgoExecutionRow>(raw).is_err(),
            "WS-PRIVATE strict WooV3WsAlgoExecutionRow record {raw}"
        );
    }
    for raw in ["null", "{}", "[3]"] {
        assert!(
            js_from_str::<WooV3WsPrivatePayload>(raw).is_err(),
            "WS-PRIVATE strict payload {raw}"
        );
    }
}
#[test]
fn native_private_decoders_match_fields_and_first_errors() {
    let data = source();
    assert_eq!(data["accounts"].as_array().unwrap().len(), 13);
    for index in 0..13 {
        let row = &data["accounts"][index];
        let value = input(row);
        let actual = decode_account(value.as_ref());
        if let Some(ok) = row.get("ok") {
            account(&actual.unwrap(), ok);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["balances"].as_array().unwrap().len(), 11);
    for index in 0..11 {
        let row = &data["balances"][index];
        let value = input(row);
        let actual = decode_balance_rows(value.as_ref());
        if let Some(ok) = row.get("ok") {
            rows(&actual.unwrap(), ok, balance_row);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["positions"].as_array().unwrap().len(), 14);
    for index in 0..14 {
        let row = &data["positions"][index];
        let value = input(row);
        let actual = decode_position_rows(value.as_ref());
        if let Some(ok) = row.get("ok") {
            rows(&actual.unwrap(), ok, position_row);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["executions"].as_array().unwrap().len(), 20);
    for index in 0..20 {
        let row = &data["executions"][index];
        let value = input(row);
        let actual = decode_execution_report(value.as_ref());
        if let Some(ok) = row.get("ok") {
            execution_report(&actual.unwrap(), ok);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["algos"].as_array().unwrap().len(), 13);
    for index in 0..13 {
        let row = &data["algos"][index];
        let value = input(row);
        let actual = decode_algo_execution_rows(value.as_ref());
        if let Some(ok) = row.get("ok") {
            rows(&actual.unwrap(), ok, algo_execution_row);
        } else {
            error(actual, row);
        }
    }
}
#[test]
fn native_private_dispatch_and_frames() {
    let data = source();
    assert_eq!(data["privateRows"].as_array().unwrap().len(), 5);
    let topics = [
        "account",
        "balance",
        "position",
        "executionreport",
        "algoexecutionreport",
    ];
    for index in 0..5 {
        let row = &data["privateRows"][index];
        assert_eq!(row["topic"], topics[index]);
        let value = input(row);
        let actual = decode_private_payload(private_kind(topics[index]), value.as_ref()).unwrap();
        payload(&actual.payload, topics[index], &row["ok"]["payload"]);
        blockers(&actual.blockers, topics[index], &row["ok"]["blockers"]);
        let typed: WooV3WsPrivateDecodedPayload =
            js_from_str(row["decoded"].as_str().unwrap()).unwrap();
        payload(&typed.payload, topics[index], &row["ok"]["payload"]);
        blockers(&typed.blockers, topics[index], &row["ok"]["blockers"]);
    }
    assert_eq!(data["frames"].as_array().unwrap().len(), 16);
    for index in 0..16 {
        let row = &data["frames"][index];
        let value = input(row);
        let actual = decode_woo_v3_ws_data_frame(value.as_ref());
        if let Some(ok) = row.get("ok") {
            frame(&actual.unwrap(), ok, false);
            let declaration: WooV3WsDecodedFrame =
                js_from_str(row["decoded"].as_str().unwrap()).unwrap();
            frame(&declaration, ok, true);
        } else {
            error(actual, row);
        }
    }
    for raw in ["[]", "null", "{}"] {
        assert!(
            js_from_str::<WooV3WsDecodedFrame>(raw).is_err(),
            "WS-PRIVATE strict frame {raw}"
        );
    }
    // Source declarations distinguish channel/topic and require exactly zero public blockers.
    let public = &data["frames"][5];
    let mut invalid: Value = serde_json::from_str(public["decoded"].as_str().unwrap()).unwrap();
    invalid["topic"] = data["frames"][0]["ok"]["topic"].clone();
    assert!(
        js_from_str::<WooV3WsDecodedFrame>(&invalid.to_string()).is_err(),
        "WS-PRIVATE strict frame topic channel"
    );
    let mut invalid: Value = serde_json::from_str(public["decoded"].as_str().unwrap()).unwrap();
    invalid["blockers"] = data["frames"][0]["ok"]["blockers"].clone();
    assert!(
        js_from_str::<WooV3WsDecodedFrame>(&invalid.to_string()).is_err(),
        "WS-PRIVATE strict public blocker arity"
    );
}

#[test]
fn nested_declaration_numbers_survive_private_union_and_frame_buffers() {
    let data = source();
    let topics = [
        "account",
        "account",
        "account",
        "balance",
        "position",
        "executionreport",
        "executionreport",
        "executionreport",
        "algoexecutionreport",
    ];
    let blocker_indices = [0, 0, 0, 1, 2, 3, 3, 3, 4];
    for index in 0..9 {
        let row = &data["declarations"][index];
        let raw = row["input"].as_str().unwrap();
        let value = &row["expected"];
        let topic = topics[index];
        let array = matches!(index, 3 | 4 | 8);
        let raw_payload = if array {
            format!("[{raw}]")
        } else {
            raw.to_owned()
        };
        let expected_payload = if array {
            serde_json::json!([value])
        } else {
            value.clone()
        };
        let native: WooV3WsPrivatePayload = js_from_str(&raw_payload).unwrap();
        payload(&native, topic, &expected_payload);
        let expected_blockers = &data["privateRows"][blocker_indices[index]]["ok"]["blockers"];
        let topic_json = serde_json::json!({"channel":"private","kind":topic,"topic":topic});
        let raw_frame = format!(
            "{{\"channel\":\"private\",\"topic\":{topic_json},\"sentTimestamp\":1e999,\"payload\":{raw_payload},\"blockers\":{expected_blockers}}}"
        );
        let expected = serde_json::json!({"channel":"private","topic":topic_json,"sentTimestamp":{"$number":"Infinity"},"payload":expected_payload,"blockers":expected_blockers});
        let native: WooV3WsDecodedFrame = js_from_str(&raw_frame).unwrap();
        frame(&native, &expected, false);
    }
}

#[test]
fn declaration_boolean_fields_have_distinct_basis_values() {
    let data = source();
    let families: [(usize, &[&str]); 2] = [
        (5, &["reduceOnly", "maker", "matchedRpi"]),
        (8, &["reduceOnly", "triggered", "activated", "maker"]),
    ];
    let mut iterations = 0;
    for (index, fields) in families {
        let fixture = &data["declarations"][index];
        for true_field in fields {
            let mut raw = fixture["input"].as_str().unwrap().to_owned();
            let mut expected = fixture["expected"].clone();
            for field in fields {
                let original = expected[*field].as_bool().unwrap();
                let value = field == true_field;
                let old = format!("\"{field}\":{original}");
                assert_eq!(
                    raw.matches(&old).count(),
                    1,
                    "WS-PRIVATE basis field occurrence"
                );
                raw = raw.replace(&old, &format!("\"{field}\":{value}"));
                expected[*field] = Value::Bool(value);
            }
            if index == 5 {
                execution_report(
                    &js_from_str::<WooV3WsExecutionReport>(&raw).unwrap(),
                    &expected,
                );
            } else {
                algo_execution_row(
                    &js_from_str::<WooV3WsAlgoExecutionRow>(&raw).unwrap(),
                    &expected,
                );
            }
            iterations += 1;
        }
    }
    assert_eq!(iterations, 7, "WS-PRIVATE boolean basis floor");
}
