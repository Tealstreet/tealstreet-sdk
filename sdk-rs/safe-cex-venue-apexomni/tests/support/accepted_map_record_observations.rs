//! Native field observations, independently reviewed against the 26 TS declarations.
#![allow(non_snake_case)]
pub(super) fn observe_retained(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Number(_) => {
            serde_json::json!({"binary64": format!("{:016x}", safe_cex_num::numeric_ingress::js_value_as_f64(value).expect("native retained number").to_bits())})
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(observe_retained).collect())
        }
        serde_json::Value::Object(items) => serde_json::Value::Object(
            items
                .iter()
                .map(|(key, value)| (key.clone(), observe_retained(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub(super) fn observe_TickerObject(
    d: &safe_cex_venue_apexomni::public_records::TickerObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("fundingRate".to_owned(), serde_json::json!(&d.funding_rate)),
        (
            "highPrice24h".to_owned(),
            serde_json::json!(&d.high_price24h),
        ),
        ("indexPrice".to_owned(), serde_json::json!(&d.index_price)),
        ("lastPrice".to_owned(), serde_json::json!(&d.last_price)),
        ("lowPrice24h".to_owned(), serde_json::json!(&d.low_price24h)),
        (
            "nextFundingTime".to_owned(),
            serde_json::json!(&d.next_funding_time),
        ),
        (
            "openInterest".to_owned(),
            serde_json::json!(&d.open_interest),
        ),
        ("oraclePrice".to_owned(), serde_json::json!(&d.oracle_price)),
        (
            "predictedFundingRate".to_owned(),
            serde_json::json!(&d.predicted_funding_rate),
        ),
        (
            "price24hPcnt".to_owned(),
            serde_json::json!(&d.price24h_pcnt),
        ),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("tradeCount".to_owned(), serde_json::json!(&d.trade_count)),
        ("turnover24h".to_owned(), serde_json::json!(&d.turnover24h)),
        ("volume24h".to_owned(), serde_json::json!(&d.volume24h)),
    ]))
}
pub(super) fn observe_DepthObject(
    d: &safe_cex_venue_apexomni::public_records::DepthObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "a".to_owned(),
            serde_json::Value::Array(
                (&d.a)
                    .iter()
                    .map(|item0| {
                        serde_json::Value::Array(
                            (item0)
                                .iter()
                                .map(|item1| serde_json::json!(item1))
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "b".to_owned(),
            serde_json::Value::Array(
                (&d.b)
                    .iter()
                    .map(|item0| {
                        serde_json::Value::Array(
                            (item0)
                                .iter()
                                .map(|item1| serde_json::json!(item1))
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        ),
        ("s".to_owned(), serde_json::json!(&d.s)),
        (
            "u".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.u).to_bits())),
        ),
    ]))
}
pub(super) fn observe_TradeObject(
    d: &safe_cex_venue_apexomni::public_records::TradeObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("i".to_owned(), serde_json::json!(&d.i)),
        ("p".to_owned(), serde_json::json!(&d.p)),
        ("S".to_owned(), serde_json::json!(&d.side)),
        ("v".to_owned(), serde_json::json!(&d.v)),
        ("s".to_owned(), serde_json::json!(&d.s)),
        (
            "t".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.t).to_bits())),
        ),
    ]))
}
pub(super) fn observe_KlineObject(
    d: &safe_cex_venue_apexomni::public_records::KlineObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("s".to_owned(), serde_json::json!(&d.s)),
        ("i".to_owned(), serde_json::json!(&d.i)),
        (
            "t".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.t).to_bits())),
        ),
        ("c".to_owned(), serde_json::json!(&d.c)),
        ("h".to_owned(), serde_json::json!(&d.h)),
        ("l".to_owned(), serde_json::json!(&d.l)),
        ("o".to_owned(), serde_json::json!(&d.o)),
        ("v".to_owned(), serde_json::json!(&d.v)),
        ("tr".to_owned(), serde_json::json!(&d.tr)),
    ]))
}
pub(super) fn observe_HistoryFundingObject(
    d: &safe_cex_venue_apexomni::public_records::HistoryFundingObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("rate".to_owned(), serde_json::json!(&d.rate)),
        ("price".to_owned(), serde_json::json!(&d.price)),
        (
            "fundingTime".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.funding_time).to_bits())),
        ),
        (
            "fundingTimestamp".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.funding_timestamp).to_bits())),
        ),
    ]))
}
pub(super) fn observe_CurrencyObject(
    d: &safe_cex_venue_apexomni::public_records::CurrencyObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("id".to_owned(), serde_json::json!(&d.id)),
        (
            "starkExAssetId".to_owned(),
            serde_json::json!(&d.stark_ex_asset_id),
        ),
        (
            "starkExResolution".to_owned(),
            serde_json::json!(&d.stark_ex_resolution),
        ),
        ("stepSize".to_owned(), serde_json::json!(&d.step_size)),
        ("showStep".to_owned(), serde_json::json!(&d.show_step)),
        ("iconUrl".to_owned(), serde_json::json!(&d.icon_url)),
    ]))
}
pub(super) fn observe_GlobalObject(
    d: &safe_cex_venue_apexomni::public_records::GlobalObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "feeAccountId".to_owned(),
            serde_json::json!(&d.fee_account_id),
        ),
        (
            "feeAccountL2Key".to_owned(),
            serde_json::json!(&d.fee_account_l2_key),
        ),
        (
            "starkExCollateralCurrencyId".to_owned(),
            serde_json::json!(&d.stark_ex_collateral_currency_id),
        ),
        (
            "starkExFundingValidityPeriod".to_owned(),
            serde_json::json!(format!(
                "{:016x}",
                (&d.stark_ex_funding_validity_period).to_bits()
            )),
        ),
        (
            "starkExMaxFundingRate".to_owned(),
            serde_json::json!(&d.stark_ex_max_funding_rate),
        ),
        (
            "starkExOrdersTreeHeight".to_owned(),
            serde_json::json!(format!(
                "{:016x}",
                (&d.stark_ex_orders_tree_height).to_bits()
            )),
        ),
        (
            "starkExPositionsTreeHeight".to_owned(),
            serde_json::json!(format!(
                "{:016x}",
                (&d.stark_ex_positions_tree_height).to_bits()
            )),
        ),
        (
            "starkExPriceValidityPeriod".to_owned(),
            serde_json::json!(format!(
                "{:016x}",
                (&d.stark_ex_price_validity_period).to_bits()
            )),
        ),
        (
            "starkExContractAddress".to_owned(),
            serde_json::json!(&d.stark_ex_contract_address),
        ),
        (
            "registerEnvId".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.register_env_id).to_bits())),
        ),
        (
            "crossChainAccountId".to_owned(),
            serde_json::json!(&d.cross_chain_account_id),
        ),
        (
            "crossChainL2Key".to_owned(),
            serde_json::json!(&d.cross_chain_l2_key),
        ),
        (
            "fastWithdrawAccountId".to_owned(),
            serde_json::json!(&d.fast_withdraw_account_id),
        ),
        (
            "fastWithdrawFactRegisterAddress".to_owned(),
            serde_json::json!(&d.fast_withdraw_fact_register_address),
        ),
        (
            "fastWithdrawL2Key".to_owned(),
            serde_json::json!(&d.fast_withdraw_l2_key),
        ),
        (
            "fastWithdrawMaxAmount".to_owned(),
            serde_json::json!(&d.fast_withdraw_max_amount),
        ),
        (
            "bybitWithdrawAccountId".to_owned(),
            serde_json::json!(&d.bybit_withdraw_account_id),
        ),
        (
            "bybitWithdrawL2Key".to_owned(),
            serde_json::json!(&d.bybit_withdraw_l2_key),
        ),
    ]))
}
pub(super) fn observe_PerpetualContractObject(
    d: &safe_cex_venue_apexomni::public_records::PerpetualContractObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "baselinePositionValue".to_owned(),
            serde_json::json!(&d.baseline_position_value),
        ),
        (
            "crossId".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.cross_id).to_bits())),
        ),
        (
            "crossSymbolId".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.cross_symbol_id).to_bits())),
        ),
        (
            "crossSymbolName".to_owned(),
            serde_json::json!(&d.cross_symbol_name),
        ),
        ("digitMerge".to_owned(), serde_json::json!(&d.digit_merge)),
        (
            "displayMaxLeverage".to_owned(),
            serde_json::json!(&d.display_max_leverage),
        ),
        (
            "displayMinLeverage".to_owned(),
            serde_json::json!(&d.display_min_leverage),
        ),
        (
            "enableDisplay".to_owned(),
            serde_json::json!(&d.enable_display),
        ),
        (
            "enableOpenPosition".to_owned(),
            serde_json::json!(&d.enable_open_position),
        ),
        ("enableTrade".to_owned(), serde_json::json!(&d.enable_trade)),
        (
            "fundingImpactMarginNotional".to_owned(),
            serde_json::json!(&d.funding_impact_margin_notional),
        ),
        (
            "fundingInterestRate".to_owned(),
            serde_json::json!(&d.funding_interest_rate),
        ),
        (
            "incrementalInitialMarginRate".to_owned(),
            serde_json::json!(&d.incremental_initial_margin_rate),
        ),
        (
            "incrementalMaintenanceMarginRate".to_owned(),
            serde_json::json!(&d.incremental_maintenance_margin_rate),
        ),
        (
            "incrementalPositionValue".to_owned(),
            serde_json::json!(&d.incremental_position_value),
        ),
        (
            "initialMarginRate".to_owned(),
            serde_json::json!(&d.initial_margin_rate),
        ),
        (
            "maintenanceMarginRate".to_owned(),
            serde_json::json!(&d.maintenance_margin_rate),
        ),
        (
            "maxOrderSize".to_owned(),
            serde_json::json!(&d.max_order_size),
        ),
        (
            "maxPositionSize".to_owned(),
            serde_json::json!(&d.max_position_size),
        ),
        (
            "minOrderSize".to_owned(),
            serde_json::json!(&d.min_order_size),
        ),
        (
            "maxMarketPriceRange".to_owned(),
            serde_json::json!(&d.max_market_price_range),
        ),
        (
            "settleCurrencyId".to_owned(),
            serde_json::json!(&d.settle_currency_id),
        ),
        (
            "starkExOraclePriceQuorum".to_owned(),
            serde_json::json!(&d.stark_ex_oracle_price_quorum),
        ),
        (
            "starkExResolution".to_owned(),
            serde_json::json!(&d.stark_ex_resolution),
        ),
        (
            "starkExRiskFactor".to_owned(),
            serde_json::json!(&d.stark_ex_risk_factor),
        ),
        (
            "starkExSyntheticAssetId".to_owned(),
            serde_json::json!(&d.stark_ex_synthetic_asset_id),
        ),
        ("stepSize".to_owned(), serde_json::json!(&d.step_size)),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        (
            "symbolDisplayName".to_owned(),
            serde_json::json!(&d.symbol_display_name),
        ),
        (
            "symbolDisplayName2".to_owned(),
            serde_json::json!(&d.symbol_display_name2),
        ),
        ("tickSize".to_owned(), serde_json::json!(&d.tick_size)),
        (
            "underlyingCurrencyId".to_owned(),
            serde_json::json!(&d.underlying_currency_id),
        ),
        (
            "maxMaintenanceMarginRate".to_owned(),
            serde_json::json!(&d.max_maintenance_margin_rate),
        ),
        (
            "maxPositionValue".to_owned(),
            serde_json::json!(&d.max_position_value),
        ),
    ]))
}
pub(super) fn observe_GetHistoryFundingOptions(
    d: &safe_cex_venue_apexomni::public_records::GetHistoryFundingOptions,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("page".to_owned(), (&d.page).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("beginTimeInclusive".to_owned(), (&d.begin_time_inclusive).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("endTimeExclusive".to_owned(), (&d.end_time_exclusive).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("limit".to_owned(), (&d.limit).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))})))]))
}
pub(super) fn observe_KeyPair(
    d: &safe_cex_venue_apexomni::public_records::KeyPair,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("publicKey".to_owned(), serde_json::json!(&d.public_key)),
        ("publicKeyYCoordinate".to_owned(), (&d.public_key_y_coordinate).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),
        ("privateKey".to_owned(), serde_json::json!(&d.private_key))]))
}
pub(super) fn observe_ApiKeyCredentials(
    d: &safe_cex_venue_apexomni::public_records::ApiKeyCredentials,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("key".to_owned(), serde_json::json!(&d.key)),
        ("secret".to_owned(), serde_json::json!(&d.secret)),
        ("passphrase".to_owned(), serde_json::json!(&d.passphrase)),
    ]))
}
pub(super) fn observe_PerpetualCurrencyObject(
    d: &safe_cex_venue_apexomni::public_records::PerpetualCurrencyObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "usdc".to_owned(),
            serde_json::Value::Array((&d.usdc).iter().map(observe_CurrencyObject).collect()),
        ),
        (
            "usdt".to_owned(),
            serde_json::Value::Array((&d.usdt).iter().map(observe_CurrencyObject).collect()),
        ),
    ]))
}
pub(super) fn observe_WalletObject(
    d: &safe_cex_venue_apexomni::private_records::WalletObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("userId".to_owned(), serde_json::json!(&d.user_id)),
        ("accountId".to_owned(), serde_json::json!(&d.account_id)),
        ("balance".to_owned(), serde_json::json!(&d.balance)),
        ("asset".to_owned(), serde_json::json!(&d.asset)),
        (
            "pendingDepositAmount".to_owned(),
            serde_json::json!(&d.pending_deposit_amount),
        ),
        (
            "pendingWithdrawAmount".to_owned(),
            serde_json::json!(&d.pending_withdraw_amount),
        ),
        (
            "pendingTransferOutAmount".to_owned(),
            serde_json::json!(&d.pending_transfer_out_amount),
        ),
        (
            "pendingTransferInAmount".to_owned(),
            serde_json::json!(&d.pending_transfer_in_amount),
        ),
    ]))
}
pub(super) fn observe_OpenPositionObject(
    d: &safe_cex_venue_apexomni::private_records::OpenPositionObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("userId".to_owned(), serde_json::json!(&d.user_id)),
        ("accountId".to_owned(), serde_json::json!(&d.account_id)),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("status".to_owned(), serde_json::json!(&d.status)),
        ("side".to_owned(), serde_json::json!(&d.side)),
        ("size".to_owned(), serde_json::json!(&d.size)),
        ("maxSize".to_owned(), serde_json::json!(&d.max_size)),
        ("entryPrice".to_owned(), serde_json::json!(&d.entry_price)),
        ("exitPrice".to_owned(), serde_json::json!(&d.exit_price)),
        (
            "unrealizedPnl".to_owned(),
            serde_json::json!(&d.unrealized_pnl),
        ),
        ("realizedPnl".to_owned(), serde_json::json!(&d.realized_pnl)),
        (
            "createdAt".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.created_at).to_bits())),
        ),
        (
            "updatedTime".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.updated_time).to_bits())),
        ),
        ("netFunding".to_owned(), serde_json::json!(&d.net_funding)),
        ("sumOpen".to_owned(), serde_json::json!(&d.sum_open)),
        ("sumClose".to_owned(), serde_json::json!(&d.sum_close)),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        ("fundingFee".to_owned(), serde_json::json!(&d.funding_fee)),
        (
            "lightNumbers".to_owned(),
            serde_json::json!(&d.light_numbers),
        ),
        (
            "customInitialMarginRate".to_owned(),
            serde_json::json!(&d.custom_initial_margin_rate),
        ),
    ]))
}
pub(super) fn observe_AccountsItem(
    d: &safe_cex_venue_apexomni::private_records::AccountsItem,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "createdAt".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.created_at).to_bits())),
        ),
        (
            "takerFeeRate".to_owned(),
            serde_json::json!(&d.taker_fee_rate),
        ),
        (
            "makerFeeRate".to_owned(),
            serde_json::json!(&d.maker_fee_rate),
        ),
        (
            "minInitialMarginRate".to_owned(),
            serde_json::json!(&d.min_initial_margin_rate),
        ),
        ("status".to_owned(), serde_json::json!(&d.status)),
        ("token".to_owned(), serde_json::json!(&d.token)),
        (
            "unrealizePnlPriceType".to_owned(),
            serde_json::json!(&d.unrealize_pnl_price_type),
        ),
    ]))
}
pub(super) fn observe_AccountObject(
    d: &safe_cex_venue_apexomni::private_records::AccountObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("starkKey".to_owned(), serde_json::json!(&d.stark_key)),
        ("positionId".to_owned(), serde_json::json!(&d.position_id)),
        ("quoteBalance".to_owned(), serde_json::json!(&d.quote_balance)),
        ("pendingDeposits".to_owned(), serde_json::json!(&d.pending_deposits)),
        ("pendingWithdrawals".to_owned(), serde_json::json!(&d.pending_withdrawals)),
        ("createdAt".to_owned(), serde_json::json!(format!("{:016x}", (&d.created_at).to_bits()))),
        ("accountNumber".to_owned(), serde_json::json!(&d.account_number)),
        ("adminSignature".to_owned(), serde_json::json!(&d.admin_signature)),
        ("ethereumAddress".to_owned(), serde_json::json!(&d.ethereum_address)),
        ("id".to_owned(), serde_json::json!(&d.id)),
        ("takerFeeRate".to_owned(), serde_json::json!(&d.taker_fee_rate)),
        ("makerFeeRate".to_owned(), serde_json::json!(&d.maker_fee_rate)),
        ("wallets".to_owned(), serde_json::Value::Array((&d.wallets).iter().map(observe_WalletObject).collect())),
        ("openPositions".to_owned(), serde_json::Value::Array((&d.open_positions).iter().map(observe_OpenPositionObject).collect())),
        ("accounts".to_owned(), (&d.accounts).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::Value::Array((optional0).iter().map(observe_AccountsItem).collect())})))]))
}
pub(super) fn observe_WorstPriceObject(
    d: &safe_cex_venue_apexomni::private_records::WorstPriceObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("worstPrice".to_owned(), serde_json::json!(&d.worst_price)),
        (
            "bidOnePrice".to_owned(),
            serde_json::json!(&d.bid_one_price),
        ),
        (
            "askOnePrice".to_owned(),
            serde_json::json!(&d.ask_one_price),
        ),
    ]))
}
pub(super) fn observe_FundingRateObject(
    d: &safe_cex_venue_apexomni::private_records::FundingRateObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("id".to_owned(), serde_json::json!(&d.id)),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("rate".to_owned(), serde_json::json!(&d.rate)),
        (
            "positionSize".to_owned(),
            serde_json::json!(&d.position_size),
        ),
        ("price".to_owned(), serde_json::json!(&d.price)),
        (
            "fundingTime".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.funding_time).to_bits())),
        ),
        ("side".to_owned(), serde_json::json!(&d.side)),
        (
            "fundingValue".to_owned(),
            serde_json::json!(&d.funding_value),
        ),
        ("status".to_owned(), serde_json::json!(&d.status)),
        (
            "transactionId".to_owned(),
            serde_json::json!(&d.transaction_id),
        ),
    ]))
}
pub(super) fn observe_HistoricalPNLObject(
    d: &safe_cex_venue_apexomni::private_records::HistoricalPNLObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("size".to_owned(), serde_json::json!(&d.size)),
        ("exitPrice".to_owned(), serde_json::json!(&d.exit_price)),
        ("price".to_owned(), serde_json::json!(&d.price)),
        ("side".to_owned(), serde_json::json!(&d.side)),
        ("totalPnl".to_owned(), serde_json::json!(&d.total_pnl)),
        (
            "createdAt".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.created_at).to_bits())),
        ),
        ("type".to_owned(), serde_json::json!(&d.r#type)),
        ("isLiquidate".to_owned(), serde_json::json!(&d.is_liquidate)),
        (
            "isDeleverage".to_owned(),
            serde_json::json!(&d.is_deleverage),
        ),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        (
            "closeSharedOpenFee".to_owned(),
            serde_json::json!(&d.close_shared_open_fee),
        ),
        (
            "liquidateFee".to_owned(),
            serde_json::json!(&d.liquidate_fee),
        ),
    ]))
}
pub(super) fn observe_AccountBalanceObject(
    d: &safe_cex_venue_apexomni::private_records::AccountBalanceObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "totalEquityValue".to_owned(),
            serde_json::json!(&d.total_equity_value),
        ),
        (
            "availableBalance".to_owned(),
            serde_json::json!(&d.available_balance),
        ),
        (
            "initialMargin".to_owned(),
            serde_json::json!(&d.initial_margin),
        ),
        (
            "maintenanceMargin".to_owned(),
            serde_json::json!(&d.maintenance_margin),
        ),
    ]))
}
pub(super) fn observe_MultiChainObject(
    d: &safe_cex_venue_apexomni::config_records::MultiChainObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("chains".to_owned(), observe_retained(&d.chains)),
        ("currency".to_owned(), serde_json::json!(&d.currency)),
        ("maxWithdraw".to_owned(), serde_json::json!(&d.max_withdraw)),
        ("minDeposit".to_owned(), serde_json::json!(&d.min_deposit)),
        ("minWithdraw".to_owned(), serde_json::json!(&d.min_withdraw)),
    ]))
}
pub(super) fn observe_SymbolObject(
    d: &safe_cex_venue_apexomni::config_records::SymbolObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "currency".to_owned(),
            serde_json::Value::Array((&d.currency).iter().map(observe_CurrencyObject).collect()),
        ),
        ("global".to_owned(), observe_GlobalObject(&d.global)),
        (
            "perpetualContract".to_owned(),
            serde_json::Value::Array(
                (&d.perpetual_contract)
                    .iter()
                    .map(observe_PerpetualContractObject)
                    .collect(),
            ),
        ),
        (
            "multiChain".to_owned(),
            observe_MultiChainObject(&d.multi_chain),
        ),
    ]))
}
pub(super) fn observe_PerpetualObject(
    d: &safe_cex_venue_apexomni::config_records::PerpetualObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "usdcConfig".to_owned(),
            observe_SymbolObject(&d.usdc_config),
        ),
        (
            "usdtConfig".to_owned(),
            observe_SymbolObject(&d.usdt_config),
        ),
    ]))
}
pub(super) fn observe_SymbolInfoObject(
    d: &safe_cex_venue_apexomni::config_records::SymbolInfoObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([("rankIdx".to_owned(), (&d.rank_idx).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("pricePrecision".to_owned(), (&d.price_precision).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("priceStep".to_owned(), (&d.price_step).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("sizePrecision".to_owned(), (&d.size_precision).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("sizeStep".to_owned(), (&d.size_step).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("baseCoin".to_owned(), (&d.base_coin).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),
        ("currentCoin".to_owned(), (&d.current_coin).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),
        ("baseCoinPrecision".to_owned(), (&d.base_coin_precision).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("baseCoinRealPrecision".to_owned(), (&d.base_coin_real_precision).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("currentCoinPrecision".to_owned(), (&d.current_coin_precision).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(format!("{:016x}", (optional0).to_bits()))}))),
        ("baseCoinIcon".to_owned(), (&d.base_coin_icon).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),
        ("currentCoinIcon".to_owned(), (&d.current_coin_icon).as_ref().map_or_else(|| serde_json::json!({"present":false}), |optional0| serde_json::json!({"present":true,"value":serde_json::json!(optional0)}))),
        ("baselinePositionValue".to_owned(), serde_json::json!(&d.baseline_position_value)),
        ("crossId".to_owned(), serde_json::json!(format!("{:016x}", (&d.cross_id).to_bits()))),
        ("crossSymbolId".to_owned(), serde_json::json!(format!("{:016x}", (&d.cross_symbol_id).to_bits()))),
        ("crossSymbolName".to_owned(), serde_json::json!(&d.cross_symbol_name)),
        ("digitMerge".to_owned(), serde_json::json!(&d.digit_merge)),
        ("displayMaxLeverage".to_owned(), serde_json::json!(&d.display_max_leverage)),
        ("displayMinLeverage".to_owned(), serde_json::json!(&d.display_min_leverage)),
        ("enableDisplay".to_owned(), serde_json::json!(&d.enable_display)),
        ("enableOpenPosition".to_owned(), serde_json::json!(&d.enable_open_position)),
        ("enableTrade".to_owned(), serde_json::json!(&d.enable_trade)),
        ("fundingImpactMarginNotional".to_owned(), serde_json::json!(&d.funding_impact_margin_notional)),
        ("fundingInterestRate".to_owned(), serde_json::json!(&d.funding_interest_rate)),
        ("incrementalInitialMarginRate".to_owned(), serde_json::json!(&d.incremental_initial_margin_rate)),
        ("incrementalMaintenanceMarginRate".to_owned(), serde_json::json!(&d.incremental_maintenance_margin_rate)),
        ("incrementalPositionValue".to_owned(), serde_json::json!(&d.incremental_position_value)),
        ("initialMarginRate".to_owned(), serde_json::json!(&d.initial_margin_rate)),
        ("maintenanceMarginRate".to_owned(), serde_json::json!(&d.maintenance_margin_rate)),
        ("maxOrderSize".to_owned(), serde_json::json!(&d.max_order_size)),
        ("maxPositionSize".to_owned(), serde_json::json!(&d.max_position_size)),
        ("minOrderSize".to_owned(), serde_json::json!(&d.min_order_size)),
        ("maxMarketPriceRange".to_owned(), serde_json::json!(&d.max_market_price_range)),
        ("settleCurrencyId".to_owned(), serde_json::json!(&d.settle_currency_id)),
        ("starkExOraclePriceQuorum".to_owned(), serde_json::json!(&d.stark_ex_oracle_price_quorum)),
        ("starkExResolution".to_owned(), serde_json::json!(&d.stark_ex_resolution)),
        ("starkExRiskFactor".to_owned(), serde_json::json!(&d.stark_ex_risk_factor)),
        ("starkExSyntheticAssetId".to_owned(), serde_json::json!(&d.stark_ex_synthetic_asset_id)),
        ("stepSize".to_owned(), serde_json::json!(&d.step_size)),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("symbolDisplayName".to_owned(), serde_json::json!(&d.symbol_display_name)),
        ("symbolDisplayName2".to_owned(), serde_json::json!(&d.symbol_display_name2)),
        ("tickSize".to_owned(), serde_json::json!(&d.tick_size)),
        ("underlyingCurrencyId".to_owned(), serde_json::json!(&d.underlying_currency_id)),
        ("maxMaintenanceMarginRate".to_owned(), serde_json::json!(&d.max_maintenance_margin_rate)),
        ("maxPositionValue".to_owned(), serde_json::json!(&d.max_position_value))]))
}
pub(super) fn observe_UserObject(
    d: &safe_cex_venue_apexomni::config_records::UserObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        (
            "ethereumAddress".to_owned(),
            serde_json::json!(&d.ethereum_address),
        ),
        (
            "isRegistered".to_owned(),
            serde_json::json!(&d.is_registered),
        ),
        ("email".to_owned(), serde_json::json!(&d.email)),
        ("username".to_owned(), serde_json::json!(&d.username)),
        (
            "referredByAffiliateLink".to_owned(),
            serde_json::json!(&d.referred_by_affiliate_link),
        ),
        (
            "affiliateLink".to_owned(),
            serde_json::json!(&d.affiliate_link),
        ),
        (
            "makerVolume30D".to_owned(),
            serde_json::json!(&d.maker_volume30_d),
        ),
        (
            "takerVolume30D".to_owned(),
            serde_json::json!(&d.taker_volume30_d),
        ),
        ("fees30D".to_owned(), serde_json::json!(&d.fees30_d)),
        (
            "userData".to_owned(),
            (&d.user_data).as_ref().map_or_else(
                || serde_json::json!({"present":false}),
                |optional0| serde_json::json!({"present":true,"value":observe_retained(optional0)}),
            ),
        ),
        (
            "apexTokenBalance".to_owned(),
            serde_json::json!(&d.apex_token_balance),
        ),
        (
            "stakedApexTokenBalance".to_owned(),
            serde_json::json!(&d.staked_apex_token_balance),
        ),
        (
            "isEmailVerified".to_owned(),
            serde_json::json!(&d.is_email_verified),
        ),
        (
            "isSharingUsername".to_owned(),
            serde_json::json!(&d.is_sharing_username),
        ),
        (
            "isSharingAddress".to_owned(),
            serde_json::json!(&d.is_sharing_address),
        ),
        ("country".to_owned(), serde_json::json!(&d.country)),
        ("id".to_owned(), serde_json::json!(&d.id)),
        ("avatarUrl".to_owned(), serde_json::json!(&d.avatar_url)),
        (
            "emailNotifyGeneralEnable".to_owned(),
            serde_json::json!(&d.email_notify_general_enable),
        ),
        (
            "emailNotifyTradingEnable".to_owned(),
            serde_json::json!(&d.email_notify_trading_enable),
        ),
        (
            "emailNotifyAccountEnable".to_owned(),
            serde_json::json!(&d.email_notify_account_enable),
        ),
    ]))
}
pub(super) fn observe_OrderObject(
    d: &safe_cex_venue_apexomni::config_records::OrderObject,
) -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::from_iter([
        ("id".to_owned(), serde_json::json!(&d.id)),
        ("clientId".to_owned(), serde_json::json!(&d.client_id)),
        (
            "clientOrderId".to_owned(),
            serde_json::json!(&d.client_order_id),
        ),
        ("accountId".to_owned(), serde_json::json!(&d.account_id)),
        ("symbol".to_owned(), serde_json::json!(&d.symbol)),
        ("side".to_owned(), serde_json::json!(&d.side)),
        ("price".to_owned(), serde_json::json!(&d.price)),
        ("limitFee".to_owned(), serde_json::json!(&d.limit_fee)),
        ("fee".to_owned(), serde_json::json!(&d.fee)),
        (
            "liquidateFee".to_owned(),
            serde_json::json!(&d.liquidate_fee),
        ),
        ("size".to_owned(), serde_json::json!(&d.size)),
        ("type".to_owned(), serde_json::json!(&d.r#type)),
        ("direction".to_owned(), serde_json::json!(&d.direction)),
        (
            "createdAt".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.created_at).to_bits())),
        ),
        (
            "updatedTime".to_owned(),
            serde_json::json!(format!("{:016x}", (&d.updated_time).to_bits())),
        ),
        ("status".to_owned(), serde_json::json!(&d.status)),
        (
            "timeInForce".to_owned(),
            serde_json::json!(&d.time_in_force),
        ),
        (
            "matchFillId".to_owned(),
            serde_json::json!(&d.match_fill_id),
        ),
        ("reduceOnly".to_owned(), serde_json::json!(&d.reduce_only)),
        (
            "isPositionTpsl".to_owned(),
            serde_json::json!(&d.is_position_tpsl),
        ),
        ("orderId".to_owned(), serde_json::json!(&d.order_id)),
        ("orderType".to_owned(), serde_json::json!(&d.order_type)),
        ("exitType".to_owned(), serde_json::json!(&d.exit_type)),
        (
            "latestMatchFillPrice".to_owned(),
            serde_json::json!(&d.latest_match_fill_price),
        ),
        (
            "cumMatchFillSize".to_owned(),
            serde_json::json!(&d.cum_match_fill_size),
        ),
        (
            "cumMatchFillValue".to_owned(),
            serde_json::json!(&d.cum_match_fill_value),
        ),
        (
            "cumMatchFillFee".to_owned(),
            serde_json::json!(&d.cum_match_fill_fee),
        ),
        (
            "cumSuccessFillSize".to_owned(),
            serde_json::json!(&d.cum_success_fill_size),
        ),
        (
            "cumSuccessFillValue".to_owned(),
            serde_json::json!(&d.cum_success_fill_value),
        ),
        (
            "cumSuccessFillFee".to_owned(),
            serde_json::json!(&d.cum_success_fill_fee),
        ),
        (
            "triggerPriceType".to_owned(),
            serde_json::json!(&d.trigger_price_type),
        ),
        (
            "isOpenTpslOrder".to_owned(),
            serde_json::json!(&d.is_open_tpsl_order),
        ),
        (
            "isSetOpenTp".to_owned(),
            serde_json::json!(&d.is_set_open_tp),
        ),
        (
            "isSetOpenSl".to_owned(),
            serde_json::json!(&d.is_set_open_sl),
        ),
        ("openTpParam".to_owned(), observe_retained(&d.open_tp_param)),
        ("openSlParam".to_owned(), observe_retained(&d.open_sl_param)),
    ]))
}
