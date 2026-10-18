//! Protocol values mirror the live TypeScript tables.
use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};
// ts: KUCOIN_SPOT_RECV_WINDOW
pub const KUCOIN_SPOT_RECV_WINDOW: u64 = 20_000;
// ts: KUCOIN_SPOT_BROKER_ID
pub const KUCOIN_SPOT_BROKER_ID: &str = "3zexp";
// ts: KUCOIN_SPOT_BASE_URL
pub const KUCOIN_SPOT_BASE_URL: &str = "https://api.kucoin.com";
// ts: KUCOIN_SPOT_ENDPOINTS
pub const KUCOIN_SPOT_ENDPOINTS: &[(&str, &str)] = &[
    ("ANNOUNCEMENTS", "/api/v3/announcements"),
    ("ACCOUNTS", "/api/v1/accounts"),
    ("ACCOUNT_SUMMARY", "/api/v2/user-info"),
    ("MARKETS", "/api/v2/symbols"),
    ("TICKERS", "/api/v1/market/allTickers"),
    ("KLINE", "/api/v1/market/candles"),
    ("OPEN_ORDERS", "/api/v1/hf/orders/active/page"),
    ("ACTIVE_ORDER_SYMBOLS", "/api/v1/hf/orders/active/symbols"),
    ("PLACE_ORDER", "/api/v1/hf/orders"),
    ("CANCEL_ORDER", "/api/v1/hf/orders"),
    ("CANCEL_SYMBOL_ORDERS", "/api/v1/hf/orders"),
    ("CANCEL_ALL_ORDERS", "/api/v1/hf/orders/cancelAll"),
    ("MODIFY_ORDER", "/api/v1/hf/orders/alter"),
    ("CLOSED_ORDERS", "/api/v1/hf/orders/done"),
    ("FILLS", "/api/v1/hf/fills"),
    ("GET_PUBLIC_WS_TOKEN", "/api/v1/bullet-public"),
    ("GET_PRIVATE_WS_TOKEN", "/api/v1/bullet-private"),
];
// ts: KUCOIN_SPOT_PUBLIC_ENDPOINTS
pub const KUCOIN_SPOT_PUBLIC_ENDPOINTS: &[&str] = &[
    KUCOIN_SPOT_ENDPOINTS[0].1,
    KUCOIN_SPOT_ENDPOINTS[3].1,
    KUCOIN_SPOT_ENDPOINTS[4].1,
    KUCOIN_SPOT_ENDPOINTS[5].1,
    KUCOIN_SPOT_ENDPOINTS[15].1,
];
// ts: KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY
pub const KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    KUCOIN_SPOT_ENDPOINTS[8].1,
    KUCOIN_SPOT_ENDPOINTS[9].1,
    KUCOIN_SPOT_ENDPOINTS[10].1,
    KUCOIN_SPOT_ENDPOINTS[11].1,
    KUCOIN_SPOT_ENDPOINTS[12].1,
];
// ts: KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC
pub const KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    KUCOIN_SPOT_ENDPOINTS[1].1,
    KUCOIN_SPOT_ENDPOINTS[6].1,
    KUCOIN_SPOT_ENDPOINTS[13].1,
    KUCOIN_SPOT_ENDPOINTS[14].1,
];
// ts: KUCOIN_SPOT_INTERVAL
pub const KUCOIN_SPOT_INTERVAL: &[(&str, &str)] = &[
    ("1m", "1min"),
    ("3m", "3min"),
    ("5m", "5min"),
    ("15m", "15min"),
    ("30m", "30min"),
    ("1h", "1hour"),
    ("2h", "2hour"),
    ("4h", "4hour"),
    ("6h", "6hour"),
    ("12h", "12hour"),
    ("1d", "1day"),
    ("1w", "1week"),
];
// ts: KUCOIN_SPOT_ORDER_STATUS
pub const KUCOIN_SPOT_ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("active", OrderStatus::Open),
    ("open", OrderStatus::Open),
    ("new", OrderStatus::Open),
    ("match", OrderStatus::Open),
    ("done", OrderStatus::Closed),
    ("filled", OrderStatus::Closed),
    ("canceled", OrderStatus::Canceled),
    ("cancelled", OrderStatus::Canceled),
];
// ts: KUCOIN_SPOT_ORDER_TYPE
pub const KUCOIN_SPOT_ORDER_TYPE: &[(&str, OrderType)] =
    &[("limit", OrderType::Limit), ("market", OrderType::Market)];
// ts: KUCOIN_SPOT_TIME_IN_FORCE
pub const KUCOIN_SPOT_TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
];

use serde::{Deserialize, Serialize};

// ts: KucoinSpotWsServer
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KucoinSpotWsServer {
    pub endpoint: String,
    pub encrypt: bool,
    pub protocol: String,
    pub ping_interval: f64,
    pub ping_timeout: f64,
}
// Private Serde companion; the public entry retains its admission checks.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "KucoinSpotWsServer")]
#[serde(rename_all = "camelCase")]
struct KucoinSpotWsServerWire {
    endpoint: String,
    encrypt: bool,
    protocol: String,
    ping_interval: f64,
    ping_timeout: f64,
}

impl<'de> Deserialize<'de> for KucoinSpotWsServer {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                KucoinSpotWsServerWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: KucoinSpotWsTokenResponse.data
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KucoinSpotWsTokenData {
    pub token: String,
    pub instance_servers: Vec<KucoinSpotWsServer>,
}
// Private Serde companion; the public entry retains its admission checks.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "KucoinSpotWsTokenData")]
#[serde(rename_all = "camelCase")]
struct KucoinSpotWsTokenDataWire {
    token: String,
    instance_servers: Vec<KucoinSpotWsServer>,
}

impl<'de> Deserialize<'de> for KucoinSpotWsTokenData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                KucoinSpotWsTokenDataWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: KucoinSpotWsTokenResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KucoinSpotWsTokenResponse {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<KucoinSpotWsTokenData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}
// Private Serde companion; the public entry retains its admission checks.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "KucoinSpotWsTokenResponse")]
#[serde(rename_all = "camelCase")]
struct KucoinSpotWsTokenResponseWire {
    code: String,
    #[serde(default, deserialize_with = "present")]
    data: Option<KucoinSpotWsTokenData>,
    #[serde(default, deserialize_with = "present")]
    msg: Option<String>,
}

impl<'de> Deserialize<'de> for KucoinSpotWsTokenResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                KucoinSpotWsTokenResponseWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

// Whole-entry numeric ingress checks every field, including nested token data.
impl safe_cex_num::numeric_ingress::JsNumberOutput for KucoinSpotWsServer {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {
            endpoint, encrypt, protocol, ping_interval, ping_timeout
        });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KucoinSpotWsTokenData {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { token, instance_servers });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KucoinSpotWsTokenResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, data, msg });
    }
}
