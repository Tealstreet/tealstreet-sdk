//! Protocol data from lighter.types.ts; table order follows the TypeScript source.
use safe_cex_types::{OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

// ts: RECV_WINDOW
pub const RECV_WINDOW: u64 = 20_000;

// ts: LighterDeploymentNetwork
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LighterDeploymentNetwork {
    Livenet,
    Testnet,
    Robinhood,
    RobinhoodTestnet,
}

// ts: LighterDeploymentProfile.quoteCurrency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LighterQuoteCurrency {
    Usdc,
    Usdg,
}

// ts: LighterDeploymentProfile
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterDeploymentProfile {
    pub network: LighterDeploymentNetwork,
    pub api_base_url: Cow<'static, str>,
    pub ws_url: Cow<'static, str>,
    pub signer_chain_id: f64,
    pub quote_currency: LighterQuoteCurrency,
    pub uses_rest_proxy: bool,
    pub supports_ws_proxy: bool,
}

// ts: LIGHTER_DEPLOYMENT_PROFILES
pub const LIGHTER_DEPLOYMENT_PROFILES: [(LighterDeploymentNetwork, LighterDeploymentProfile); 4] = [
    (
        LighterDeploymentNetwork::Livenet,
        LighterDeploymentProfile {
            network: LighterDeploymentNetwork::Livenet,
            api_base_url: Cow::Borrowed("https://mainnet.zklighter.elliot.ai"),
            ws_url: Cow::Borrowed("wss://mainnet.zklighter.elliot.ai/stream"),
            signer_chain_id: 304.0,
            quote_currency: LighterQuoteCurrency::Usdc,
            uses_rest_proxy: true,
            supports_ws_proxy: true,
        },
    ),
    (
        LighterDeploymentNetwork::Testnet,
        LighterDeploymentProfile {
            network: LighterDeploymentNetwork::Testnet,
            api_base_url: Cow::Borrowed("https://testnet-api.lighter.xyz"),
            ws_url: Cow::Borrowed("wss://testnet.zklighter.elliot.ai/stream"),
            signer_chain_id: 300.0,
            quote_currency: LighterQuoteCurrency::Usdc,
            uses_rest_proxy: true,
            supports_ws_proxy: false,
        },
    ),
    (
        LighterDeploymentNetwork::Robinhood,
        LighterDeploymentProfile {
            network: LighterDeploymentNetwork::Robinhood,
            api_base_url: Cow::Borrowed("https://api.rh.lighter.xyz"),
            ws_url: Cow::Borrowed("wss://api.rh.lighter.xyz/stream"),
            signer_chain_id: 466324.0,
            quote_currency: LighterQuoteCurrency::Usdg,
            uses_rest_proxy: true,
            supports_ws_proxy: true,
        },
    ),
    (
        LighterDeploymentNetwork::RobinhoodTestnet,
        LighterDeploymentProfile {
            network: LighterDeploymentNetwork::RobinhoodTestnet,
            api_base_url: Cow::Borrowed("https://api.rh-testnet.lighter.xyz"),
            ws_url: Cow::Borrowed("wss://api.rh-testnet.lighter.xyz/stream"),
            signer_chain_id: 300.0,
            quote_currency: LighterQuoteCurrency::Usdg,
            uses_rest_proxy: false,
            supports_ws_proxy: false,
        },
    ),
];

