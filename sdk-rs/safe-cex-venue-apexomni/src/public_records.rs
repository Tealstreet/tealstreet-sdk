//! Public wire records. Capture numeric input before requiring map-shaped fields.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

// ts: TickerObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TickerObject {
    #[serde(rename = "fundingRate")]
    pub funding_rate: String,
    #[serde(rename = "highPrice24h")]
    pub high_price24h: String,
    #[serde(rename = "indexPrice")]
    pub index_price: String,
    #[serde(rename = "lastPrice")]
    pub last_price: String,
    #[serde(rename = "lowPrice24h")]
    pub low_price24h: String,
    #[serde(rename = "nextFundingTime")]
    pub next_funding_time: String,
    #[serde(rename = "openInterest")]
    pub open_interest: String,
    #[serde(rename = "oraclePrice")]
    pub oracle_price: String,
    #[serde(rename = "predictedFundingRate")]
    pub predicted_funding_rate: String,
    #[serde(rename = "price24hPcnt")]
    pub price24h_pcnt: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "tradeCount")]
    pub trade_count: String,
    #[serde(rename = "turnover24h")]
    pub turnover24h: String,
    #[serde(rename = "volume24h")]
    pub volume24h: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "TickerObject")]
struct TickerObjectWire {
    #[serde(rename = "fundingRate")]
    funding_rate: String,
    #[serde(rename = "highPrice24h")]
    high_price24h: String,
    #[serde(rename = "indexPrice")]
    index_price: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "lowPrice24h")]
    low_price24h: String,
    #[serde(rename = "nextFundingTime")]
    next_funding_time: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    #[serde(rename = "oraclePrice")]
    oracle_price: String,
    #[serde(rename = "predictedFundingRate")]
    predicted_funding_rate: String,
    #[serde(rename = "price24hPcnt")]
    price24h_pcnt: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "tradeCount")]
    trade_count: String,
    #[serde(rename = "turnover24h")]
    turnover24h: String,
    #[serde(rename = "volume24h")]
    volume24h: String,
}
impl<'de> serde::Deserialize<'de> for TickerObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                TickerObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: DepthObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DepthObject {
    #[serde(rename = "a")]
    pub a: Vec<Vec<String>>,
    #[serde(rename = "b")]
    pub b: Vec<Vec<String>>,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "u")]
    pub u: f64,
}
#[derive(serde::Deserialize)]
#[serde(remote = "DepthObject")]
struct DepthObjectWire {
    #[serde(rename = "a")]
    a: Vec<Vec<String>>,
    #[serde(rename = "b")]
    b: Vec<Vec<String>>,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "u")]
    u: f64,
}
impl<'de> serde::Deserialize<'de> for DepthObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                DepthObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: TradeObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TradeObject {
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "p")]
    pub p: String,
    #[serde(rename = "S")]
    pub side: String,
    #[serde(rename = "v")]
    pub v: String,
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "t")]
    pub t: f64,
}
#[derive(serde::Deserialize)]
#[serde(remote = "TradeObject")]
struct TradeObjectWire {
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "S")]
    side: String,
    #[serde(rename = "v")]
    v: String,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "t")]
    t: f64,
}
impl<'de> serde::Deserialize<'de> for TradeObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                TradeObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: KlineObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct KlineObject {
    #[serde(rename = "s")]
    pub s: String,
    #[serde(rename = "i")]
    pub i: String,
    #[serde(rename = "t")]
    pub t: f64,
    #[serde(rename = "c")]
    pub c: String,
    #[serde(rename = "h")]
    pub h: String,
    #[serde(rename = "l")]
    pub l: String,
    #[serde(rename = "o")]
    pub o: String,
    #[serde(rename = "v")]
    pub v: String,
    #[serde(rename = "tr")]
    pub tr: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "KlineObject")]
