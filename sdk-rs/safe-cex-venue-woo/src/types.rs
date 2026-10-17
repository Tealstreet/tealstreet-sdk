//! Legacy WOO response declarations; these are not V3 migration decoders.
use serde::{Deserialize, Serialize};

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// ts: ACCOUNT_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct AccountResponse {
    pub application_id: String,
    pub account: String,
    pub alias: String,
    pub account_mode: String,
    pub position_mode: String,
    pub leverage: f64,
    pub taker_fee_rate: f64,
    pub maker_fee_rate: f64,
    pub interest_rate: f64,
    pub futures_taker_fee_rate: f64,
    pub futures_maker_fee_rate: f64,
    pub otpauth: bool,
    pub margin_ratio: f64,
    pub open_margin_ratio: f64,
    pub initial_margin_ratio: f64,
    pub maintenance_margin_ratio: f64,
    pub total_collateral: f64,
    pub free_collateral: f64,
    pub total_account_value: f64,
    pub total_vault_value: f64,
    pub total_staking_value: f64,
    pub referrer_id: String,
    pub account_type: String,
    pub total_launchpad_vault_value: f64,
    pub total_earn_value: f64,
}

// ts: PLACE_ORDER_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOrderResponse {
    pub success: bool,
    pub timestamp: String,
    pub order_id: f64,
    pub order_type: String,
    pub order_price: f64,
    pub order_quantity: f64,
    pub order_amount: f64,
    pub client_order_id: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PlaceAlgoOrderResponseDataRowsItemOrderId {
    Number(f64),
    String(String),
}

#[derive(Deserialize)]
#[serde(remote = "PlaceAlgoOrderResponseDataRowsItemOrderId", untagged)]
enum PlaceAlgoOrderResponseDataRowsItemOrderIdNumericWire {
    Number(f64),
    String(String),
}

impl<'de> Deserialize<'de> for PlaceAlgoOrderResponseDataRowsItemOrderId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(PlaceAlgoOrderResponseDataRowsItemOrderIdNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: PLACE_ALGO_ORDER_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceAlgoOrderResponseDataRowsItem {
    pub order_id: PlaceAlgoOrderResponseDataRowsItemOrderId,
}

// ts: PLACE_ALGO_ORDER_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceAlgoOrderResponseData {
    pub rows: Vec<PlaceAlgoOrderResponseDataRowsItem>,
}

// ts: PLACE_ALGO_ORDER_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceAlgoOrderResponse {
    pub success: bool,
    pub timestamp: f64,
    pub data: PlaceAlgoOrderResponseData,
}

// ts: GET_FUTURES_LEVERAGE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetFuturesLeverageResponseDataDetailsItem {
    pub position_side: String,
    pub leverage: String,
}

// ts: GET_FUTURES_LEVERAGE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetFuturesLeverageResponseData {
    pub symbol: String,
    pub margin_mode: String,
    pub default_margin_mode: Option<String>,
    pub position_mode: String,
    pub details: Vec<GetFuturesLeverageResponseDataDetailsItem>,
}

// ts: GET_FUTURES_LEVERAGE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetFuturesLeverageResponse {
    pub success: bool,
    pub data: GetFuturesLeverageResponseData,
    pub timestamp: f64,
}

// ts: GET_KLINE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetKlineResponseDataRowsItem {
    pub open: f64,
    pub close: f64,
    pub low: f64,
    pub high: f64,
    pub volume: f64,
    pub amount: f64,
    pub symbol: String,
    pub r#type: String,
    pub start_timestamp: f64,
    pub end_timestamp: f64,
}

// ts: GET_KLINE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetKlineResponseDataMeta {
    pub total: f64,
    pub records_per_page: f64,
    pub current_page: f64,
}

// ts: GET_KLINE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetKlineResponseData {
    pub rows: Vec<GetKlineResponseDataRowsItem>,
    pub meta: GetKlineResponseDataMeta,
}

// ts: GET_KLINE_RESPONSE
#[derive(Debug, Clone, PartialEq)]
pub struct GetKlineResponse {
    pub success: bool,
    pub data: GetKlineResponseData,
    pub timestamp: f64,
}

// Map-only record boundaries. Remote companions are private.

