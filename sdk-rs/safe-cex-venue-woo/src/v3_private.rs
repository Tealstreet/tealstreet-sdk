//! Strict inert account, balance and futures-position V3 reads. No transport or store mutation.
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::{PositionMode, PositionSide, object_map::JsObjectMap};
use serde::{Deserialize, Serialize};
type Record = JsObjectMap<JsNumberInput>;
// ts: WooV3AccountMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AccountMode {
    #[serde(rename = "MARGIN")]
    Margin,
    #[serde(rename = "SPOT")]
    Spot,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AccountMode")]
enum AcceptedWooV3AccountModeWire {
    #[serde(rename = "MARGIN")]
    Margin,
    #[serde(rename = "SPOT")]
    Spot,
}
impl<'de> Deserialize<'de> for WooV3AccountMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AccountModeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Margin,Spot});
    }
}
// ts: WooV3AccountPositionMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AccountPositionMode {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AccountPositionMode")]
enum AcceptedWooV3AccountPositionModeWire {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode,
}
impl<'de> Deserialize<'de> for WooV3AccountPositionMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AccountPositionModeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountPositionMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{OneWay,HedgeMode});
    }
}
// ts: WooV3PositionSide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3PositionSide {
    #[serde(rename = "BOTH")]
    Both,
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3PositionSide")]
enum AcceptedWooV3PositionSideWire {
    #[serde(rename = "BOTH")]
    Both,
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl<'de> Deserialize<'de> for WooV3PositionSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3PositionSideWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PositionSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Both,Long,Short});
    }
}
fn account_type<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "Main" | "Sub") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("unsupported account type"))
    }
}
// ts: WooV3PageMeta
#[derive(Debug, Clone)]
pub struct WooV3PageMeta {
    pub total: f64,
    pub records_per_page: f64,
    pub current_page: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PageMeta")]
struct WooV3PageMetaFields {
    #[serde(rename = "total")]
    total: f64,
    #[serde(rename = "recordsPerPage")]
    records_per_page: f64,
    #[serde(rename = "currentPage")]
    current_page: f64,
}
impl Serialize for WooV3PageMeta {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3PageMetaFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3PageMeta {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3PageMetaFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PageMeta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{total,records_per_page,current_page});
    }
}
// ts: WooV3AccountInfo
#[derive(Debug, Clone)]
pub struct WooV3AccountInfo {
    pub application_id: String,
    pub account: String,
    pub alias: String,
    pub otpauth: bool,
    pub account_mode: WooV3AccountMode,
    pub position_mode: WooV3AccountPositionMode,
    pub leverage: f64,
    pub margin_ratio: String,
    pub open_margin_ratio: String,
    pub initial_margin_ratio: String,
    pub maintenance_margin_ratio: String,
    pub total_collateral: String,
    pub free_collateral: String,
    pub total_account_value: String,
    pub total_trading_value: String,
    pub total_vault_value: String,
    pub total_staking_value: String,
    pub total_earn_value: String,
    pub total_launchpad_value: String,
    pub referrer_id: String,
    pub account_type: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AccountInfo")]
struct WooV3AccountInfoFields {
    #[serde(rename = "applicationId")]
    application_id: String,
    #[serde(rename = "account")]
    account: String,
    #[serde(rename = "alias")]
    alias: String,
    #[serde(rename = "otpauth")]
    otpauth: bool,
    #[serde(rename = "accountMode")]
    account_mode: WooV3AccountMode,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "marginRatio")]
    margin_ratio: String,
    #[serde(rename = "openMarginRatio")]
    open_margin_ratio: String,
    #[serde(rename = "initialMarginRatio")]
    initial_margin_ratio: String,
    #[serde(rename = "maintenanceMarginRatio")]
    maintenance_margin_ratio: String,
    #[serde(rename = "totalCollateral")]
    total_collateral: String,
    #[serde(rename = "freeCollateral")]
    free_collateral: String,
    #[serde(rename = "totalAccountValue")]
    total_account_value: String,
    #[serde(rename = "totalTradingValue")]
    total_trading_value: String,
    #[serde(rename = "totalVaultValue")]
    total_vault_value: String,
    #[serde(rename = "totalStakingValue")]
    total_staking_value: String,
    #[serde(rename = "totalEarnValue")]
    total_earn_value: String,
    #[serde(rename = "totalLaunchpadValue")]
    total_launchpad_value: String,
    #[serde(rename = "referrerID")]
    referrer_id: String,
    #[serde(rename = "accountType", deserialize_with = "account_type")]
    account_type: String,
}
impl Serialize for WooV3AccountInfo {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AccountInfoFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AccountInfo {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AccountInfoFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{application_id,account,alias,otpauth,account_mode,position_mode,leverage,margin_ratio,open_margin_ratio,initial_margin_ratio,maintenance_margin_ratio,total_collateral,free_collateral,total_account_value,total_trading_value,total_vault_value,total_staking_value,total_earn_value,total_launchpad_value,referrer_id,account_type});
    }
}
// ts: WooV3AssetBalance
#[derive(Debug, Clone)]
pub struct WooV3AssetBalance {
    pub token: String,
    pub holding: String,
    pub frozen: String,
    pub staked: String,
    pub unbonding: String,
    pub vault: String,
    pub interest: String,
    pub earn: String,
    pub pending_short_qty: String,
    pub pending_long_qty: String,
    pub available_balance: String,
    pub average_open_price: String,
    pub mark_price: String,
    pub pnl24_h: String,
    pub fee24_h: String,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AssetBalance")]
struct WooV3AssetBalanceFields {
    #[serde(rename = "token")]
    token: String,
    #[serde(rename = "holding")]
    holding: String,
    #[serde(rename = "frozen")]
    frozen: String,
    #[serde(rename = "staked")]
    staked: String,
    #[serde(rename = "unbonding")]
    unbonding: String,
    #[serde(rename = "vault")]
    vault: String,
    #[serde(rename = "interest")]
    interest: String,
    #[serde(rename = "earn")]
    earn: String,
    #[serde(rename = "pendingShortQty")]
    pending_short_qty: String,
    #[serde(rename = "pendingLongQty")]
    pending_long_qty: String,
    #[serde(rename = "availableBalance")]
    available_balance: String,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "pnl24H")]
    pnl24_h: String,
    #[serde(rename = "fee24H")]
    fee24_h: String,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3AssetBalance {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AssetBalanceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AssetBalance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AssetBalanceFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AssetBalance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{token,holding,frozen,staked,unbonding,vault,interest,earn,pending_short_qty,pending_long_qty,available_balance,average_open_price,mark_price,pnl24_h,fee24_h,timestamp});
    }
}
// ts: WooV3AssetBalanceReadModel
#[derive(Debug, Clone)]
pub struct WooV3AssetBalanceReadModel {
    pub token: String,
    pub trading_total: f64,
    pub trading_free: f64,
    pub trading_frozen: f64,
    pub staked: f64,
    pub unbonding: f64,
    pub vault: f64,
    pub interest: f64,
    pub earn: f64,
    pub pending_short_quantity: f64,
    pub pending_long_quantity: f64,
    pub average_open_price: f64,
    pub mark_price: f64,
    pub pnl24_h: f64,
    pub fee24_h: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AssetBalanceReadModel")]
struct WooV3AssetBalanceReadModelFields {
    #[serde(rename = "token")]
    token: String,
    #[serde(rename = "tradingTotal")]
    trading_total: f64,
    #[serde(rename = "tradingFree")]
    trading_free: f64,
    #[serde(rename = "tradingFrozen")]
    trading_frozen: f64,
    #[serde(rename = "staked")]
    staked: f64,
    #[serde(rename = "unbonding")]
    unbonding: f64,
    #[serde(rename = "vault")]
    vault: f64,
    #[serde(rename = "interest")]
    interest: f64,
    #[serde(rename = "earn")]
    earn: f64,
    #[serde(rename = "pendingShortQuantity")]
    pending_short_quantity: f64,
    #[serde(rename = "pendingLongQuantity")]
    pending_long_quantity: f64,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: f64,
    #[serde(rename = "markPrice")]
    mark_price: f64,
    #[serde(rename = "pnl24H")]
    pnl24_h: f64,
    #[serde(rename = "fee24H")]
    fee24_h: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3AssetBalanceReadModel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AssetBalanceReadModelFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AssetBalanceReadModel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AssetBalanceReadModelFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AssetBalanceReadModel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{token,trading_total,trading_free,trading_frozen,staked,unbonding,vault,interest,earn,pending_short_quantity,pending_long_quantity,average_open_price,mark_price,pnl24_h,fee24_h,timestamp});
    }
}
// ts: WooV3FuturesPosition
#[derive(Debug, Clone)]
pub struct WooV3FuturesPosition {
    pub symbol: String,
    pub holding: String,
    pub position_side: WooV3PositionSide,
    pub pending_long_qty: String,
    pub pending_short_qty: String,
    pub settle_price: String,
    pub average_open_price: String,
    pub pnl24_h: String,
    pub fee24_h: String,
    pub mark_price: String,
    pub est_liq_price: String,
    pub adl_quantile: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3FuturesPosition")]
struct WooV3FuturesPositionFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "holding")]
    holding: String,
    #[serde(rename = "positionSide")]
    position_side: WooV3PositionSide,
    #[serde(rename = "pendingLongQty")]
    pending_long_qty: String,
    #[serde(rename = "pendingShortQty")]
    pending_short_qty: String,
    #[serde(rename = "settlePrice")]
    settle_price: String,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: String,
    #[serde(rename = "pnl24H")]
    pnl24_h: String,
    #[serde(rename = "fee24H")]
    fee24_h: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "estLiqPrice")]
    est_liq_price: String,
    #[serde(rename = "adlQuantile")]
    adl_quantile: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3FuturesPosition {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3FuturesPositionFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3FuturesPosition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3FuturesPositionFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3FuturesPosition {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,holding,position_side,pending_long_qty,pending_short_qty,settle_price,average_open_price,pnl24_h,fee24_h,mark_price,est_liq_price,adl_quantile,timestamp});
    }
}
// ts: WooV3PositionReadModel
#[derive(Debug, Clone)]
pub struct WooV3PositionReadModel {
    pub symbol: String,
    pub side: PositionSide,
    pub position_mode: PositionMode,
    pub gateway_position_side: WooV3PositionSide,
    pub signed_holding: f64,
    pub contracts: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub notional: f64,
    pub estimated_liquidation_price: f64,
    pub pending_long_quantity: f64,
    pub pending_short_quantity: f64,
    pub pnl24_h: f64,
    pub fee24_h: f64,
    pub adl_quantile: f64,
    pub timestamp: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PositionReadModel")]
struct WooV3PositionReadModelFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "side")]
    side: PositionSide,
    #[serde(rename = "positionMode")]
    position_mode: PositionMode,
    #[serde(rename = "gatewayPositionSide")]
    gateway_position_side: WooV3PositionSide,
    #[serde(rename = "signedHolding")]
    signed_holding: f64,
    #[serde(rename = "contracts")]
    contracts: f64,
    #[serde(rename = "entryPrice")]
    entry_price: f64,
    #[serde(rename = "markPrice")]
    mark_price: f64,
    #[serde(rename = "notional")]
    notional: f64,
    #[serde(rename = "estimatedLiquidationPrice")]
    estimated_liquidation_price: f64,
    #[serde(rename = "pendingLongQuantity")]
    pending_long_quantity: f64,
    #[serde(rename = "pendingShortQuantity")]
    pending_short_quantity: f64,
    #[serde(rename = "pnl24H")]
    pnl24_h: f64,
    #[serde(rename = "fee24H")]
    fee24_h: f64,
    #[serde(rename = "adlQuantile")]
    adl_quantile: f64,
    #[serde(rename = "timestamp")]
    timestamp: f64,
}
impl Serialize for WooV3PositionReadModel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3PositionReadModelFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3PositionReadModel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3PositionReadModelFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PositionReadModel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,side,position_mode,gateway_position_side,signed_holding,contracts,entry_price,mark_price,notional,estimated_liquidation_price,pending_long_quantity,pending_short_quantity,pnl24_h,fee24_h,adl_quantile,timestamp});
    }
}

