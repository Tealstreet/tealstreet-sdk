//! Wire contracts mirrored from asterdex.types.ts.

fn present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXAPIVersion {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v3")]
    V3,
}
#[derive(serde::Deserialize)]
enum AsterDEXAPIVersionWire {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v3")]
    V3,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAPIVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXAPIVersionWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXAPIVersionWire::V1 => Self::V1,
            AsterDEXAPIVersionWire::V3 => Self::V3,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXFapiVersion {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v3")]
    V3,
    #[serde(rename = "v4")]
    V4,
}
#[derive(serde::Deserialize)]
enum AsterDEXFapiVersionWire {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v3")]
    V3,
    #[serde(rename = "v4")]
    V4,
}

impl<'de> serde::Deserialize<'de> for AsterDEXFapiVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXFapiVersionWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXFapiVersionWire::V1 => Self::V1,
            AsterDEXFapiVersionWire::V3 => Self::V3,
            AsterDEXFapiVersionWire::V4 => Self::V4,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXStpMode {
    #[serde(rename = "EXPIRE_TAKER")]
    V0,
    #[serde(rename = "EXPIRE_MAKER")]
    V1,
    #[serde(rename = "EXPIRE_BOTH")]
    V2,
}
#[derive(serde::Deserialize)]
enum AsterDEXStpModeWire {
    #[serde(rename = "EXPIRE_TAKER")]
    V0,
    #[serde(rename = "EXPIRE_MAKER")]
    V1,
    #[serde(rename = "EXPIRE_BOTH")]
    V2,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStpMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXStpModeWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXStpModeWire::V0 => Self::V0,
            AsterDEXStpModeWire::V1 => Self::V1,
            AsterDEXStpModeWire::V2 => Self::V2,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXV3AuthContext {
    #[serde(rename = "userAddress")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_address: Option<String>,
    #[serde(rename = "signerAddress")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub signer_address: Option<String>,
    #[serde(rename = "signerPrivateKey")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub signer_private_key: Option<String>,
}
#[derive(serde::Deserialize)]
struct AsterDEXV3AuthContextWire {
    #[serde(rename = "userAddress")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    user_address: Option<String>,
    #[serde(rename = "signerAddress")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    signer_address: Option<String>,
    #[serde(rename = "signerPrivateKey")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    signer_private_key: Option<String>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXV3AuthContext {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterDEXV3AuthContextWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            user_address: w.user_address,
            signer_address: w.signer_address,
            signer_private_key: w.signer_private_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXMarketResponseFiltersItem {
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "filterType")]
    pub filter_type: String,
    #[serde(rename = "maxPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_price: Option<String>,
    #[serde(rename = "minPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_price: Option<String>,
    #[serde(rename = "tickSize")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tick_size: Option<String>,
    #[serde(rename = "maxQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_qty: Option<String>,
    #[serde(rename = "minQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_qty: Option<String>,
    #[serde(rename = "stepSize")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub step_size: Option<String>,
    #[serde(rename = "limit")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
    #[serde(rename = "notional")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub notional: Option<String>,
    #[serde(rename = "multiplierUp")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub multiplier_up: Option<String>,
    #[serde(rename = "multiplierDown")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub multiplier_down: Option<String>,
    #[serde(rename = "multiplierDecimal")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub multiplier_decimal: Option<String>,
}
#[derive(serde::Deserialize)]
struct AsterDEXMarketResponseFiltersItemWire {
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "filterType")]
    filter_type: String,
    #[serde(rename = "maxPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_price: Option<String>,
    #[serde(rename = "minPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    min_price: Option<String>,
    #[serde(rename = "tickSize")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    tick_size: Option<String>,
    #[serde(rename = "maxQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_qty: Option<String>,
    #[serde(rename = "minQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    min_qty: Option<String>,
    #[serde(rename = "stepSize")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    step_size: Option<String>,
    #[serde(rename = "limit")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<f64>,
    #[serde(rename = "notional")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    notional: Option<String>,
    #[serde(rename = "multiplierUp")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    multiplier_up: Option<String>,
    #[serde(rename = "multiplierDown")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    multiplier_down: Option<String>,
    #[serde(rename = "multiplierDecimal")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    multiplier_decimal: Option<String>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXMarketResponseFiltersItem {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXMarketResponseFiltersItemWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    extra: w.extra,
                    filter_type: w.filter_type,
                    max_price: w.max_price,
                    min_price: w.min_price,
                    tick_size: w.tick_size,
                    max_qty: w.max_qty,
                    min_qty: w.min_qty,
                    step_size: w.step_size,
                    limit: w.limit,
                    notional: w.notional,
                    multiplier_up: w.multiplier_up,
                    multiplier_down: w.multiplier_down,
                    multiplier_decimal: w.multiplier_decimal,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXMarketResponse {
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "baseAsset")]
    pub base_asset: String,
    #[serde(rename = "quoteAsset")]
    pub quote_asset: String,
    #[serde(rename = "contractType")]
    pub contract_type: String,
    #[serde(rename = "deliveryDate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub delivery_date: Option<f64>,
    #[serde(rename = "onboardDate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub onboard_date: Option<f64>,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "maintMarginPercent")]
    pub maint_margin_percent: String,
    #[serde(rename = "requiredMarginPercent")]
    pub required_margin_percent: String,
    #[serde(rename = "pricePrecision")]
    pub price_precision: f64,
    #[serde(rename = "quantityPrecision")]
    pub quantity_precision: f64,
    #[serde(rename = "baseAssetPrecision")]
    pub base_asset_precision: f64,
    #[serde(rename = "quotePrecision")]
    pub quote_precision: f64,
    #[serde(rename = "underlyingType")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub underlying_type: Option<String>,
    #[serde(rename = "underlyingSubType")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub underlying_sub_type: Option<Vec<String>>,
    #[serde(rename = "settlePlan")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub settle_plan: Option<f64>,
    #[serde(rename = "triggerProtect")]
    pub trigger_protect: String,
    #[serde(rename = "filters")]
    pub filters: Vec<AsterDEXMarketResponseFiltersItem>,
    #[serde(rename = "orderTypes")]
    pub order_types: Vec<String>,
    #[serde(rename = "timeInForce")]
    pub time_in_force: Vec<String>,
    #[serde(rename = "liquidationFee")]
    pub liquidation_fee: String,
    #[serde(rename = "marketTakeBound")]
    pub market_take_bound: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXMarketResponseWire {
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    #[serde(rename = "contractType")]
    contract_type: String,
    #[serde(rename = "deliveryDate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    delivery_date: Option<f64>,
    #[serde(rename = "onboardDate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    onboard_date: Option<f64>,
    #[serde(rename = "status")]
    status: String,
    #[serde(rename = "maintMarginPercent")]
    maint_margin_percent: String,
    #[serde(rename = "requiredMarginPercent")]
    required_margin_percent: String,
    #[serde(rename = "pricePrecision")]
    price_precision: f64,
    #[serde(rename = "quantityPrecision")]
    quantity_precision: f64,
    #[serde(rename = "baseAssetPrecision")]
    base_asset_precision: f64,
    #[serde(rename = "quotePrecision")]
    quote_precision: f64,
    #[serde(rename = "underlyingType")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    underlying_type: Option<String>,
    #[serde(rename = "underlyingSubType")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    underlying_sub_type: Option<Vec<String>>,
    #[serde(rename = "settlePlan")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    settle_plan: Option<f64>,
    #[serde(rename = "triggerProtect")]
    trigger_protect: String,
    #[serde(rename = "filters")]
    filters: Vec<AsterDEXMarketResponseFiltersItem>,
    #[serde(rename = "orderTypes")]
    order_types: Vec<String>,
    #[serde(rename = "timeInForce")]
    time_in_force: Vec<String>,
    #[serde(rename = "liquidationFee")]
    liquidation_fee: String,
    #[serde(rename = "marketTakeBound")]
    market_take_bound: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXMarketResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXMarketResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    extra: w.extra,
                    symbol: w.symbol,
                    base_asset: w.base_asset,
                    quote_asset: w.quote_asset,
                    contract_type: w.contract_type,
                    delivery_date: w.delivery_date,
                    onboard_date: w.onboard_date,
                    status: w.status,
                    maint_margin_percent: w.maint_margin_percent,
                    required_margin_percent: w.required_margin_percent,
                    price_precision: w.price_precision,
                    quantity_precision: w.quantity_precision,
                    base_asset_precision: w.base_asset_precision,
                    quote_precision: w.quote_precision,
                    underlying_type: w.underlying_type,
                    underlying_sub_type: w.underlying_sub_type,
                    settle_plan: w.settle_plan,
                    trigger_protect: w.trigger_protect,
                    filters: w.filters,
                    order_types: w.order_types,
                    time_in_force: w.time_in_force,
                    liquidation_fee: w.liquidation_fee,
                    market_take_bound: w.market_take_bound,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountResponseAssetsItem {
    #[serde(rename = "asset")]
    pub asset: String,
    #[serde(rename = "walletBalance")]
    pub wallet_balance: String,
    #[serde(rename = "unrealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "marginBalance")]
    pub margin_balance: String,
    #[serde(rename = "maintMargin")]
    pub maint_margin: String,
    #[serde(rename = "initialMargin")]
    pub initial_margin: String,
    #[serde(rename = "positionInitialMargin")]
    pub position_initial_margin: String,
    #[serde(rename = "openOrderInitialMargin")]
    pub open_order_initial_margin: String,
    #[serde(rename = "maxWithdrawAmount")]
    pub max_withdraw_amount: String,
    #[serde(rename = "crossWalletBalance")]
    pub cross_wallet_balance: String,
    #[serde(rename = "crossUnPnl")]
    pub cross_un_pnl: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
    #[serde(rename = "marginAvailable")]
    pub margin_available: bool,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountResponseAssetsItemWire {
    #[serde(rename = "asset")]
    asset: String,
    #[serde(rename = "walletBalance")]
    wallet_balance: String,
    #[serde(rename = "unrealizedProfit")]
    unrealized_profit: String,
    #[serde(rename = "marginBalance")]
    margin_balance: String,
    #[serde(rename = "maintMargin")]
    maint_margin: String,
    #[serde(rename = "initialMargin")]
    initial_margin: String,
    #[serde(rename = "positionInitialMargin")]
    position_initial_margin: String,
    #[serde(rename = "openOrderInitialMargin")]
    open_order_initial_margin: String,
    #[serde(rename = "maxWithdrawAmount")]
    max_withdraw_amount: String,
    #[serde(rename = "crossWalletBalance")]
    cross_wallet_balance: String,
    #[serde(rename = "crossUnPnl")]
    cross_un_pnl: String,
    #[serde(rename = "availableBalance")]
    available_balance: String,
    #[serde(rename = "marginAvailable")]
    margin_available: bool,
    #[serde(rename = "updateTime")]
    update_time: f64,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountResponseAssetsItem {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountResponseAssetsItemWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    asset: w.asset,
                    wallet_balance: w.wallet_balance,
                    unrealized_profit: w.unrealized_profit,
                    margin_balance: w.margin_balance,
                    maint_margin: w.maint_margin,
                    initial_margin: w.initial_margin,
                    position_initial_margin: w.position_initial_margin,
                    open_order_initial_margin: w.open_order_initial_margin,
                    max_withdraw_amount: w.max_withdraw_amount,
                    cross_wallet_balance: w.cross_wallet_balance,
                    cross_un_pnl: w.cross_un_pnl,
                    available_balance: w.available_balance,
                    margin_available: w.margin_available,
                    update_time: w.update_time,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountResponseAssetsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                asset,
                wallet_balance,
                unrealized_profit,
                margin_balance,
                maint_margin,
                initial_margin,
                position_initial_margin,
                open_order_initial_margin,
                max_withdraw_amount,
                cross_wallet_balance,
                cross_un_pnl,
                available_balance,
                margin_available,
                update_time
            }
        );
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountResponsePositionsItem {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "initialMargin")]
    pub initial_margin: String,
    #[serde(rename = "maintMargin")]
    pub maint_margin: String,
    #[serde(rename = "unrealizedProfit")]
    pub unrealized_profit: String,
    #[serde(rename = "positionInitialMargin")]
    pub position_initial_margin: String,
    #[serde(rename = "openOrderInitialMargin")]
    pub open_order_initial_margin: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "isolated")]
    pub isolated: bool,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "maxNotional")]
    pub max_notional: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "notional")]
    pub notional: String,
    #[serde(rename = "isolatedWallet")]
    pub isolated_wallet: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(rename = "bidNotional")]
    pub bid_notional: String,
    #[serde(rename = "askNotional")]
    pub ask_notional: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountResponsePositionsItemWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "initialMargin")]
    initial_margin: String,
    #[serde(rename = "maintMargin")]
    maint_margin: String,
    #[serde(rename = "unrealizedProfit")]
    unrealized_profit: String,
    #[serde(rename = "positionInitialMargin")]
    position_initial_margin: String,
    #[serde(rename = "openOrderInitialMargin")]
    open_order_initial_margin: String,
    #[serde(rename = "leverage")]
    leverage: String,
    #[serde(rename = "isolated")]
    isolated: bool,
    #[serde(rename = "entryPrice")]
    entry_price: String,
    #[serde(rename = "maxNotional")]
    max_notional: String,
    #[serde(rename = "positionSide")]
    position_side: String,
    #[serde(rename = "positionAmt")]
    position_amt: String,
    #[serde(rename = "notional")]
    notional: String,
    #[serde(rename = "isolatedWallet")]
    isolated_wallet: String,
    #[serde(rename = "updateTime")]
    update_time: f64,
    #[serde(rename = "bidNotional")]
    bid_notional: String,
    #[serde(rename = "askNotional")]
    ask_notional: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountResponsePositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w =
                    <AsterDEXAccountResponsePositionsItemWire as serde::Deserialize>::deserialize(
                        safe_cex_types::strict_decode::object(d),
                    )?;
                Ok(Self {
                    symbol: w.symbol,
                    initial_margin: w.initial_margin,
                    maint_margin: w.maint_margin,
                    unrealized_profit: w.unrealized_profit,
                    position_initial_margin: w.position_initial_margin,
                    open_order_initial_margin: w.open_order_initial_margin,
                    leverage: w.leverage,
                    isolated: w.isolated,
                    entry_price: w.entry_price,
                    max_notional: w.max_notional,
                    position_side: w.position_side,
                    position_amt: w.position_amt,
                    notional: w.notional,
                    isolated_wallet: w.isolated_wallet,
                    update_time: w.update_time,
                    bid_notional: w.bid_notional,
                    ask_notional: w.ask_notional,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountResponsePositionsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                symbol,
                initial_margin,
                maint_margin,
                unrealized_profit,
                position_initial_margin,
                open_order_initial_margin,
                leverage,
                isolated,
                entry_price,
                max_notional,
                position_side,
                position_amt,
                notional,
                isolated_wallet,
                update_time,
                bid_notional,
                ask_notional
            }
        );
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountResponse {
    #[serde(rename = "accountAlias")]
    pub account_alias: String,
    #[serde(rename = "feeTier")]
    pub fee_tier: f64,
    #[serde(rename = "canTrade")]
    pub can_trade: bool,
    #[serde(rename = "canDeposit")]
    pub can_deposit: bool,
    #[serde(rename = "canWithdraw")]
    pub can_withdraw: bool,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(rename = "totalInitialMargin")]
    pub total_initial_margin: String,
    #[serde(rename = "totalMaintMargin")]
    pub total_maint_margin: String,
    #[serde(rename = "totalWalletBalance")]
    pub total_wallet_balance: String,
    #[serde(rename = "totalUnrealizedProfit")]
    pub total_unrealized_profit: String,
    #[serde(rename = "totalMarginBalance")]
    pub total_margin_balance: String,
    #[serde(rename = "totalPositionInitialMargin")]
    pub total_position_initial_margin: String,
    #[serde(rename = "totalOpenOrderInitialMargin")]
    pub total_open_order_initial_margin: String,
    #[serde(rename = "totalCrossWalletBalance")]
    pub total_cross_wallet_balance: String,
    #[serde(rename = "totalCrossUnPnl")]
    pub total_cross_un_pnl: String,
    #[serde(rename = "availableBalance")]
    pub available_balance: String,
    #[serde(rename = "maxWithdrawAmount")]
    pub max_withdraw_amount: String,
    #[serde(rename = "assets")]
    pub assets: Vec<AsterDEXAccountResponseAssetsItem>,
    #[serde(rename = "positions")]
    pub positions: Vec<AsterDEXAccountResponsePositionsItem>,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountResponseWire {
    #[serde(rename = "accountAlias")]
    account_alias: String,
    #[serde(rename = "feeTier")]
    fee_tier: f64,
    #[serde(rename = "canTrade")]
    can_trade: bool,
    #[serde(rename = "canDeposit")]
    can_deposit: bool,
    #[serde(rename = "canWithdraw")]
    can_withdraw: bool,
    #[serde(rename = "updateTime")]
    update_time: f64,
    #[serde(rename = "totalInitialMargin")]
    total_initial_margin: String,
    #[serde(rename = "totalMaintMargin")]
    total_maint_margin: String,
    #[serde(rename = "totalWalletBalance")]
    total_wallet_balance: String,
    #[serde(rename = "totalUnrealizedProfit")]
    total_unrealized_profit: String,
    #[serde(rename = "totalMarginBalance")]
    total_margin_balance: String,
    #[serde(rename = "totalPositionInitialMargin")]
    total_position_initial_margin: String,
    #[serde(rename = "totalOpenOrderInitialMargin")]
    total_open_order_initial_margin: String,
    #[serde(rename = "totalCrossWalletBalance")]
    total_cross_wallet_balance: String,
    #[serde(rename = "totalCrossUnPnl")]
    total_cross_un_pnl: String,
    #[serde(rename = "availableBalance")]
    available_balance: String,
    #[serde(rename = "maxWithdrawAmount")]
    max_withdraw_amount: String,
    #[serde(rename = "assets")]
    assets: Vec<AsterDEXAccountResponseAssetsItem>,
    #[serde(rename = "positions")]
    positions: Vec<AsterDEXAccountResponsePositionsItem>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    account_alias: w.account_alias,
                    fee_tier: w.fee_tier,
                    can_trade: w.can_trade,
                    can_deposit: w.can_deposit,
                    can_withdraw: w.can_withdraw,
                    update_time: w.update_time,
                    total_initial_margin: w.total_initial_margin,
                    total_maint_margin: w.total_maint_margin,
                    total_wallet_balance: w.total_wallet_balance,
                    total_unrealized_profit: w.total_unrealized_profit,
                    total_margin_balance: w.total_margin_balance,
                    total_position_initial_margin: w.total_position_initial_margin,
                    total_open_order_initial_margin: w.total_open_order_initial_margin,
                    total_cross_wallet_balance: w.total_cross_wallet_balance,
                    total_cross_un_pnl: w.total_cross_un_pnl,
                    available_balance: w.available_balance,
                    max_withdraw_amount: w.max_withdraw_amount,
                    assets: w.assets,
                    positions: w.positions,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                account_alias,
                fee_tier,
                can_trade,
                can_deposit,
                can_withdraw,
                update_time,
                total_initial_margin,
                total_maint_margin,
                total_wallet_balance,
                total_unrealized_profit,
                total_margin_balance,
                total_position_initial_margin,
                total_open_order_initial_margin,
                total_cross_wallet_balance,
                total_cross_un_pnl,
                available_balance,
                max_withdraw_amount,
                assets,
                positions
            }
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXPositionModeResponseDualSidePositionString {
    #[serde(rename = "true")]
    V0,
    #[serde(rename = "false")]
    V1,
}
#[derive(serde::Deserialize)]
enum AsterDEXPositionModeResponseDualSidePositionStringWire {
    #[serde(rename = "true")]
    V0,
    #[serde(rename = "false")]
    V1,
}

