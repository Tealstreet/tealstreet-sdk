//! Remaining public configuration records and private opaque-payload records.
use crate::public_records::{CurrencyObject, GlobalObject, PerpetualContractObject};
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};

// ts: MultiChainObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MultiChainObject {
    #[serde(rename = "chains")]
    pub chains: serde_json::Value,
    #[serde(rename = "currency")]
    pub currency: String,
    #[serde(rename = "maxWithdraw")]
    pub max_withdraw: String,
    #[serde(rename = "minDeposit")]
    pub min_deposit: String,
    #[serde(rename = "minWithdraw")]
    pub min_withdraw: String,
}
#[derive(serde::Deserialize)]
#[serde(remote = "MultiChainObject")]
struct MultiChainObjectWire {
    #[serde(rename = "chains")]
    #[serde(deserialize_with = "deserialize_chains")]
    chains: serde_json::Value,
    #[serde(rename = "currency")]
    currency: String,
    #[serde(rename = "maxWithdraw")]
    max_withdraw: String,
    #[serde(rename = "minDeposit")]
    min_deposit: String,
    #[serde(rename = "minWithdraw")]
    min_withdraw: String,
}
impl<'de> serde::Deserialize<'de> for MultiChainObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                MultiChainObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for MultiChainObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { currency, max_withdraw, min_deposit, min_withdraw } passthrough { chains });
    }
}

// ts: SymbolObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SymbolObject {
    #[serde(rename = "currency")]
    pub currency: Vec<CurrencyObject>,
    #[serde(rename = "global")]
    pub global: GlobalObject,
    #[serde(rename = "perpetualContract")]
    pub perpetual_contract: Vec<PerpetualContractObject>,
    #[serde(rename = "multiChain")]
    pub multi_chain: MultiChainObject,
}
#[derive(serde::Deserialize)]
#[serde(remote = "SymbolObject")]
struct SymbolObjectWire {
    #[serde(rename = "currency")]
    currency: Vec<CurrencyObject>,
    #[serde(rename = "global")]
    global: GlobalObject,
    #[serde(rename = "perpetualContract")]
    perpetual_contract: Vec<PerpetualContractObject>,
    #[serde(rename = "multiChain")]
    multi_chain: MultiChainObject,
}
impl<'de> serde::Deserialize<'de> for SymbolObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                SymbolObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SymbolObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { currency, global, perpetual_contract, multi_chain });
    }
}

// ts: PerpetualObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PerpetualObject {
    #[serde(rename = "usdcConfig")]
    pub usdc_config: SymbolObject,
    #[serde(rename = "usdtConfig")]
    pub usdt_config: SymbolObject,
}
#[derive(serde::Deserialize)]
#[serde(remote = "PerpetualObject")]
struct PerpetualObjectWire {
    #[serde(rename = "usdcConfig")]
    usdc_config: SymbolObject,
    #[serde(rename = "usdtConfig")]
    usdt_config: SymbolObject,
}
impl<'de> serde::Deserialize<'de> for PerpetualObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                PerpetualObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for PerpetualObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { usdc_config, usdt_config });
    }
}

