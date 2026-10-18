//! Full-format GRVT records. Wire ingress is map-only.
//! No permissive inherent decoder is exported:
//! ```compile_fail
//! use safe_cex_venue_grvt::records::GrvtInstrument;
//! let _ = GrvtInstrument::deserialize(serde_json::json!({}));
//! ```
//! Private metadata companions cannot be called by consumers:
//! ```compile_fail
//! use safe_cex_venue_grvt::records::WireGrvtInstrument;
//! ```
use super::types::{
    GrvtAssetKind, GrvtJsonRpcVersion, GrvtLiquidity, GrvtOrderSide, GrvtOrderStatus,
    GrvtOrderType, GrvtSubscriptionMethod, GrvtTimeInForce,
};
use serde::{Deserialize, Deserializer, Serialize};

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// TypeScript `GrvtInstrument` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtInstrument {
    pub instrument_id: String,
    pub instrument_hash: String,
    pub base: String,
    pub quote: String,
    pub kind: GrvtAssetKind,
    pub venue: String,
    pub settlement_period: String,
    pub tick_size: String,
    pub min_size: String,
    pub max_size: String,
    pub step_size: String,
    pub mark_price: String,
    pub index_price: String,
    pub last_trade_price: String,
    pub last_trade_time: String,
    pub is_active: bool,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtInstrument")]
