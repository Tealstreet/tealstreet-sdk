//! SDK response records preserve source field domains and optional presence.
use crate::sdk_protocol_types::ResultCode;
use serde::{Deserialize, Serialize};
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

// ts: Result
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Result {
    pub code: ResultCode,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(remote = "Result", rename_all = "camelCase")]
struct ResultWire {
    code: ResultCode,
    message: String,
}
impl<'de> Deserialize<'de> for Result {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ResultWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Result {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, message });
    }
}

// ts: NextNonce
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextNonce {
    pub account: String,
    pub nonce: f64,
    pub pending_nonce: f64,
    pub confirmed_nonce: f64,
}
#[derive(Deserialize)]
#[serde(remote = "NextNonce", rename_all = "camelCase")]
struct NextNonceWire {
    account: String,
    nonce: f64,
    pending_nonce: f64,
    confirmed_nonce: f64,
}
impl<'de> Deserialize<'de> for NextNonce {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                NextNonceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for NextNonce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, nonce, pending_nonce, confirmed_nonce });
    }
}

// ts: ApiKeyInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyInfo {
    pub index: f64,
    pub public_key: String,
    pub permissions: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub is_active: bool,
    pub created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "ApiKeyInfo", rename_all = "camelCase")]
struct ApiKeyInfoWire {
    index: f64,
    public_key: String,
    permissions: f64,
    #[serde(default, deserialize_with = "present")]
    label: Option<String>,
    is_active: bool,
    created_at: f64,
    #[serde(default, deserialize_with = "present")]
    last_used_at: Option<f64>,
}
impl<'de> Deserialize<'de> for ApiKeyInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ApiKeyInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ApiKeyInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { index, public_key, permissions, label, is_active, created_at, last_used_at });
    }
}

// ts: TxHash
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxHash {
    pub tx_hash: String,
    pub nonce: f64,
    pub submitted_at: f64,
}
#[derive(Deserialize)]
#[serde(remote = "TxHash", rename_all = "camelCase")]
struct TxHashWire {
    tx_hash: String,
    nonce: f64,
    submitted_at: f64,
}
impl<'de> Deserialize<'de> for TxHash {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TxHashWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TxHash {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tx_hash, nonce, submitted_at });
    }
}

// ts: ServerTime
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTime {
    pub server_time: f64,
    pub timezone: String,
}
#[derive(Deserialize)]
#[serde(remote = "ServerTime", rename_all = "camelCase")]
struct ServerTimeWire {
    server_time: f64,
    timezone: String,
}
impl<'de> Deserialize<'de> for ServerTime {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ServerTimeWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ServerTime {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { server_time, timezone });
    }
}

// ts: RateLimit
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub r#type: String,
    pub interval: f64,
    pub limit: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "RateLimit", rename_all = "camelCase")]
struct RateLimitWire {
    r#type: String,
    interval: f64,
    limit: f64,
    #[serde(default, deserialize_with = "present")]
    current: Option<f64>,
}
impl<'de> Deserialize<'de> for RateLimit {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                RateLimitWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for RateLimit {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type, interval, limit, current });
    }
}

// ts: DepositAddress
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositAddress {
    pub token: String,
    pub chain: String,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub min_deposit: String,
    pub confirmations: f64,
}
#[derive(Deserialize)]
#[serde(remote = "DepositAddress", rename_all = "camelCase")]
struct DepositAddressWire {
    token: String,
    chain: String,
    address: String,
    #[serde(default, deserialize_with = "present")]
    memo: Option<String>,
    min_deposit: String,
    confirmations: f64,
}
impl<'de> Deserialize<'de> for DepositAddress {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                DepositAddressWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for DepositAddress {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { token, chain, address, memo, min_deposit, confirmations });
    }
}

// ts: TransferHistoryItem
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferHistoryItem {
    pub transfer_id: String,
    pub token: String,
    pub amount: String,
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    pub tx_hash: String,
    pub created_at: f64,
}
#[derive(Deserialize)]
#[serde(remote = "TransferHistoryItem", rename_all = "camelCase")]
struct TransferHistoryItemWire {
    transfer_id: String,
    token: String,
    amount: String,
    from: String,
    to: String,
    #[serde(default, deserialize_with = "present")]
    memo: Option<String>,
    tx_hash: String,
    created_at: f64,
}
impl<'de> Deserialize<'de> for TransferHistoryItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                TransferHistoryItemWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for TransferHistoryItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { transfer_id, token, amount, from, to, memo, tx_hash, created_at });
    }
}