// ts: SymbolInfoObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SymbolInfoObject {
    #[serde(rename = "rankIdx")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_idx: Option<f64>,
    #[serde(rename = "pricePrecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_precision: Option<f64>,
    #[serde(rename = "priceStep")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_step: Option<f64>,
    #[serde(rename = "sizePrecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_precision: Option<f64>,
    #[serde(rename = "sizeStep")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_step: Option<f64>,
    #[serde(rename = "baseCoin")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin: Option<String>,
    #[serde(rename = "currentCoin")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_coin: Option<String>,
    #[serde(rename = "baseCoinPrecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin_precision: Option<f64>,
    #[serde(rename = "baseCoinRealPrecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin_real_precision: Option<f64>,
    #[serde(rename = "currentCoinPrecision")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_coin_precision: Option<f64>,
    #[serde(rename = "baseCoinIcon")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_coin_icon: Option<String>,
    #[serde(rename = "currentCoinIcon")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_coin_icon: Option<String>,
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
#[serde(remote = "SymbolInfoObject")]
struct SymbolInfoObjectWire {
    #[serde(rename = "rankIdx")]
    #[serde(default, deserialize_with = "deserialize_present")]
    rank_idx: Option<f64>,
    #[serde(rename = "pricePrecision")]
    #[serde(default, deserialize_with = "deserialize_present")]
    price_precision: Option<f64>,
    #[serde(rename = "priceStep")]
    #[serde(default, deserialize_with = "deserialize_present")]
    price_step: Option<f64>,
    #[serde(rename = "sizePrecision")]
    #[serde(default, deserialize_with = "deserialize_present")]
    size_precision: Option<f64>,
    #[serde(rename = "sizeStep")]
    #[serde(default, deserialize_with = "deserialize_present")]
    size_step: Option<f64>,
    #[serde(rename = "baseCoin")]
    #[serde(default, deserialize_with = "deserialize_present")]
    base_coin: Option<String>,
    #[serde(rename = "currentCoin")]
    #[serde(default, deserialize_with = "deserialize_present")]
    current_coin: Option<String>,
    #[serde(rename = "baseCoinPrecision")]
    #[serde(default, deserialize_with = "deserialize_present")]
    base_coin_precision: Option<f64>,
    #[serde(rename = "baseCoinRealPrecision")]
    #[serde(default, deserialize_with = "deserialize_present")]
    base_coin_real_precision: Option<f64>,
    #[serde(rename = "currentCoinPrecision")]
    #[serde(default, deserialize_with = "deserialize_present")]
    current_coin_precision: Option<f64>,
    #[serde(rename = "baseCoinIcon")]
    #[serde(default, deserialize_with = "deserialize_present")]
    base_coin_icon: Option<String>,
    #[serde(rename = "currentCoinIcon")]
    #[serde(default, deserialize_with = "deserialize_present")]
    current_coin_icon: Option<String>,
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
impl<'de> serde::Deserialize<'de> for SymbolInfoObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                SymbolInfoObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for SymbolInfoObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { rank_idx, price_precision, price_step, size_precision, size_step, base_coin, current_coin, base_coin_precision, base_coin_real_precision, current_coin_precision, base_coin_icon, current_coin_icon, baseline_position_value, cross_id, cross_symbol_id, cross_symbol_name, digit_merge, display_max_leverage, display_min_leverage, enable_display, enable_open_position, enable_trade, funding_impact_margin_notional, funding_interest_rate, incremental_initial_margin_rate, incremental_maintenance_margin_rate, incremental_position_value, initial_margin_rate, maintenance_margin_rate, max_order_size, max_position_size, min_order_size, max_market_price_range, settle_currency_id, stark_ex_oracle_price_quorum, stark_ex_resolution, stark_ex_risk_factor, stark_ex_synthetic_asset_id, step_size, symbol, symbol_display_name, symbol_display_name2, tick_size, underlying_currency_id, max_maintenance_margin_rate, max_position_value });
    }
}