impl<'de> serde::Deserialize<'de> for AsterDEXPositionModeResponseDualSidePositionString {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire =
            <AsterDEXPositionModeResponseDualSidePositionStringWire as serde::Deserialize>::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(text))?;
        Ok(match wire {
            AsterDEXPositionModeResponseDualSidePositionStringWire::V0 => Self::V0,
            AsterDEXPositionModeResponseDualSidePositionStringWire::V1 => Self::V1,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum AsterDEXPositionModeResponseDualSidePosition {
    Boolean(bool),
    String(AsterDEXPositionModeResponseDualSidePositionString),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXPositionModeResponse {
    #[serde(rename = "dualSidePosition")]
    pub dual_side_position: AsterDEXPositionModeResponseDualSidePosition,
}
#[derive(serde::Deserialize)]
struct AsterDEXPositionModeResponseWire {
    #[serde(rename = "dualSidePosition")]
    dual_side_position: AsterDEXPositionModeResponseDualSidePosition,
}

impl<'de> serde::Deserialize<'de> for AsterDEXPositionModeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterDEXPositionModeResponseWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            dual_side_position: w.dual_side_position,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXPositionResponse {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "breakEvenPrice")]
    pub break_even_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    pub un_realized_profit: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "maxNotionalValue")]
    pub max_notional_value: String,
    #[serde(rename = "marginType")]
    pub margin_type: String,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "isAutoAddMargin")]
    pub is_auto_add_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "notional")]
    pub notional: String,
    #[serde(rename = "isolatedWallet")]
    pub isolated_wallet: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