// ts: MaintenanceWindow
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceWindow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub start_time: f64,
    pub end_time: f64,
    pub affected_services: Vec<String>,
}
#[derive(Deserialize)]
#[serde(remote = "MaintenanceWindow", rename_all = "camelCase")]
struct MaintenanceWindowWire {
    id: String,
    title: String,
    description: String,
    start_time: f64,
    end_time: f64,
    affected_services: Vec<String>,
}
impl<'de> Deserialize<'de> for MaintenanceWindow {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                MaintenanceWindowWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for MaintenanceWindow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, title, description, start_time, end_time, affected_services });
    }
}

// ts: FeeTier
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeTier {
    pub tier: f64,
    pub min_volume: String,
    pub fee_rate: String,
}
#[derive(Deserialize)]
#[serde(remote = "FeeTier", rename_all = "camelCase")]
struct FeeTierWire {
    tier: f64,
    min_volume: String,
    fee_rate: String,
}
impl<'de> Deserialize<'de> for FeeTier {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                FeeTierWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FeeTier {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { tier, min_volume, fee_rate });
    }
}

// ts: FundingRate
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    pub symbol: String,
    pub funding_rate: String,
    pub next_funding_time: f64,
    pub funding_interval: f64,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "FundingRate", rename_all = "camelCase")]
struct FundingRateWire {
    symbol: String,
    funding_rate: String,
    next_funding_time: f64,
    funding_interval: f64,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for FundingRate {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                FundingRateWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FundingRate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, funding_rate, next_funding_time, funding_interval, timestamp });
    }
}

// ts: AccountApiKeys
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountApiKeys {
    pub account: String,
    pub api_keys: Vec<ApiKeyInfo>,
}
#[derive(Deserialize)]
#[serde(remote = "AccountApiKeys", rename_all = "camelCase")]
struct AccountApiKeysWire {
    account: String,
    api_keys: Vec<ApiKeyInfo>,
}
impl<'de> Deserialize<'de> for AccountApiKeys {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                AccountApiKeysWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AccountApiKeys {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, api_keys });
    }
}

// ts: ExchangeInfo
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInfo {
    pub name: String,
    pub version: String,
    pub markets: Vec<String>,
    pub tokens: Vec<String>,
    pub rate_limits: Vec<RateLimit>,
    pub server_time: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ExchangeInfo", rename_all = "camelCase")]
struct ExchangeInfoWire {
    name: String,
    version: String,
    markets: Vec<String>,
    tokens: Vec<String>,
    rate_limits: Vec<RateLimit>,
    server_time: f64,
}
impl<'de> Deserialize<'de> for ExchangeInfo {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ExchangeInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ExchangeInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, version, markets, tokens, rate_limits, server_time });
    }
}

// ts: ErrorResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub code: ResultCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_errors: Option<safe_cex_types::object_map::JsObjectMap<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub timestamp: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ErrorResponse", rename_all = "camelCase")]
struct ErrorResponseWire {
    code: ResultCode,
    message: String,
    #[serde(default, deserialize_with = "present")]
    details: Option<String>,
    #[serde(default, deserialize_with = "present")]
    field_errors: Option<safe_cex_types::object_map::JsObjectMap<String>>,
    #[serde(default, deserialize_with = "present")]
    request_id: Option<String>,
    timestamp: f64,
}
impl<'de> Deserialize<'de> for ErrorResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ErrorResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ErrorResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, message, details, field_errors, request_id, timestamp });
    }
}

// ts: FeeSchedule
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeSchedule {
    pub maker_tiers: Vec<FeeTier>,
    pub taker_tiers: Vec<FeeTier>,
    pub withdrawal_fees: safe_cex_types::object_map::JsObjectMap<String>,
    pub deposit_fees: safe_cex_types::object_map::JsObjectMap<String>,
}
#[derive(Deserialize)]
#[serde(remote = "FeeSchedule", rename_all = "camelCase")]
struct FeeScheduleWire {
    maker_tiers: Vec<FeeTier>,
    taker_tiers: Vec<FeeTier>,
    withdrawal_fees: safe_cex_types::object_map::JsObjectMap<String>,
    deposit_fees: safe_cex_types::object_map::JsObjectMap<String>,
}
impl<'de> Deserialize<'de> for FeeSchedule {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                FeeScheduleWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for FeeSchedule {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { maker_tiers, taker_tiers, withdrawal_fees, deposit_fees });
    }
}