// ts: UserObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UserObject {
    #[serde(rename = "ethereumAddress")]
    pub ethereum_address: String,
    #[serde(rename = "isRegistered")]
    pub is_registered: bool,
    #[serde(rename = "email")]
    pub email: String,
    #[serde(rename = "username")]
    pub username: String,
    #[serde(rename = "referredByAffiliateLink")]
    pub referred_by_affiliate_link: String,
    #[serde(rename = "affiliateLink")]
    pub affiliate_link: String,
    #[serde(rename = "makerVolume30D")]
    pub maker_volume30_d: String,
    #[serde(rename = "takerVolume30D")]
    pub taker_volume30_d: String,
    #[serde(rename = "fees30D")]
    pub fees30_d: String,
    #[serde(rename = "userData")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<serde_json::Value>,
    #[serde(rename = "apexTokenBalance")]
    pub apex_token_balance: String,
    #[serde(rename = "stakedApexTokenBalance")]
    pub staked_apex_token_balance: String,
    #[serde(rename = "isEmailVerified")]
    pub is_email_verified: bool,
    #[serde(rename = "isSharingUsername")]
    pub is_sharing_username: bool,
    #[serde(rename = "isSharingAddress")]
    pub is_sharing_address: bool,
    #[serde(rename = "country")]
    pub country: String,
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "avatarUrl")]
    pub avatar_url: String,
    #[serde(rename = "emailNotifyGeneralEnable")]
    pub email_notify_general_enable: bool,
    #[serde(rename = "emailNotifyTradingEnable")]
    pub email_notify_trading_enable: bool,
    #[serde(rename = "emailNotifyAccountEnable")]
    pub email_notify_account_enable: bool,
}
#[derive(serde::Deserialize)]
#[serde(remote = "UserObject")]
struct UserObjectWire {
    #[serde(rename = "ethereumAddress")]
    ethereum_address: String,
    #[serde(rename = "isRegistered")]
    is_registered: bool,
    #[serde(rename = "email")]
    email: String,
    #[serde(rename = "username")]
    username: String,
    #[serde(rename = "referredByAffiliateLink")]
    referred_by_affiliate_link: String,
    #[serde(rename = "affiliateLink")]
    affiliate_link: String,
    #[serde(rename = "makerVolume30D")]
    maker_volume30_d: String,
    #[serde(rename = "takerVolume30D")]
    taker_volume30_d: String,
    #[serde(rename = "fees30D")]
    fees30_d: String,
    #[serde(rename = "userData")]
    #[serde(
        default,
        deserialize_with = "safe_cex_types::retained_json::deserialize_present_retained_json"
    )]
    user_data: Option<serde_json::Value>,
    #[serde(rename = "apexTokenBalance")]
    apex_token_balance: String,
    #[serde(rename = "stakedApexTokenBalance")]
    staked_apex_token_balance: String,
    #[serde(rename = "isEmailVerified")]
    is_email_verified: bool,
    #[serde(rename = "isSharingUsername")]
    is_sharing_username: bool,
    #[serde(rename = "isSharingAddress")]
    is_sharing_address: bool,
    #[serde(rename = "country")]
    country: String,
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "avatarUrl")]
    avatar_url: String,
    #[serde(rename = "emailNotifyGeneralEnable")]
    email_notify_general_enable: bool,
    #[serde(rename = "emailNotifyTradingEnable")]
    email_notify_trading_enable: bool,
    #[serde(rename = "emailNotifyAccountEnable")]
    email_notify_account_enable: bool,
}
impl<'de> serde::Deserialize<'de> for UserObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                UserObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for UserObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { ethereum_address, is_registered, email, username, referred_by_affiliate_link, affiliate_link, maker_volume30_d, taker_volume30_d, fees30_d, apex_token_balance, staked_apex_token_balance, is_email_verified, is_sharing_username, is_sharing_address, country, id, avatar_url, email_notify_general_enable, email_notify_trading_enable, email_notify_account_enable } passthrough { user_data });
    }
}