struct KlineObjectWire {
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "t")]
    t: f64,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "h")]
    h: String,
    #[serde(rename = "l")]
    l: String,
    #[serde(rename = "o")]
    o: String,
    #[serde(rename = "v")]
    v: String,
    #[serde(rename = "tr")]
    tr: String,
}
impl<'de> serde::Deserialize<'de> for KlineObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                KlineObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: HistoryFundingObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct HistoryFundingObject {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "rate")]
    pub rate: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "fundingTime")]
    pub funding_time: f64,
    #[serde(rename = "fundingTimestamp")]
    pub funding_timestamp: f64,
}
#[derive(serde::Deserialize)]
#[serde(remote = "HistoryFundingObject")]
struct HistoryFundingObjectWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "rate")]
    rate: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "fundingTime")]
    funding_time: f64,
    #[serde(rename = "fundingTimestamp")]
    funding_timestamp: f64,
}
impl<'de> serde::Deserialize<'de> for HistoryFundingObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                HistoryFundingObjectWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
// ts: CurrencyObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CurrencyObject {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "starkExAssetId")]
    pub stark_ex_asset_id: String,
    #[serde(rename = "starkExResolution")]
    pub stark_ex_resolution: String,
    #[serde(rename = "stepSize")]
    pub step_size: String,
    #[serde(rename = "showStep")]
    pub show_step: String,
    #[serde(rename = "iconUrl")]
    pub icon_url: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "CurrencyObject")]
struct CurrencyObjectWire {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "starkExAssetId")]
    stark_ex_asset_id: String,
    #[serde(rename = "starkExResolution")]
    stark_ex_resolution: String,
    #[serde(rename = "stepSize")]
    step_size: String,
    #[serde(rename = "showStep")]
    show_step: String,
    #[serde(rename = "iconUrl")]
    icon_url: String,
}
impl<'de> serde::Deserialize<'de> for CurrencyObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                CurrencyObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: GlobalObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GlobalObject {
    #[serde(rename = "feeAccountId")]
    pub fee_account_id: String,
    #[serde(rename = "feeAccountL2Key")]
    pub fee_account_l2_key: String,
    #[serde(rename = "starkExCollateralCurrencyId")]
    pub stark_ex_collateral_currency_id: String,
    #[serde(rename = "starkExFundingValidityPeriod")]
    pub stark_ex_funding_validity_period: f64,
    #[serde(rename = "starkExMaxFundingRate")]
    pub stark_ex_max_funding_rate: String,
    #[serde(rename = "starkExOrdersTreeHeight")]
    pub stark_ex_orders_tree_height: f64,
    #[serde(rename = "starkExPositionsTreeHeight")]
    pub stark_ex_positions_tree_height: f64,
    #[serde(rename = "starkExPriceValidityPeriod")]
    pub stark_ex_price_validity_period: f64,
    #[serde(rename = "starkExContractAddress")]
    pub stark_ex_contract_address: String,
    #[serde(rename = "registerEnvId")]
    pub register_env_id: f64,
    #[serde(rename = "crossChainAccountId")]
    pub cross_chain_account_id: String,
    #[serde(rename = "crossChainL2Key")]
    pub cross_chain_l2_key: String,
    #[serde(rename = "fastWithdrawAccountId")]
    pub fast_withdraw_account_id: String,
    #[serde(rename = "fastWithdrawFactRegisterAddress")]
    pub fast_withdraw_fact_register_address: String,
    #[serde(rename = "fastWithdrawL2Key")]
    pub fast_withdraw_l2_key: String,
    #[serde(rename = "fastWithdrawMaxAmount")]
    pub fast_withdraw_max_amount: String,
    #[serde(rename = "bybitWithdrawAccountId")]
    pub bybit_withdraw_account_id: String,
    #[serde(rename = "bybitWithdrawL2Key")]
    pub bybit_withdraw_l2_key: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "GlobalObject")]