// ts: InsuranceFund
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsuranceFund {
    pub total_size: String,
    pub balances: safe_cex_types::object_map::JsObjectMap<String>,
    pub last_update: f64,
}
#[derive(Deserialize)]
#[serde(remote = "InsuranceFund", rename_all = "camelCase")]
struct InsuranceFundWire {
    total_size: String,
    balances: safe_cex_types::object_map::JsObjectMap<String>,
    last_update: f64,
}
impl<'de> Deserialize<'de> for InsuranceFund {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                InsuranceFundWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for InsuranceFund {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { total_size, balances, last_update });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}
#[derive(Deserialize)]
#[serde(remote = "HealthStatus", rename_all = "lowercase")]
enum HealthStatusWire {
    Healthy,
    Degraded,
    Unhealthy,
}
impl<'de> Deserialize<'de> for HealthStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        HealthStatusWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for HealthStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Healthy, Degraded, Unhealthy });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}
#[derive(Deserialize)]
#[serde(remote = "ConnectionStatus", rename_all = "lowercase")]
enum ConnectionStatusWire {
    Connected,
    Disconnected,
}
impl<'de> Deserialize<'de> for ConnectionStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        ConnectionStatusWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ConnectionStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Connected, Disconnected });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentState {
    Operational,
    Degraded,
    Down,
}
#[derive(Deserialize)]
#[serde(remote = "ComponentState", rename_all = "lowercase")]
enum ComponentStateWire {
    Operational,
    Degraded,
    Down,
}
impl<'de> Deserialize<'de> for ComponentState {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        ComponentStateWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ComponentState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Operational, Degraded, Down });
    }
}

// ts: HealthResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub timestamp: f64,
    pub uptime: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<ConnectionStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub websocket: Option<ConnectionStatus>,
}
#[derive(Deserialize)]
#[serde(remote = "HealthResponse", rename_all = "camelCase")]
struct HealthResponseWire {
    status: HealthStatus,
    version: String,
    timestamp: f64,
    uptime: f64,
    #[serde(default, deserialize_with = "present")]
    database: Option<ConnectionStatus>,
    #[serde(default, deserialize_with = "present")]
    websocket: Option<ConnectionStatus>,
}
impl<'de> Deserialize<'de> for HealthResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                HealthResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for HealthResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { status, version, timestamp, uptime, database, websocket });
    }
}

// ts: ComponentStatus
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub name: String,
    pub status: ComponentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub last_check: f64,
}
#[derive(Deserialize)]
#[serde(remote = "ComponentStatus", rename_all = "camelCase")]
struct ComponentStatusWire {
    name: String,
    status: ComponentState,
    #[serde(default, deserialize_with = "present")]
    message: Option<String>,
    last_check: f64,
}
impl<'de> Deserialize<'de> for ComponentStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                ComponentStatusWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for ComponentStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { name, status, message, last_check });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KycStatus {
    None,
    Pending,
    Verified,
    Rejected,
}
#[derive(Deserialize)]
#[serde(remote = "KycStatus", rename_all = "lowercase")]
enum KycStatusWire {
    None,
    Pending,
    Verified,
    Rejected,
}
impl<'de> Deserialize<'de> for KycStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        KycStatusWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KycStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { None, Pending, Verified, Rejected });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WithdrawalState {
    Pending,
    Processing,
    Completed,
    Failed,
}
#[derive(Deserialize)]
#[serde(remote = "WithdrawalState", rename_all = "lowercase")]
enum WithdrawalStateWire {
    Pending,
    Processing,
    Completed,
    Failed,
}
impl<'de> Deserialize<'de> for WithdrawalState {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        WithdrawalStateWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WithdrawalState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Pending, Processing, Completed, Failed });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DepositState {
    Pending,
    Confirming,
    Completed,
    Failed,
}
#[derive(Deserialize)]
#[serde(remote = "DepositState", rename_all = "lowercase")]
enum DepositStateWire {
    Pending,
    Confirming,
    Completed,
    Failed,
}
impl<'de> Deserialize<'de> for DepositState {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        DepositStateWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for DepositState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Pending, Confirming, Completed, Failed });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemState {
    Operational,
    Degraded,
    Maintenance,
    Outage,
}
#[derive(Deserialize)]
#[serde(remote = "SystemState", rename_all = "lowercase")]
enum SystemStateWire {
    Operational,
    Degraded,
    Maintenance,
    Outage,
}
impl<'de> Deserialize<'de> for SystemState {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        SystemStateWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for SystemState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Operational, Degraded, Maintenance, Outage });
    }
}