// Dense JSON capture owns records; source object identities and executable getters are not retained.
// ts: isRecord
pub fn is_record(value: Option<&JsNumberInput>) -> bool {
    value.is_some_and(|v| v.clone().decode::<Record>().is_ok())
}
// ts: requireRecord
pub fn require_record(value: Option<&JsNumberInput>, path: &str) -> Result<Record, String> {
    value
        .and_then(|v| v.clone().decode::<Record>().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
// ts: requireArray
pub fn require_array(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<Vec<JsNumberInput>, String> {
    value
        .and_then(|v| v.clone().decode::<Vec<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an array"))
}
// ts: requireString
pub fn require_string(
    value: Option<&JsNumberInput>,
    path: &str,
    allow_empty: bool,
) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|s| allow_empty || !s.is_empty())
        .ok_or_else(|| {
            format!(
                "{path} must be {}",
                if allow_empty {
                    "a string"
                } else {
                    "a non-empty string"
                }
            )
        })
}
// ts: requireBoolean
pub fn require_boolean(value: Option<&JsNumberInput>, path: &str) -> Result<bool, String> {
    value
        .and_then(|v| v.clone().decode::<bool>().ok())
        .ok_or_else(|| format!("{path} must be a boolean"))
}
// ts: requireInteger
pub fn require_integer(
    value: Option<&JsNumberInput>,
    path: &str,
    minimum: f64,
) -> Result<f64, String> {
    let number = value.and_then(|v| v.clone().decode::<f64>().ok());
    // JS `!(n < minimum)`. is_finite() above means NaN cannot reach here
    // today, so clippy's `>=` rewrite would be equivalent - but the guard
    // order is what makes it equivalent, and the source expression is what
    // the parity row maps. Keep the spelling; the allow records why.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    number
        .filter(|n| {
            n.is_finite()
                && n.fract() == 0.0
                && n.abs() <= 9_007_199_254_740_991.0
                && !(*n < minimum)
        })
        .ok_or_else(|| {
            format!(
                "{path} must be a safe integer of at least {}",
                safe_cex_num::js_number_string(minimum)
            )
        })
}
// ts: requireFiniteNumber
pub fn require_finite_number(value: Option<&JsNumberInput>, path: &str) -> Result<f64, String> {
    value
        .and_then(|v| v.clone().decode::<f64>().ok())
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{path} must be a finite number"))
}
fn decimal_pattern(text: &str) -> bool {
    let text = text.strip_prefix('-').unwrap_or(text);
    let mut parts = text.split('.');
    let integer = parts.next().unwrap();
    let fraction = parts.next();
    if parts.next().is_some() {
        return false;
    }
    let digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    digits(integer)
        && match fraction {
            None => !integer.is_empty(),
            Some(f) => digits(f) && (!integer.is_empty() || !f.is_empty()),
        }
}
// ts: requireDecimalString
pub fn require_decimal_string(value: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|s| decimal_pattern(s) && safe_cex_num::number_from_string(s).is_finite())
        .ok_or_else(|| format!("{path} must be a finite decimal string"))
}
// ts: optionalDecimalString
pub fn optional_decimal_string(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<Option<String>, String> {
    value
        .map(|v| require_decimal_string(Some(v), path))
        .transpose()
}
// ts: requireEnum
pub fn require_enum(
    value: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|v| values.contains(&v.as_str()))
        .ok_or_else(|| format!("{path} must be one of {}", values.join(", ")))
}
// ts: decodeEnvelope
pub fn decode_envelope(input: Option<&JsNumberInput>, name: &str) -> Result<Record, String> {
    let response = require_record(input, &format!("WOO V3 {name} response"))?;
    if !require_boolean(response.get("success"), &format!("WOO V3 {name}.success"))? {
        return Err(format!("WOO V3 {name} response reports success=false"));
    }
    require_integer(
        response.get("timestamp"),
        &format!("WOO V3 {name}.timestamp"),
        0.0,
    )?;
    require_record(response.get("data"), &format!("WOO V3 {name}.data"))
}
// ts: decodeMeta
pub fn decode_meta(input: Option<&JsNumberInput>, path: &str) -> Result<WooV3PageMeta, String> {
    let meta = require_record(input, path)?;
    Ok(WooV3PageMeta {
        total: require_integer(meta.get("total"), &format!("{path}.total"), 0.0)?,
        records_per_page: require_integer(
            meta.get("recordsPerPage"),
            &format!("{path}.recordsPerPage"),
            1.0,
        )?,
        current_page: require_integer(
            meta.get("currentPage"),
            &format!("{path}.currentPage"),
            1.0,
        )?,
    })
}
#[derive(Debug, Clone)]
pub struct WooV3PaginatedRows<T> {
    pub rows: Vec<T>,
    pub meta: WooV3PageMeta,
}
// ts: decodePaginatedRows
pub fn decode_paginated_rows<T>(
    input: Option<&JsNumberInput>,
    name: &str,
    mut decoder: impl FnMut(&JsNumberInput, &str) -> Result<T, String>,
) -> Result<WooV3PaginatedRows<T>, String> {
    let data = decode_envelope(input, name)?;
    let meta = decode_meta(data.get("meta"), &format!("WOO V3 {name}.data.meta"))?;
    let rows = require_array(data.get("rows"), &format!("WOO V3 {name}.data.rows"))?;
    let rows = rows
        .iter()
        .enumerate()
        .map(|(i, row)| decoder(row, &format!("WOO V3 {name}.data.rows[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() as f64 > meta.records_per_page {
        return Err(format!("WOO V3 {name} page exceeds recordsPerPage"));
    }
    Ok(WooV3PaginatedRows { rows, meta })
}
// ts: decimalToNumber
pub fn decimal_to_number(value: &str, path: &str, minimum: Option<f64>) -> Result<f64, String> {
    let number = safe_cex_num::number_from_string(value);
    if !number.is_finite() || minimum.is_some_and(|min| number < min) {
        return Err(format!(
            "{path} must be{}",
            minimum
                .map(|n| format!(" at least {}", safe_cex_num::js_number_string(n)))
                .unwrap_or_default()
        ));
    }
    Ok(number)
}
// ts: normalizePerpetualSymbol
pub fn normalize_perpetual_symbol(value: &str, path: &str) -> Result<String, String> {
    let parts = value.split('_').collect::<Vec<_>>();
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    };
    if parts.len() != 3 || parts[0] != "PERP" || !token(parts[1]) || !token(parts[2]) {
        return Err(format!("{path} must use the PERP_BASE_QUOTE form"));
    }
    Ok(format!("{}{}", parts[1], parts[2]))
}
// ts: mapPositionMode
pub fn map_position_mode(mode: WooV3AccountPositionMode) -> PositionMode {
    match mode {
        WooV3AccountPositionMode::OneWay => PositionMode::OneWay,
        WooV3AccountPositionMode::HedgeMode => PositionMode::Hedged,
    }
}
fn mode_text(mode: WooV3AccountPositionMode) -> &'static str {
    match mode {
        WooV3AccountPositionMode::OneWay => "ONE_WAY",
        WooV3AccountPositionMode::HedgeMode => "HEDGE_MODE",
    }
}
fn side_text(side: WooV3PositionSide) -> &'static str {
    match side {
        WooV3PositionSide::Both => "BOTH",
        WooV3PositionSide::Long => "LONG",
        WooV3PositionSide::Short => "SHORT",
    }
}
// ts: assertPositionSideMatchesMode
pub fn assert_position_side_matches_mode(
    side: WooV3PositionSide,
    mode: WooV3AccountPositionMode,
    path: &str,
) -> Result<(), String> {
    if (mode == WooV3AccountPositionMode::OneWay && side != WooV3PositionSide::Both)
        || (mode == WooV3AccountPositionMode::HedgeMode && side == WooV3PositionSide::Both)
    {
        return Err(format!(
            "{path}={} is incompatible with {}",
            side_text(side),
            mode_text(mode)
        ));
    }
    Ok(())
}
// ts: decodeWooV3AccountInfo
pub fn decode_woo_v3_account_info(
    input: Option<&JsNumberInput>,
) -> Result<WooV3AccountInfo, String> {
    let data = decode_envelope(input, "account info")?;
    let path = "WOO V3 account info.data";
    Ok(WooV3AccountInfo {
        application_id: require_string(
            data.get("applicationId"),
            &format!("{path}.applicationId"),
            false,
        )?,
        account: require_string(data.get("account"), &format!("{path}.account"), false)?,
        alias: require_string(data.get("alias"), &format!("{path}.alias"), false)?,
        otpauth: require_boolean(data.get("otpauth"), &format!("{path}.otpauth"))?,
        account_mode: match require_enum(
            data.get("accountMode"),
            &["MARGIN", "SPOT"],
            &format!("{path}.accountMode"),
        )?
        .as_str()
        {
            "MARGIN" => WooV3AccountMode::Margin,
            "SPOT" => WooV3AccountMode::Spot,
            _ => unreachable!(),
        },
        position_mode: match require_enum(
            data.get("positionMode"),
            &["ONE_WAY", "HEDGE_MODE"],
            &format!("{path}.positionMode"),
        )?
        .as_str()
        {
            "ONE_WAY" => WooV3AccountPositionMode::OneWay,
            "HEDGE_MODE" => WooV3AccountPositionMode::HedgeMode,
            _ => unreachable!(),
        },
        leverage: require_integer(data.get("leverage"), &format!("{path}.leverage"), 1.0)?,
        margin_ratio: require_decimal_string(
            data.get("marginRatio"),
            &format!("{path}.marginRatio"),
        )?,
        open_margin_ratio: require_decimal_string(
            data.get("openMarginRatio"),
            &format!("{path}.openMarginRatio"),
        )?,
        initial_margin_ratio: require_decimal_string(
            data.get("initialMarginRatio"),
            &format!("{path}.initialMarginRatio"),
        )?,
        maintenance_margin_ratio: require_decimal_string(
            data.get("maintenanceMarginRatio"),
            &format!("{path}.maintenanceMarginRatio"),
        )?,
        total_collateral: require_decimal_string(
            data.get("totalCollateral"),
            &format!("{path}.totalCollateral"),
        )?,
        free_collateral: require_decimal_string(
            data.get("freeCollateral"),
            &format!("{path}.freeCollateral"),
        )?,
        total_account_value: require_decimal_string(
            data.get("totalAccountValue"),
            &format!("{path}.totalAccountValue"),
        )?,
        total_trading_value: require_decimal_string(
            data.get("totalTradingValue"),
            &format!("{path}.totalTradingValue"),
        )?,
        total_vault_value: require_decimal_string(
            data.get("totalVaultValue"),
            &format!("{path}.totalVaultValue"),
        )?,
        total_staking_value: require_decimal_string(
            data.get("totalStakingValue"),
            &format!("{path}.totalStakingValue"),
        )?,
        total_earn_value: require_decimal_string(
            data.get("totalEarnValue"),
            &format!("{path}.totalEarnValue"),
        )?,
        total_launchpad_value: require_decimal_string(
            data.get("totalLaunchpadValue"),
            &format!("{path}.totalLaunchpadValue"),
        )?,
        referrer_id: require_string(data.get("referrerID"), &format!("{path}.referrerID"), false)?,
        account_type: require_enum(
            data.get("accountType"),
            &["Main", "Sub"],
            &format!("{path}.accountType"),
        )?,
    })
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WooV3AccountBalance {
    pub free: f64,
    pub used: f64,
    pub total: f64,
    pub upnl: f64,
    pub info: WooV3AccountInfo,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountBalance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{free,used,total,upnl,info});
    }
}
// ts: mapWooV3AccountBalance
pub fn map_woo_v3_account_balance(
    info: &WooV3AccountInfo,
    previous_upnl: f64,
) -> Result<WooV3AccountBalance, String> {
    if info.account_mode != WooV3AccountMode::Margin {
        return Err("WOO V3 account mode SPOT is not futures-capable".into());
    }
    if !previous_upnl.is_finite() {
        return Err("WOO V3 previous uPnL must be finite".into());
    }
    let total_collateral =
        decimal_to_number(&info.total_collateral, "WOO V3 totalCollateral", None)?;
    let free = decimal_to_number(&info.free_collateral, "WOO V3 freeCollateral", None)?;
    let total = decimal_to_number(&info.total_account_value, "WOO V3 totalAccountValue", None)?;
    let used = total_collateral - free;
    if used < 0.0 {
        return Err("WOO V3 freeCollateral must not exceed totalCollateral".into());
    }
    Ok(WooV3AccountBalance {
        free,
        used,
        total,
        upnl: previous_upnl,
        info: info.clone(),
    })
}
// ts: decodeWooV3AssetBalances
pub fn decode_woo_v3_asset_balances(
    input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3AssetBalance>, String> {
    let data = decode_envelope(input, "asset balances")?;
    require_array(data.get("holding"), "WOO V3 asset balances.data.holding")?
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let path = format!("WOO V3 asset balances.data.holding[{i}]");
            let row = require_record(Some(input), &path)?;
            Ok(WooV3AssetBalance {
                token: require_string(row.get("token"), &format!("{path}.token"), false)?,
                holding: require_decimal_string(row.get("holding"), &format!("{path}.holding"))?,
                frozen: require_decimal_string(row.get("frozen"), &format!("{path}.frozen"))?,
                staked: require_decimal_string(row.get("staked"), &format!("{path}.staked"))?,
                unbonding: require_decimal_string(
                    row.get("unbonding"),
                    &format!("{path}.unbonding"),
                )?,
                vault: require_decimal_string(row.get("vault"), &format!("{path}.vault"))?,
                interest: require_decimal_string(row.get("interest"), &format!("{path}.interest"))?,
                earn: require_decimal_string(row.get("earn"), &format!("{path}.earn"))?,
                pending_short_qty: require_decimal_string(
                    row.get("pendingShortQty"),
                    &format!("{path}.pendingShortQty"),
                )?,
                pending_long_qty: require_decimal_string(
                    row.get("pendingLongQty"),
                    &format!("{path}.pendingLongQty"),
                )?,
                available_balance: require_decimal_string(
                    row.get("availableBalance"),
                    &format!("{path}.availableBalance"),
                )?,
                average_open_price: require_decimal_string(
                    row.get("averageOpenPrice"),
                    &format!("{path}.averageOpenPrice"),
                )?,
                mark_price: require_decimal_string(
                    row.get("markPrice"),
                    &format!("{path}.markPrice"),
                )?,
                pnl24_h: require_decimal_string(row.get("pnl24H"), &format!("{path}.pnl24H"))?,
                fee24_h: require_decimal_string(row.get("fee24H"), &format!("{path}.fee24H"))?,
                timestamp: require_integer(
                    row.get("timestamp"),
                    &format!("{path}.timestamp"),
                    0.0,
                )?,
            })
        })
        .collect()
}
// ts: mapWooV3AssetBalances
pub fn map_woo_v3_asset_balances(
    rows: &[WooV3AssetBalance],
) -> Result<Vec<WooV3AssetBalanceReadModel>, String> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let path = format!("WOO V3 asset balance[{i}]");
            Ok(WooV3AssetBalanceReadModel {
                token: row.token.clone(),
                trading_total: decimal_to_number(
                    &row.holding,
                    &format!("{path}.holding"),
                    Some(0.0),
                )?,
                trading_free: decimal_to_number(
                    &row.available_balance,
                    &format!("{path}.availableBalance"),
                    Some(0.0),
                )?,
                trading_frozen: decimal_to_number(
                    &row.frozen,
                    &format!("{path}.frozen"),
                    Some(0.0),
                )?,
                staked: decimal_to_number(&row.staked, &format!("{path}.staked"), Some(0.0))?,
                unbonding: decimal_to_number(
                    &row.unbonding,
                    &format!("{path}.unbonding"),
                    Some(0.0),
                )?,
                vault: decimal_to_number(&row.vault, &format!("{path}.vault"), Some(0.0))?,
                interest: decimal_to_number(&row.interest, &format!("{path}.interest"), None)?,
                earn: decimal_to_number(&row.earn, &format!("{path}.earn"), Some(0.0))?,
                pending_short_quantity: decimal_to_number(
                    &row.pending_short_qty,
                    &format!("{path}.pendingShortQty"),
                    Some(0.0),
                )?,
                pending_long_quantity: decimal_to_number(
                    &row.pending_long_qty,
                    &format!("{path}.pendingLongQty"),
                    Some(0.0),
                )?,
                average_open_price: decimal_to_number(
                    &row.average_open_price,
                    &format!("{path}.averageOpenPrice"),
                    Some(0.0),
                )?,
                mark_price: decimal_to_number(
                    &row.mark_price,
                    &format!("{path}.markPrice"),
                    Some(0.0),
                )?,
                pnl24_h: decimal_to_number(&row.pnl24_h, &format!("{path}.pnl24H"), None)?,
                fee24_h: decimal_to_number(&row.fee24_h, &format!("{path}.fee24H"), None)?,
                timestamp: row.timestamp,
            })
        })
        .collect()
}
// ts: decodeWooV3FuturesPositions
pub fn decode_woo_v3_futures_positions(
    input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3FuturesPosition>, String> {
    let data = decode_envelope(input, "futures positions")?;
    let has_rows = data.get("rows").is_some();
    let has_positions = data.get("positions").is_some();
    if has_rows == has_positions {
        return Err(
            "WOO V3 futures positions must contain exactly one of data.rows or data.positions"
                .into(),
        );
    }
    let key = if has_rows { "rows" } else { "positions" };
    require_array(
        data.get(key),
        &format!("WOO V3 futures positions.data.{key}"),
    )?
    .iter()
    .enumerate()
    .map(|(i, input)| {
        let path = format!("WOO V3 futures positions.data.{key}[{i}]");
        let row = require_record(Some(input), &path)?;
        Ok(WooV3FuturesPosition {
            symbol: require_string(row.get("symbol"), &format!("{path}.symbol"), false)?,
            holding: require_decimal_string(row.get("holding"), &format!("{path}.holding"))?,
            position_side: match require_enum(
                row.get("positionSide"),
                &["BOTH", "LONG", "SHORT"],
                &format!("{path}.positionSide"),
            )?
            .as_str()
            {
                "BOTH" => WooV3PositionSide::Both,
                "LONG" => WooV3PositionSide::Long,
                "SHORT" => WooV3PositionSide::Short,
                _ => unreachable!(),
            },
            pending_long_qty: require_decimal_string(
                row.get("pendingLongQty"),
                &format!("{path}.pendingLongQty"),
            )?,
            pending_short_qty: require_decimal_string(
                row.get("pendingShortQty"),
                &format!("{path}.pendingShortQty"),
            )?,
            settle_price: require_decimal_string(
                row.get("settlePrice"),
                &format!("{path}.settlePrice"),
            )?,
            average_open_price: require_decimal_string(
                row.get("averageOpenPrice"),
                &format!("{path}.averageOpenPrice"),
            )?,
            pnl24_h: require_decimal_string(row.get("pnl24H"), &format!("{path}.pnl24H"))?,
            fee24_h: require_decimal_string(row.get("fee24H"), &format!("{path}.fee24H"))?,
            mark_price: require_decimal_string(row.get("markPrice"), &format!("{path}.markPrice"))?,
            est_liq_price: require_decimal_string(
                row.get("estLiqPrice"),
                &format!("{path}.estLiqPrice"),
            )?,
            adl_quantile: require_integer(
                row.get("adlQuantile"),
                &format!("{path}.adlQuantile"),
                0.0,
            )?,
            timestamp: require_integer(row.get("timestamp"), &format!("{path}.timestamp"), 0.0)?,
        })
    })
    .collect()
}
// ts: mapWooV3FuturesPositions
pub fn map_woo_v3_futures_positions(
    rows: &[WooV3FuturesPosition],
    account_position_mode: WooV3AccountPositionMode,
) -> Result<Vec<WooV3PositionReadModel>, String> {
    let mut result = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let path = format!("WOO V3 futures position[{i}]");
        assert_position_side_matches_mode(
            row.position_side,
            account_position_mode,
            &format!("{path}.positionSide"),
        )?;
        let signed_holding = decimal_to_number(&row.holding, &format!("{path}.holding"), None)?;
        if signed_holding == 0.0 {
            continue;
        }
        let side = if account_position_mode == WooV3AccountPositionMode::HedgeMode {
            if row.position_side == WooV3PositionSide::Long {
                PositionSide::Long
            } else {
                PositionSide::Short
            }
        } else if signed_holding > 0.0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        if account_position_mode == WooV3AccountPositionMode::HedgeMode
            && ((side == PositionSide::Long && signed_holding < 0.0)
                || (side == PositionSide::Short && signed_holding > 0.0))
        {
            return Err(format!("{path}.holding sign conflicts with positionSide"));
        }
        let contracts = signed_holding.abs();
        let mark_price = decimal_to_number(
            &row.mark_price,
            &format!("{path}.markPrice"),
            Some(f64::from_bits(1)),
        )?;
        result.push(WooV3PositionReadModel {
            symbol: normalize_perpetual_symbol(&row.symbol, &format!("{path}.symbol"))?,
            side,
            position_mode: map_position_mode(account_position_mode),
            gateway_position_side: row.position_side,
            signed_holding,
            contracts,
            entry_price: decimal_to_number(
                &row.average_open_price,
                &format!("{path}.averageOpenPrice"),
                Some(0.0),
            )?,
            mark_price,
            notional: contracts * mark_price,
            estimated_liquidation_price: decimal_to_number(
                &row.est_liq_price,
                &format!("{path}.estLiqPrice"),
                Some(0.0),
            )?,
            pending_long_quantity: decimal_to_number(
                &row.pending_long_qty,
                &format!("{path}.pendingLongQty"),
                Some(0.0),
            )?,
            pending_short_quantity: decimal_to_number(
                &row.pending_short_qty,
                &format!("{path}.pendingShortQty"),
                Some(0.0),
            )?,
            pnl24_h: decimal_to_number(&row.pnl24_h, &format!("{path}.pnl24H"), None)?,
            fee24_h: decimal_to_number(&row.fee24_h, &format!("{path}.fee24H"), None)?,
            adl_quantile: row.adl_quantile,
            timestamp: row.timestamp,
        });
    }
    Ok(result)
}
