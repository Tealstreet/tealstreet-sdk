// Generated direct native field expectations from independent live TS property keys.
use safe_cex_types::exchange_metadata::{ExchangeFeatures, ExchangeMetadata};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_types::{ExchangeName, MarginType, OrderStopTriggerType, PositionMode};
pub fn assert_exchange_features_native(native: &ExchangeFeatures, source: &serde_json::Value) {
    assert_eq!(
        native.kline,
        serde_json::from_value::<bool>(
            source
                .get("kline")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field kline"),
        "native ExchangeFeatures.kline"
    );
    assert_eq!(
        native.orderbook,
        serde_json::from_value::<bool>(
            source
                .get("orderbook")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field orderbook"),
        "native ExchangeFeatures.orderbook"
    );
    assert_eq!(
        native.orders,
        serde_json::from_value::<bool>(
            source
                .get("orders")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field orders"),
        "native ExchangeFeatures.orders"
    );
    assert_eq!(
        native.positions,
        serde_json::from_value::<bool>(
            source
                .get("positions")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field positions"),
        "native ExchangeFeatures.positions"
    );
    assert_eq!(
        native.balance,
        serde_json::from_value::<bool>(
            source
                .get("balance")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field balance"),
        "native ExchangeFeatures.balance"
    );
}
pub fn assert_exchange_metadata_native(native: &ExchangeMetadata, source: &serde_json::Value) {
    assert_eq!(
        native.exchange_name,
        serde_json::from_value::<ExchangeName>(
            source
                .get("exchangeName")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field exchangeName"),
        "native ExchangeMetadata.exchangeName"
    );
    assert_eq!(
        native.logo,
        serde_json::from_value::<String>(
            source
                .get("logo")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field logo"),
        "native ExchangeMetadata.logo"
    );
    assert_eq!(
        native.light_logo,
        serde_json::from_value::<String>(
            source
                .get("lightLogo")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field lightLogo"),
        "native ExchangeMetadata.lightLogo"
    );
    assert_eq!(
        native.dark_logo,
        serde_json::from_value::<String>(
            source
                .get("darkLogo")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field darkLogo"),
        "native ExchangeMetadata.darkLogo"
    );
    assert_eq!(
        native.ref_image,
        serde_json::from_value::<String>(
            source
                .get("refImage")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field refImage"),
        "native ExchangeMetadata.refImage"
    );
    assert_eq!(
        native.requires_passphrase,
        serde_json::from_value::<bool>(
            source
                .get("requiresPassphrase")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresPassphrase"),
        "native ExchangeMetadata.requiresPassphrase"
    );
    assert_eq!(
        native.requires_application_id,
        serde_json::from_value::<bool>(
            source
                .get("requiresApplicationId")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresApplicationId"),
        "native ExchangeMetadata.requiresApplicationId"
    );
    assert_eq!(
        native.requires_api_key_index,
        serde_json::from_value::<bool>(
            source
                .get("requiresApiKeyIndex")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresApiKeyIndex"),
        "native ExchangeMetadata.requiresApiKeyIndex"
    );
    assert_eq!(
        native.requires_no_secret,
        serde_json::from_value::<bool>(
            source
                .get("requiresNoSecret")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresNoSecret"),
        "native ExchangeMetadata.requiresNoSecret"
    );
    assert_eq!(
        native.ref_link,
        serde_json::from_value::<Option<String>>(
            source
                .get("refLink")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field refLink"),
        "native ExchangeMetadata.refLink"
    );
    assert_eq!(
        native.enabled,
        serde_json::from_value::<bool>(
            source
                .get("enabled")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field enabled"),
        "native ExchangeMetadata.enabled"
    );
    assert_eq!(
        native.enabled_in_dev,
        serde_json::from_value::<bool>(
            source
                .get("enabledInDev")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field enabledInDev"),
        "native ExchangeMetadata.enabledInDev"
    );
    assert_eq!(
        native.enable_in_preview,
        serde_json::from_value::<Option<bool>>(
            source
                .get("enableInPreview")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field enableInPreview"),
        "native ExchangeMetadata.enableInPreview"
    );
    assert_eq!(
        native.hidden_from_new_account_picker,
        serde_json::from_value::<Option<bool>>(
            source
                .get("hiddenFromNewAccountPicker")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hiddenFromNewAccountPicker"),
        "native ExchangeMetadata.hiddenFromNewAccountPicker"
    );
    assert_eq!(
        native.supported_on_mobile,
        serde_json::from_value::<bool>(
            source
                .get("supportedOnMobile")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportedOnMobile"),
        "native ExchangeMetadata.supportedOnMobile"
    );
    assert_eq!(
        native.supports_spot_toggle,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsSpotToggle")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsSpotToggle"),
        "native ExchangeMetadata.supportsSpotToggle"
    );
    assert_eq!(
        native.supports_options_toggle,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsOptionsToggle")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsOptionsToggle"),
        "native ExchangeMetadata.supportsOptionsToggle"
    );
    assert_eq!(
        native.options_only,
        serde_json::from_value::<Option<bool>>(
            source
                .get("optionsOnly")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field optionsOnly"),
        "native ExchangeMetadata.optionsOnly"
    );
    assert_eq!(
        native.supports_inverse_toggle,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsInverseToggle")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsInverseToggle"),
        "native ExchangeMetadata.supportsInverseToggle"
    );
    assert_eq!(
        native.supports_testnet,
        serde_json::from_value::<bool>(
            source
                .get("supportsTestnet")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsTestnet"),
        "native ExchangeMetadata.supportsTestnet"
    );
    assert_eq!(
        native.leverage_magnets,
        serde_json::from_value::<Vec<f64>>(
            source
                .get("leverageMagnets")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field leverageMagnets"),
        "native ExchangeMetadata.leverageMagnets"
    );
    assert_eq!(
        native.orderbook_tick_delay,
        serde_json::from_value::<f64>(
            source
                .get("orderbookTickDelay")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field orderbookTickDelay"),
        "native ExchangeMetadata.orderbookTickDelay"
    );
    assert_eq!(
        native.pretty_name,
        serde_json::from_value::<String>(
            source
                .get("prettyName")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field prettyName"),
        "native ExchangeMetadata.prettyName"
    );
    assert_eq!(
        native.tv_intra_day_multipliers,
        serde_json::from_value::<Vec<String>>(
            source
                .get("tvIntraDayMultipliers")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field tvIntraDayMultipliers"),
        "native ExchangeMetadata.tvIntraDayMultipliers"
    );
    assert_eq!(
        native.tv_has_daily,
        serde_json::from_value::<Option<bool>>(
            source
                .get("tvHasDaily")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field tvHasDaily"),
        "native ExchangeMetadata.tvHasDaily"
    );
    assert_eq!(
        native.tv_has_weekly_and_monthly,
        serde_json::from_value::<Option<bool>>(
            source
                .get("tvHasWeeklyAndMonthly")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field tvHasWeeklyAndMonthly"),
        "native ExchangeMetadata.tvHasWeeklyAndMonthly"
    );
    assert_eq!(
        native.tv_max_get_bar_fetches,
        serde_json::from_value::<f64>(
            source
                .get("tvMaxGetBarFetches")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field tvMaxGetBarFetches"),
        "native ExchangeMetadata.tvMaxGetBarFetches"
    );
    assert_eq!(
        native.candle_api_count_limit,
        serde_json::from_value::<f64>(
            source
                .get("candleApiCountLimit")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field candleApiCountLimit"),
        "native ExchangeMetadata.candleApiCountLimit"
    );
    assert_eq!(
        native.default_quote,
        serde_json::from_value::<String>(
            source
                .get("defaultQuote")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field defaultQuote"),
        "native ExchangeMetadata.defaultQuote"
    );
    assert_eq!(
        native.position_modes,
        serde_json::from_value::<Vec<PositionMode>>(
            source
                .get("positionModes")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field positionModes"),
        "native ExchangeMetadata.positionModes"
    );
    assert_eq!(
        native.margin_types,
        serde_json::from_value::<Vec<MarginType>>(
            source
                .get("marginTypes")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field marginTypes"),
        "native ExchangeMetadata.marginTypes"
    );
    assert_eq!(
        native.leverage_slider_disabled,
        serde_json::from_value::<bool>(
            source
                .get("leverageSliderDisabled")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field leverageSliderDisabled"),
        "native ExchangeMetadata.leverageSliderDisabled"
    );
    assert_eq!(
        native.has_independent_hedged_leverage_in_isolated_margin,
        serde_json::from_value::<bool>(
            source
                .get("hasIndependentHedgedLeverageInIsolatedMargin")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hasIndependentHedgedLeverageInIsolatedMargin"),
        "native ExchangeMetadata.hasIndependentHedgedLeverageInIsolatedMargin"
    );
    assert_eq!(
        native.has_independent_hedged_leverage_in_cross_margin,
        serde_json::from_value::<bool>(
            source
                .get("hasIndependentHedgedLeverageInCrossMargin")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hasIndependentHedgedLeverageInCrossMargin"),
        "native ExchangeMetadata.hasIndependentHedgedLeverageInCrossMargin"
    );
    assert_eq!(
        native.supports_trailing_stop_loss,
        serde_json::from_value::<bool>(
            source
                .get("supportsTrailingStopLoss")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsTrailingStopLoss"),
        "native ExchangeMetadata.supportsTrailingStopLoss"
    );
    assert_eq!(
        native.supports_opening_trigger_orders,
        serde_json::from_value::<bool>(
            source
                .get("supportsOpeningTriggerOrders")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsOpeningTriggerOrders"),
        "native ExchangeMetadata.supportsOpeningTriggerOrders"
    );
    assert_eq!(
        native.supported_tpsl_trigger_types,
        serde_json::from_value::<Vec<OrderStopTriggerType>>(
            source
                .get("supportedTPSLTriggerTypes")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportedTPSLTriggerTypes"),
        "native ExchangeMetadata.supportedTPSLTriggerTypes"
    );
    assert_eq!(
        native.private_proxy_ips,
        serde_json::from_value::<Vec<String>>(
            source
                .get("privateProxyIps")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field privateProxyIps"),
        "native ExchangeMetadata.privateProxyIps"
    );
    assert_eq!(
        native.default_tp_trigger_type,
        serde_json::from_value::<Option<OrderStopTriggerType>>(
            source
                .get("defaultTPTriggerType")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field defaultTPTriggerType"),
        "native ExchangeMetadata.defaultTPTriggerType"
    );
    assert_eq!(
        native.default_sl_trigger_type,
        serde_json::from_value::<Option<OrderStopTriggerType>>(
            source
                .get("defaultSLTriggerType")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field defaultSLTriggerType"),
        "native ExchangeMetadata.defaultSLTriggerType"
    );
    assert_eq!(
        native.order_pending_update_delay,
        serde_json::from_value::<f64>(
            source
                .get("orderPendingUpdateDelay")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field orderPendingUpdateDelay"),
        "native ExchangeMetadata.orderPendingUpdateDelay"
    );
    assert_eq!(
        native.stop_order_pending_update_delay,
        serde_json::from_value::<f64>(
            source
                .get("stopOrderPendingUpdateDelay")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field stopOrderPendingUpdateDelay"),
        "native ExchangeMetadata.stopOrderPendingUpdateDelay"
    );
    assert_eq!(
        native.requires_cors,
        serde_json::from_value::<bool>(
            source
                .get("requiresCors")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresCors"),
        "native ExchangeMetadata.requiresCors"
    );
    assert_eq!(
        native.forbid_cors,
        serde_json::from_value::<bool>(
            source
                .get("forbidCors")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field forbidCors"),
        "native ExchangeMetadata.forbidCors"
    );
    assert_eq!(
        native.tick_interval,
        serde_json::from_value::<f64>(
            source
                .get("tickInterval")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field tickInterval"),
        "native ExchangeMetadata.tickInterval"
    );
    assert_eq!(
        native.has_empty_positions,
        serde_json::from_value::<bool>(
            source
                .get("hasEmptyPositions")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hasEmptyPositions"),
        "native ExchangeMetadata.hasEmptyPositions"
    );
    assert_eq!(
        native.position_mode_is_global,
        serde_json::from_value::<bool>(
            source
                .get("positionModeIsGlobal")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field positionModeIsGlobal"),
        "native ExchangeMetadata.positionModeIsGlobal"
    );
    assert_eq!(
        native.margin_type_is_global,
        serde_json::from_value::<bool>(
            source
                .get("marginTypeIsGlobal")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field marginTypeIsGlobal"),
        "native ExchangeMetadata.marginTypeIsGlobal"
    );
    assert_eq!(
        native.leverage_is_global,
        serde_json::from_value::<bool>(
            source
                .get("leverageIsGlobal")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field leverageIsGlobal"),
        "native ExchangeMetadata.leverageIsGlobal"
    );
    assert_eq!(
        native.disconnect_warning_threshold,
        serde_json::from_value::<f64>(
            source
                .get("disconnectWarningThreshold")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field disconnectWarningThreshold"),
        "native ExchangeMetadata.disconnectWarningThreshold"
    );
    assert_eq!(
        native.supports_fee_rates,
        serde_json::from_value::<bool>(
            source
                .get("supportsFeeRates")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsFeeRates"),
        "native ExchangeMetadata.supportsFeeRates"
    );
    assert_eq!(
        native.market_order_delay_in_seconds,
        serde_json::from_value::<f64>(
            source
                .get("marketOrderDelayInSeconds")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field marketOrderDelayInSeconds"),
        "native ExchangeMetadata.marketOrderDelayInSeconds"
    );
    assert_eq!(
        native.private_proxy_regions,
        serde_json::from_value::<JsObjectMap<Vec<String>>>(
            source
                .get("privateProxyRegions")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field privateProxyRegions"),
        "native ExchangeMetadata.privateProxyRegions"
    );
    assert_eq!(
        native.auto_down_size_reduce_orders,
        serde_json::from_value::<bool>(
            source
                .get("autoDownSizeReduceOrders")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field autoDownSizeReduceOrders"),
        "native ExchangeMetadata.autoDownSizeReduceOrders"
    );
    assert_eq!(
        native.supports_chaser,
        serde_json::from_value::<bool>(
            source
                .get("supportsChaser")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsChaser"),
        "native ExchangeMetadata.supportsChaser"
    );
    assert_eq!(
        native.supports_position_margin,
        serde_json::from_value::<bool>(
            source
                .get("supportsPositionMargin")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsPositionMargin"),
        "native ExchangeMetadata.supportsPositionMargin"
    );
    assert_eq!(
        native.supports_closed_pn_l_endpoint,
        serde_json::from_value::<bool>(
            source
                .get("supportsClosedPnLEndpoint")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsClosedPnLEndpoint"),
        "native ExchangeMetadata.supportsClosedPnLEndpoint"
    );
    assert_eq!(
        native.supports_partial_stops,
        serde_json::from_value::<bool>(
            source
                .get("supportsPartialStops")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsPartialStops"),
        "native ExchangeMetadata.supportsPartialStops"
    );
    assert_eq!(
        native.supports_infinity_stops,
        serde_json::from_value::<bool>(
            source
                .get("supportsInfinityStops")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsInfinityStops"),
        "native ExchangeMetadata.supportsInfinityStops"
    );
    assert_eq!(
        native.supports_max_cross_leverage,
        serde_json::from_value::<bool>(
            source
                .get("supportsMaxCrossLeverage")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsMaxCrossLeverage"),
        "native ExchangeMetadata.supportsMaxCrossLeverage"
    );
    assert_eq!(
        native.supports_tunnel,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsTunnel")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsTunnel"),
        "native ExchangeMetadata.supportsTunnel"
    );
    assert_eq!(
        native.supports_ws_route,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsWsRoute")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsWsRoute"),
        "native ExchangeMetadata.supportsWsRoute"
    );
    assert_eq!(
        native.supports_vault,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsVault")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsVault"),
        "native ExchangeMetadata.supportsVault"
    );
    assert_eq!(
        native.requires_omni_key_seed,
        serde_json::from_value::<Option<bool>>(
            source
                .get("requiresOmniKeySeed")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresOmniKeySeed"),
        "native ExchangeMetadata.requiresOmniKeySeed"
    );
    assert_eq!(
        native.requires_paradex_address,
        serde_json::from_value::<Option<bool>>(
            source
                .get("requiresParadexAddress")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresParadexAddress"),
        "native ExchangeMetadata.requiresParadexAddress"
    );
    assert_eq!(
        native.supports_subaccount,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsSubaccount")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsSubaccount"),
        "native ExchangeMetadata.supportsSubaccount"
    );
    assert_eq!(
        native.supports_fetch_symbol_max_leverage,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsFetchSymbolMaxLeverage")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsFetchSymbolMaxLeverage"),
        "native ExchangeMetadata.supportsFetchSymbolMaxLeverage"
    );
    assert_eq!(
        native.unified_margin,
        serde_json::from_value::<Option<bool>>(
            source
                .get("unifiedMargin")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field unifiedMargin"),
        "native ExchangeMetadata.unifiedMargin"
    );
    assert_eq!(
        native.supports_kline_seconds,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsKlineSeconds")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsKlineSeconds"),
        "native ExchangeMetadata.supportsKlineSeconds"
    );
    assert_eq!(
        native.default_symbol,
        serde_json::from_value::<Option<String>>(
            source
                .get("defaultSymbol")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field defaultSymbol"),
        "native ExchangeMetadata.defaultSymbol"
    );
    assert_eq!(
        native.default_trading_view_symbol,
        serde_json::from_value::<Option<String>>(
            source
                .get("defaultTradingViewSymbol")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field defaultTradingViewSymbol"),
        "native ExchangeMetadata.defaultTradingViewSymbol"
    );
    assert_eq!(
        native.supports_read_only_key,
        serde_json::from_value::<bool>(
            source
                .get("supportsReadOnlyKey")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsReadOnlyKey"),
        "native ExchangeMetadata.supportsReadOnlyKey"
    );
    assert_eq!(
        native.secondary_api_key_tick_interval,
        serde_json::from_value::<Option<f64>>(
            source
                .get("secondaryApiKeyTickInterval")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field secondaryApiKeyTickInterval"),
        "native ExchangeMetadata.secondaryApiKeyTickInterval"
    );
    assert_eq!(
        native.always_calculate_unrealized_pnl_from_last_price,
        serde_json::from_value::<Option<bool>>(
            source
                .get("alwaysCalculateUnrealizedPnlFromLastPrice")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field alwaysCalculateUnrealizedPnlFromLastPrice"),
        "native ExchangeMetadata.alwaysCalculateUnrealizedPnlFromLastPrice"
    );
    assert_eq!(
        native.force_account_position_mode_change_on_exchange_itself,
        serde_json::from_value::<Option<bool>>(
            source
                .get("forceAccountPositionModeChangeOnExchangeItself")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field forceAccountPositionModeChangeOnExchangeItself"),
        "native ExchangeMetadata.forceAccountPositionModeChangeOnExchangeItself"
    );
    assert_eq!(
        native.hide_margin_type_selector,
        serde_json::from_value::<Option<bool>>(
            source
                .get("hideMarginTypeSelector")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hideMarginTypeSelector"),
        "native ExchangeMetadata.hideMarginTypeSelector"
    );
    assert_eq!(
        native.hide_position_mode_selector,
        serde_json::from_value::<Option<bool>>(
            source
                .get("hidePositionModeSelector")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hidePositionModeSelector"),
        "native ExchangeMetadata.hidePositionModeSelector"
    );
    assert_eq!(
        native.hide_lightweight_charts_popovers,
        serde_json::from_value::<Option<bool>>(
            source
                .get("hideLightweightChartsPopovers")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field hideLightweightChartsPopovers"),
        "native ExchangeMetadata.hideLightweightChartsPopovers"
    );
    assert_eq!(
        native.supports_tpsl_close_attribute,
        serde_json::from_value::<Option<bool>>(
            source
                .get("supportsTPSLCloseAttribute")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsTPSLCloseAttribute"),
        "native ExchangeMetadata.supportsTPSLCloseAttribute"
    );
    assert_eq!(
        native.kline_callback_reset_threshold,
        serde_json::from_value::<Option<f64>>(
            source
                .get("klineCallbackResetThreshold")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field klineCallbackResetThreshold"),
        "native ExchangeMetadata.klineCallbackResetThreshold"
    );
    assert_eq!(
        native.fetch_leverage_on_margin_symbol_change,
        serde_json::from_value::<Option<bool>>(
            source
                .get("fetchLeverageOnMarginSymbolChange")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field fetchLeverageOnMarginSymbolChange"),
        "native ExchangeMetadata.fetchLeverageOnMarginSymbolChange"
    );
    assert_eq!(
        native.enforce_ws_tunnel,
        serde_json::from_value::<Option<bool>>(
            source
                .get("enforceWsTunnel")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field enforceWsTunnel"),
        "native ExchangeMetadata.enforceWsTunnel"
    );
    assert_eq!(
        native.does_not_support_open_interest,
        serde_json::from_value::<bool>(
            source
                .get("doesNotSupportOpenInterest")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field doesNotSupportOpenInterest"),
        "native ExchangeMetadata.doesNotSupportOpenInterest"
    );
    assert_eq!(
        native.latency_domain,
        serde_json::from_value::<Option<String>>(
            source
                .get("latencyDomain")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field latencyDomain"),
        "native ExchangeMetadata.latencyDomain"
    );
    assert_eq!(
        native.requires_tickers_for_markets_loaded,
        serde_json::from_value::<Option<bool>>(
            source
                .get("requiresTickersForMarketsLoaded")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field requiresTickersForMarketsLoaded"),
        "native ExchangeMetadata.requiresTickersForMarketsLoaded"
    );
    assert_exchange_features_native(&native.features, &source["features"]);
    assert_eq!(
        native.fetch_ohlcv_in_worker,
        serde_json::from_value::<Option<bool>>(
            source
                .get("fetchOHLCVInWorker")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field fetchOHLCVInWorker"),
        "native ExchangeMetadata.fetchOHLCVInWorker"
    );
    assert_eq!(
        native.supports_public_only,
        serde_json::from_value::<bool>(
            source
                .get("supportsPublicOnly")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        )
        .expect("source field supportsPublicOnly"),
        "native ExchangeMetadata.supportsPublicOnly"
    );
}