// ts: OrderObject
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OrderObject {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "limitFee")]
    pub limit_fee: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "liquidateFee")]
    pub liquidate_fee: String,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "direction")]
    pub direction: String,
    #[serde(rename = "createdAt")]
    pub created_at: f64,
    #[serde(rename = "updatedTime")]
    pub updated_time: f64,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: String,
    #[serde(rename = "matchFillId")]
    pub match_fill_id: String,
    #[serde(rename = "reduceOnly")]
    pub reduce_only: bool,
    #[serde(rename = "isPositionTpsl")]
    pub is_position_tpsl: bool,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    #[serde(rename = "exitType")]
    pub exit_type: String,
    #[serde(rename = "latestMatchFillPrice")]
    pub latest_match_fill_price: String,
    #[serde(rename = "cumMatchFillSize")]
    pub cum_match_fill_size: String,
    #[serde(rename = "cumMatchFillValue")]
    pub cum_match_fill_value: String,
    #[serde(rename = "cumMatchFillFee")]
    pub cum_match_fill_fee: String,
    #[serde(rename = "cumSuccessFillSize")]
    pub cum_success_fill_size: String,
    #[serde(rename = "cumSuccessFillValue")]
    pub cum_success_fill_value: String,
    #[serde(rename = "cumSuccessFillFee")]
    pub cum_success_fill_fee: String,
    #[serde(rename = "triggerPriceType")]
    pub trigger_price_type: String,
    #[serde(rename = "isOpenTpslOrder")]
    pub is_open_tpsl_order: bool,
    #[serde(rename = "isSetOpenTp")]
    pub is_set_open_tp: bool,
    #[serde(rename = "isSetOpenSl")]
    pub is_set_open_sl: bool,
    #[serde(rename = "openTpParam")]
    pub open_tp_param: serde_json::Value,
    #[serde(rename = "openSlParam")]
    pub open_sl_param: serde_json::Value,
}
#[derive(serde::Deserialize)]
#[serde(remote = "OrderObject")]
struct OrderObjectWire {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "clientId")]
    client_id: String,
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: String,
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "limitFee")]
    limit_fee: String,
    #[serde(rename = "fee")]
    fee: String,
    #[serde(rename = "liquidateFee")]
    liquidate_fee: String,
    #[serde(rename = "size")]
    size: String,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(rename = "direction")]
    direction: String,
    #[serde(rename = "createdAt")]
    created_at: f64,
    #[serde(rename = "updatedTime")]
    updated_time: f64,
    #[serde(rename = "status")]
    status: String,
    #[serde(rename = "timeInForce")]
    time_in_force: String,
    #[serde(rename = "matchFillId")]
    match_fill_id: String,
    #[serde(rename = "reduceOnly")]
    reduce_only: bool,
    #[serde(rename = "isPositionTpsl")]
    is_position_tpsl: bool,
    #[serde(rename = "orderId")]
    order_id: String,
    #[serde(rename = "orderType")]
    order_type: String,
    #[serde(rename = "exitType")]
    exit_type: String,
    #[serde(rename = "latestMatchFillPrice")]
    latest_match_fill_price: String,
    #[serde(rename = "cumMatchFillSize")]
    cum_match_fill_size: String,
    #[serde(rename = "cumMatchFillValue")]
    cum_match_fill_value: String,
    #[serde(rename = "cumMatchFillFee")]
    cum_match_fill_fee: String,
    #[serde(rename = "cumSuccessFillSize")]
    cum_success_fill_size: String,
    #[serde(rename = "cumSuccessFillValue")]
    cum_success_fill_value: String,
    #[serde(rename = "cumSuccessFillFee")]
    cum_success_fill_fee: String,
    #[serde(rename = "triggerPriceType")]
    trigger_price_type: String,
    #[serde(rename = "isOpenTpslOrder")]
    is_open_tpsl_order: bool,
    #[serde(rename = "isSetOpenTp")]
    is_set_open_tp: bool,
    #[serde(rename = "isSetOpenSl")]
    is_set_open_sl: bool,
    #[serde(rename = "openTpParam")]
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    open_tp_param: serde_json::Value,
    #[serde(rename = "openSlParam")]
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    open_sl_param: serde_json::Value,
}
impl<'de> serde::Deserialize<'de> for OrderObject {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                OrderObjectWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for OrderObject {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, client_id, client_order_id, account_id, symbol, side, price, limit_fee, fee, liquidate_fee, size, r#type, direction, created_at, updated_time, status, time_in_force, match_fill_id, reduce_only, is_position_tpsl, order_id, order_type, exit_type, latest_match_fill_price, cum_match_fill_size, cum_match_fill_value, cum_match_fill_fee, cum_success_fill_size, cum_success_fill_value, cum_success_fill_fee, trigger_price_type, is_open_tpsl_order, is_set_open_tp, is_set_open_sl } passthrough { open_tp_param, open_sl_param });
    }
}

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
fn deserialize_chains<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<serde_json::Value, D::Error> {
    let value = safe_cex_num::numeric_ingress::js_deserialize_passthrough(decoder)?;
    if !value
        .as_array()
        .is_some_and(|rows| rows.iter().all(serde_json::Value::is_array))
    {
        return Err(serde::de::Error::custom("chains requires array of arrays"));
    }
    Ok(value)
}