struct GlobalObjectWire {
    #[serde(rename = "feeAccountId")]
    fee_account_id: String,
    #[serde(rename = "feeAccountL2Key")]
    fee_account_l2_key: String,
    #[serde(rename = "starkExCollateralCurrencyId")]
    stark_ex_collateral_currency_id: String,
    #[serde(rename = "starkExFundingValidityPeriod")]
    stark_ex_funding_validity_period: f64,
    #[serde(rename = "starkExMaxFundingRate")]
    stark_ex_max_funding_rate: String,
    #[serde(rename = "starkExOrdersTreeHeight")]
    stark_ex_orders_tree_height: f64,
    #[serde(rename = "starkExPositionsTreeHeight")]
    stark_ex_positions_tree_height: f64,
    #[serde(rename = "starkExPriceValidityPeriod")]
    stark_ex_price_validity_period: f64,
    #[serde(rename = "starkExContractAddress")]
    stark_ex_contract_address: String,
    #[serde(rename = "registerEnvId")]
    register_env_id: f64,
    #[serde(rename = "crossChainAccountId")]
    cross_chain_account_id: String,
    #[serde(rename = "crossChainL2Key")]
    cross_chain_l2_key: String,
    #[serde(rename = "fastWithdrawAccountId")]
    fast_withdraw_account_id: String,
    #[serde(rename = "fastWithdrawFactRegisterAddress")]
    fast_withdraw_fact_register_address: String,
    #[serde(rename = "fastWithdrawL2Key")]
    fast_withdraw_l2_key: String,
    #[serde(rename = "fastWithdrawMaxAmount")]
    fast_withdraw_max_amount: String,
    #[serde(rename = "bybitWithdrawAccountId")]
    bybit_withdraw_account_id: String,
    #[serde(rename = "bybitWithdrawL2Key")]
    bybit_withdraw_l2_key: String,
}
impl<'de> serde::Deserialize<'de> for GlobalObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                GlobalObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: PerpetualContractObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PerpetualContractObject {
    #[serde(rename = "baselinePositionValue")]
    pub baseline_position_value: String,
    #[serde(rename = "crossId")]
    pub cross_id: f64,
    #[serde(rename = "crossSymbolId")]
    pub cross_symbol_id: f64,
    #[serde(rename = "crossSymbolName")]
    pub cross_symbol_name: String,
    #[serde(rename = "digitMerge")]
    pub digit_merge: String,
    #[serde(rename = "displayMaxLeverage")]
    pub display_max_leverage: String,
    #[serde(rename = "displayMinLeverage")]
    pub display_min_leverage: String,
    #[serde(rename = "enableDisplay")]
    pub enable_display: bool,
    #[serde(rename = "enableOpenPosition")]
    pub enable_open_position: bool,
    #[serde(rename = "enableTrade")]
    pub enable_trade: bool,
    #[serde(rename = "fundingImpactMarginNotional")]
    pub funding_impact_margin_notional: String,
    #[serde(rename = "fundingInterestRate")]
    pub funding_interest_rate: String,
    #[serde(rename = "incrementalInitialMarginRate")]
    pub incremental_initial_margin_rate: String,
    #[serde(rename = "incrementalMaintenanceMarginRate")]
    pub incremental_maintenance_margin_rate: String,
    #[serde(rename = "incrementalPositionValue")]
    pub incremental_position_value: String,
    #[serde(rename = "initialMarginRate")]
    pub initial_margin_rate: String,
    #[serde(rename = "maintenanceMarginRate")]
    pub maintenance_margin_rate: String,
    #[serde(rename = "maxOrderSize")]
    pub max_order_size: String,
    #[serde(rename = "maxPositionSize")]
    pub max_position_size: String,
    #[serde(rename = "minOrderSize")]
    pub min_order_size: String,
    #[serde(rename = "maxMarketPriceRange")]
    pub max_market_price_range: String,
    #[serde(rename = "settleCurrencyId")]
    pub settle_currency_id: String,
    #[serde(rename = "starkExOraclePriceQuorum")]
    pub stark_ex_oracle_price_quorum: String,
    #[serde(rename = "starkExResolution")]
    pub stark_ex_resolution: String,
    #[serde(rename = "starkExRiskFactor")]
    pub stark_ex_risk_factor: String,
    #[serde(rename = "starkExSyntheticAssetId")]
    pub stark_ex_synthetic_asset_id: String,
    #[serde(rename = "stepSize")]
    pub step_size: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "symbolDisplayName")]
    pub symbol_display_name: String,
    #[serde(rename = "symbolDisplayName2")]
    pub symbol_display_name2: String,
    #[serde(rename = "tickSize")]
    pub tick_size: String,
    #[serde(rename = "underlyingCurrencyId")]
    pub underlying_currency_id: String,
    #[serde(rename = "maxMaintenanceMarginRate")]
    pub max_maintenance_margin_rate: String,
    #[serde(rename = "maxPositionValue")]
    pub max_position_value: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "PerpetualContractObject")]
