//! Wire declarations mirrored from bitrue.types.ts. Deserializers capture numeric ingress before buffering.
use safe_cex_num::numeric_ingress::{JsNumberInput, JsNumberOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
/// Opaque array element: capture before Value construction, serialize transparently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueValue {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    pub value: Value,
}
impl JsNumberOutput for OpaqueValue {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { } passthrough { value });
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum BitrueOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TAKE_PROFIT_MARKET")]
    TakeProfitMarket,
}
#[derive(Deserialize)]
#[serde(remote = "BitrueOrderType")]
enum BitrueOrderTypeWire {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TAKE_PROFIT_MARKET")]
    TakeProfitMarket,
}
impl<'de> Deserialize<'de> for BitrueOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        BitrueOrderTypeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

impl JsNumberOutput for BitrueOrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market, StopMarket, TakeProfitMarket });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchAccountResponse {
    pub code: String,
    pub msg: String,
    pub data: FetchAccountResponseData,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchAccountResponse")]
struct FetchAccountResponseWire {
    #[serde(rename = "code")]
    code: String,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: FetchAccountResponseData,
}
impl Serialize for FetchAccountResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchAccountResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchAccountResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchAccountResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchAccountResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchAccountResponseData {
    pub account: Vec<FetchAccountResponseDataAccountItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchAccountResponseData")]
struct FetchAccountResponseDataWire {
    #[serde(rename = "account")]
    account: Vec<FetchAccountResponseDataAccountItem>,
}
impl Serialize for FetchAccountResponseData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchAccountResponseDataWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchAccountResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchAccountResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchAccountResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchAccountResponseDataAccountItem {
    pub margin_coin: String,
    pub coin_precious: f64,
    pub ex_usdt_rate: f64,
    pub ex_btc_rate: f64,
    pub account_normal: f64,
    pub account_lock: f64,
    pub part_position_normal: f64,
    pub total_position_normal: f64,
    pub achieved_amount: f64,
    pub unrealized_amount: f64,
    pub total_margin_rate: f64,
    pub total_equity: f64,
    pub part_equity: f64,
    pub total_cost: f64,
    pub sum_margin_rate: f64,
    pub sum_open_realized_amount: f64,
    pub can_use_trial_fund: f64,
    pub sum_maintenance_margin: f64,
    pub future_model: Value,
    pub position_vos: Option<Vec<FetchAccountResponseDataAccountItemPositionVosItem>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchAccountResponseDataAccountItem")]
struct FetchAccountResponseDataAccountItemWire {
    #[serde(rename = "marginCoin")]
    margin_coin: String,
    #[serde(rename = "coinPrecious")]
    coin_precious: f64,
    #[serde(rename = "exUsdtRate")]
    ex_usdt_rate: f64,
    #[serde(rename = "exBtcRate")]
    ex_btc_rate: f64,
    #[serde(rename = "accountNormal")]
    account_normal: f64,
    #[serde(rename = "accountLock")]
    account_lock: f64,
    #[serde(rename = "partPositionNormal")]
    part_position_normal: f64,
    #[serde(rename = "totalPositionNormal")]
    total_position_normal: f64,
    #[serde(rename = "achievedAmount")]
    achieved_amount: f64,
    #[serde(rename = "unrealizedAmount")]
    unrealized_amount: f64,
    #[serde(rename = "totalMarginRate")]
    total_margin_rate: f64,
    #[serde(rename = "totalEquity")]
    total_equity: f64,
    #[serde(rename = "partEquity")]
    part_equity: f64,
    #[serde(rename = "totalCost")]
    total_cost: f64,
    #[serde(rename = "sumMarginRate")]
    sum_margin_rate: f64,
    #[serde(rename = "sumOpenRealizedAmount")]
    sum_open_realized_amount: f64,
    #[serde(rename = "canUseTrialFund")]
    can_use_trial_fund: f64,
    #[serde(rename = "sumMaintenanceMargin")]
    sum_maintenance_margin: f64,
    #[serde(
        rename = "futureModel",
        deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough"
    )]
    future_model: Value,
    #[serde(
        rename = "positionVos",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    position_vos: Option<Vec<FetchAccountResponseDataAccountItemPositionVosItem>>,
}
impl Serialize for FetchAccountResponseDataAccountItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchAccountResponseDataAccountItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchAccountResponseDataAccountItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchAccountResponseDataAccountItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchAccountResponseDataAccountItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { margin_coin, coin_precious, ex_usdt_rate, ex_btc_rate, account_normal, account_lock, part_position_normal, total_position_normal, achieved_amount, unrealized_amount, total_margin_rate, total_equity, part_equity, total_cost, sum_margin_rate, sum_open_realized_amount, can_use_trial_fund, sum_maintenance_margin, position_vos } passthrough { future_model });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchAccountResponseDataAccountItemPositionVosItem {
    pub contract_id: f64,
    pub contract_name: String,
    pub contract_symbol: String,
    pub adl_enabled: bool,
    pub positions: Vec<FetchAccountResponseDataAccountItemPositionVosItemPositionsItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchAccountResponseDataAccountItemPositionVosItem")]
struct FetchAccountResponseDataAccountItemPositionVosItemWire {
    #[serde(rename = "contractId")]
    contract_id: f64,
    #[serde(rename = "contractName")]
    contract_name: String,
    #[serde(rename = "contractSymbol")]
    contract_symbol: String,
    #[serde(rename = "adlEnabled")]
    adl_enabled: bool,
    #[serde(rename = "positions")]
    positions: Vec<FetchAccountResponseDataAccountItemPositionVosItemPositionsItem>,
}
impl Serialize for FetchAccountResponseDataAccountItemPositionVosItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchAccountResponseDataAccountItemPositionVosItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchAccountResponseDataAccountItemPositionVosItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchAccountResponseDataAccountItemPositionVosItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchAccountResponseDataAccountItemPositionVosItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { contract_id, contract_name, contract_symbol, adl_enabled, positions });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchAccountResponseDataAccountItemPositionVosItemPositionsItem {
    pub id: f64,
    pub uid: f64,
    pub contract_id: f64,
    pub position_type: f64,
    pub side: String,
    pub volume: f64,
    pub open_price: f64,
    pub avg_price: f64,
    pub close_price: f64,
    pub leverage_level: f64,
    pub open_amount: f64,
    pub hold_amount: f64,
    pub close_volume: f64,
    pub pending_close_volume: f64,
    pub realized_amount: f64,
    pub history_realized_amount: f64,
    pub force_liquidation_volume: f64,
    pub force_liquidation_price: f64,
    pub trade_fee: f64,
    pub capital_fee: f64,
    pub close_profit: f64,
    pub share_amount: f64,
    pub freeze_lock: f64,
    pub status: f64,
    pub ctime: String,
    pub mtime: String,
    pub broker_id: f64,
    pub using_account_type: f64,
    pub pre_share_amount: f64,
    pub margin_rate: f64,
    pub reduce_price: f64,
    pub return_rate: f64,
    pub un_realized_amount: f64,
    pub open_realized_amount: f64,
    pub position_balance: f64,
    pub settle_profit: f64,
    pub keep_rate: f64,
    pub max_fee_rate: f64,
    pub version: f64,
    pub index_price: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchAccountResponseDataAccountItemPositionVosItemPositionsItem")]
struct FetchAccountResponseDataAccountItemPositionVosItemPositionsItemWire {
    #[serde(rename = "id")]
    id: f64,
    #[serde(rename = "uid")]
    uid: f64,
    #[serde(rename = "contractId")]
    contract_id: f64,
    #[serde(rename = "positionType")]
    position_type: f64,
    #[serde(rename = "side")]
    side: String,
    #[serde(rename = "volume")]
    volume: f64,
    #[serde(rename = "openPrice")]
    open_price: f64,
    #[serde(rename = "avgPrice")]
    avg_price: f64,
    #[serde(rename = "closePrice")]
    close_price: f64,
    #[serde(rename = "leverageLevel")]
    leverage_level: f64,
    #[serde(rename = "openAmount")]
    open_amount: f64,
    #[serde(rename = "holdAmount")]
    hold_amount: f64,
    #[serde(rename = "closeVolume")]
    close_volume: f64,
    #[serde(rename = "pendingCloseVolume")]
    pending_close_volume: f64,
    #[serde(rename = "realizedAmount")]
    realized_amount: f64,
    #[serde(rename = "historyRealizedAmount")]
    history_realized_amount: f64,
    #[serde(rename = "forceLiquidationVolume")]
    force_liquidation_volume: f64,
    #[serde(rename = "forceLiquidationPrice")]
    force_liquidation_price: f64,
    #[serde(rename = "tradeFee")]
    trade_fee: f64,
    #[serde(rename = "capitalFee")]
    capital_fee: f64,
    #[serde(rename = "closeProfit")]
    close_profit: f64,
    #[serde(rename = "shareAmount")]
    share_amount: f64,
    #[serde(rename = "freezeLock")]
    freeze_lock: f64,
    #[serde(rename = "status")]
    status: f64,
    #[serde(rename = "ctime")]
    ctime: String,
    #[serde(rename = "mtime")]
    mtime: String,
    #[serde(rename = "brokerId")]
    broker_id: f64,
    #[serde(rename = "usingAccountType")]
    using_account_type: f64,
    #[serde(rename = "preShareAmount")]
    pre_share_amount: f64,
    #[serde(rename = "marginRate")]
    margin_rate: f64,
    #[serde(rename = "reducePrice")]
    reduce_price: f64,
    #[serde(rename = "returnRate")]
    return_rate: f64,
    #[serde(rename = "unRealizedAmount")]
    un_realized_amount: f64,
    #[serde(rename = "openRealizedAmount")]
    open_realized_amount: f64,
    #[serde(rename = "positionBalance")]
    position_balance: f64,
    #[serde(rename = "settleProfit")]
    settle_profit: f64,
    #[serde(rename = "keepRate")]
    keep_rate: f64,
    #[serde(rename = "maxFeeRate")]
    max_fee_rate: f64,
    #[serde(rename = "version")]
    version: f64,
    #[serde(rename = "indexPrice")]
    index_price: f64,
}
impl Serialize for FetchAccountResponseDataAccountItemPositionVosItemPositionsItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchAccountResponseDataAccountItemPositionVosItemPositionsItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchAccountResponseDataAccountItemPositionVosItemPositionsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchAccountResponseDataAccountItemPositionVosItemPositionsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchAccountResponseDataAccountItemPositionVosItemPositionsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, uid, contract_id, position_type, side, volume, open_price, avg_price, close_price, leverage_level, open_amount, hold_amount, close_volume, pending_close_volume, realized_amount, history_realized_amount, force_liquidation_volume, force_liquidation_price, trade_fee, capital_fee, close_profit, share_amount, freeze_lock, status, ctime, mtime, broker_id, using_account_type, pre_share_amount, margin_rate, reduce_price, return_rate, un_realized_amount, open_realized_amount, position_balance, settle_profit, keep_rate, max_fee_rate, version, index_price });
    }
}