#[derive(serde::Deserialize)]
struct AsterDEXPositionResponseWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "positionAmt")]
    position_amt: String,
    #[serde(rename = "entryPrice")]
    entry_price: String,
    #[serde(rename = "breakEvenPrice")]
    break_even_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    un_realized_profit: String,
    #[serde(rename = "liquidationPrice")]
    liquidation_price: String,
    #[serde(rename = "leverage")]
    leverage: String,
    #[serde(rename = "maxNotionalValue")]
    max_notional_value: String,
    #[serde(rename = "marginType")]
    margin_type: String,
    #[serde(rename = "isolatedMargin")]
    isolated_margin: String,
    #[serde(rename = "isAutoAddMargin")]
    is_auto_add_margin: String,
    #[serde(rename = "positionSide")]
    position_side: String,
    #[serde(rename = "notional")]
    notional: String,
    #[serde(rename = "isolatedWallet")]
    isolated_wallet: String,
    #[serde(rename = "updateTime")]
    update_time: f64,
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXPositionResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXPositionResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    extra: w.extra,
                    symbol: w.symbol,
                    position_amt: w.position_amt,
                    entry_price: w.entry_price,
                    break_even_price: w.break_even_price,
                    mark_price: w.mark_price,
                    un_realized_profit: w.un_realized_profit,
                    liquidation_price: w.liquidation_price,
                    leverage: w.leverage,
                    max_notional_value: w.max_notional_value,
                    margin_type: w.margin_type,
                    isolated_margin: w.isolated_margin,
                    is_auto_add_margin: w.is_auto_add_margin,
                    position_side: w.position_side,
                    notional: w.notional,
                    isolated_wallet: w.isolated_wallet,
                    update_time: w.update_time,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXOrderResponse {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "orderId")]
    pub order_id: f64,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "origQty")]
    pub orig_qty: String,
    #[serde(rename = "executedQty")]
    pub executed_qty: String,
    #[serde(rename = "cummulativeQuoteQty")]
    pub cummulative_quote_qty: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "stopPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_price: Option<String>,
    #[serde(rename = "icebergQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub iceberg_qty: Option<String>,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(rename = "isWorking")]
    pub is_working: bool,
    #[serde(rename = "workingTime")]
    pub working_time: f64,
    #[serde(rename = "origQuoteOrderQty")]
    pub orig_quote_order_qty: String,
    #[serde(rename = "positionSide")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_side: Option<String>,
    #[serde(rename = "closePosition")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub close_position: Option<bool>,
    #[serde(rename = "reduceOnly")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reduce_only: Option<bool>,
    #[serde(rename = "activatePrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub activate_price: Option<String>,
    #[serde(rename = "priceRate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub price_rate: Option<String>,
    #[serde(rename = "priceProtect")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub price_protect: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