struct WireGrvtInstrument {
    instrument_id: String,
    instrument_hash: String,
    base: String,
    quote: String,
    kind: GrvtAssetKind,
    venue: String,
    settlement_period: String,
    tick_size: String,
    min_size: String,
    max_size: String,
    step_size: String,
    mark_price: String,
    index_price: String,
    last_trade_price: String,
    last_trade_time: String,
    is_active: bool,
}
impl<'de> Deserialize<'de> for GrvtInstrument {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtInstrument::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtTicker` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtTicker {
    pub instrument_id: String,
    pub last_price: String,
    pub bid_price: String,
    pub ask_price: String,
    pub volume_24h: String,
    pub price_change_24h: String,
    pub price_change_pct_24h: String,
    pub high_24h: String,
    pub low_24h: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub funding_rate: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub next_funding_time: Option<String>,
    pub mark_price: String,
    pub index_price: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub open_interest: Option<String>,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtTicker")]
struct WireGrvtTicker {
    instrument_id: String,
    last_price: String,
    bid_price: String,
    ask_price: String,
    volume_24h: String,
    price_change_24h: String,
    price_change_pct_24h: String,
    high_24h: String,
    low_24h: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    funding_rate: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    next_funding_time: Option<String>,
    mark_price: String,
    index_price: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    open_interest: Option<String>,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtTicker {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtTicker::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtOrderBookLevel` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtOrderBookLevel {
    pub p: String,
    pub s: String,
    pub n: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtOrderBookLevel")]
struct WireGrvtOrderBookLevel {
    p: String,
    s: String,
    n: String,
}
impl<'de> Deserialize<'de> for GrvtOrderBookLevel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtOrderBookLevel::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtOrderBook` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtOrderBook {
    pub instrument_id: String,
    pub bids: Vec<GrvtOrderBookLevel>,
    pub asks: Vec<GrvtOrderBookLevel>,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtOrderBook")]
struct WireGrvtOrderBook {
    instrument_id: String,
    bids: Vec<GrvtOrderBookLevel>,
    asks: Vec<GrvtOrderBookLevel>,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtOrderBook {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtOrderBook::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtTrade` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtTrade {
    pub trade_id: String,
    pub instrument_id: String,
    pub price: String,
    pub size: String,
    pub side: GrvtOrderSide,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtTrade")]
struct WireGrvtTrade {
    trade_id: String,
    instrument_id: String,
    price: String,
    size: String,
    side: GrvtOrderSide,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtTrade {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtTrade::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtKline` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtKline {
    pub timestamp: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtKline")]
struct WireGrvtKline {
    timestamp: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}
impl<'de> Deserialize<'de> for GrvtKline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtKline::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtPosition` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtPosition {
    pub sub_account_id: String,
    pub instrument_id: String,
    pub size: String,
    pub notional: String,
    pub entry_price: String,
    pub mark_price: String,
    pub unrealized_pnl: String,
    pub realized_pnl: String,
    pub total_pnl: String,
    pub roi: String,
    pub leverage: String,
    pub liquidation_price: String,
    pub margin: String,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtPosition")]
struct WireGrvtPosition {
    sub_account_id: String,
    instrument_id: String,
    size: String,
    notional: String,
    entry_price: String,
    mark_price: String,
    unrealized_pnl: String,
    realized_pnl: String,
    total_pnl: String,
    roi: String,
    leverage: String,
    liquidation_price: String,
    margin: String,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtPosition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtPosition::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtBalance` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtBalance {
    pub currency: String,
    pub total: String,
    pub available: String,
    pub reserved: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtBalance")]
struct WireGrvtBalance {
    currency: String,
    total: String,
    available: String,
    reserved: String,
}
impl<'de> Deserialize<'de> for GrvtBalance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtBalance::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtAccountSummary` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtAccountSummary {
    pub sub_account_id: String,
    pub balances: Vec<GrvtBalance>,
    pub total_collateral: String,
    pub total_notional: String,
    pub total_upnl: String,
    pub account_leverage: String,
    pub margin_usage: String,
    pub is_liquidatable: bool,
    pub timestamp: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtAccountSummary")]
struct WireGrvtAccountSummary {
    sub_account_id: String,
    balances: Vec<GrvtBalance>,
    total_collateral: String,
    total_notional: String,
    total_upnl: String,
    account_leverage: String,
    margin_usage: String,
    is_liquidatable: bool,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtAccountSummary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtAccountSummary::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtOrder` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtOrder {
    pub order_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub client_order_id: Option<String>,
    pub sub_account_id: String,
    pub instrument_id: String,
    pub side: GrvtOrderSide,
    pub r#type: GrvtOrderType,
    pub time_in_force: GrvtTimeInForce,
    pub price: String,
    pub size: String,
    pub filled_size: String,
    pub remaining_size: String,
    pub average_fill_price: String,
    pub status: GrvtOrderStatus,
    pub post_only: bool,
    pub reduce_only: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtOrder")]
struct WireGrvtOrder {
    order_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    client_order_id: Option<String>,
    sub_account_id: String,
    instrument_id: String,
    side: GrvtOrderSide,
    r#type: GrvtOrderType,
    time_in_force: GrvtTimeInForce,
    price: String,
    size: String,
    filled_size: String,
    remaining_size: String,
    average_fill_price: String,
    status: GrvtOrderStatus,
    post_only: bool,
    reduce_only: bool,
    created_at: String,
    updated_at: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    expires_at: Option<String>,
}
impl<'de> Deserialize<'de> for GrvtOrder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtOrder::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtAuthRequest` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtAuthRequest {
    pub api_key: String,
    pub signature: String,
    pub nonce: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtAuthRequest")]
struct WireGrvtAuthRequest {
    api_key: String,
    signature: String,
    nonce: String,
    expires_at: String,
}
impl<'de> Deserialize<'de> for GrvtAuthRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtAuthRequest::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtAuthResponse` record; see the shared UTF-8 boundary declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtAuthResponse {
    pub session_token: String,
    pub account_id: String,
    pub sub_account_id: String,
    pub expires_at: String,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtAuthResponse")]
struct WireGrvtAuthResponse {
    session_token: String,
    account_id: String,
    sub_account_id: String,
    expires_at: String,
}
impl<'de> Deserialize<'de> for GrvtAuthResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtAuthResponse::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// TypeScript `GrvtFill`, with its closed liquidity union.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrvtFill {
    pub fill_id: String,
    pub trade_id: String,
    pub order_id: String,
    pub sub_account_id: String,
    pub instrument_id: String,
    pub side: GrvtOrderSide,
    pub price: String,
    pub size: String,
    pub fee: String,
    pub fee_currency: String,
    pub liquidity: GrvtLiquidity,
    pub timestamp: String,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtFill")]
struct WireGrvtFill {
    fill_id: String,
    trade_id: String,
    order_id: String,
    sub_account_id: String,
    instrument_id: String,
    side: GrvtOrderSide,
    price: String,
    size: String,
    fee: String,
    fee_currency: String,
    liquidity: GrvtLiquidity,
    timestamp: String,
}
impl<'de> Deserialize<'de> for GrvtFill {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtFill::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// Legacy signing order leg; optional limitPrice is non-null when present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLeg {
    pub instrument: String,
    pub size: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub limit_price: Option<String>,
    pub is_buying_asset: bool,
}
#[derive(Deserialize)]
#[serde(remote = "OrderLeg", rename_all = "camelCase")]
struct WireOrderLeg {
    instrument: String,
    size: String,
    #[serde(default, deserialize_with = "present")]
    limit_price: Option<String>,
    is_buying_asset: bool,
}
impl<'de> Deserialize<'de> for OrderLeg {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireOrderLeg::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

/// Builder authorization result, including the source message unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrvtBuilderAuthorizationResult {
    pub success: bool,
    pub message: String,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtBuilderAuthorizationResult")]
struct WireGrvtBuilderAuthorizationResult {
    success: bool,
    message: String,
}
impl<'de> Deserialize<'de> for GrvtBuilderAuthorizationResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireGrvtBuilderAuthorizationResult::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

/// TypeScript compact instrument record; numeric ingress captures before buffering.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrvtInstrumentLite {
    pub i: String,
    pub ih: String,
    pub b: String,
    pub q: String,
    pub k: GrvtAssetKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sp1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qd: Option<f64>,
    pub ts: String,
    pub ms: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ltp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ltt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ia: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ct: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub af: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub af1: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "GrvtInstrumentLite")]
struct WireGrvtInstrumentLite {
    i: String,
    ih: String,
    b: String,
    q: String,
    k: GrvtAssetKind,
    #[serde(default, deserialize_with = "present")]
    v: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    sp1: Option<String>,
    #[serde(default, deserialize_with = "present")]
    bd: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    qd: Option<f64>,
    ts: String,
    ms: String,
    #[serde(default, deserialize_with = "present")]
    ss: Option<String>,
    #[serde(default, deserialize_with = "present")]
    mp: Option<String>,
    #[serde(default, deserialize_with = "present")]
    ip: Option<String>,
    #[serde(default, deserialize_with = "present")]
    ltp: Option<String>,
    #[serde(default, deserialize_with = "present")]
    ltt: Option<String>,
    #[serde(default, deserialize_with = "present")]
    ia: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    ct: Option<String>,
    #[serde(default, deserialize_with = "present")]
    fi: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    af: Option<String>,
    #[serde(default, deserialize_with = "present")]
    af1: Option<String>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtInstrumentLite {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { i, ih, b, q, k, v, sp1, bd, qd, ts, ms, ss, mp, ip, ltp, ltt, ia, ct, fi, af, af1 });
    }
}
impl<'de> Deserialize<'de> for GrvtInstrumentLite {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtInstrumentLite::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// TypeScript `GrvtSignature`; captures numbers before Serde buffers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrvtSignature {
    pub r: String,
    pub s: String,
    pub v: f64,
    pub signer: String,
    pub nonce: f64,
    pub expiration: String,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtSignature", rename_all = "camelCase")]
struct WireGrvtSignature {
    r: String,
    s: String,
    v: f64,
    signer: String,
    nonce: f64,
    expiration: String,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtSignature {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r, s, v, signer, nonce, expiration });
    }
}
impl<'de> Deserialize<'de> for GrvtSignature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtSignature::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// TypeScript `OrderPayload`; captures numbers before Serde buffers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderPayload {
    pub sub_account_id: String,
    pub is_market: bool,
    pub time_in_force: String,
    pub post_only: bool,
    pub reduce_only: bool,
    pub legs: Vec<OrderLeg>,
    pub nonce: f64,
    pub expiration: String,
}
#[derive(Deserialize)]
#[serde(remote = "OrderPayload", rename_all = "camelCase")]
struct WireOrderPayload {
    sub_account_id: String,
    is_market: bool,
    time_in_force: String,
    post_only: bool,
    reduce_only: bool,
    legs: Vec<OrderLeg>,
    nonce: f64,
    expiration: String,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { sub_account_id, is_market, time_in_force, post_only, reduce_only, legs, nonce, expiration });
    }
}
impl<'de> Deserialize<'de> for OrderPayload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireOrderPayload::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// TypeScript `GrvtBuilderCheckResult`; captures numbers before Serde buffers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrvtBuilderCheckResult {
    pub authorized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee_rate_raw: Option<f64>,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtBuilderCheckResult", rename_all = "camelCase")]
struct WireGrvtBuilderCheckResult {
    authorized: bool,
    #[serde(default, deserialize_with = "present")]
    max_fee_rate: Option<String>,
    #[serde(default, deserialize_with = "present")]
    max_fee_rate_raw: Option<f64>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtBuilderCheckResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { authorized, max_fee_rate, max_fee_rate_raw });
    }
}
impl<'de> Deserialize<'de> for GrvtBuilderCheckResult {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtBuilderCheckResult::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// TypeScript `GrvtBuilderInfo`; captures numbers before Serde buffers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrvtBuilderInfo {
    pub builder: String,
    pub builder_fee: String,
    pub builder_fee_int: f64,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtBuilderInfo", rename_all = "camelCase")]
struct WireGrvtBuilderInfo {
    builder: String,
    builder_fee: String,
    builder_fee_int: f64,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtBuilderInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { builder, builder_fee, builder_fee_int });
    }
}
impl<'de> Deserialize<'de> for GrvtBuilderInfo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtBuilderInfo::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderLeg {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { instrument, size, limit_price, is_buying_asset });
    }
}

/// GRVT JSON wire record; arbitrary payloads retain the shared numeric buffer.
#[derive(Debug, Clone, Serialize)]
pub struct GrvtWebSocketSubscriptionParams {
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtWebSocketSubscriptionParams")]
struct WireGrvtWebSocketSubscriptionParams {
    channel: String,
    #[serde(default, deserialize_with = "present")]
    instrument_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    interval: Option<String>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtWebSocketSubscriptionParams {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { channel, instrument_id, interval });
    }
}
impl<'de> Deserialize<'de> for GrvtWebSocketSubscriptionParams {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtWebSocketSubscriptionParams::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

/// GRVT JSON wire record; arbitrary payloads retain the shared numeric buffer.
#[derive(Debug, Clone, Serialize)]
pub struct GrvtWebSocketSubscription {
    pub jsonrpc: GrvtJsonRpcVersion,
    pub id: f64,
    pub method: GrvtSubscriptionMethod,
    pub params: GrvtWebSocketSubscriptionParams,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtWebSocketSubscription")]
struct WireGrvtWebSocketSubscription {
    jsonrpc: GrvtJsonRpcVersion,
    id: f64,
    method: GrvtSubscriptionMethod,
    params: GrvtWebSocketSubscriptionParams,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtWebSocketSubscription {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { jsonrpc, id, method, params });
    }
}
impl<'de> Deserialize<'de> for GrvtWebSocketSubscription {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtWebSocketSubscription::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// GRVT JSON wire record; arbitrary payloads retain the shared numeric buffer.
#[derive(Debug, Clone, Serialize)]
pub struct GrvtWebSocketError {
    pub code: f64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtWebSocketError")]
struct WireGrvtWebSocketError {
    code: f64,
    message: String,
    #[serde(default, deserialize_with = "present")]
    data: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtWebSocketError {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, message, data });
    }
}
impl<'de> Deserialize<'de> for GrvtWebSocketError {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtWebSocketError::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// GRVT JSON wire record; arbitrary payloads retain the shared numeric buffer.
#[derive(Debug, Clone, Serialize)]
pub struct GrvtWebSocketMessage {
    pub jsonrpc: GrvtJsonRpcVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GrvtWebSocketError>,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtWebSocketMessage")]
struct WireGrvtWebSocketMessage {
    jsonrpc: GrvtJsonRpcVersion,
    #[serde(default, deserialize_with = "present")]
    method: Option<String>,
    #[serde(default, deserialize_with = "present")]
    params: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
    #[serde(default, deserialize_with = "present")]
    result: Option<safe_cex_num::numeric_ingress::JsNumberInput>,
    #[serde(default, deserialize_with = "present")]
    error: Option<GrvtWebSocketError>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtWebSocketMessage {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { jsonrpc, method, params, result, error });
    }
}
impl<'de> Deserialize<'de> for GrvtWebSocketMessage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtWebSocketMessage::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

/// Trigger metadata; public ingress preserves Number bits before nested decoding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrvtTriggerMetadata {
    pub r#type: crate::types::GrvtTriggerKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_type: Option<safe_cex_types::OrderStopTriggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_split_position: Option<bool>,
}
#[derive(Deserialize)]
#[serde(remote = "GrvtTriggerMetadata", rename_all = "camelCase")]
struct WireGrvtTriggerMetadata {
    r#type: crate::types::GrvtTriggerKind,
    #[serde(default, deserialize_with = "present")]
    trigger_price: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    trigger_type: Option<safe_cex_types::OrderStopTriggerType>,
    #[serde(default, deserialize_with = "present")]
    close: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    is_split_position: Option<bool>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GrvtTriggerMetadata {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type, trigger_price, trigger_type, close, is_split_position });
    }
}
impl<'de> Deserialize<'de> for GrvtTriggerMetadata {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                WireGrvtTriggerMetadata::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