// ts: AccountInfoResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfoResponse {
    pub address: String,
    pub account_index: f64,
    pub nonce: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    pub maker_fee_rate: String,
    pub taker_fee_rate: String,
    pub trading_volume30d: String,
    pub created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kyc_status: Option<KycStatus>,
}
#[derive(Deserialize)]
#[serde(remote = "AccountInfoResponse", rename_all = "camelCase")]
struct AccountInfoResponseWire {
    address: String,
    account_index: f64,
    nonce: f64,
    #[serde(default, deserialize_with = "present")]
    tier: Option<String>,
    maker_fee_rate: String,
    taker_fee_rate: String,
    trading_volume30d: String,
    created_at: f64,
    #[serde(default, deserialize_with = "present")]
    kyc_status: Option<KycStatus>,
}
impl<'de> Deserialize<'de> for AccountInfoResponse {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                AccountInfoResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for AccountInfoResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { address, account_index, nonce, tier, maker_fee_rate, taker_fee_rate, trading_volume30d, created_at, kyc_status });
    }
}

// ts: WithdrawalHistoryItem
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawalHistoryItem {
    pub withdrawal_id: String,
    pub token: String,
    pub amount: String,
    pub fee: String,
    pub l1_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    pub status: WithdrawalState,
    pub created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "WithdrawalHistoryItem", rename_all = "camelCase")]
struct WithdrawalHistoryItemWire {
    withdrawal_id: String,
    token: String,
    amount: String,
    fee: String,
    l1_address: String,
    #[serde(default, deserialize_with = "present")]
    tx_hash: Option<String>,
    status: WithdrawalState,
    created_at: f64,
    #[serde(default, deserialize_with = "present")]
    completed_at: Option<f64>,
}
impl<'de> Deserialize<'de> for WithdrawalHistoryItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                WithdrawalHistoryItemWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WithdrawalHistoryItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { withdrawal_id, token, amount, fee, l1_address, tx_hash, status, created_at, completed_at });
    }
}

// ts: DepositHistoryItem
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositHistoryItem {
    pub deposit_id: String,
    pub token: String,
    pub amount: String,
    pub tx_hash: String,
    pub status: DepositState,
    pub confirmations: f64,
    pub required_confirmations: f64,
    pub created_at: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "DepositHistoryItem", rename_all = "camelCase")]
struct DepositHistoryItemWire {
    deposit_id: String,
    token: String,
    amount: String,
    tx_hash: String,
    status: DepositState,
    confirmations: f64,
    required_confirmations: f64,
    created_at: f64,
    #[serde(default, deserialize_with = "present")]
    completed_at: Option<f64>,
}
impl<'de> Deserialize<'de> for DepositHistoryItem {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                DepositHistoryItemWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for DepositHistoryItem {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { deposit_id, token, amount, tx_hash, status, confirmations, required_confirmations, created_at, completed_at });
    }
}

// ts: SystemStatus
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub status: SystemState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub components: Vec<ComponentStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_maintenance: Option<Vec<MaintenanceWindow>>,
    pub last_update: f64,
}
#[derive(Deserialize)]
#[serde(remote = "SystemStatus", rename_all = "camelCase")]
struct SystemStatusWire {
    status: SystemState,
    #[serde(default, deserialize_with = "present")]
    message: Option<String>,
    components: Vec<ComponentStatus>,
    #[serde(default, deserialize_with = "present")]
    scheduled_maintenance: Option<Vec<MaintenanceWindow>>,
    last_update: f64,
}
impl<'de> Deserialize<'de> for SystemStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> std::result::Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?;
        input
            .decode_with(|decoder| {
                SystemStatusWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for SystemStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { status, message, components, scheduled_maintenance, last_update });
    }
}