#[derive(serde::Deserialize)]
struct AsterDEXOrderResponseWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "origQty")]
    orig_qty: String,
    #[serde(rename = "executedQty")]
    executed_qty: String,
    #[serde(rename = "cummulativeQuoteQty")]
    cummulative_quote_qty: String,
    #[serde(rename = "status")]
    status: String,
    #[serde(rename = "timeInForce")]
    time_in_force: String,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(rename = "side")]
    side: String,
    #[serde(rename = "stopPrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    stop_price: Option<String>,
    #[serde(rename = "icebergQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    iceberg_qty: Option<String>,
    #[serde(rename = "time")]
    time: f64,
    #[serde(rename = "updateTime")]
    update_time: f64,
    #[serde(rename = "isWorking")]
    is_working: bool,
    #[serde(rename = "workingTime")]
    working_time: f64,
    #[serde(rename = "origQuoteOrderQty")]
    orig_quote_order_qty: String,
    #[serde(rename = "positionSide")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    position_side: Option<String>,
    #[serde(rename = "closePosition")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    close_position: Option<bool>,
    #[serde(rename = "reduceOnly")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    reduce_only: Option<bool>,
    #[serde(rename = "activatePrice")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    activate_price: Option<String>,
    #[serde(rename = "priceRate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price_rate: Option<String>,
    #[serde(rename = "priceProtect")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    price_protect: Option<bool>,
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXOrderResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    extra: w.extra,
                    symbol: w.symbol,
                    order_id: w.order_id,
                    client_order_id: w.client_order_id,
                    price: w.price,
                    orig_qty: w.orig_qty,
                    executed_qty: w.executed_qty,
                    cummulative_quote_qty: w.cummulative_quote_qty,
                    status: w.status,
                    time_in_force: w.time_in_force,
                    r#type: w.r#type,
                    side: w.side,
                    stop_price: w.stop_price,
                    iceberg_qty: w.iceberg_qty,
                    time: w.time,
                    update_time: w.update_time,
                    is_working: w.is_working,
                    working_time: w.working_time,
                    orig_quote_order_qty: w.orig_quote_order_qty,
                    position_side: w.position_side,
                    close_position: w.close_position,
                    reduce_only: w.reduce_only,
                    activate_price: w.activate_price,
                    price_rate: w.price_rate,
                    price_protect: w.price_protect,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXTickerResponse {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "priceChange")]
    pub price_change: String,
    #[serde(rename = "priceChangePercent")]
    pub price_change_percent: String,
    #[serde(rename = "weightedAvgPrice")]
    pub weighted_avg_price: String,
    #[serde(rename = "lastPrice")]
    pub last_price: String,
    #[serde(rename = "lastQty")]
    pub last_qty: String,
    #[serde(rename = "openPrice")]
    pub open_price: String,
    #[serde(rename = "highPrice")]
    pub high_price: String,
    #[serde(rename = "lowPrice")]
    pub low_price: String,
    #[serde(rename = "volume")]
    pub volume: String,
    #[serde(rename = "quoteVolume")]
    pub quote_volume: String,
    #[serde(rename = "openTime")]
    pub open_time: f64,
    #[serde(rename = "closeTime")]
    pub close_time: f64,
    #[serde(rename = "firstId")]
    pub first_id: f64,
    #[serde(rename = "lastId")]
    pub last_id: f64,
    #[serde(rename = "count")]
    pub count: f64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
