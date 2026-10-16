// Generated direct native projections; no model Serialize/roundtrip.
impl Native for Tif {
    fn native(&self) -> Value {
        match self {
            Self::Alo => serde_json::json!("Alo"),
            Self::Ioc => serde_json::json!("Ioc"),
            Self::Gtc => serde_json::json!("Gtc"),
        }
    }
}
impl Native for Tpsl {
    fn native(&self) -> Value {
        match self {
            Self::Tp => serde_json::json!("tp"),
            Self::Sl => serde_json::json!("sl"),
        }
    }
}
impl Native for LimitOrderType {
    fn native(&self) -> Value {
        Value::Object([("tif".into(), (&self.tif).native())].into_iter().collect())
    }
}
impl Native for TriggerOrderType {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("triggerPx".into(), number(&self.trigger_px)),
                ("isMarket".into(), serde_json::json!(&self.is_market)),
                ("tpsl".into(), (&self.tpsl).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderType {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "limit".into(),
                    self.limit
                        .as_ref()
                        .map(|value| (value).native())
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
                (
                    "trigger".into(),
                    self.trigger
                        .as_ref()
                        .map(|value| (value).native())
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OidOrCloid {
    fn native(&self) -> Value {
        match self {
            Self::Number(value) => number(value),
            Self::Cloid(value) => value.native(),
        }
    }
}
impl Native for MetaUniverseItem {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("name".into(), serde_json::json!((&self.name).0)),
                ("szDecimals".into(), number(&self.sz_decimals)),
                ("maxLeverage".into(), number(&self.max_leverage)),
                ("index".into(), number(&self.index)),
                (
                    "onlyIsolated".into(),
                    serde_json::json!(&self.only_isolated),
                ),
                (
                    "tokens".into(),
                    Value::Array(vec![number(&(&self.tokens)[0]), number(&(&self.tokens)[1])]),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for MetaTokensItem {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("name".into(), serde_json::json!((&self.name).0)),
                ("index".into(), number(&self.index)),
                ("szDecimals".into(), number(&self.sz_decimals)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for Meta {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "universe".into(),
                    Value::Array(
                        (&self.universe)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
                (
                    "tokens".into(),
                    Value::Array((&self.tokens).iter().map(|item| (item).native()).collect()),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateAssetPositionsItemPositionCumFunding {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("allTime".into(), serde_json::json!((&self.all_time).0)),
                (
                    "sinceChange".into(),
                    serde_json::json!((&self.since_change).0),
                ),
                ("sinceOpen".into(), serde_json::json!((&self.since_open).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateAssetPositionsItemPositionLeverage {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("rawUsd".into(), serde_json::json!((&self.raw_usd).0)),
                ("type".into(), serde_json::json!((&self.r#type).0)),
                ("value".into(), number(&self.value)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateAssetPositionsItemPosition {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("cumFunding".into(), (&self.cum_funding).native()),
                ("entryPx".into(), serde_json::json!((&self.entry_px).0)),
                ("leverage".into(), (&self.leverage).native()),
                (
                    "liquidationPx".into(),
                    serde_json::json!((&self.liquidation_px).0),
                ),
                (
                    "marginUsed".into(),
                    serde_json::json!((&self.margin_used).0),
                ),
                ("maxLeverage".into(), number(&self.max_leverage)),
                (
                    "positionValue".into(),
                    serde_json::json!((&self.position_value).0),
                ),
                (
                    "returnOnEquity".into(),
                    serde_json::json!((&self.return_on_equity).0),
                ),
                ("szi".into(), serde_json::json!((&self.szi).0)),
                (
                    "unrealizedPnl".into(),
                    serde_json::json!((&self.unrealized_pnl).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateAssetPositionsItem {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("position".into(), (&self.position).native()),
                ("type".into(), serde_json::json!((&self.r#type).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateCrossMarginSummary {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "accountValue".into(),
                    serde_json::json!((&self.account_value).0),
                ),
                (
                    "totalMarginUsed".into(),
                    serde_json::json!((&self.total_margin_used).0),
                ),
                (
                    "totalNtlPos".into(),
                    serde_json::json!((&self.total_ntl_pos).0),
                ),
                (
                    "totalRawUsd".into(),
                    serde_json::json!((&self.total_raw_usd).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseStateMarginSummary {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "accountValue".into(),
                    serde_json::json!((&self.account_value).0),
                ),
                (
                    "totalMarginUsed".into(),
                    serde_json::json!((&self.total_margin_used).0),
                ),
                (
                    "totalNtlPos".into(),
                    serde_json::json!((&self.total_ntl_pos).0),
                ),
                (
                    "totalRawUsd".into(),
                    serde_json::json!((&self.total_raw_usd).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ClearinghouseState {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "assetPositions".into(),
                    Value::Array(
                        (&self.asset_positions)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
                (
                    "crossMaintenanceMarginUsed".into(),
                    serde_json::json!((&self.cross_maintenance_margin_used).0),
                ),
                (
                    "crossMarginSummary".into(),
                    (&self.cross_margin_summary).native(),
                ),
                ("marginSummary".into(), (&self.margin_summary).native()),
                ("time".into(), number(&self.time)),
                (
                    "withdrawable".into(),
                    serde_json::json!((&self.withdrawable).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserFills {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("closedPnl".into(), serde_json::json!((&self.closed_pnl).0)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("crossed".into(), serde_json::json!(&self.crossed)),
                ("dir".into(), serde_json::json!((&self.dir).0)),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("oid".into(), number(&self.oid)),
                ("px".into(), serde_json::json!((&self.px).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                (
                    "startPosition".into(),
                    serde_json::json!((&self.start_position).0),
                ),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("time".into(), number(&self.time)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderResponseResponseDataStatusesItemResting {
    fn native(&self) -> Value {
        Value::Object([("oid".into(), number(&self.oid))].into_iter().collect())
    }
}
impl Native for OrderResponseResponseDataStatusesItemFilled {
    fn native(&self) -> Value {
        Value::Object([("oid".into(), number(&self.oid))].into_iter().collect())
    }
}
impl Native for OrderResponseResponseDataStatusesItem {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "resting".into(),
                    self.resting
                        .as_ref()
                        .map(|value| (value).native())
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
                (
                    "filled".into(),
                    self.filled
                        .as_ref()
                        .map(|value| (value).native())
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderResponseResponseData {
    fn native(&self) -> Value {
        Value::Object(
            [(
                "statuses".into(),
                Value::Array(
                    (&self.statuses)
                        .iter()
                        .map(|item| (item).native())
                        .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderResponseResponse {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("type".into(), serde_json::json!((&self.r#type).0)),
                ("data".into(), (&self.data).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderResponse {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("status".into(), serde_json::json!((&self.status).0)),
                ("response".into(), (&self.response).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsTrade {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("time".into(), number(&self.time)),
                ("tid".into(), number(&self.tid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsBook {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                (
                    "levels".into(),
                    Value::Array(vec![
                        Value::Array(
                            (&(&self.levels).0)
                                .iter()
                                .map(|item| (item).native())
                                .collect(),
                        ),
                        Value::Array(
                            (&(&self.levels).1)
                                .iter()
                                .map(|item| (item).native())
                                .collect(),
                        ),
                    ]),
                ),
                ("time".into(), number(&self.time)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsLevel {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("n".into(), number(&self.n)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsOrderOrder {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("limitPx".into(), serde_json::json!((&self.limit_px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("oid".into(), number(&self.oid)),
                ("timestamp".into(), number(&self.timestamp)),
                ("origSz".into(), serde_json::json!((&self.orig_sz).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsOrder {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("order".into(), (&self.order).native()),
                ("status".into(), serde_json::json!((&self.status).0)),
                ("statusTimestamp".into(), number(&self.status_timestamp)),
                ("user".into(), serde_json::json!((&self.user).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsFill {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("time".into(), number(&self.time)),
                (
                    "startPosition".into(),
                    serde_json::json!((&self.start_position).0),
                ),
                ("dir".into(), serde_json::json!((&self.dir).0)),
                ("closedPnl".into(), serde_json::json!((&self.closed_pnl).0)),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("oid".into(), number(&self.oid)),
                ("crossed".into(), serde_json::json!(&self.crossed)),
                ("fee".into(), serde_json::json!((&self.fee).0)),
                ("tid".into(), number(&self.tid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserFunding {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("time".into(), number(&self.time)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("usdc".into(), serde_json::json!((&self.usdc).0)),
                ("szi".into(), serde_json::json!((&self.szi).0)),
                (
                    "fundingRate".into(),
                    serde_json::json!((&self.funding_rate).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsLiquidation {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("lid".into(), number(&self.lid)),
                ("liquidator".into(), serde_json::json!((&self.liquidator).0)),
                (
                    "liquidated_user".into(),
                    serde_json::json!((&self.liquidated_user).0),
                ),
                (
                    "liquidated_ntl_pos".into(),
                    serde_json::json!((&self.liquidated_ntl_pos).0),
                ),
                (
                    "liquidated_account_value".into(),
                    serde_json::json!((&self.liquidated_account_value).0),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsNonUserCancel {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("oid".into(), number(&self.oid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotClearinghouseStateBalancesItem {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                (
                    "token".into(),
                    self.token
                        .as_ref()
                        .map(number)
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
                ("hold".into(), serde_json::json!((&self.hold).0)),
                ("total".into(), serde_json::json!((&self.total).0)),
                (
                    "entryNtl".into(),
                    self.entry_ntl
                        .as_ref()
                        .map(|value| serde_json::json!((value).0))
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotClearinghouseState {
    fn native(&self) -> Value {
        Value::Object(
            [(
                "balances".into(),
                Value::Array(
                    (&self.balances)
                        .iter()
                        .map(|item| (item).native())
                        .collect(),
                ),
            )]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for FrontendOpenOrders {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                (
                    "isPositionTpsl".into(),
                    serde_json::json!(&self.is_position_tpsl),
                ),
                ("isTrigger".into(), serde_json::json!(&self.is_trigger)),
                ("limitPx".into(), serde_json::json!((&self.limit_px).0)),
                ("oid".into(), number(&self.oid)),
                ("orderType".into(), serde_json::json!((&self.order_type).0)),
                ("origSz".into(), serde_json::json!((&self.orig_sz).0)),
                ("reduceOnly".into(), serde_json::json!(&self.reduce_only)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("timestamp".into(), number(&self.timestamp)),
                (
                    "triggerCondition".into(),
                    serde_json::json!((&self.trigger_condition).0),
                ),
                ("triggerPx".into(), serde_json::json!((&self.trigger_px).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for L2BookLevelsItem0Item {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("n".into(), number(&self.n)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for L2BookLevelsItem1Item {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("n".into(), number(&self.n)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for L2Book {
    fn native(&self) -> Value {
        Value::Object(
            [(
                "levels".into(),
                Value::Array(vec![
                    Value::Array(
                        (&(&self.levels).0)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                    Value::Array(
                        (&(&self.levels).1)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ]),
            )]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for CandleSnapshot {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("T".into(), number(&self.t_upper)),
                ("c".into(), serde_json::json!((&self.c).0)),
                ("h".into(), serde_json::json!((&self.h).0)),
                ("i".into(), serde_json::json!((&self.i).0)),
                ("l".into(), serde_json::json!((&self.l).0)),
                ("n".into(), number(&self.n)),
                ("o".into(), serde_json::json!((&self.o).0)),
                ("s".into(), serde_json::json!((&self.s).0)),
                ("t".into(), number(&self.t)),
                ("v".into(), serde_json::json!((&self.v).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for AssetCtx {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("dayNtlVlm".into(), serde_json::json!((&self.day_ntl_vlm).0)),
                (
                    "dayBaseVlm".into(),
                    self.day_base_vlm
                        .as_ref()
                        .map(|value| serde_json::json!((value).0))
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
                ("funding".into(), serde_json::json!((&self.funding).0)),
                (
                    "impactPxs".into(),
                    Value::Array(vec![
                        serde_json::json!((&(&self.impact_pxs).0).0),
                        serde_json::json!((&(&self.impact_pxs).1).0),
                    ]),
                ),
                ("markPx".into(), serde_json::json!((&self.mark_px).0)),
                ("midPx".into(), serde_json::json!((&self.mid_px).0)),
                (
                    "openInterest".into(),
                    serde_json::json!((&self.open_interest).0),
                ),
                ("oraclePx".into(), serde_json::json!((&self.oracle_px).0)),
                ("premium".into(), serde_json::json!((&self.premium).0)),
                ("prevDayPx".into(), serde_json::json!((&self.prev_day_px).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for MetaAndAssetCtxs {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("meta".into(), (&self.meta).native()),
                (
                    "assetCtxs".into(),
                    Value::Array(
                        (&self.asset_ctxs)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserFundingDeltaType {
    fn native(&self) -> Value {
        match self {
            Self::Funding => serde_json::json!("funding"),
        }
    }
}
impl Native for UserFundingDelta {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                (
                    "fundingRate".into(),
                    serde_json::json!((&self.funding_rate).0),
                ),
                ("szi".into(), serde_json::json!((&self.szi).0)),
                ("type".into(), (&self.r#type).native()),
                ("usdc".into(), serde_json::json!((&self.usdc).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserFundingEntry {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("delta".into(), (&self.delta).native()),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("time".into(), number(&self.time)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserNonFundingLedgerDeltaType {
    fn native(&self) -> Value {
        match self {
            Self::Deposit => serde_json::json!("deposit"),
            Self::Withdraw => serde_json::json!("withdraw"),
            Self::Transfer => serde_json::json!("transfer"),
            Self::Liquidation => serde_json::json!("liquidation"),
        }
    }
}
impl Native for UserNonFundingLedgerDelta {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("type".into(), (&self.r#type).native()),
                ("usdc".into(), serde_json::json!((&self.usdc).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserNonFundingLedgerEntry {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("delta".into(), (&self.delta).native()),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("time".into(), number(&self.time)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for FundingHistoryEntry {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                (
                    "fundingRate".into(),
                    serde_json::json!((&self.funding_rate).0),
                ),
                ("premium".into(), serde_json::json!((&self.premium).0)),
                ("time".into(), number(&self.time)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotToken {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("name".into(), serde_json::json!((&self.name).0)),
                ("szDecimals".into(), number(&self.sz_decimals)),
                ("weiDecimals".into(), number(&self.wei_decimals)),
                ("index".into(), number(&self.index)),
                ("tokenId".into(), serde_json::json!((&self.token_id).0)),
                ("isCanonical".into(), serde_json::json!(&self.is_canonical)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotMarket {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("name".into(), serde_json::json!((&self.name).0)),
                (
                    "tokens".into(),
                    Value::Array(vec![number(&(&self.tokens)[0]), number(&(&self.tokens)[1])]),
                ),
                ("index".into(), number(&self.index)),
                ("isCanonical".into(), serde_json::json!(&self.is_canonical)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotMeta {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "tokens".into(),
                    Value::Array((&self.tokens).iter().map(|item| (item).native()).collect()),
                ),
                (
                    "universe".into(),
                    Value::Array(
                        (&self.universe)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotAssetCtx {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("dayNtlVlm".into(), serde_json::json!((&self.day_ntl_vlm).0)),
                (
                    "dayBaseVlm".into(),
                    self.day_base_vlm
                        .as_ref()
                        .map(|value| serde_json::json!((value).0))
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
                ("markPx".into(), serde_json::json!((&self.mark_px).0)),
                ("midPx".into(), serde_json::json!((&self.mid_px).0)),
                ("prevDayPx".into(), serde_json::json!((&self.prev_day_px).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for SpotMetaAndAssetCtxs {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("meta".into(), (&self.meta).native()),
                (
                    "assetCtxs".into(),
                    Value::Array(
                        (&self.asset_ctxs)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for UserOpenOrder {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("limitPx".into(), serde_json::json!((&self.limit_px).0)),
                ("oid".into(), number(&self.oid)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("timestamp".into(), number(&self.timestamp)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderRequest {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("is_buy".into(), serde_json::json!(&self.is_buy)),
                ("sz".into(), number(&self.sz)),
                ("limit_px".into(), number(&self.limit_px)),
                ("order_type".into(), (&self.order_type).native()),
                ("reduce_only".into(), serde_json::json!(&self.reduce_only)),
                (
                    "cloid".into(),
                    self.cloid
                        .as_ref()
                        .map(|value| (value).native())
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for OrderWire {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("a".into(), number(&self.a)),
                ("b".into(), serde_json::json!(&self.b)),
                ("p".into(), serde_json::json!((&self.p).0)),
                ("s".into(), serde_json::json!((&self.s).0)),
                ("r".into(), serde_json::json!(&self.r)),
                ("t".into(), (&self.t).native()),
                (
                    "c".into(),
                    self.c
                        .as_ref()
                        .map(|value| serde_json::json!((value).0))
                        .unwrap_or_else(|| serde_json::json!({"absent":true})),
                ),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for CancelOrderRequest {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("o".into(), number(&self.o)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for CancelByCloidRequest {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("cloid".into(), (&self.cloid).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ModifyRequest {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("oid".into(), (&self.oid).native()),
                ("order".into(), (&self.order).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for ModifyWire {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("oid".into(), number(&self.oid)),
                ("order".into(), (&self.order).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for Signature {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("r".into(), serde_json::json!((&self.r).0)),
                ("s".into(), serde_json::json!((&self.s).0)),
                ("v".into(), number(&self.v)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for Notification {
    fn native(&self) -> Value {
        Value::Object(
            [
                (
                    "notification".into(),
                    serde_json::json!((&self.notification).0),
                ),
                ("user".into(), serde_json::json!((&self.user).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for Candle {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("t".into(), number(&self.t)),
                ("T".into(), number(&self.t_upper)),
                ("s".into(), serde_json::json!((&self.s).0)),
                ("i".into(), serde_json::json!((&self.i).0)),
                ("o".into(), serde_json::json!((&self.o).0)),
                ("c".into(), serde_json::json!((&self.c).0)),
                ("h".into(), serde_json::json!((&self.h).0)),
                ("l".into(), serde_json::json!((&self.l).0)),
                ("v".into(), serde_json::json!((&self.v).0)),
                ("n".into(), number(&self.n)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("interval".into(), serde_json::json!((&self.interval).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for HyperliquidCandle {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("t".into(), number(&self.t)),
                ("T".into(), number(&self.t_upper)),
                ("s".into(), serde_json::json!((&self.s).0)),
                ("i".into(), serde_json::json!((&self.i).0)),
                ("o".into(), serde_json::json!((&self.o).0)),
                ("c".into(), serde_json::json!((&self.c).0)),
                ("h".into(), serde_json::json!((&self.h).0)),
                ("l".into(), serde_json::json!((&self.l).0)),
                ("v".into(), serde_json::json!((&self.v).0)),
                ("n".into(), number(&self.n)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("interval".into(), serde_json::json!((&self.interval).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserFill {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("px".into(), serde_json::json!((&self.px).0)),
                ("sz".into(), serde_json::json!((&self.sz).0)),
                ("side".into(), serde_json::json!((&self.side).0)),
                ("time".into(), number(&self.time)),
                (
                    "startPosition".into(),
                    serde_json::json!((&self.start_position).0),
                ),
                ("dir".into(), serde_json::json!((&self.dir).0)),
                ("closedPnl".into(), serde_json::json!((&self.closed_pnl).0)),
                ("hash".into(), serde_json::json!((&self.hash).0)),
                ("oid".into(), number(&self.oid)),
                ("crossed".into(), serde_json::json!(&self.crossed)),
                ("fee".into(), serde_json::json!((&self.fee).0)),
                ("tid".into(), number(&self.tid)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserFills {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("isSnapshot".into(), serde_json::json!(&self.is_snapshot)),
                (
                    "fills".into(),
                    Value::Array((&self.fills).iter().map(|item| (item).native()).collect()),
                ),
                ("user".into(), serde_json::json!((&self.user).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserFundings {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("isSnapshot".into(), serde_json::json!(&self.is_snapshot)),
                (
                    "fundings".into(),
                    Value::Array(
                        (&self.fundings)
                            .iter()
                            .map(|item| (item).native())
                            .collect(),
                    ),
                ),
                ("user".into(), serde_json::json!((&self.user).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserNonFundingLedgerUpdateType {
    fn native(&self) -> Value {
        match self {
            Self::Deposit => serde_json::json!("deposit"),
            Self::Withdraw => serde_json::json!("withdraw"),
            Self::Transfer => serde_json::json!("transfer"),
            Self::Liquidation => serde_json::json!("liquidation"),
        }
    }
}
impl Native for WsUserNonFundingLedgerUpdate {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("time".into(), number(&self.time)),
                ("coin".into(), serde_json::json!((&self.coin).0)),
                ("usdc".into(), serde_json::json!((&self.usdc).0)),
                ("type".into(), (&self.r#type).native()),
            ]
            .into_iter()
            .collect(),
        )
    }
}
impl Native for WsUserNonFundingLedgerUpdates {
    fn native(&self) -> Value {
        Value::Object(
            [
                ("isSnapshot".into(), serde_json::json!(&self.is_snapshot)),
                (
                    "updates".into(),
                    Value::Array((&self.updates).iter().map(|item| (item).native()).collect()),
                ),
                ("user".into(), serde_json::json!((&self.user).0)),
            ]
            .into_iter()
            .collect(),
        )
    }
}