pub type FetchContractsResponse = Vec<FetchContractsResponseItem>;

#[derive(Debug, Clone, PartialEq)]
pub struct FetchContractsResponseItem {
    pub symbol: String,
    pub price_precision: f64,
    pub side: f64,
    pub max_market_volume: f64,
    pub multiplier: f64,
    pub min_order_volume: f64,
    pub max_market_money: f64,
    pub r#type: String,
    pub max_limit_volume: f64,
    pub max_valid_order: f64,
    pub multiplier_coin: String,
    pub min_order_money: f64,
    pub max_limit_money: f64,
    pub status: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchContractsResponseItem")]
struct FetchContractsResponseItemWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "pricePrecision")]
    price_precision: f64,
    #[serde(rename = "side")]
    side: f64,
    #[serde(rename = "maxMarketVolume")]
    max_market_volume: f64,
    #[serde(rename = "multiplier")]
    multiplier: f64,
    #[serde(rename = "minOrderVolume")]
    min_order_volume: f64,
    #[serde(rename = "maxMarketMoney")]
    max_market_money: f64,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(rename = "maxLimitVolume")]
    max_limit_volume: f64,
    #[serde(rename = "maxValidOrder")]
    max_valid_order: f64,
    #[serde(rename = "multiplierCoin")]
    multiplier_coin: String,
    #[serde(rename = "minOrderMoney")]
    min_order_money: f64,
    #[serde(rename = "maxLimitMoney")]
    max_limit_money: f64,
    #[serde(rename = "status")]
    status: f64,
}
impl Serialize for FetchContractsResponseItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchContractsResponseItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchContractsResponseItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchContractsResponseItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchContractsResponseItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, price_precision, side, max_market_volume, multiplier, min_order_volume, max_market_money, r#type, max_limit_volume, max_valid_order, multiplier_coin, min_order_money, max_limit_money, status });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchLeverageBracketResponse {
    pub code: NumberOrString,
    pub msg: String,
    pub data: Option<FetchLeverageBracketResponseData>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchLeverageBracketResponse")]