#[derive(serde::Deserialize)]
struct AsterDEXTickerResponseWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "priceChange")]
    price_change: String,
    #[serde(rename = "priceChangePercent")]
    price_change_percent: String,
    #[serde(rename = "weightedAvgPrice")]
    weighted_avg_price: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "lastQty")]
    last_qty: String,
    #[serde(rename = "openPrice")]
    open_price: String,
    #[serde(rename = "highPrice")]
    high_price: String,
    #[serde(rename = "lowPrice")]
    low_price: String,
    #[serde(rename = "volume")]
    volume: String,
    #[serde(rename = "quoteVolume")]
    quote_volume: String,
    #[serde(rename = "openTime")]
    open_time: f64,
    #[serde(rename = "closeTime")]
    close_time: f64,
    #[serde(rename = "firstId")]
    first_id: f64,
    #[serde(rename = "lastId")]
    last_id: f64,
    #[serde(rename = "count")]
    count: f64,
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXTickerResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXTickerResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    extra: w.extra,
                    symbol: w.symbol,
                    price_change: w.price_change,
                    price_change_percent: w.price_change_percent,
                    weighted_avg_price: w.weighted_avg_price,
                    last_price: w.last_price,
                    last_qty: w.last_qty,
                    open_price: w.open_price,
                    high_price: w.high_price,
                    low_price: w.low_price,
                    volume: w.volume,
                    quote_volume: w.quote_volume,
                    open_time: w.open_time,
                    close_time: w.close_time,
                    first_id: w.first_id,
                    last_id: w.last_id,
                    count: w.count,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXOpenInterestResponse {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "openInterest")]
    pub open_interest: String,
    #[serde(rename = "time")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub time: Option<f64>,
}
#[derive(serde::Deserialize)]
struct AsterDEXOpenInterestResponseWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    #[serde(rename = "time")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    time: Option<f64>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXOpenInterestResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXOpenInterestResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    symbol: w.symbol,
                    open_interest: w.open_interest,
                    time: w.time,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXOpenInterestResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, open_interest, time });
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXDepthResponse {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: f64,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "bids")]
    pub bids: Vec<(String, String)>,
    #[serde(rename = "asks")]
    pub asks: Vec<(String, String)>,
}
#[derive(serde::Deserialize)]
struct AsterDEXDepthResponseWire {
    #[serde(rename = "lastUpdateId")]
    last_update_id: f64,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "bids")]
    bids: Vec<(String, String)>,
    #[serde(rename = "asks")]
    asks: Vec<(String, String)>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXDepthResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXDepthResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    last_update_id: w.last_update_id,
                    capital_e: w.capital_e,
                    capital_t: w.capital_t,
                    bids: w.bids,
                    asks: w.asks,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXTradeResponse {
    #[serde(rename = "id")]
    pub id: f64,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "qty")]
    pub qty: String,
    #[serde(rename = "quoteQty")]
    pub quote_qty: String,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "isBuyerMaker")]
    pub is_buyer_maker: bool,
}
#[derive(serde::Deserialize)]
struct AsterDEXTradeResponseWire {
    #[serde(rename = "id")]
    id: f64,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "qty")]
    qty: String,
    #[serde(rename = "quoteQty")]
    quote_qty: String,
    #[serde(rename = "time")]
    time: f64,
    #[serde(rename = "isBuyerMaker")]
    is_buyer_maker: bool,
}

impl<'de> serde::Deserialize<'de> for AsterDEXTradeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXTradeResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    id: w.id,
                    price: w.price,
                    qty: w.qty,
                    quote_qty: w.quote_qty,
                    time: w.time,
                    is_buyer_maker: w.is_buyer_maker,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXTradeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                id,
                price,
                qty,
                quote_qty,
                time,
                is_buyer_maker
            }
        );
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXUserTradeResponse {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "id")]
    pub id: f64,
    #[serde(rename = "orderId")]
    pub order_id: f64,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "qty")]
    pub qty: String,
    #[serde(rename = "quoteQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quote_qty: Option<String>,
    #[serde(rename = "realizedPnl")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<String>,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "positionSide")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_side: Option<String>,
    #[serde(rename = "buyer")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub buyer: Option<bool>,
    #[serde(rename = "maker")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker: Option<bool>,
    #[serde(rename = "commission")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission: Option<String>,
    #[serde(rename = "commissionAsset")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission_asset: Option<String>,
    #[serde(rename = "time")]
    pub time: f64,
}
#[derive(serde::Deserialize)]
struct AsterDEXUserTradeResponseWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "id")]
    id: f64,
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "qty")]
    qty: String,
    #[serde(rename = "quoteQty")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    quote_qty: Option<String>,
    #[serde(rename = "realizedPnl")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    realized_pnl: Option<String>,
    #[serde(rename = "side")]
    side: String,
    #[serde(rename = "positionSide")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    position_side: Option<String>,
    #[serde(rename = "buyer")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    buyer: Option<bool>,
    #[serde(rename = "maker")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    maker: Option<bool>,
    #[serde(rename = "commission")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    commission: Option<String>,
    #[serde(rename = "commissionAsset")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    commission_asset: Option<String>,
    #[serde(rename = "time")]
    time: f64,
}

impl<'de> serde::Deserialize<'de> for AsterDEXUserTradeResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXUserTradeResponseWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    symbol: w.symbol,
                    id: w.id,
                    order_id: w.order_id,
                    price: w.price,
                    qty: w.qty,
                    quote_qty: w.quote_qty,
                    realized_pnl: w.realized_pnl,
                    side: w.side,
                    position_side: w.position_side,
                    buyer: w.buyer,
                    maker: w.maker,
                    commission: w.commission,
                    commission_asset: w.commission_asset,
                    time: w.time,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXUserTradeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                symbol,
                id,
                order_id,
                price,
                qty,
                quote_qty,
                realized_pnl,
                side,
                position_side,
                buyer,
                maker,
                commission,
                commission_asset,
                time
            }
        );
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXKlineResponse(
    pub f64,
    pub String,
    pub String,
    pub String,
    pub String,
    pub String,
    pub f64,
    pub String,
    pub f64,
    pub String,
    pub String,
    pub String,
);
#[derive(serde::Deserialize)]
struct AsterDEXKlineResponseWire(
    f64,
    String,
    String,
    String,
    String,
    String,
    f64,
    String,
    f64,
    String,
    String,
    String,
);