struct PerpetualContractObjectWire {
    #[serde(rename = "baselinePositionValue")]
    baseline_position_value: String,
    #[serde(rename = "crossId")]
    cross_id: f64,
    #[serde(rename = "crossSymbolId")]
    cross_symbol_id: f64,
    #[serde(rename = "crossSymbolName")]
    cross_symbol_name: String,
    #[serde(rename = "digitMerge")]
    digit_merge: String,
    #[serde(rename = "displayMaxLeverage")]
    display_max_leverage: String,
    #[serde(rename = "displayMinLeverage")]
    display_min_leverage: String,
    #[serde(rename = "enableDisplay")]
    enable_display: bool,
    #[serde(rename = "enableOpenPosition")]
    enable_open_position: bool,
    #[serde(rename = "enableTrade")]
    enable_trade: bool,
    #[serde(rename = "fundingImpactMarginNotional")]
    funding_impact_margin_notional: String,
    #[serde(rename = "fundingInterestRate")]
    funding_interest_rate: String,
    #[serde(rename = "incrementalInitialMarginRate")]
    incremental_initial_margin_rate: String,
    #[serde(rename = "incrementalMaintenanceMarginRate")]
    incremental_maintenance_margin_rate: String,
    #[serde(rename = "incrementalPositionValue")]
    incremental_position_value: String,
    #[serde(rename = "initialMarginRate")]
    initial_margin_rate: String,
    #[serde(rename = "maintenanceMarginRate")]
    maintenance_margin_rate: String,
    #[serde(rename = "maxOrderSize")]
    max_order_size: String,
    #[serde(rename = "maxPositionSize")]
    max_position_size: String,
    #[serde(rename = "minOrderSize")]
    min_order_size: String,
    #[serde(rename = "maxMarketPriceRange")]
    max_market_price_range: String,
    #[serde(rename = "settleCurrencyId")]
    settle_currency_id: String,
    #[serde(rename = "starkExOraclePriceQuorum")]
    stark_ex_oracle_price_quorum: String,
    #[serde(rename = "starkExResolution")]
    stark_ex_resolution: String,
    #[serde(rename = "starkExRiskFactor")]
    stark_ex_risk_factor: String,
    #[serde(rename = "starkExSyntheticAssetId")]
    stark_ex_synthetic_asset_id: String,
    #[serde(rename = "stepSize")]
    step_size: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "symbolDisplayName")]
    symbol_display_name: String,
    #[serde(rename = "symbolDisplayName2")]
    symbol_display_name2: String,
    #[serde(rename = "tickSize")]
    tick_size: String,
    #[serde(rename = "underlyingCurrencyId")]
    underlying_currency_id: String,
    #[serde(rename = "maxMaintenanceMarginRate")]
    max_maintenance_margin_rate: String,
    #[serde(rename = "maxPositionValue")]
    max_position_value: String,
}
impl<'de> serde::Deserialize<'de> for PerpetualContractObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PerpetualContractObjectWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: GetHistoryFundingOptions
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GetHistoryFundingOptions {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "page")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<f64>,
    #[serde(rename = "beginTimeInclusive")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub begin_time_inclusive: Option<f64>,
    #[serde(rename = "endTimeExclusive")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time_exclusive: Option<f64>,
    #[serde(rename = "limit")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}
#[derive(serde::Deserialize)]
#[serde(remote = "GetHistoryFundingOptions")]
struct GetHistoryFundingOptionsWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "page")]
    #[serde(default, deserialize_with = "deserialize_present")]
    page: Option<f64>,
    #[serde(rename = "beginTimeInclusive")]
    #[serde(default, deserialize_with = "deserialize_present")]
    begin_time_inclusive: Option<f64>,
    #[serde(rename = "endTimeExclusive")]
    #[serde(default, deserialize_with = "deserialize_present")]
    end_time_exclusive: Option<f64>,
    #[serde(rename = "limit")]
    #[serde(default, deserialize_with = "deserialize_present")]
    limit: Option<f64>,
}
impl<'de> serde::Deserialize<'de> for GetHistoryFundingOptions {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                GetHistoryFundingOptionsWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: KeyPair
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct KeyPair {
    #[serde(rename = "publicKey")]
    pub public_key: String,
    #[serde(rename = "publicKeyYCoordinate")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key_y_coordinate: Option<String>,
    #[serde(rename = "privateKey")]
    pub private_key: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "KeyPair")]