#[derive(Serialize, Deserialize)]
#[serde(remote = "AccountResponse")]
struct AccountResponseWire {
    #[serde(rename = "applicationId")]
    application_id: String,
    account: String,
    alias: String,
    #[serde(rename = "accountMode")]
    account_mode: String,
    #[serde(rename = "positionMode")]
    position_mode: String,
    leverage: f64,
    #[serde(rename = "takerFeeRate")]
    taker_fee_rate: f64,
    #[serde(rename = "makerFeeRate")]
    maker_fee_rate: f64,
    #[serde(rename = "interestRate")]
    interest_rate: f64,
    #[serde(rename = "futuresTakerFeeRate")]
    futures_taker_fee_rate: f64,
    #[serde(rename = "futuresMakerFeeRate")]
    futures_maker_fee_rate: f64,
    otpauth: bool,
    #[serde(rename = "marginRatio")]
    margin_ratio: f64,
    #[serde(rename = "openMarginRatio")]
    open_margin_ratio: f64,
    #[serde(rename = "initialMarginRatio")]
    initial_margin_ratio: f64,
    #[serde(rename = "maintenanceMarginRatio")]
    maintenance_margin_ratio: f64,
    #[serde(rename = "totalCollateral")]
    total_collateral: f64,
    #[serde(rename = "freeCollateral")]
    free_collateral: f64,
    #[serde(rename = "totalAccountValue")]
    total_account_value: f64,
    #[serde(rename = "totalVaultValue")]
    total_vault_value: f64,
    #[serde(rename = "totalStakingValue")]
    total_staking_value: f64,
    #[serde(rename = "referrerID")]
    referrer_id: String,
    #[serde(rename = "accountType")]
    account_type: String,
    #[serde(rename = "totalLaunchpadVaultValue")]
    total_launchpad_vault_value: f64,
    #[serde(rename = "totalEarnValue")]
    total_earn_value: f64,
}
impl Serialize for AccountResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AccountResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for AccountResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                AccountResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceOrderResponse")]
struct PlaceOrderResponseWire {
    success: bool,
    timestamp: String,
    order_id: f64,
    order_type: String,
    order_price: f64,
    order_quantity: f64,
    order_amount: f64,
    client_order_id: f64,
}
impl Serialize for PlaceOrderResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PlaceOrderResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PlaceOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                PlaceOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceAlgoOrderResponseDataRowsItem")]
struct PlaceAlgoOrderResponseDataRowsItemWire {
    #[serde(rename = "orderId")]
    order_id: PlaceAlgoOrderResponseDataRowsItemOrderId,
}
impl Serialize for PlaceAlgoOrderResponseDataRowsItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PlaceAlgoOrderResponseDataRowsItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PlaceAlgoOrderResponseDataRowsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                PlaceAlgoOrderResponseDataRowsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceAlgoOrderResponseData")]
struct PlaceAlgoOrderResponseDataWire {
    rows: Vec<PlaceAlgoOrderResponseDataRowsItem>,
}
impl Serialize for PlaceAlgoOrderResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PlaceAlgoOrderResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PlaceAlgoOrderResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                PlaceAlgoOrderResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PlaceAlgoOrderResponse")]
struct PlaceAlgoOrderResponseWire {
    success: bool,
    timestamp: f64,
    data: PlaceAlgoOrderResponseData,
}
impl Serialize for PlaceAlgoOrderResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PlaceAlgoOrderResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for PlaceAlgoOrderResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                PlaceAlgoOrderResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetFuturesLeverageResponseDataDetailsItem")]
struct GetFuturesLeverageResponseDataDetailsItemWire {
    position_side: String,
    leverage: String,
}
impl Serialize for GetFuturesLeverageResponseDataDetailsItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetFuturesLeverageResponseDataDetailsItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetFuturesLeverageResponseDataDetailsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetFuturesLeverageResponseDataDetailsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetFuturesLeverageResponseData")]
struct GetFuturesLeverageResponseDataWire {
    symbol: String,
    margin_mode: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    default_margin_mode: Option<String>,
    position_mode: String,
    details: Vec<GetFuturesLeverageResponseDataDetailsItem>,
}
impl Serialize for GetFuturesLeverageResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetFuturesLeverageResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetFuturesLeverageResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetFuturesLeverageResponseDataWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetFuturesLeverageResponse")]
struct GetFuturesLeverageResponseWire {
    success: bool,
    data: GetFuturesLeverageResponseData,
    timestamp: f64,
}
impl Serialize for GetFuturesLeverageResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetFuturesLeverageResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetFuturesLeverageResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetFuturesLeverageResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetKlineResponseDataRowsItem")]
struct GetKlineResponseDataRowsItemWire {
    open: f64,
    close: f64,
    low: f64,
    high: f64,
    volume: f64,
    amount: f64,
    symbol: String,
    r#type: String,
    start_timestamp: f64,
    end_timestamp: f64,
}
impl Serialize for GetKlineResponseDataRowsItem {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetKlineResponseDataRowsItemWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetKlineResponseDataRowsItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetKlineResponseDataRowsItemWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetKlineResponseDataMeta")]
struct GetKlineResponseDataMetaWire {
    total: f64,
    records_per_page: f64,
    current_page: f64,
}
impl Serialize for GetKlineResponseDataMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetKlineResponseDataMetaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetKlineResponseDataMeta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetKlineResponseDataMetaWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetKlineResponseData")]
struct GetKlineResponseDataWire {
    rows: Vec<GetKlineResponseDataRowsItem>,
    meta: GetKlineResponseDataMeta,
}
impl Serialize for GetKlineResponseData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetKlineResponseDataWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetKlineResponseData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetKlineResponseDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "GetKlineResponse")]
struct GetKlineResponseWire {
    success: bool,
    data: GetKlineResponseData,
    timestamp: f64,
}
impl Serialize for GetKlineResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        GetKlineResponseWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for GetKlineResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                GetKlineResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// Explicit registrations keep the complete output graph visible to parity indexing.
impl safe_cex_num::numeric_ingress::JsNumberOutput for AccountResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { application_id, account, alias, account_mode, position_mode, leverage, taker_fee_rate, maker_fee_rate, interest_rate, futures_taker_fee_rate, futures_maker_fee_rate, otpauth, margin_ratio, open_margin_ratio, initial_margin_ratio, maintenance_margin_ratio, total_collateral, free_collateral, total_account_value, total_vault_value, total_staking_value, referrer_id, account_type, total_launchpad_vault_value, total_earn_value });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlaceOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, timestamp, order_id, order_type, order_price, order_quantity, order_amount, client_order_id });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlaceAlgoOrderResponseDataRowsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlaceAlgoOrderResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { rows });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlaceAlgoOrderResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, timestamp, data });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetFuturesLeverageResponseDataDetailsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { position_side, leverage });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetFuturesLeverageResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, margin_mode, default_margin_mode, position_mode, details });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetFuturesLeverageResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, data, timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetKlineResponseDataRowsItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { open, close, low, high, volume, amount, symbol, r#type, start_timestamp, end_timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetKlineResponseDataMeta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { total, records_per_page, current_page });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetKlineResponseData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { rows, meta });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetKlineResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, data, timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PlaceAlgoOrderResponseDataRowsItemOrderId {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Number(value), String(value) });
    }
}