impl<'de> serde::Deserialize<'de> for AsterDEXKlineResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                let w = AsterDEXKlineResponseWire::deserialize(d)?;
                Ok(Self(
                    w.0, w.1, w.2, w.3, w.4, w.5, w.6, w.7, w.8, w.9, w.10, w.11,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXKlineResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            tuple(v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11)
        );
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXWebSocketMessage {
    #[serde(rename = "e")]
    pub e: String,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "s")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub s: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
#[derive(serde::Deserialize)]
struct AsterDEXWebSocketMessageWire {
    #[serde(rename = "e")]
    e: String,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "s")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    s: Option<String>,
    #[serde(flatten, deserialize_with = "opaque_fields")]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXWebSocketMessage {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXWebSocketMessageWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    s: w.s,
                    extra: w.extra,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXStreamTickerE {
    #[serde(rename = "ticker")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXStreamTickerEWire {
    #[serde(rename = "ticker")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamTickerE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXStreamTickerEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXStreamTickerEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXStreamTicker {
    #[serde(rename = "e")]
    pub e: AsterDEXStreamTickerE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "P")]
    pub capital_p: String,
    #[serde(rename = "w")]
    pub w: String,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "Q")]
    pub capital_q: String,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "h")]
    pub h: String,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "v")]
    pub v: String,
    #[serde(rename = "q")]
    pub q: String,
    #[serde(rename = "O")]
    pub capital_o: f64,
    #[serde(rename = "C")]
    pub capital_c: f64,
    #[serde(rename = "F")]
    pub capital_f: f64,
    #[serde(rename = "L")]
    pub capital_l: f64,
    #[serde(rename = "n")]
    pub n: f64,
}
#[derive(serde::Deserialize)]
struct AsterDEXStreamTickerWire {
    #[serde(rename = "e")]
    e: AsterDEXStreamTickerE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "P")]
    capital_p: String,
    #[serde(rename = "w")]
    w: String,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "Q")]
    capital_q: String,
    #[serde(rename = "o")]
    o: String,
    #[serde(rename = "h")]
    h: String,
    #[serde(rename = "l")]
    l: String,
    #[serde(rename = "v")]
    v: String,
    #[serde(rename = "q")]
    q: String,
    #[serde(rename = "O")]
    capital_o: f64,
    #[serde(rename = "C")]
    capital_c: f64,
    #[serde(rename = "F")]
    capital_f: f64,
    #[serde(rename = "L")]
    capital_l: f64,
    #[serde(rename = "n")]
    n: f64,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamTicker {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXStreamTickerWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    s: w.s,
                    p: w.p,
                    capital_p: w.capital_p,
                    w: w.w,
                    c: w.c,
                    capital_q: w.capital_q,
                    o: w.o,
                    h: w.h,
                    l: w.l,
                    v: w.v,
                    q: w.q,
                    capital_o: w.capital_o,
                    capital_c: w.capital_c,
                    capital_f: w.capital_f,
                    capital_l: w.capital_l,
                    n: w.n,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXStreamDepthE {
    #[serde(rename = "depthUpdate")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXStreamDepthEWire {
    #[serde(rename = "depthUpdate")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamDepthE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXStreamDepthEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXStreamDepthEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXStreamDepth {
    #[serde(rename = "e")]
    pub e: AsterDEXStreamDepthE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "U")]
    pub capital_u: f64,
    #[serde(rename = "u")]
    pub u: f64,
    #[serde(rename = "pu")]
    pub pu: f64,
    #[serde(rename = "b")]
    pub b: Vec<(String, String)>,
    #[serde(rename = "a")]
    pub a: Vec<(String, String)>,
}
#[derive(serde::Deserialize)]
struct AsterDEXStreamDepthWire {
    #[serde(rename = "e")]
    e: AsterDEXStreamDepthE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "U")]
    capital_u: f64,
    #[serde(rename = "u")]
    u: f64,
    #[serde(rename = "pu")]
    pu: f64,
    #[serde(rename = "b")]
    b: Vec<(String, String)>,
    #[serde(rename = "a")]
    a: Vec<(String, String)>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamDepth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXStreamDepthWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    capital_t: w.capital_t,
                    s: w.s,
                    capital_u: w.capital_u,
                    u: w.u,
                    pu: w.pu,
                    b: w.b,
                    a: w.a,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXStreamTradeE {
    #[serde(rename = "aggTrade")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXStreamTradeEWire {
    #[serde(rename = "aggTrade")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamTradeE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXStreamTradeEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXStreamTradeEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXStreamTrade {
    #[serde(rename = "e")]
    pub e: AsterDEXStreamTradeE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "a")]
    pub a: f64,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "q")]
    pub q: String,
    #[serde(rename = "f")]
    pub f: f64,
    #[serde(rename = "l")]
    pub l: f64,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "m")]
    pub m: bool,
}
#[derive(serde::Deserialize)]
struct AsterDEXStreamTradeWire {
    #[serde(rename = "e")]
    e: AsterDEXStreamTradeE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "a")]
    a: f64,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "q")]
    q: String,
    #[serde(rename = "f")]
    f: f64,
    #[serde(rename = "l")]
    l: f64,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "m")]
    m: bool,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamTrade {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXStreamTradeWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    s: w.s,
                    a: w.a,
                    p: w.p,
                    q: w.q,
                    f: w.f,
                    l: w.l,
                    capital_t: w.capital_t,
                    m: w.m,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXStreamMarkPriceE {
    #[serde(rename = "markPriceUpdate")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXStreamMarkPriceEWire {
    #[serde(rename = "markPriceUpdate")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamMarkPriceE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXStreamMarkPriceEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXStreamMarkPriceEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXStreamMarkPrice {
    #[serde(rename = "e")]
    pub e: AsterDEXStreamMarkPriceE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "P")]
    pub capital_p: String,
    #[serde(rename = "r")]
    pub r: String,
    #[serde(rename = "T")]
    pub capital_t: f64,
}
#[derive(serde::Deserialize)]
struct AsterDEXStreamMarkPriceWire {
    #[serde(rename = "e")]
    e: AsterDEXStreamMarkPriceE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "P")]
    capital_p: String,
    #[serde(rename = "r")]
    r: String,
    #[serde(rename = "T")]
    capital_t: f64,
}