struct KeyPairWire {
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "publicKeyYCoordinate")]
    #[serde(default, deserialize_with = "deserialize_present")]
    public_key_y_coordinate: Option<String>,
    #[serde(rename = "privateKey")]
    private_key: String,
}
impl<'de> serde::Deserialize<'de> for KeyPair {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                KeyPairWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: ApiKeyCredentials
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ApiKeyCredentials {
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "secret")]
    pub secret: String,
    #[serde(rename = "passphrase")]
    pub passphrase: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "ApiKeyCredentials")]
struct ApiKeyCredentialsWire {
    #[serde(rename = "key")]
    key: String,
    #[serde(rename = "secret")]
    secret: String,
    #[serde(rename = "passphrase")]
    passphrase: String,
}
impl<'de> serde::Deserialize<'de> for ApiKeyCredentials {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                ApiKeyCredentialsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
// ts: PerpetualCurrencyObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PerpetualCurrencyObject {
    pub usdc: Vec<CurrencyObject>,
    pub usdt: Vec<CurrencyObject>,
}
#[derive(serde::Deserialize)]
#[serde(remote = "PerpetualCurrencyObject")]
struct PerpetualCurrencyObjectWire {
    usdc: Vec<CurrencyObject>,
    usdt: Vec<CurrencyObject>,
}
impl<'de> serde::Deserialize<'de> for PerpetualCurrencyObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PerpetualCurrencyObjectWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for TickerObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { funding_rate, high_price24h, index_price, last_price, low_price24h, next_funding_time, open_interest, oracle_price, predicted_funding_rate, price24h_pcnt, symbol, trade_count, turnover24h, volume24h });
    }
}

impl JsNumberOutput for DepthObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { a, b, s, u });
    }
}

impl JsNumberOutput for TradeObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { i, p, side, v, s, t });
    }
}

impl JsNumberOutput for KlineObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { s, i, t, c, h, l, o, v, tr });
    }
}

impl JsNumberOutput for HistoryFundingObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, rate, price, funding_time, funding_timestamp });
    }
}

impl JsNumberOutput for CurrencyObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, stark_ex_asset_id, stark_ex_resolution, step_size, show_step, icon_url });
    }
}

impl JsNumberOutput for GlobalObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { fee_account_id, fee_account_l2_key, stark_ex_collateral_currency_id, stark_ex_funding_validity_period, stark_ex_max_funding_rate, stark_ex_orders_tree_height, stark_ex_positions_tree_height, stark_ex_price_validity_period, stark_ex_contract_address, register_env_id, cross_chain_account_id, cross_chain_l2_key, fast_withdraw_account_id, fast_withdraw_fact_register_address, fast_withdraw_l2_key, fast_withdraw_max_amount, bybit_withdraw_account_id, bybit_withdraw_l2_key });
    }
}

impl JsNumberOutput for PerpetualContractObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { baseline_position_value, cross_id, cross_symbol_id, cross_symbol_name, digit_merge, display_max_leverage, display_min_leverage, enable_display, enable_open_position, enable_trade, funding_impact_margin_notional, funding_interest_rate, incremental_initial_margin_rate, incremental_maintenance_margin_rate, incremental_position_value, initial_margin_rate, maintenance_margin_rate, max_order_size, max_position_size, min_order_size, max_market_price_range, settle_currency_id, stark_ex_oracle_price_quorum, stark_ex_resolution, stark_ex_risk_factor, stark_ex_synthetic_asset_id, step_size, symbol, symbol_display_name, symbol_display_name2, tick_size, underlying_currency_id, max_maintenance_margin_rate, max_position_value });
    }
}

impl JsNumberOutput for GetHistoryFundingOptions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, page, begin_time_inclusive, end_time_exclusive, limit });
    }
}

impl JsNumberOutput for KeyPair {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { public_key, public_key_y_coordinate, private_key });
    }
}

impl JsNumberOutput for ApiKeyCredentials {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { key, secret, passphrase });
    }
}

impl JsNumberOutput for PerpetualCurrencyObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { usdc, usdt });
    }
}