// ts: ENDPOINTS
pub const ENDPOINTS: [(&str, &str); 13] = [
    ("ACCOUNT", "/api/v1/account"),
    ("ACCOUNTS_BY_L1_ADDRESS", "/api/v1/accountsByL1Address"),
    ("ACCOUNT_LIMITS", "/api/v1/accountLimits"),
    ("CHANGE_ACCOUNT_TIER", "/api/v1/changeAccountTier"),
    ("MARKETS", "/api/v1/orderBooks"),
    ("ORDER_BOOK_DETAILS", "/api/v1/orderBookDetails"),
    ("EXCHANGE_STATS", "/api/v1/exchangeStats"),
    ("CANDLES", "/api/v1/candles"),
    ("TRADES", "/api/v1/trades"),
    ("OPEN_ORDERS", "/api/v1/accountActiveOrders"),
    ("INACTIVE_ORDERS", "/api/v1/accountInactiveOrders"),
    ("SEND_TX", "/api/v1/sendTx"),
    ("SEND_TX_BATCH", "/api/v1/sendTxBatch"),
];
// ts: GRAFANA_ENDPOINTS_EVERY
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 2] = ["/api/v1/sendTx", "/api/v1/sendTxBatch"];
// ts: GRAFANA_ENDPOINTS_PERIODIC
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 3] = [
    "/api/v1/account",
    "/api/v1/accountLimits",
    "/api/v1/accountActiveOrders",
];
// ts: INTERVAL
pub const INTERVAL: [(&str, &str); 8] = [
    ("1m", "1m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "1h"),
    ("4h", "4h"),
    ("12h", "12h"),
    ("1d", "1d"),
];
// ts: ORDER_STATUS
pub const ORDER_STATUS: [(&str, OrderStatus); 8] = [
    ("PENDING", OrderStatus::Open),
    ("OPEN", OrderStatus::Open),
    ("PARTIALLY_FILLED", OrderStatus::Open),
    ("FILLED", OrderStatus::Closed),
    ("CANCELLED", OrderStatus::Canceled),
    ("CANCELED", OrderStatus::Canceled),
    ("EXPIRED", OrderStatus::Closed),
    ("REJECTED", OrderStatus::Canceled),
];
// ts: ORDER_TYPE
pub const ORDER_TYPE: [(&str, OrderType); 8] = [
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_LOSS", OrderType::StopLoss),
    ("STOP_MARKET", OrderType::StopLoss),
    ("STOP_LIMIT", OrderType::StopLoss),
    ("TAKE_PROFIT", OrderType::TakeProfit),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TAKE_PROFIT_LIMIT", OrderType::TakeProfit),
];
// ts: ORDER_SIDE
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
// ts: ORDER_TIME_IN_FORCE
pub const ORDER_TIME_IN_FORCE: [(&str, OrderTimeInForce); 6] = [
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("GOOD_TILL_TIME", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("IMMEDIATE_OR_CANCEL", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("POST_ONLY", OrderTimeInForce::PostOnly),
];
// ts: ORDER_TRIGGER_TYPE
pub const ORDER_TRIGGER_TYPE: [(&str, OrderStopTriggerType); 3] = [
    ("INDEX", OrderStopTriggerType::IndexPrice),
    ("MARK", OrderStopTriggerType::MarkPrice),
    ("LAST", OrderStopTriggerType::LastPrice),
];

// Require a string before applying serde's variant table. Its default unit-enum
// decoder also accepts tagged objects, which the TypeScript literal union rejects.
#[derive(Deserialize)]
#[serde(remote = "LighterDeploymentNetwork", rename_all = "kebab-case")]
enum LighterDeploymentNetworkWire {
    Livenet,
    Testnet,
    Robinhood,
    RobinhoodTestnet,
}
impl<'de> Deserialize<'de> for LighterDeploymentNetwork {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterDeploymentNetworkWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// Require a string before applying serde's variant table. Its default unit-enum
// decoder also accepts tagged objects, which the TypeScript literal union rejects.
#[derive(Deserialize)]
#[serde(remote = "LighterQuoteCurrency", rename_all = "UPPERCASE")]
enum LighterQuoteCurrencyWire {
    Usdc,
    Usdg,
}
impl<'de> Deserialize<'de> for LighterQuoteCurrency {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterQuoteCurrencyWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterDeploymentProfile", rename_all = "camelCase")]
struct LighterDeploymentProfileWire {
    network: LighterDeploymentNetwork,
    api_base_url: Cow<'static, str>,
    ws_url: Cow<'static, str>,
    signer_chain_id: f64,
    quote_currency: LighterQuoteCurrency,
    uses_rest_proxy: bool,
    supports_ws_proxy: bool,
}
impl<'de> serde::Deserialize<'de> for LighterDeploymentProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                LighterDeploymentProfileWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterDeploymentNetwork {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Livenet, Testnet, Robinhood, RobinhoodTestnet });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterQuoteCurrency {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Usdc, Usdg });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterDeploymentProfile {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { network, api_base_url, ws_url, signer_chain_id, quote_currency, uses_rest_proxy, supports_ws_proxy });
    }
}