impl<'de> serde::Deserialize<'de> for AsterDEXStreamMarkPrice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXStreamMarkPriceWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    s: w.s,
                    p: w.p,
                    i: w.i,
                    capital_p: w.capital_p,
                    r: w.r,
                    capital_t: w.capital_t,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXAccountUpdateE {
    #[serde(rename = "ACCOUNT_UPDATE")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXAccountUpdateEWire {
    #[serde(rename = "ACCOUNT_UPDATE")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXAccountUpdateEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXAccountUpdateEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountUpdateABItem {
    #[serde(rename = "a")]
    pub a: String,
    #[serde(rename = "wb")]
    pub wb: String,
    #[serde(rename = "cw")]
    pub cw: String,
    #[serde(rename = "bc")]
    pub bc: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountUpdateABItemWire {
    #[serde(rename = "a")]
    a: String,
    #[serde(rename = "wb")]
    wb: String,
    #[serde(rename = "cw")]
    cw: String,
    #[serde(rename = "bc")]
    bc: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountUpdateABItem {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountUpdateABItemWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    a: w.a,
                    wb: w.wb,
                    cw: w.cw,
                    bc: w.bc,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountUpdateAPItem {
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "pa")]
    pub pa: String,
    #[serde(rename = "ep")]
    pub ep: String,
    #[serde(rename = "cr")]
    pub cr: String,
    #[serde(rename = "up")]
    pub up: String,
    #[serde(rename = "mt")]
    pub mt: String,
    #[serde(rename = "iw")]
    pub iw: String,
    #[serde(rename = "ps")]
    pub ps: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountUpdateAPItemWire {
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "pa")]
    pa: String,
    #[serde(rename = "ep")]
    ep: String,
    #[serde(rename = "cr")]
    cr: String,
    #[serde(rename = "up")]
    up: String,
    #[serde(rename = "mt")]
    mt: String,
    #[serde(rename = "iw")]
    iw: String,
    #[serde(rename = "ps")]
    ps: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountUpdateAPItem {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountUpdateAPItemWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    s: w.s,
                    pa: w.pa,
                    ep: w.ep,
                    cr: w.cr,
                    up: w.up,
                    mt: w.mt,
                    iw: w.iw,
                    ps: w.ps,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountUpdateA {
    #[serde(rename = "m")]
    pub m: String,
    #[serde(rename = "B")]
    pub capital_b: Vec<AsterDEXAccountUpdateABItem>,
    #[serde(rename = "P")]
    pub capital_p: Vec<AsterDEXAccountUpdateAPItem>,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountUpdateAWire {
    #[serde(rename = "m")]
    m: String,
    #[serde(rename = "B")]
    capital_b: Vec<AsterDEXAccountUpdateABItem>,
    #[serde(rename = "P")]
    capital_p: Vec<AsterDEXAccountUpdateAPItem>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountUpdateA {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountUpdateAWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    m: w.m,
                    capital_b: w.capital_b,
                    capital_p: w.capital_p,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXAccountUpdate {
    #[serde(rename = "e")]
    pub e: AsterDEXAccountUpdateE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "a")]
    pub a: AsterDEXAccountUpdateA,
}
#[derive(serde::Deserialize)]
struct AsterDEXAccountUpdateWire {
    #[serde(rename = "e")]
    e: AsterDEXAccountUpdateE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "a")]
    a: AsterDEXAccountUpdateA,
}

impl<'de> serde::Deserialize<'de> for AsterDEXAccountUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXAccountUpdateWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    capital_t: w.capital_t,
                    a: w.a,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterDEXOrderUpdateE {
    #[serde(rename = "ORDER_TRADE_UPDATE")]
    V0,
}
#[derive(serde::Deserialize)]
enum AsterDEXOrderUpdateEWire {
    #[serde(rename = "ORDER_TRADE_UPDATE")]
    V0,
}

impl<'de> serde::Deserialize<'de> for AsterDEXOrderUpdateE {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterDEXOrderUpdateEWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterDEXOrderUpdateEWire::V0 => Self::V0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXOrderUpdateO {
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "S")]
    pub capital_s: String,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "f")]
    pub f: String,
    #[serde(rename = "q")]
    pub q: String,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "ap")]
    pub ap: String,
    #[serde(rename = "sp")]
    pub sp: String,
    #[serde(rename = "x")]
    pub x: String,
    #[serde(rename = "X")]
    pub capital_x: String,
    #[serde(rename = "i")]
    pub i: f64,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "z")]
    pub z: String,
    #[serde(rename = "L")]
    pub capital_l: String,
    #[serde(rename = "N")]
    pub capital_n: String,
    #[serde(rename = "n")]
    pub n: String,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "t")]
    pub t: f64,
    #[serde(rename = "b")]
    pub b: String,
    #[serde(rename = "a")]
    pub a: String,
    #[serde(rename = "m")]
    pub m: bool,
    #[serde(rename = "R")]
    pub capital_r: bool,
    #[serde(rename = "wt")]
    pub wt: String,
    #[serde(rename = "ot")]
    pub ot: String,
    #[serde(rename = "ps")]
    pub ps: String,
    #[serde(rename = "cp")]
    pub cp: bool,
    #[serde(rename = "AP")]
    pub capital_ap: String,
    #[serde(rename = "cr")]
    pub cr: String,
    #[serde(rename = "pP")]
    pub p_p: bool,
    #[serde(rename = "si")]
    pub si: f64,
    #[serde(rename = "ss")]
    pub ss: f64,
    #[serde(rename = "rp")]
    pub rp: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXOrderUpdateOWire {
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "S")]
    capital_s: String,
    #[serde(rename = "o")]
    o: String,
    #[serde(rename = "f")]
    f: String,
    #[serde(rename = "q")]
    q: String,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "ap")]
    ap: String,
    #[serde(rename = "sp")]
    sp: String,
    #[serde(rename = "x")]
    x: String,
    #[serde(rename = "X")]
    capital_x: String,
    #[serde(rename = "i")]
    i: f64,
    #[serde(rename = "l")]
    l: String,
    #[serde(rename = "z")]
    z: String,
    #[serde(rename = "L")]
    capital_l: String,
    #[serde(rename = "N")]
    capital_n: String,
    #[serde(rename = "n")]
    n: String,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "t")]
    t: f64,
    #[serde(rename = "b")]
    b: String,
    #[serde(rename = "a")]
    a: String,
    #[serde(rename = "m")]
    m: bool,
    #[serde(rename = "R")]
    capital_r: bool,
    #[serde(rename = "wt")]
    wt: String,
    #[serde(rename = "ot")]
    ot: String,
    #[serde(rename = "ps")]
    ps: String,
    #[serde(rename = "cp")]
    cp: bool,
    #[serde(rename = "AP")]
    capital_ap: String,
    #[serde(rename = "cr")]
    cr: String,
    #[serde(rename = "pP")]
    p_p: bool,
    #[serde(rename = "si")]
    si: f64,
    #[serde(rename = "ss")]
    ss: f64,
    #[serde(rename = "rp")]
    rp: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXOrderUpdateO {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXOrderUpdateOWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    s: w.s,
                    c: w.c,
                    capital_s: w.capital_s,
                    o: w.o,
                    f: w.f,
                    q: w.q,
                    p: w.p,
                    ap: w.ap,
                    sp: w.sp,
                    x: w.x,
                    capital_x: w.capital_x,
                    i: w.i,
                    l: w.l,
                    z: w.z,
                    capital_l: w.capital_l,
                    capital_n: w.capital_n,
                    n: w.n,
                    capital_t: w.capital_t,
                    t: w.t,
                    b: w.b,
                    a: w.a,
                    m: w.m,
                    capital_r: w.capital_r,
                    wt: w.wt,
                    ot: w.ot,
                    ps: w.ps,
                    cp: w.cp,
                    capital_ap: w.capital_ap,
                    cr: w.cr,
                    p_p: w.p_p,
                    si: w.si,
                    ss: w.ss,
                    rp: w.rp,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXOrderUpdate {
    #[serde(rename = "e")]
    pub e: AsterDEXOrderUpdateE,
    #[serde(rename = "E")]
    pub capital_e: f64,
    #[serde(rename = "T")]
    pub capital_t: f64,
    #[serde(rename = "o")]
    pub o: AsterDEXOrderUpdateO,
}
#[derive(serde::Deserialize)]
struct AsterDEXOrderUpdateWire {
    #[serde(rename = "e")]
    e: AsterDEXOrderUpdateE,
    #[serde(rename = "E")]
    capital_e: f64,
    #[serde(rename = "T")]
    capital_t: f64,
    #[serde(rename = "o")]
    o: AsterDEXOrderUpdateO,
}

impl<'de> serde::Deserialize<'de> for AsterDEXOrderUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXOrderUpdateWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    e: w.e,
                    capital_e: w.capital_e,
                    capital_t: w.capital_t,
                    o: w.o,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXListenKeyResponse {
    #[serde(rename = "listenKey")]
    pub listen_key: String,
}
#[derive(serde::Deserialize)]
struct AsterDEXListenKeyResponseWire {
    #[serde(rename = "listenKey")]
    listen_key: String,
}

impl<'de> serde::Deserialize<'de> for AsterDEXListenKeyResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterDEXListenKeyResponseWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            listen_key: w.listen_key,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterDEXPlaceOrderOpts {
    #[serde(flatten)]
    pub order: safe_cex_types::PlaceOrderOpts,
    #[serde(
        rename = "stpMode",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub stp_mode: Option<AsterDEXStpMode>,
}
impl std::borrow::Borrow<safe_cex_types::PlaceOrderOpts> for AsterDEXPlaceOrderOpts {
    fn borrow(&self) -> &safe_cex_types::PlaceOrderOpts {
        &self.order
    }
}
impl From<safe_cex_types::PlaceOrderOpts> for AsterDEXPlaceOrderOpts {
    fn from(order: safe_cex_types::PlaceOrderOpts) -> Self {
        Self {
            order,
            stp_mode: None,
        }
    }
}

#[derive(serde::Serialize)]
struct AsterDEXPlaceOrderNumbers {
    amount: f64,
    price: Option<f64>,
    trigger_price: Option<f64>,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    max_slippage_pct: Option<f64>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXPlaceOrderNumbers {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(
            self,
            struct {
                amount,
                price,
                trigger_price,
                stop_loss,
                take_profit,
                max_slippage_pct
            }
        );
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXPlaceOrderOpts {
    fn check_number_output(&self) {
        let Self { order, stp_mode: _ } = self;
        let safe_cex_types::PlaceOrderOpts {
            symbol: _,
            r#type: _,
            side: _,
            amount,
            price,
            trigger_price,
            stop_loss,
            take_profit,
            reduce_only: _,
            time_in_force: _,
            trigger_type: _,
            tp_trigger_type: _,
            sl_trigger_type: _,
            client_order_id: _,
            max_slippage_pct,
            close: _,
            implied_volatility: _,
        } = order;
        AsterDEXPlaceOrderNumbers {
            amount: *amount,
            price: *price,
            trigger_price: *trigger_price,
            stop_loss: *stop_loss,
            take_profit: *take_profit,
            max_slippage_pct: *max_slippage_pct,
        }
        .check_number_output();
    }
}

#[derive(serde::Deserialize)]
struct AsterDEXPlaceOrderOptsWire {
    #[serde(flatten)]
    order: safe_cex_types::PlaceOrderOpts,
    #[serde(rename = "stpMode", default, deserialize_with = "present")]
    stp_mode: Option<AsterDEXStpMode>,
}

impl<'de> serde::Deserialize<'de> for AsterDEXPlaceOrderOpts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|d| {
                let w = <AsterDEXPlaceOrderOptsWire as serde::Deserialize>::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )?;
                Ok(Self {
                    order: w.order,
                    stp_mode: w.stp_mode,
                })
            })
            .map_err(serde::de::Error::custom)
    }
}

