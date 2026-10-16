use safe_cex_venue_coinbase::{starbase_cutover, types};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/unions.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn records(source: &str) -> BTreeSet<String> {
    syn::parse_file(source)
        .unwrap()
        .items
        .into_iter()
        .filter_map(|item| match item {
            syn::Item::Struct(s) if matches!(s.vis, syn::Visibility::Public(_)) => {
                Some(s.ident.to_string())
            }
            _ => None,
        })
        .collect()
}
macro_rules! probe {
    ($ty:path,$value:expr) => {{
        let value = $value;
        assert!(
            serde_json::from_value::<$ty>(value.clone()).is_ok(),
            "{} valid object",
            stringify!($ty)
        );
        let sequence = Value::Array(value.as_object().unwrap().values().cloned().collect());
        for rejected in [
            sequence,
            json!([]),
            json!(null),
            json!("sample"),
            json!(1),
            json!(false),
        ] {
            assert!(
                serde_json::from_value::<$ty>(rejected.clone()).is_err(),
                "{} trait accepted {rejected}",
                stringify!($ty)
            );
            // Qualified calls expose an inherent decoder shadowing the strict trait.
            assert!(
                <$ty>::deserialize(rejected.clone()).is_err(),
                "{} qualified accepted {rejected}",
                stringify!($ty)
            );
        }
    }};
}