struct FetchLeverageBracketResponseWire {
    #[serde(rename = "code")]
    code: NumberOrString,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(
        rename = "data",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    data: Option<FetchLeverageBracketResponseData>,
}
impl Serialize for FetchLeverageBracketResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchLeverageBracketResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchLeverageBracketResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchLeverageBracketResponseWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchLeverageBracketResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(f64),
    String(String),
}
#[derive(Deserialize)]
#[serde(remote = "NumberOrString", untagged)]
enum NumberOrStringWire {
    Number(f64),
    String(String),
}
impl<'de> Deserialize<'de> for NumberOrString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(NumberOrStringWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for NumberOrString {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Number(value), String(value) });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FetchLeverageBracketResponseData {
    Object(BitrueLeverageBracketDataItem),
    Array(Vec<BitrueLeverageBracketDataItem>),
}
#[derive(Deserialize)]
#[serde(remote = "FetchLeverageBracketResponseData", untagged)]
enum FetchLeverageBracketResponseDataWire {
    Object(BitrueLeverageBracketDataItem),
    Array(Vec<BitrueLeverageBracketDataItem>),
}
impl<'de> Deserialize<'de> for FetchLeverageBracketResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(FetchLeverageBracketResponseDataWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchLeverageBracketResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Object(value), Array(value) });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitrueLeverageBracketDataItem {
    pub contract_name: Option<String>,
    pub brackets: Option<Vec<BitrueLeverageBracket>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BitrueLeverageBracketDataItem")]
struct BitrueLeverageBracketDataItemWire {
    #[serde(
        rename = "contractName",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    contract_name: Option<String>,
    #[serde(
        rename = "brackets",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    brackets: Option<Vec<BitrueLeverageBracket>>,
}
impl Serialize for BitrueLeverageBracketDataItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BitrueLeverageBracketDataItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BitrueLeverageBracketDataItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                BitrueLeverageBracketDataItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BitrueLeverageBracketDataItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { contract_name, brackets });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitrueLeverageBracket {
    pub bracket: f64,
    pub initial_leverage: NumberOrString,
    pub max_position_value: NumberOrString,
    pub min_position_value: NumberOrString,
    pub maint_margin_ratio: NumberOrString,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "BitrueLeverageBracket")]