fn opaque_fields<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<serde_json::Map<String, serde_json::Value>, D::Error> {
    let value = safe_cex_num::numeric_ingress::js_deserialize_passthrough(d)?;
    match value {
        serde_json::Value::Object(fields) => Ok(fields),
        _ => Err(serde::de::Error::custom(
            "extension fields must be an object",
        )),
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXV3AuthContext {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{user_address, signer_address, signer_private_key});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXMarketResponseFiltersItem {
    fn check_number_output(&self) {
        let Self {
            extra,
            filter_type,
            max_price,
            min_price,
            tick_size,
            max_qty,
            min_qty,
            step_size,
            limit,
            notional,
            multiplier_up,
            multiplier_down,
            multiplier_decimal,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(filter_type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(max_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(min_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(tick_size);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(max_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(min_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(step_size);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(limit);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(notional);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(multiplier_up);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(multiplier_down);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(multiplier_decimal);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXMarketResponse {
    fn check_number_output(&self) {
        let Self {
            extra,
            symbol,
            base_asset,
            quote_asset,
            contract_type,
            delivery_date,
            onboard_date,
            status,
            maint_margin_percent,
            required_margin_percent,
            price_precision,
            quantity_precision,
            base_asset_precision,
            quote_precision,
            underlying_type,
            underlying_sub_type,
            settle_plan,
            trigger_protect,
            filters,
            order_types,
            time_in_force,
            liquidation_fee,
            market_take_bound,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(symbol);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(base_asset);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(quote_asset);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(contract_type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(delivery_date);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(onboard_date);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(status);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(maint_margin_percent);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(required_margin_percent);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price_precision);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(quantity_precision);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(base_asset_precision);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(quote_precision);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(underlying_type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(underlying_sub_type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(settle_plan);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(trigger_protect);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(filters);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(order_types);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(time_in_force);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(liquidation_fee);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(market_take_bound);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXPositionModeResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{dual_side_position});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXPositionResponse {
    fn check_number_output(&self) {
        let Self {
            symbol,
            position_amt,
            entry_price,
            break_even_price,
            mark_price,
            un_realized_profit,
            liquidation_price,
            leverage,
            max_notional_value,
            margin_type,
            isolated_margin,
            is_auto_add_margin,
            position_side,
            notional,
            isolated_wallet,
            update_time,
            extra,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(symbol);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(position_amt);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(entry_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(break_even_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(mark_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(un_realized_profit);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(liquidation_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(leverage);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(max_notional_value);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(margin_type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(isolated_margin);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(is_auto_add_margin);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(position_side);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(notional);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(isolated_wallet);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(update_time);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXOrderResponse {
    fn check_number_output(&self) {
        let Self {
            symbol,
            order_id,
            client_order_id,
            price,
            orig_qty,
            executed_qty,
            cummulative_quote_qty,
            status,
            time_in_force,
            r#type,
            side,
            stop_price,
            iceberg_qty,
            time,
            update_time,
            is_working,
            working_time,
            orig_quote_order_qty,
            position_side,
            close_position,
            reduce_only,
            activate_price,
            price_rate,
            price_protect,
            extra,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(symbol);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(order_id);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(client_order_id);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(orig_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(executed_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(cummulative_quote_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(status);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(time_in_force);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(r#type);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(side);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(stop_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(iceberg_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(time);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(update_time);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(is_working);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(working_time);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(orig_quote_order_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(position_side);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(close_position);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(reduce_only);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(activate_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price_rate);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price_protect);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXTickerResponse {
    fn check_number_output(&self) {
        let Self {
            symbol,
            price_change,
            price_change_percent,
            weighted_avg_price,
            last_price,
            last_qty,
            open_price,
            high_price,
            low_price,
            volume,
            quote_volume,
            open_time,
            close_time,
            first_id,
            last_id,
            count,
            extra,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(symbol);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price_change);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(price_change_percent);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(weighted_avg_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(last_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(last_qty);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(open_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(high_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(low_price);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(volume);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(quote_volume);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(open_time);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(close_time);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(first_id);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(last_id);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(count);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXDepthResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{last_update_id, capital_e, capital_t, bids, asks});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXWebSocketMessage {
    fn check_number_output(&self) {
        let Self {
            e,
            capital_e,
            s,
            extra,
        } = self;
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(e);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(capital_e);
        safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(s);
        safe_cex_num::numeric_ingress::JsNumberPassthrough::check_number_passthrough(
            &serde_json::Value::Object(extra.clone()),
        );
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamTicker {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, s, p, capital_p, w, c, capital_q, o, h, l, v, q, capital_o, capital_c, capital_f, capital_l, n});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamDepth {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, capital_t, s, capital_u, u, pu, b, a});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamTrade {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, s, a, p, q, f, l, capital_t, m});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamMarkPrice {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, s, p, i, capital_p, r, capital_t});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountUpdateABItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{a, wb, cw, bc});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountUpdateAPItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{s, pa, ep, cr, up, mt, iw, ps});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountUpdateA {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{m, capital_b, capital_p});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, capital_t, a});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXOrderUpdateO {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{s, c, capital_s, o, f, q, p, ap, sp, x, capital_x, i, l, z, capital_l, capital_n, n, capital_t, t, b, a, m, capital_r, wt, ot, ps, cp, capital_ap, cr, p_p, si, ss, rp});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXOrderUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{e, capital_e, capital_t, o});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXListenKeyResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct{listen_key});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAPIVersion {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V1, V3});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXFapiVersion {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V1, V3, V4});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStpMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0, V1, V2});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for AsterDEXPositionModeResponseDualSidePositionString
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0, V1});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput
    for AsterDEXPositionModeResponseDualSidePosition
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{Boolean(value), String(value)});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamTickerE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamDepthE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamTradeE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXStreamMarkPriceE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXAccountUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for AsterDEXOrderUpdateE {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum{V0});
    }
}