#[test]
fn legacy_record_trait_and_qualified_decoders_are_map_only() {
    let mut visited = BTreeSet::new();
    probe!(
        types::MarketResponseProductsItemFcmTradingSessionDetails,
        &oracle()["legacyRecords"]["MarketResponseProductsItemFcmTradingSessionDetails"]
    );
    visited.insert("MarketResponseProductsItemFcmTradingSessionDetails".to_owned());
    probe!(
        types::MarketResponseProductsItemFutureProductDetailsPerpetualDetails,
        &oracle()["legacyRecords"]["MarketResponseProductsItemFutureProductDetailsPerpetualDetails"]
    );
    visited.insert("MarketResponseProductsItemFutureProductDetailsPerpetualDetails".to_owned());
    probe!(
        types::MarketResponseProductsItemFutureProductDetails,
        &oracle()["legacyRecords"]["MarketResponseProductsItemFutureProductDetails"]
    );
    visited.insert("MarketResponseProductsItemFutureProductDetails".to_owned());
    probe!(
        types::MarketResponseProductsItem,
        &oracle()["legacyRecords"]["MarketResponseProductsItem"]
    );
    visited.insert("MarketResponseProductsItem".to_owned());
    probe!(
        types::MarketResponse,
        &oracle()["legacyRecords"]["MarketResponse"]
    );
    visited.insert("MarketResponse".to_owned());
    probe!(
        types::CandlesResponseCandlesItem,
        &oracle()["legacyRecords"]["CandlesResponseCandlesItem"]
    );
    visited.insert("CandlesResponseCandlesItem".to_owned());
    probe!(
        types::CandlesResponse,
        &oracle()["legacyRecords"]["CandlesResponse"]
    );
    visited.insert("CandlesResponse".to_owned());
    probe!(
        types::ListAccountsResponseItemAvailableBalance,
        &oracle()["legacyRecords"]["ListAccountsResponseItemAvailableBalance"]
    );
    visited.insert("ListAccountsResponseItemAvailableBalance".to_owned());
    probe!(
        types::ListAccountsResponseItemHold,
        &oracle()["legacyRecords"]["ListAccountsResponseItemHold"]
    );
    visited.insert("ListAccountsResponseItemHold".to_owned());
    probe!(
        types::ListAccountsResponseItem,
        &oracle()["legacyRecords"]["ListAccountsResponseItem"]
    );
    visited.insert("ListAccountsResponseItem".to_owned());
    probe!(
        types::ListPortfoliosResponsePortfoliosItem,
        &oracle()["legacyRecords"]["ListPortfoliosResponsePortfoliosItem"]
    );
    visited.insert("ListPortfoliosResponsePortfoliosItem".to_owned());
    probe!(
        types::ListPortfoliosResponse,
        &oracle()["legacyRecords"]["ListPortfoliosResponse"]
    );
    visited.insert("ListPortfoliosResponse".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemEntryVwap,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemEntryVwap"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemEntryVwap".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemVwap,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemVwap"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemVwap".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemUnrealizedPnl,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemUnrealizedPnl"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemUnrealizedPnl".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemMarkPrice,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemMarkPrice"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemMarkPrice".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemLiquidationPrice,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemLiquidationPrice"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemLiquidationPrice".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemImNotional,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemImNotional"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemImNotional".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemMmNotional,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemMmNotional"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemMmNotional".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPositionNotional,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPositionNotional"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemPositionNotional".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional"]
    );
    visited.insert(
        "ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional".to_owned(),
    );
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional"]
    );
    visited.insert(
        "ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional".to_owned(),
    );
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl"]
    );
    visited
        .insert("ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal"]
    );
    visited
        .insert("ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItemPortfolioSummary,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItemPortfolioSummary"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItemPortfolioSummary".to_owned());
    probe!(
        types::ListPositionsIntxResponsePositionsItem,
        &oracle()["legacyRecords"]["ListPositionsIntxResponsePositionsItem"]
    );
    visited.insert("ListPositionsIntxResponsePositionsItem".to_owned());
    probe!(
        types::ListPositionsIntxResponse,
        &oracle()["legacyRecords"]["ListPositionsIntxResponse"]
    );
    visited.insert("ListPositionsIntxResponse".to_owned());
    probe!(
        types::ListPositionsCfmResponsePositionsItem,
        &oracle()["legacyRecords"]["ListPositionsCfmResponsePositionsItem"]
    );
    visited.insert("ListPositionsCfmResponsePositionsItem".to_owned());
    probe!(
        types::ListPositionsCfmResponse,
        &oracle()["legacyRecords"]["ListPositionsCfmResponse"]
    );
    visited.insert("ListPositionsCfmResponse".to_owned());
    probe!(
        types::CandleEventCandlesItem,
        &oracle()["legacyRecords"]["CandleEventCandlesItem"]
    );
    visited.insert("CandleEventCandlesItem".to_owned());
    probe!(
        types::CandleEvent,
        &oracle()["legacyRecords"]["CandleEvent"]
    );
    visited.insert("CandleEvent".to_owned());
    probe!(
        types::TradeEventTradesItem,
        &oracle()["legacyRecords"]["TradeEventTradesItem"]
    );
    visited.insert("TradeEventTradesItem".to_owned());
    probe!(types::TradeEvent, &oracle()["legacyRecords"]["TradeEvent"]);
    visited.insert("TradeEvent".to_owned());
    probe!(
        types::TickerEventTickersItem,
        &oracle()["legacyRecords"]["TickerEventTickersItem"]
    );
    visited.insert("TickerEventTickersItem".to_owned());
    probe!(
        types::TickerEvent,
        &oracle()["legacyRecords"]["TickerEvent"]
    );
    visited.insert("TickerEvent".to_owned());
    probe!(
        types::Level2EventUpdatesItem,
        &oracle()["legacyRecords"]["Level2EventUpdatesItem"]
    );
    visited.insert("Level2EventUpdatesItem".to_owned());
    probe!(
        types::Level2Event,
        &oracle()["legacyRecords"]["Level2Event"]
    );
    visited.insert("Level2Event".to_owned());
    probe!(
        types::IntxInstrumentsResponseItemQuote,
        &oracle()["legacyRecords"]["IntxInstrumentsResponseItemQuote"]
    );
    visited.insert("IntxInstrumentsResponseItemQuote".to_owned());
    probe!(
        types::IntxInstrumentsResponseItem,
        &oracle()["legacyRecords"]["IntxInstrumentsResponseItem"]
    );
    visited.insert("IntxInstrumentsResponseItem".to_owned());
    probe!(
        types::BalanceResponseSummaryPortfolioImNotional,
        &oracle()["legacyRecords"]["BalanceResponseSummaryPortfolioImNotional"]
    );
    visited.insert("BalanceResponseSummaryPortfolioImNotional".to_owned());
    probe!(
        types::BalanceResponseSummaryPortfolioMmNotional,
        &oracle()["legacyRecords"]["BalanceResponseSummaryPortfolioMmNotional"]
    );
    visited.insert("BalanceResponseSummaryPortfolioMmNotional".to_owned());
    probe!(
        types::BalanceResponseSummaryUnrealizedPnl,
        &oracle()["legacyRecords"]["BalanceResponseSummaryUnrealizedPnl"]
    );
    visited.insert("BalanceResponseSummaryUnrealizedPnl".to_owned());
    probe!(
        types::BalanceResponseSummaryBuyingPower,
        &oracle()["legacyRecords"]["BalanceResponseSummaryBuyingPower"]
    );
    visited.insert("BalanceResponseSummaryBuyingPower".to_owned());
    probe!(
        types::BalanceResponseSummaryTotalBalance,
        &oracle()["legacyRecords"]["BalanceResponseSummaryTotalBalance"]
    );
    visited.insert("BalanceResponseSummaryTotalBalance".to_owned());
    probe!(
        types::BalanceResponseSummaryMaxWithdrawal,
        &oracle()["legacyRecords"]["BalanceResponseSummaryMaxWithdrawal"]
    );
    visited.insert("BalanceResponseSummaryMaxWithdrawal".to_owned());
    probe!(
        types::BalanceResponseSummary,
        &oracle()["legacyRecords"]["BalanceResponseSummary"]
    );
    visited.insert("BalanceResponseSummary".to_owned());
    probe!(
        types::BalanceResponse,
        &oracle()["legacyRecords"]["BalanceResponse"]
    );
    visited.insert("BalanceResponse".to_owned());
    probe!(
        types::OrderEventOrdersItem,
        &oracle()["legacyRecords"]["OrderEventOrdersItem"]
    );
    visited.insert("OrderEventOrdersItem".to_owned());
    probe!(types::OrderEvent, &oracle()["legacyRecords"]["OrderEvent"]);
    visited.insert("OrderEvent".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc"]
    );
    visited
        .insert("ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd"]
    );
    visited
        .insert("ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItemOrderConfiguration,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItemOrderConfiguration"]
    );
    visited.insert("ListOrdersResponseOrdersItemOrderConfiguration".to_owned());
    probe!(
        types::ListOrdersResponseOrdersItem,
        &oracle()["legacyRecords"]["ListOrdersResponseOrdersItem"]
    );
    visited.insert("ListOrdersResponseOrdersItem".to_owned());
    probe!(
        types::ListOrdersResponse,
        &oracle()["legacyRecords"]["ListOrdersResponse"]
    );
    visited.insert("ListOrdersResponse".to_owned());
    probe!(
        types::OrderRequestConfigMarketMarketIoc,
        &oracle()["legacyRecords"]["OrderRequestConfigMarketMarketIoc"]
    );
    visited.insert("OrderRequestConfigMarketMarketIoc".to_owned());
    probe!(
        types::OrderRequestConfigSorLimitIoc,
        &oracle()["legacyRecords"]["OrderRequestConfigSorLimitIoc"]
    );
    visited.insert("OrderRequestConfigSorLimitIoc".to_owned());
    probe!(
        types::OrderRequestConfigLimitLimitGtc,
        &oracle()["legacyRecords"]["OrderRequestConfigLimitLimitGtc"]
    );
    visited.insert("OrderRequestConfigLimitLimitGtc".to_owned());
    probe!(
        types::OrderRequestConfigLimitLimitGtd,
        &oracle()["legacyRecords"]["OrderRequestConfigLimitLimitGtd"]
    );
    visited.insert("OrderRequestConfigLimitLimitGtd".to_owned());
    probe!(
        types::OrderRequestConfigStopLimitStopLimitGtc,
        &oracle()["legacyRecords"]["OrderRequestConfigStopLimitStopLimitGtc"]
    );
    visited.insert("OrderRequestConfigStopLimitStopLimitGtc".to_owned());
    probe!(
        types::OrderRequestConfigStopLimitStopLimitGtd,
        &oracle()["legacyRecords"]["OrderRequestConfigStopLimitStopLimitGtd"]
    );
    visited.insert("OrderRequestConfigStopLimitStopLimitGtd".to_owned());
    probe!(
        types::OrderRequestConfigTriggerBracketGtc,
        &oracle()["legacyRecords"]["OrderRequestConfigTriggerBracketGtc"]
    );
    visited.insert("OrderRequestConfigTriggerBracketGtc".to_owned());
    probe!(
        types::OrderRequestConfigTriggerBracketGtd,
        &oracle()["legacyRecords"]["OrderRequestConfigTriggerBracketGtd"]
    );
    visited.insert("OrderRequestConfigTriggerBracketGtd".to_owned());
    probe!(
        types::OrderRequestConfig,
        &oracle()["legacyRecords"]["OrderRequestConfig"]
    );
    visited.insert("OrderRequestConfig".to_owned());
    probe!(
        types::PlaceOrderRequest,
        &oracle()["legacyRecords"]["PlaceOrderRequest"]
    );
    visited.insert("PlaceOrderRequest".to_owned());
    probe!(
        types::PlaceOrderResponseSuccessResponse,
        &oracle()["legacyRecords"]["PlaceOrderResponseSuccessResponse"]
    );
    visited.insert("PlaceOrderResponseSuccessResponse".to_owned());
    probe!(
        types::PlaceOrderResponseErrorResponse,
        &oracle()["legacyRecords"]["PlaceOrderResponseErrorResponse"]
    );
    visited.insert("PlaceOrderResponseErrorResponse".to_owned());
    probe!(
        types::PlaceOrderResponse,
        &oracle()["legacyRecords"]["PlaceOrderResponse"]
    );
    visited.insert("PlaceOrderResponse".to_owned());
    probe!(
        types::EditOrderRequest,
        &oracle()["legacyRecords"]["EditOrderRequest"]
    );
    visited.insert("EditOrderRequest".to_owned());
    probe!(
        types::EditOrderResponseErrorsItem,
        &oracle()["legacyRecords"]["EditOrderResponseErrorsItem"]
    );
    visited.insert("EditOrderResponseErrorsItem".to_owned());
    probe!(
        types::EditOrderResponse,
        &oracle()["legacyRecords"]["EditOrderResponse"]
    );
    visited.insert("EditOrderResponse".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryFuturesBuyingPower,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryFuturesBuyingPower"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryFuturesBuyingPower".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryTotalUsdBalance,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryTotalUsdBalance"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryTotalUsdBalance".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryCbiUsdBalance,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryCbiUsdBalance"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryCbiUsdBalance".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryCfmUsdBalance,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryCfmUsdBalance"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryCfmUsdBalance".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryUnrealizedPnl,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryUnrealizedPnl"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryUnrealizedPnl".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryDailyRealizedPnl,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryDailyRealizedPnl"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryDailyRealizedPnl".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryInitialMargin,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryInitialMargin"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryInitialMargin".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryAvailableMargin,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryAvailableMargin"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryAvailableMargin".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryLiquidationThreshold,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryLiquidationThreshold"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryLiquidationThreshold".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryLiquidationBufferAmount,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryLiquidationBufferAmount"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryLiquidationBufferAmount".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure"]
    );
    visited.insert("BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure".to_owned());
    probe!(
        types::BalanceResponseCfmBalanceSummary,
        &oracle()["legacyRecords"]["BalanceResponseCfmBalanceSummary"]
    );
    visited.insert("BalanceResponseCfmBalanceSummary".to_owned());
    probe!(
        types::BalanceResponseCfm,
        &oracle()["legacyRecords"]["BalanceResponseCfm"]
    );
    visited.insert("BalanceResponseCfm".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolio,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolio"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownPortfolio".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance"]
    );
    visited.insert(
        "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance".to_owned(),
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance"]
    );
    visited.insert(
        "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance".to_owned(),
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance"]
    );
    visited.insert(
        "PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance".to_owned(),
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl"]
    );
    visited.insert(
        "PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl".to_owned(),
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl"]
    );
    visited
        .insert("PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownPortfolioBalances,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPortfolioBalances"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownPortfolioBalances".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownSpotPositionsItem"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownSpotPositionsItem".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownPerpPositionsItem,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownPerpPositionsItem"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownPerpPositionsItem".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdownFuturesPositionsItem,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdownFuturesPositionsItem"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdownFuturesPositionsItem".to_owned());
    probe!(
        types::PortfolioBreakdownResponseBreakdown,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponseBreakdown"]
    );
    visited.insert("PortfolioBreakdownResponseBreakdown".to_owned());
    probe!(
        types::PortfolioBreakdownResponse,
        &oracle()["legacyRecords"]["PortfolioBreakdownResponse"]
    );
    visited.insert("PortfolioBreakdownResponse".to_owned());
    assert_eq!(
        visited,
        records(include_str!("../src/types.rs")),
        "all public records audited"
    );
    assert_eq!(
        visited,
        oracle()["legacyRecords"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
        "all TS records audited"
    );
}

#[test]
fn legacy_market_product_field_identity_supplement() {
    let mut raw = oracle()["legacyRecords"]["MarketResponseProductsItem"].clone();
    let string_fields = [
        ("quote_name", "quote-name"),
        ("base_name", "base-name"),
        ("volume_percentage_change_24h", "volume-percent"),
        ("mid_market_price", "mid-market"),
        ("base_display_symbol", "base-display"),
        ("quote_display_symbol", "quote-display"),
        ("price_percentage_change_24h", "price-percent"),
        ("quote_increment", "quote-increment"),
        ("base_max_size", "base-max"),
        ("price", "price-value"),
        ("product_id", "product-id"),
        ("alias", "alias-value"),
        ("quote_currency_id", "quote-currency"),
        ("base_currency_id", "base-currency"),
        ("price_increment", "price-increment"),
        ("quote_min_size", "quote-min"),
        ("product_type", "product-type"),
        ("volume_24h", "volume-24h"),
        ("base_increment", "base-increment"),
        ("base_min_size", "base-min"),
        ("quote_max_size", "quote-max"),
        ("status", "status-value"),
    ];
    for (field, value) in string_fields {
        raw[field] = json!(value);
    }
    let decoded: types::MarketResponseProductsItem =
        serde_json::from_value(raw.clone()).expect("field identity record");
    macro_rules! assert_string_field {
        ($wire:literal, $field:ident) => {
            assert_eq!(decoded.$field, raw[$wire].as_str().unwrap(), "{}", $wire);
        };
    }
    assert_string_field!("quote_name", quote_name);
    assert_string_field!("base_name", base_name);
    assert_string_field!("volume_percentage_change_24h", volume_percentage_change_24h);
    assert_string_field!("mid_market_price", mid_market_price);
    assert_string_field!("base_display_symbol", base_display_symbol);
    assert_string_field!("quote_display_symbol", quote_display_symbol);
    assert_string_field!("price_percentage_change_24h", price_percentage_change_24h);
    assert_string_field!("quote_increment", quote_increment);
    assert_string_field!("base_max_size", base_max_size);
    assert_string_field!("price", price);
    assert_string_field!("product_id", product_id);
    assert_string_field!("alias", alias);
    assert_string_field!("quote_currency_id", quote_currency_id);
    assert_string_field!("base_currency_id", base_currency_id);
    assert_string_field!("price_increment", price_increment);
    assert_string_field!("quote_min_size", quote_min_size);
    assert_string_field!("product_type", product_type);
    assert_string_field!("volume_24h", volume_24h);
    assert_string_field!("base_increment", base_increment);
    assert_string_field!("base_min_size", base_min_size);
    assert_string_field!("quote_max_size", quote_max_size);
    assert_string_field!("status", status);

    let boolean_fields = [
        ("cancel_only", 0),
        ("post_only", 1),
        ("limit_only", 2),
        ("auction_mode", 3),
        ("new", 4),
        ("trading_disabled", 5),
        ("is_disabled", 6),
        ("watched", 7),
        ("view_only", 8),
    ];
    for (field, _) in boolean_fields {
        let mut probe = raw.clone();
        for (name, _) in boolean_fields {
            probe[name] = json!(name == field);
        }
        let decoded: types::MarketResponseProductsItem =
            serde_json::from_value(probe).expect("one-hot boolean identity record");
        assert_eq!(decoded.cancel_only, field == "cancel_only");
        assert_eq!(decoded.post_only, field == "post_only");
        assert_eq!(decoded.limit_only, field == "limit_only");
        assert_eq!(decoded.auction_mode, field == "auction_mode");
        assert_eq!(decoded.r#new, field == "new");
        assert_eq!(decoded.trading_disabled, field == "trading_disabled");
        assert_eq!(decoded.is_disabled, field == "is_disabled");
        assert_eq!(decoded.watched, field == "watched");
        assert_eq!(decoded.view_only, field == "view_only");
    }
}

#[test]
fn cutover_record_trait_and_qualified_decoders_are_map_only() {
    let mut visited = BTreeSet::new();
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverEvidence,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverEvidence"]
    );
    visited.insert("CoinbaseStarbaseCutoverEvidence".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlan,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseHttpShadowReadinessPlan"]
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlan".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadiness,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseHttpShadowReadiness"]
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadiness".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanProductBinding,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlanProductBinding"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlanProductBinding".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritative,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlanAuthoritative"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlanAuthoritative".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanHttpShadow,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlanHttpShadow"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlanHttpShadow".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocket,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlanWebsocket"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlanWebsocket".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanCutover,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlanCutover"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlanCutover".to_owned());
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlan,
        &oracle()["cutoverRecords"]["CoinbaseStarbaseCutoverPlan"]
    );
    visited.insert("CoinbaseStarbaseCutoverPlan".to_owned());
    assert_eq!(
        visited,
        records(include_str!("../src/starbase_cutover.rs")),
        "all public records audited"
    );
    assert_eq!(
        visited,
        oracle()["cutoverRecords"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
        "all TS records audited"
    );
}