struct BitrueLeverageBracketWire {
    #[serde(rename = "bracket")]
    bracket: f64,
    #[serde(rename = "initialLeverage")]
    initial_leverage: NumberOrString,
    #[serde(rename = "maxPositionValue")]
    max_position_value: NumberOrString,
    #[serde(rename = "minPositionValue")]
    min_position_value: NumberOrString,
    #[serde(rename = "maintMarginRatio")]
    maint_margin_ratio: NumberOrString,
}
impl Serialize for BitrueLeverageBracket {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BitrueLeverageBracketWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for BitrueLeverageBracket {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                BitrueLeverageBracketWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for BitrueLeverageBracket {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { bracket, initial_leverage, max_position_value, min_position_value, maint_margin_ratio });
    }
}

pub type FetchKlineResponse = Vec<FetchKlineResponseItem>;

#[derive(Debug, Clone, PartialEq)]
pub struct FetchKlineResponseItem {
    pub high: String,
    pub vol: String,
    pub last: String,
    pub low: String,
    pub rose: String,
    pub time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchKlineResponseItem")]
struct FetchKlineResponseItemWire {
    #[serde(rename = "high")]
    high: String,
    #[serde(rename = "vol")]
    vol: String,
    #[serde(rename = "last")]
    last: String,
    #[serde(rename = "low")]
    low: String,
    #[serde(rename = "rose")]
    rose: String,
    #[serde(rename = "time")]
    time: f64,
}
impl Serialize for FetchKlineResponseItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchKlineResponseItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchKlineResponseItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchKlineResponseItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchKlineResponseItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { high, vol, last, low, rose, time });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBookWsUpdate {
    pub asks: Vec<(f64, f64)>,
    pub buys: Vec<(f64, f64)>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderBookWsUpdate")]
struct OrderBookWsUpdateWire {
    #[serde(rename = "asks")]
    asks: Vec<(f64, f64)>,
    #[serde(rename = "buys")]
    buys: Vec<(f64, f64)>,
}
impl Serialize for OrderBookWsUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        OrderBookWsUpdateWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for OrderBookWsUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                OrderBookWsUpdateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for OrderBookWsUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { asks, buys });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickerWsUpdate {
    pub amount: String,
    pub close: String,
    pub data: Vec<OpaqueValue>,
    pub high: String,
    pub low: String,
    pub open: String,
    pub rose: String,
    pub vol: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TickerWsUpdate")]
struct TickerWsUpdateWire {
    #[serde(rename = "amount")]
    amount: String,
    #[serde(rename = "close")]
    close: String,
    #[serde(rename = "data")]
    data: Vec<OpaqueValue>,
    #[serde(rename = "high")]
    high: String,
    #[serde(rename = "low")]
    low: String,
    #[serde(rename = "open")]
    open: String,
    #[serde(rename = "rose")]
    rose: String,
    #[serde(rename = "vol")]
    vol: String,
}
impl Serialize for TickerWsUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TickerWsUpdateWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for TickerWsUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                TickerWsUpdateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for TickerWsUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { amount, close, data, high, low, open, rose, vol });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchOrdersResponse {
    pub code: String,
    pub msg: String,
    pub data: Vec<FetchOrdersResponseDataItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchOrdersResponse")]
struct FetchOrdersResponseWire {
    #[serde(rename = "code")]
    code: String,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: Vec<FetchOrdersResponseDataItem>,
}
impl Serialize for FetchOrdersResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchOrdersResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchOrdersResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchOrdersResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchOrdersResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchOrdersResponseDataItem {
    pub trigger_price: f64,
    pub trigger_price_type: Option<FetchOrdersResponseDataItemTriggerPriceType>,
    pub trigger_type: Option<FetchOrdersResponseDataItemTriggerType>,
    pub order_id: f64,
    pub client_order_id: String,
    pub price: f64,
    pub orig_qty: f64,
    pub orig_amount: f64,
    pub executed_qty: f64,
    pub avg_price: f64,
    pub status: String,
    pub r#type: Option<BitrueOrderType>,
    pub side: FetchOrdersResponseDataItemSide,
    pub action: String,
    pub transact_time: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchOrdersResponseDataItem")]
struct FetchOrdersResponseDataItemWire {
    #[serde(rename = "triggerPrice")]
    trigger_price: f64,
    #[serde(
        rename = "triggerPriceType",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    trigger_price_type: Option<FetchOrdersResponseDataItemTriggerPriceType>,
    #[serde(
        rename = "triggerType",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    trigger_type: Option<FetchOrdersResponseDataItemTriggerType>,
    #[serde(rename = "orderId")]
    order_id: f64,
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    #[serde(rename = "price")]
    price: f64,
    #[serde(rename = "origQty")]
    orig_qty: f64,
    #[serde(rename = "origAmount")]
    orig_amount: f64,
    #[serde(rename = "executedQty")]
    executed_qty: f64,
    #[serde(rename = "avgPrice")]
    avg_price: f64,
    #[serde(rename = "status")]
    status: String,
    #[serde(
        rename = "type",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    r#type: Option<BitrueOrderType>,
    #[serde(rename = "side")]
    side: FetchOrdersResponseDataItemSide,
    #[serde(rename = "action")]
    action: String,
    #[serde(rename = "transactTime")]
    transact_time: f64,
}
impl Serialize for FetchOrdersResponseDataItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchOrdersResponseDataItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchOrdersResponseDataItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchOrdersResponseDataItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchOrdersResponseDataItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { trigger_price, trigger_price_type, trigger_type, order_id, client_order_id, price, orig_qty, orig_amount, executed_qty, avg_price, status, r#type, side, action, transact_time });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub enum FetchOrdersResponseDataItemTriggerPriceType {
    V1,
    V2,
}
impl TryFrom<f64> for FetchOrdersResponseDataItemTriggerPriceType {
    type Error = String;
    fn try_from(value: f64) -> Result<Self, String> {
        if value == 1.0 {
            return Ok(Self::V1);
        }
        if value == 2.0 {
            return Ok(Self::V2);
        }
        Err(format!(
            "invalid FetchOrdersResponseDataItemTriggerPriceType: {value}"
        ))
    }
}
impl From<FetchOrdersResponseDataItemTriggerPriceType> for f64 {
    fn from(value: FetchOrdersResponseDataItemTriggerPriceType) -> f64 {
        match value {
            FetchOrdersResponseDataItemTriggerPriceType::V1 => 1.0,
            FetchOrdersResponseDataItemTriggerPriceType::V2 => 2.0,
        }
    }
}
impl JsNumberOutput for FetchOrdersResponseDataItemTriggerPriceType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { V1, V2 });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub enum FetchOrdersResponseDataItemTriggerType {
    V1,
    V2,
}
impl TryFrom<f64> for FetchOrdersResponseDataItemTriggerType {
    type Error = String;
    fn try_from(value: f64) -> Result<Self, String> {
        if value == 1.0 {
            return Ok(Self::V1);
        }
        if value == 2.0 {
            return Ok(Self::V2);
        }
        Err(format!(
            "invalid FetchOrdersResponseDataItemTriggerType: {value}"
        ))
    }
}
impl From<FetchOrdersResponseDataItemTriggerType> for f64 {
    fn from(value: FetchOrdersResponseDataItemTriggerType) -> f64 {
        match value {
            FetchOrdersResponseDataItemTriggerType::V1 => 1.0,
            FetchOrdersResponseDataItemTriggerType::V2 => 2.0,
        }
    }
}
impl JsNumberOutput for FetchOrdersResponseDataItemTriggerType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { V1, V2 });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum FetchOrdersResponseDataItemSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
#[derive(Deserialize)]
#[serde(remote = "FetchOrdersResponseDataItemSide")]
enum FetchOrdersResponseDataItemSideWire {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> Deserialize<'de> for FetchOrdersResponseDataItemSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        FetchOrdersResponseDataItemSideWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

impl JsNumberOutput for FetchOrdersResponseDataItemSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchMyTradesResponse {
    pub code: NumberOrString,
    pub msg: String,
    pub data: Vec<FetchMyTradesResponseDataItem>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchMyTradesResponse")]
struct FetchMyTradesResponseWire {
    #[serde(rename = "code")]
    code: NumberOrString,
    #[serde(rename = "msg")]
    msg: String,
    #[serde(rename = "data")]
    data: Vec<FetchMyTradesResponseDataItem>,
}
impl Serialize for FetchMyTradesResponse {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchMyTradesResponseWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchMyTradesResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchMyTradesResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchMyTradesResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, data });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchMyTradesResponseDataItem {
    pub trade_id: NumberOrString,
    pub price: NumberOrString,
    pub qty: NumberOrString,
    pub amount: NumberOrString,
    pub contract_name: String,
    pub side: String,
    pub fee: Option<NumberOrString>,
    pub bid_id: Option<NumberOrString>,
    pub ask_id: Option<NumberOrString>,
    pub bid_user_id: Option<NumberOrString>,
    pub ask_user_id: Option<NumberOrString>,
    pub is_buyer: Option<bool>,
    pub is_maker: Option<bool>,
    pub ctime: Option<NumberOrString>,
    pub c_time: Option<NumberOrString>,
    pub time: Option<NumberOrString>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "FetchMyTradesResponseDataItem")]
