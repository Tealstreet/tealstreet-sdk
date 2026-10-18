//! `kraken.types.ts`: ordered own entries; use shared `js_table_lookup` for JS reads.
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide};
use serde::{Deserialize, Serialize};

pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://futures.kraken.com/derivatives/api"),
    ("testnet", "https://demo-futures.kraken.com/derivatives/api"),
];
pub const HISTORY_BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://futures.kraken.com/api"),
    ("testnet", "https://demo-futures.kraken.com/api"),
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KrakenBaseUrl {
    Derivatives,
    History,
}

#[derive(Deserialize)]
#[serde(remote = "KrakenBaseUrl", rename_all = "lowercase")]
enum KrakenBaseUrlSerde {
    Derivatives,
    History,
}

impl<'de> serde::Deserialize<'de> for KrakenBaseUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tag = String::deserialize(deserializer)?;
        KrakenBaseUrlSerde::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(tag))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KrakenBaseUrl {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Derivatives, History });
    }
}

const WS_NETWORKS: &[(&str, &str)] = &[
    ("livenet", "wss://futures.kraken.com/ws/v1"),
    ("testnet", "wss://demo-futures.kraken.com/ws/v1"),
];
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    ("public", WS_NETWORKS),
    ("private", WS_NETWORKS),
    ("frontendfutures", WS_NETWORKS),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("ACCOUNT_LOG", "/history/v3/account-log"),
    ("FILLS", "/v3/fills"),
    ("MARKETS", "/v3/instruments"),
    ("TICKERS", "/v3/tickers"),
    ("ORDERBOOK", "/v3/orderbook"),
    ("CANDLES", "/api/charts/v1"),
    ("ACCOUNT", "/v3/accounts"),
    ("OPEN_ORDERS", "/v3/openorders"),
    ("POSITIONS", "/v3/openpositions"),
    ("ORDER", "/v3/sendorder"),
    ("EDIT_ORDER", "/v3/editorder"),
    ("CANCEL_ORDER", "/v3/cancelorder"),
    ("BATCH_ORDERS", "/v3/batchorder"),
    ("LEVERAGE_PREFERENCES", "/v3/leveragepreferences"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &["/v3/instruments", "/v3/tickers", "/v3/orderbook"];
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
];
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];
pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("POST_ONLY", OrderTimeInForce::PostOnly),
];
pub const ORDER_TRIGGER_TYPE: &[(&str, OrderStopTriggerType)] = &[
    ("MARK_PRICE", OrderStopTriggerType::MarkPrice),
    ("LAST_PRICE", OrderStopTriggerType::LastPrice),
    ("INDEX_PRICE", OrderStopTriggerType::IndexPrice),
];