struct FetchMyTradesResponseDataItemWire {
    #[serde(rename = "tradeId")]
    trade_id: NumberOrString,
    #[serde(rename = "price")]
    price: NumberOrString,
    #[serde(rename = "qty")]
    qty: NumberOrString,
    #[serde(rename = "amount")]
    amount: NumberOrString,
    #[serde(rename = "contractName")]
    contract_name: String,
    #[serde(rename = "side")]
    side: String,
    #[serde(
        rename = "fee",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    fee: Option<NumberOrString>,
    #[serde(
        rename = "bidId",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    bid_id: Option<NumberOrString>,
    #[serde(
        rename = "askId",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    ask_id: Option<NumberOrString>,
    #[serde(
        rename = "bidUserId",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    bid_user_id: Option<NumberOrString>,
    #[serde(
        rename = "askUserId",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    ask_user_id: Option<NumberOrString>,
    #[serde(
        rename = "isBuyer",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    is_buyer: Option<bool>,
    #[serde(
        rename = "isMaker",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    is_maker: Option<bool>,
    #[serde(
        rename = "ctime",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    ctime: Option<NumberOrString>,
    #[serde(
        rename = "cTime",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    c_time: Option<NumberOrString>,
    #[serde(
        rename = "time",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    time: Option<NumberOrString>,
}
impl Serialize for FetchMyTradesResponseDataItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FetchMyTradesResponseDataItemWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for FetchMyTradesResponseDataItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                FetchMyTradesResponseDataItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for FetchMyTradesResponseDataItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { trade_id, price, qty, amount, contract_name, side, fee, bid_id, ask_id, bid_user_id, ask_user_id, is_buyer, is_maker, ctime, c_time, time });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsOrderTradeUpdate {
    pub e_upper: f64,
    pub e: String,
    pub o: WsOrderTradeUpdateO,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsOrderTradeUpdate")]
struct WsOrderTradeUpdateWire {
    #[serde(rename = "E")]
    e_upper: f64,
    #[serde(rename = "e")]
    e: String,
    #[serde(rename = "o")]
    o: WsOrderTradeUpdateO,
}
impl Serialize for WsOrderTradeUpdate {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WsOrderTradeUpdateWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WsOrderTradeUpdate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WsOrderTradeUpdateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WsOrderTradeUpdate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { e_upper, e, o });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsOrderTradeUpdateO {
    pub l_upper: String,
    pub n_upper: String,
    pub r_upper: bool,
    pub s_upper: String,
    pub t_upper: f64,
    pub x_upper: String,
    pub ap: String,
    pub c: String,
    pub i: NumberOrString,
    pub l: String,
    pub volume: Option<f64>,
    pub m: bool,
    pub n: String,
    pub o: BitrueOrderType,
    pub p: String,
    pub q: String,
    pub s: String,
    pub t: f64,
    pub x: String,
    pub z: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WsOrderTradeUpdateO")]
struct WsOrderTradeUpdateOWire {
    #[serde(rename = "L")]
    l_upper: String,
    #[serde(rename = "N")]
    n_upper: String,
    #[serde(rename = "R")]
    r_upper: bool,
    #[serde(rename = "S")]
    s_upper: String,
    #[serde(rename = "T")]
    t_upper: f64,
    #[serde(rename = "X")]
    x_upper: String,
    #[serde(rename = "ap")]
    ap: String,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "i")]
    i: NumberOrString,
    #[serde(rename = "l")]
    l: String,
    #[serde(
        rename = "volume",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    volume: Option<f64>,
    #[serde(rename = "m")]
    m: bool,
    #[serde(rename = "n")]
    n: String,
    #[serde(rename = "o")]
    o: BitrueOrderType,
    #[serde(rename = "p")]
    p: String,
    #[serde(rename = "q")]
    q: String,
    #[serde(rename = "s")]
    s: String,
    #[serde(rename = "t")]
    t: f64,
    #[serde(rename = "x")]
    x: String,
    #[serde(rename = "z")]
    z: String,
}
impl Serialize for WsOrderTradeUpdateO {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WsOrderTradeUpdateOWire::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WsOrderTradeUpdateO {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WsOrderTradeUpdateOWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for WsOrderTradeUpdateO {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { l_upper, n_upper, r_upper, s_upper, t_upper, x_upper, ap, c, i, l, volume, m, n, o, p, q, s, t, x, z });
    }
}
