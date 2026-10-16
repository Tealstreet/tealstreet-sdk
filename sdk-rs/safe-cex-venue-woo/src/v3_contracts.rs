//! Inert WOO V3 migration declarations. No runtime route is enabled here.
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// ts: WooV3Endpoint
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3Endpoint {
    PublicInstruments,
    PublicFutures,
    PublicOrderbook,
    PublicKline,
    PublicKlineHistory,
    TradeOrder,
    TradeOrders,
    TradeAlgoOrder,
    TradeAlgoOrders,
    TradeAllOrders,
    TradeCancelAllAfter,
    TransactionHistory,
    AccountInfo,
    AccountListenKey,
    AssetBalances,
    FuturesPositions,
    FuturesPositionMode,
    FuturesLeverage,
    SpotMarginLeverage,
}

// ts: WooV3HttpMethod
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

// ts: WooV3MigrationGate
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3MigrationGate {
    PublicReadParity,
    PrivateReadParity,
    RegularWriteDemoLifecycle,
    AlgoWriteDemoLifecycle,
    CancelAllAfterDemoLifecycle,
    AccountWriteDemoLifecycle,
    PublicWebsocketParity,
    PrivateWebsocketParity,
}

// ts: WooV3MigrationRoute
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3MigrationRouteRuntimeLifecycle {
    AlreadyV3,
    Ownerless,
}

// ts: WooV3MigrationRoute
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3MigrationRoute {
    pub id: String,
    pub legacy_method: WooV3HttpMethod,
    pub legacy_path: String,
    pub v3_method: WooV3HttpMethod,
    pub v3_path: WooV3Endpoint,
    pub gate: WooV3MigrationGate,
    pub consumer: String,
    pub runtime_lifecycle: Option<WooV3MigrationRouteRuntimeLifecycle>,
}

// ts: WooV3Decimal
pub type WooV3Decimal = String;

// ts: WooV3Response
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3Response<T> {
    pub success: bool,
    pub data: T,
    pub timestamp: f64,
}

// ts: WooV3PageMeta
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3PageMeta {
    pub total: f64,
    pub records_per_page: f64,
    pub current_page: f64,
}

// ts: WooV3Rows
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3Rows<T> {
    pub rows: Vec<T>,
    pub meta: Option<WooV3PageMeta>,
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentBaseAssetMultiplier {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentBaseAssetMultiplier", untagged)]
enum WooV3InstrumentBaseAssetMultiplierNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentBaseAssetMultiplier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentBaseAssetMultiplierNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentBaseMin {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentBaseMin", untagged)]
enum WooV3InstrumentBaseMinNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentBaseMin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentBaseMinNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentBaseMax {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentBaseMax", untagged)]
enum WooV3InstrumentBaseMaxNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentBaseMax {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentBaseMaxNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentBaseTick {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentBaseTick", untagged)]
enum WooV3InstrumentBaseTickNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentBaseTick {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentBaseTickNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentQuoteTick {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentQuoteTick", untagged)]
enum WooV3InstrumentQuoteTickNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentQuoteTick {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentQuoteTickNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentMinNotional {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentMinNotional", untagged)]
enum WooV3InstrumentMinNotionalNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentMinNotional {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentMinNotionalNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3InstrumentBaseIMR {
    Decimal(WooV3Decimal),
    Number(f64),
}

#[derive(Deserialize)]
#[serde(remote = "WooV3InstrumentBaseIMR", untagged)]
enum WooV3InstrumentBaseIMRNumericWire {
    Decimal(WooV3Decimal),
    Number(f64),
}

impl<'de> Deserialize<'de> for WooV3InstrumentBaseIMR {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(WooV3InstrumentBaseIMRNumericWire::deserialize)
            .map_err(serde::de::Error::custom)
    }
}

// ts: WooV3Instrument
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3Instrument {
    pub symbol: String,
    pub status: Option<String>,
    pub base_asset: String,
    pub quote_asset: String,
    pub base_asset_multiplier: Option<WooV3InstrumentBaseAssetMultiplier>,
    pub base_min: WooV3InstrumentBaseMin,
    pub base_max: WooV3InstrumentBaseMax,
    pub base_tick: WooV3InstrumentBaseTick,
    pub quote_tick: WooV3InstrumentQuoteTick,
    pub min_notional: WooV3InstrumentMinNotional,
    pub base_imr: WooV3InstrumentBaseIMR,
    pub funding_interval_hours: Option<f64>,
    pub is_allowed_rpi: Option<bool>,
}

// ts: WooV3Balance
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3Balance {
    pub token: String,
    pub holding: WooV3Decimal,
    pub frozen: WooV3Decimal,
    pub available_balance: WooV3Decimal,
    pub pending_short_qty: WooV3Decimal,
    pub pending_long_qty: WooV3Decimal,
    pub average_open_price: WooV3Decimal,
    pub mark_price: WooV3Decimal,
    pub timestamp: f64,
}

// ts: WooV3Position
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PositionPositionSide {
    Both,
    Long,
    Short,
}

// ts: WooV3Position
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3Position {
    pub symbol: String,
    pub holding: WooV3Decimal,
    pub position_side: WooV3PositionPositionSide,
    pub pending_long_qty: WooV3Decimal,
    pub pending_short_qty: WooV3Decimal,
    pub settle_price: WooV3Decimal,
    pub average_open_price: WooV3Decimal,
    pub mark_price: WooV3Decimal,
    pub est_liq_price: WooV3Decimal,
    pub timestamp: f64,
}

// ts: WooV3AccountInfo
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3AccountInfoPositionMode {
    OneWay,
    HedgeMode,
}

// ts: WooV3AccountInfo
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3AccountInfo {
    pub application_id: String,
    pub account: String,
    pub alias: String,
    pub account_mode: String,
    pub position_mode: WooV3AccountInfoPositionMode,
    pub leverage: f64,
    pub margin_ratio: WooV3Decimal,
    pub total_collateral: WooV3Decimal,
    pub total_account_value: WooV3Decimal,
    pub account_type: String,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PlaceOrderRequestSide {
    Buy,
    Sell,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PlaceOrderRequestType {
    Limit,
    Market,
    Ioc,
    Fok,
    PostOnly,
    Ask,
    Bid,
    Ac,
    Rpi,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PlaceOrderRequestPositionSide {
    Both,
    Long,
    Short,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PlaceOrderRequestMarginMode {
    Cross,
    Isolated,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3PlaceOrderRequestBidAskLevel {
    Level1,
    Level2,
    Level3,
    Level4,
    Level5,
}

// ts: WooV3PlaceOrderRequest
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3PlaceOrderRequest {
    pub symbol: String,
    pub side: WooV3PlaceOrderRequestSide,
    pub r#type: WooV3PlaceOrderRequestType,
    pub client_order_id: Option<f64>,
    pub order_tag: Option<String>,
    pub position_side: Option<WooV3PlaceOrderRequestPositionSide>,
    pub price: Option<WooV3Decimal>,
    pub quantity: Option<WooV3Decimal>,
    pub amount: Option<WooV3Decimal>,
    pub reduce_only: Option<bool>,
    pub visible_quantity: Option<WooV3Decimal>,
    pub margin_mode: Option<WooV3PlaceOrderRequestMarginMode>,
    pub bid_ask_level: Option<WooV3PlaceOrderRequestBidAskLevel>,
    pub post_only_adjusted: Option<bool>,
}

// ts: WooV3PlaceOrderResult
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3PlaceOrderResult {
    pub order_id: f64,
    pub r#type: String,
    pub price: WooV3Decimal,
    pub quantity: WooV3Decimal,
    pub amount: WooV3Decimal,
    pub client_order_id: f64,
    pub bid_ask_level: f64,
}

// ts: WooV3ListenKeyRequest
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WooV3ListenKeyRequestType {
    Websocket,
}

// ts: WooV3ListenKeyRequest
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3ListenKeyRequest {
    pub r#type: WooV3ListenKeyRequestType,
}

// ts: WooV3ListenKeyResult
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3ListenKeyResult {
    pub auth_key: String,
    pub expired_time: f64,
}

// ts: WOO_V3_REST_BASE_URL
pub const WOO_V3_REST_BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.woox.io"),
    ("testnet", "https://api.staging.woox.io"),
];

// ts: WOO_V3_WS_URL
pub const WOO_V3_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://wss.woox.io/v3/public"),
            ("testnet", "wss://wss.staging.woox.io/v3/public"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://wss.woox.io/v3/private"),
            ("testnet", "wss://wss.staging.woox.io/v3/private"),
        ],
    ),
];

// ts: WOO_V3_ENDPOINTS
pub const WOO_V3_ENDPOINTS: &[(&str, &str)] = &[
    ("PUBLIC_INSTRUMENTS", "/v3/public/instruments"),
    ("PUBLIC_FUTURES", "/v3/public/futures"),
    ("PUBLIC_ORDERBOOK", "/v3/public/orderbook"),
    ("PUBLIC_KLINE", "/v3/public/kline"),
    ("PUBLIC_KLINE_HISTORY", "/v3/public/klineHistory"),
    ("TRADE_ORDER", "/v3/trade/order"),
    ("TRADE_ORDERS", "/v3/trade/orders"),
    ("TRADE_ALGO_ORDER", "/v3/trade/algoOrder"),
    ("TRADE_ALGO_ORDERS", "/v3/trade/algoOrders"),
    ("TRADE_ALL_ORDERS", "/v3/trade/allOrders"),
    ("TRADE_CANCEL_ALL_AFTER", "/v3/trade/cancelAllAfter"),
    ("TRANSACTION_HISTORY", "/v3/trade/transactionHistory"),
    ("ACCOUNT_INFO", "/v3/account/info"),
    ("ACCOUNT_LISTEN_KEY", "/v3/account/listenKey"),
    ("ASSET_BALANCES", "/v3/asset/balances"),
    ("FUTURES_POSITIONS", "/v3/futures/positions"),
    ("FUTURES_POSITION_MODE", "/v3/futures/positionMode"),
    ("FUTURES_LEVERAGE", "/v3/futures/leverage"),
    ("SPOT_MARGIN_LEVERAGE", "/v3/spotMargin/leverage"),
];

// ts: WOO_V3_MIGRATION_GATES
pub const WOO_V3_MIGRATION_GATES: &[(WooV3MigrationGate, bool)] = &[
    (WooV3MigrationGate::PublicReadParity, false),
    (WooV3MigrationGate::PrivateReadParity, false),
    (WooV3MigrationGate::RegularWriteDemoLifecycle, false),
    (WooV3MigrationGate::AlgoWriteDemoLifecycle, false),
    (WooV3MigrationGate::CancelAllAfterDemoLifecycle, false),
    (WooV3MigrationGate::AccountWriteDemoLifecycle, false),
    (WooV3MigrationGate::PublicWebsocketParity, false),
    (WooV3MigrationGate::PrivateWebsocketParity, false),
];

// ts: WOO_V3_RUNTIME_STATUS
pub const WOO_V3_RUNTIME_STATUS: &str = "scaffold-only";

// ts: WOO_V3_WEBSOCKET_MIGRATION
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WooV3WebsocketMigration {
    #[serde(rename = "legacyUrl")]
    pub legacy_url: &'static str,
    #[serde(rename = "v3Url")]
    pub v3_url: &'static str,
    #[serde(rename = "listenKeyEndpoint", skip_serializing_if = "Option::is_none")]
    pub listen_key_endpoint: Option<WooV3Endpoint>,
    pub authentication: &'static str,
    pub gate: WooV3MigrationGate,
}

// ts: WOO_V3_WEBSOCKET_MIGRATION
pub const WOO_V3_WEBSOCKET_MIGRATION: &[(&str, WooV3WebsocketMigration)] = &[
    (
        "public",
        WooV3WebsocketMigration {
            legacy_url: "wss://wss.woox.io/ws/stream/:applicationId",
            v3_url: "wss://wss.woox.io/v3/public",
            listen_key_endpoint: None,
            authentication: "none",
            gate: WooV3MigrationGate::PublicWebsocketParity,
        },
    ),
    (
        "private",
        WooV3WebsocketMigration {
            legacy_url: "wss://wss.woox.io/v2/ws/private/stream/:applicationId",
            v3_url: "wss://wss.woox.io/v3/private?key=:listenKey",
            listen_key_endpoint: Some(WooV3Endpoint::AccountListenKey),
            authentication: "listen-key-query",
            gate: WooV3MigrationGate::PrivateWebsocketParity,
        },
    ),
];

// ts: WOO_V3_MIGRATION_ROUTES
pub static WOO_V3_MIGRATION_ROUTES: LazyLock<Vec<WooV3MigrationRoute>> = LazyLock::new(|| {
    vec![
        WooV3MigrationRoute {
            id: "markets".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/public/info".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::PublicInstruments,
            gate: WooV3MigrationGate::PublicReadParity,
            consumer: "fetchMarkets".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "tickers".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/public/futures".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::PublicFutures,
            gate: WooV3MigrationGate::PublicReadParity,
            consumer: "fetchTickersRaw".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "orderbook".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/public/orderbook/:symbol".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::PublicOrderbook,
            gate: WooV3MigrationGate::PublicReadParity,
            consumer: "fetchOrderBook".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "recent-klines".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/kline".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::PublicKline,
            gate: WooV3MigrationGate::PublicReadParity,
            consumer: "fetchOHLCV".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "historical-klines".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "https://api-pub.woox.io/v1/hist/kline".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::PublicKlineHistory,
            gate: WooV3MigrationGate::PublicReadParity,
            consumer: "fetchOHLCV".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "place-order".into(),
            legacy_method: WooV3HttpMethod::Post,
            legacy_path: "/v1/order".into(),
            v3_method: WooV3HttpMethod::Post,
            v3_path: WooV3Endpoint::TradeOrder,
            gate: WooV3MigrationGate::RegularWriteDemoLifecycle,
            consumer: "placeOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "cancel-order".into(),
            legacy_method: WooV3HttpMethod::Delete,
            legacy_path: "/v1/order".into(),
            v3_method: WooV3HttpMethod::Delete,
            v3_path: WooV3Endpoint::TradeOrder,
            gate: WooV3MigrationGate::RegularWriteDemoLifecycle,
            consumer: "cancelOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "fetch-orders".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/orders".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::TradeOrders,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchLimitOrders".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "cancel-symbol-orders".into(),
            legacy_method: WooV3HttpMethod::Delete,
            legacy_path: "/v1/orders".into(),
            v3_method: WooV3HttpMethod::Delete,
            v3_path: WooV3Endpoint::TradeOrders,
            gate: WooV3MigrationGate::RegularWriteDemoLifecycle,
            consumer: "cancelSymbolOrders".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "cancel-all-orders".into(),
            legacy_method: WooV3HttpMethod::Delete,
            legacy_path: "/v3/orders/pending".into(),
            v3_method: WooV3HttpMethod::Delete,
            v3_path: WooV3Endpoint::TradeOrders,
            gate: WooV3MigrationGate::RegularWriteDemoLifecycle,
            consumer: "cancelAllOrders".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "amend-order".into(),
            legacy_method: WooV3HttpMethod::Put,
            legacy_path: "/v3/order/:orderId".into(),
            v3_method: WooV3HttpMethod::Put,
            v3_path: WooV3Endpoint::TradeOrder,
            gate: WooV3MigrationGate::RegularWriteDemoLifecycle,
            consumer: "updateOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "place-algo-order".into(),
            legacy_method: WooV3HttpMethod::Post,
            legacy_path: "/v3/algo/order".into(),
            v3_method: WooV3HttpMethod::Post,
            v3_path: WooV3Endpoint::TradeAlgoOrder,
            gate: WooV3MigrationGate::AlgoWriteDemoLifecycle,
            consumer: "placeAlgoOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "amend-algo-order".into(),
            legacy_method: WooV3HttpMethod::Put,
            legacy_path: "/v3/algo/order/:orderId".into(),
            v3_method: WooV3HttpMethod::Put,
            v3_path: WooV3Endpoint::TradeAlgoOrder,
            gate: WooV3MigrationGate::AlgoWriteDemoLifecycle,
            consumer: "updateOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "cancel-algo-order".into(),
            legacy_method: WooV3HttpMethod::Delete,
            legacy_path: "/v3/algo/order/:orderId".into(),
            v3_method: WooV3HttpMethod::Delete,
            v3_path: WooV3Endpoint::TradeAlgoOrder,
            gate: WooV3MigrationGate::AlgoWriteDemoLifecycle,
            consumer: "cancelAlgoOrder".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "fetch-algo-orders".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v3/algo/orders".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::TradeAlgoOrders,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchAlgoOrders".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "cancel-all-algo-orders".into(),
            legacy_method: WooV3HttpMethod::Delete,
            legacy_path: "/v3/algo/orders/pending".into(),
            v3_method: WooV3HttpMethod::Delete,
            v3_path: WooV3Endpoint::TradeAlgoOrders,
            gate: WooV3MigrationGate::AlgoWriteDemoLifecycle,
            consumer: "no active safe-cex runtime owner".into(),
            runtime_lifecycle: Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless),
        },
        WooV3MigrationRoute {
            id: "cancel-all-after".into(),
            legacy_method: WooV3HttpMethod::Post,
            legacy_path: "/v1/order/cancel_all_after".into(),
            v3_method: WooV3HttpMethod::Post,
            v3_path: WooV3Endpoint::TradeCancelAllAfter,
            gate: WooV3MigrationGate::CancelAllAfterDemoLifecycle,
            consumer: "dead-man-switch (not currently owned by safe-cex runtime)".into(),
            runtime_lifecycle: Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless),
        },
        WooV3MigrationRoute {
            id: "recent-trades".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/client/trades".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::TransactionHistory,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchHistoricalFills".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "historical-trades".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/client/hist_trades".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::TransactionHistory,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchHistoricalTrades".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "transaction-history".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v3/trade/transactionHistory".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::TransactionHistory,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchHistoricalFills".into(),
            runtime_lifecycle: Some(WooV3MigrationRouteRuntimeLifecycle::AlreadyV3),
        },
        WooV3MigrationRoute {
            id: "account-info".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v3/accountinfo".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::AccountInfo,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchBalance/validateAccount".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "balances".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v3/balances".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::AssetBalances,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "no active safe-cex runtime owner; fetchBalance uses account-info".into(),
            runtime_lifecycle: Some(WooV3MigrationRouteRuntimeLifecycle::Ownerless),
        },
        WooV3MigrationRoute {
            id: "positions".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v3/positions".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::FuturesPositions,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "fetchPositions".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "position-mode".into(),
            legacy_method: WooV3HttpMethod::Post,
            legacy_path: "/v1/client/position_mode".into(),
            v3_method: WooV3HttpMethod::Put,
            v3_path: WooV3Endpoint::FuturesPositionMode,
            gate: WooV3MigrationGate::AccountWriteDemoLifecycle,
            consumer: "changePositionMode".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "fetch-futures-leverage".into(),
            legacy_method: WooV3HttpMethod::Get,
            legacy_path: "/v1/client/futures_leverage".into(),
            v3_method: WooV3HttpMethod::Get,
            v3_path: WooV3Endpoint::FuturesLeverage,
            gate: WooV3MigrationGate::PrivateReadParity,
            consumer: "getLeverage".into(),
            runtime_lifecycle: None,
        },
        WooV3MigrationRoute {
            id: "set-futures-leverage".into(),
            legacy_method: WooV3HttpMethod::Post,
            legacy_path: "/v1/client/futures_leverage".into(),
            v3_method: WooV3HttpMethod::Put,
            v3_path: WooV3Endpoint::FuturesLeverage,
            gate: WooV3MigrationGate::AccountWriteDemoLifecycle,
            consumer: "setLeverage".into(),
            runtime_lifecycle: None,
        },
    ]
});

// String-only boundaries exclude serde tagged objects; remote derives retain checked metadata.
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Endpoint")]
enum WooV3EndpointWire {
    #[serde(rename = "/v3/public/instruments")]
    PublicInstruments,
    #[serde(rename = "/v3/public/futures")]
    PublicFutures,
    #[serde(rename = "/v3/public/orderbook")]
    PublicOrderbook,
    #[serde(rename = "/v3/public/kline")]
    PublicKline,
    #[serde(rename = "/v3/public/klineHistory")]
    PublicKlineHistory,
    #[serde(rename = "/v3/trade/order")]
    TradeOrder,
    #[serde(rename = "/v3/trade/orders")]
    TradeOrders,
    #[serde(rename = "/v3/trade/algoOrder")]
    TradeAlgoOrder,
    #[serde(rename = "/v3/trade/algoOrders")]
    TradeAlgoOrders,
    #[serde(rename = "/v3/trade/allOrders")]
    TradeAllOrders,
    #[serde(rename = "/v3/trade/cancelAllAfter")]
    TradeCancelAllAfter,
    #[serde(rename = "/v3/trade/transactionHistory")]
    TransactionHistory,
    #[serde(rename = "/v3/account/info")]
    AccountInfo,
    #[serde(rename = "/v3/account/listenKey")]
    AccountListenKey,
    #[serde(rename = "/v3/asset/balances")]
    AssetBalances,
    #[serde(rename = "/v3/futures/positions")]
    FuturesPositions,
    #[serde(rename = "/v3/futures/positionMode")]
    FuturesPositionMode,
    #[serde(rename = "/v3/futures/leverage")]
    FuturesLeverage,
    #[serde(rename = "/v3/spotMargin/leverage")]
    SpotMarginLeverage,
}
impl Serialize for WooV3Endpoint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3EndpointWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3Endpoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3EndpointWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3HttpMethod")]
enum WooV3HttpMethodWire {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
}
impl Serialize for WooV3HttpMethod {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3HttpMethodWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3HttpMethod {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3HttpMethodWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MigrationGate")]
enum WooV3MigrationGateWire {
    #[serde(rename = "publicReadParity")]
    PublicReadParity,
    #[serde(rename = "privateReadParity")]
    PrivateReadParity,
    #[serde(rename = "regularWriteDemoLifecycle")]
    RegularWriteDemoLifecycle,
    #[serde(rename = "algoWriteDemoLifecycle")]
    AlgoWriteDemoLifecycle,
    #[serde(rename = "cancelAllAfterDemoLifecycle")]
    CancelAllAfterDemoLifecycle,
    #[serde(rename = "accountWriteDemoLifecycle")]
    AccountWriteDemoLifecycle,
    #[serde(rename = "publicWebsocketParity")]
    PublicWebsocketParity,
    #[serde(rename = "privateWebsocketParity")]
    PrivateWebsocketParity,
}
impl Serialize for WooV3MigrationGate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3MigrationGateWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3MigrationGate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3MigrationGateWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MigrationRouteRuntimeLifecycle")]
enum WooV3MigrationRouteRuntimeLifecycleWire {
    #[serde(rename = "already-v3")]
    AlreadyV3,
    #[serde(rename = "ownerless")]
    Ownerless,
}
impl Serialize for WooV3MigrationRouteRuntimeLifecycle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3MigrationRouteRuntimeLifecycleWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3MigrationRouteRuntimeLifecycle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3MigrationRouteRuntimeLifecycleWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PositionPositionSide")]
enum WooV3PositionPositionSideWire {
    #[serde(rename = "BOTH")]
    Both,
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl Serialize for WooV3PositionPositionSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PositionPositionSideWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PositionPositionSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PositionPositionSideWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AccountInfoPositionMode")]
enum WooV3AccountInfoPositionModeWire {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode,
}
impl Serialize for WooV3AccountInfoPositionMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3AccountInfoPositionModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3AccountInfoPositionMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3AccountInfoPositionModeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequestSide")]
enum WooV3PlaceOrderRequestSideWire {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl Serialize for WooV3PlaceOrderRequestSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestSideWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequestSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PlaceOrderRequestSideWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequestType")]
enum WooV3PlaceOrderRequestTypeWire {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "IOC")]
    Ioc,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
    #[serde(rename = "ASK")]
    Ask,
    #[serde(rename = "BID")]
    Bid,
    #[serde(rename = "AC")]
    Ac,
    #[serde(rename = "RPI")]
    Rpi,
}
impl Serialize for WooV3PlaceOrderRequestType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequestType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PlaceOrderRequestTypeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequestPositionSide")]
enum WooV3PlaceOrderRequestPositionSideWire {
    #[serde(rename = "BOTH")]
    Both,
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
}
impl Serialize for WooV3PlaceOrderRequestPositionSide {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestPositionSideWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequestPositionSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PlaceOrderRequestPositionSideWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequestMarginMode")]
enum WooV3PlaceOrderRequestMarginModeWire {
    #[serde(rename = "CROSS")]
    Cross,
    #[serde(rename = "ISOLATED")]
    Isolated,
}
impl Serialize for WooV3PlaceOrderRequestMarginMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestMarginModeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequestMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PlaceOrderRequestMarginModeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequestBidAskLevel")]
enum WooV3PlaceOrderRequestBidAskLevelWire {
    #[serde(rename = "1")]
    Level1,
    #[serde(rename = "2")]
    Level2,
    #[serde(rename = "3")]
    Level3,
    #[serde(rename = "4")]
    Level4,
    #[serde(rename = "5")]
    Level5,
}
impl Serialize for WooV3PlaceOrderRequestBidAskLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestBidAskLevelWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequestBidAskLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3PlaceOrderRequestBidAskLevelWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3ListenKeyRequestType")]
enum WooV3ListenKeyRequestTypeWire {
    #[serde(rename = "WEBSOCKET")]
    Websocket,
}
impl Serialize for WooV3ListenKeyRequestType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3ListenKeyRequestTypeWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3ListenKeyRequestType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WooV3ListenKeyRequestTypeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

#[cfg(test)]
#[path = "literal_audit.rs"]
mod literal_audit;

// Map-only record boundaries. Remote companions are private.

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MigrationRoute")]
struct WooV3MigrationRouteWire {
    id: String,
    #[serde(rename = "legacyMethod")]
    legacy_method: WooV3HttpMethod,
    #[serde(rename = "legacyPath")]
    legacy_path: String,
    #[serde(rename = "v3Method")]
    v3_method: WooV3HttpMethod,
    #[serde(rename = "v3Path")]
    v3_path: WooV3Endpoint,
    gate: WooV3MigrationGate,
    consumer: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "runtimeLifecycle")]
    runtime_lifecycle: Option<WooV3MigrationRouteRuntimeLifecycle>,
}
impl Serialize for WooV3MigrationRoute {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3MigrationRouteWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3MigrationRoute {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3MigrationRouteWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Response")]
struct WooV3ResponseWire<T> {
    success: bool,
    data: T,
    timestamp: f64,
}
impl<T: Serialize> Serialize for WooV3Response<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3ResponseWire::serialize(self, serializer)
    }
}
impl<'de, T: Deserialize<'de> + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput>
    Deserialize<'de> for WooV3Response<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3ResponseWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PageMeta")]
struct WooV3PageMetaWire {
    total: f64,
    #[serde(rename = "recordsPerPage")]
    records_per_page: f64,
    #[serde(rename = "currentPage")]
    current_page: f64,
}
impl Serialize for WooV3PageMeta {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PageMetaWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PageMeta {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3PageMetaWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Rows")]
struct WooV3RowsWire<T> {
    rows: Vec<T>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    meta: Option<WooV3PageMeta>,
}
impl<T: Serialize> Serialize for WooV3Rows<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3RowsWire::serialize(self, serializer)
    }
}
impl<'de, T: Deserialize<'de> + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput>
    Deserialize<'de> for WooV3Rows<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3RowsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Instrument")]
struct WooV3InstrumentWire {
    symbol: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    status: Option<String>,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "baseAssetMultiplier")]
    base_asset_multiplier: Option<WooV3InstrumentBaseAssetMultiplier>,
    #[serde(rename = "baseMin")]
    base_min: WooV3InstrumentBaseMin,
    #[serde(rename = "baseMax")]
    base_max: WooV3InstrumentBaseMax,
    #[serde(rename = "baseTick")]
    base_tick: WooV3InstrumentBaseTick,
    #[serde(rename = "quoteTick")]
    quote_tick: WooV3InstrumentQuoteTick,
    #[serde(rename = "minNotional")]
    min_notional: WooV3InstrumentMinNotional,
    #[serde(rename = "baseIMR")]
    base_imr: WooV3InstrumentBaseIMR,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "fundingIntervalHours")]
    funding_interval_hours: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "isAllowedRpi")]
    is_allowed_rpi: Option<bool>,
}
impl Serialize for WooV3Instrument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3InstrumentWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3Instrument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3InstrumentWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Balance")]
struct WooV3BalanceWire {
    token: String,
    holding: WooV3Decimal,
    frozen: WooV3Decimal,
    #[serde(rename = "availableBalance")]
    available_balance: WooV3Decimal,
    #[serde(rename = "pendingShortQty")]
    pending_short_qty: WooV3Decimal,
    #[serde(rename = "pendingLongQty")]
    pending_long_qty: WooV3Decimal,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: WooV3Decimal,
    #[serde(rename = "markPrice")]
    mark_price: WooV3Decimal,
    timestamp: f64,
}
impl Serialize for WooV3Balance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3BalanceWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3Balance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3BalanceWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3Position")]
struct WooV3PositionWire {
    symbol: String,
    holding: WooV3Decimal,
    #[serde(rename = "positionSide")]
    position_side: WooV3PositionPositionSide,
    #[serde(rename = "pendingLongQty")]
    pending_long_qty: WooV3Decimal,
    #[serde(rename = "pendingShortQty")]
    pending_short_qty: WooV3Decimal,
    #[serde(rename = "settlePrice")]
    settle_price: WooV3Decimal,
    #[serde(rename = "averageOpenPrice")]
    average_open_price: WooV3Decimal,
    #[serde(rename = "markPrice")]
    mark_price: WooV3Decimal,
    #[serde(rename = "estLiqPrice")]
    est_liq_price: WooV3Decimal,
    timestamp: f64,
}
impl Serialize for WooV3Position {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PositionWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3Position {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3PositionWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AccountInfo")]
struct WooV3AccountInfoWire {
    #[serde(rename = "applicationId")]
    application_id: String,
    account: String,
    alias: String,
    #[serde(rename = "accountMode")]
    account_mode: String,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountInfoPositionMode,
    leverage: f64,
    #[serde(rename = "marginRatio")]
    margin_ratio: WooV3Decimal,
    #[serde(rename = "totalCollateral")]
    total_collateral: WooV3Decimal,
    #[serde(rename = "totalAccountValue")]
    total_account_value: WooV3Decimal,
    #[serde(rename = "accountType")]
    account_type: String,
}
impl Serialize for WooV3AccountInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3AccountInfoWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3AccountInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3AccountInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderRequest")]
struct WooV3PlaceOrderRequestWire {
    symbol: String,
    side: WooV3PlaceOrderRequestSide,
    r#type: WooV3PlaceOrderRequestType,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "clientOrderId")]
    client_order_id: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "orderTag")]
    order_tag: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "positionSide")]
    position_side: Option<WooV3PlaceOrderRequestPositionSide>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    price: Option<WooV3Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    quantity: Option<WooV3Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    amount: Option<WooV3Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "reduceOnly")]
    reduce_only: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "visibleQuantity")]
    visible_quantity: Option<WooV3Decimal>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "marginMode")]
    margin_mode: Option<WooV3PlaceOrderRequestMarginMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "bidAskLevel")]
    bid_ask_level: Option<WooV3PlaceOrderRequestBidAskLevel>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    #[serde(rename = "postOnlyAdjusted")]
    post_only_adjusted: Option<bool>,
}
impl Serialize for WooV3PlaceOrderRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3PlaceOrderRequestWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3PlaceOrderResult")]
struct WooV3PlaceOrderResultWire {
    #[serde(rename = "orderId")]
    order_id: f64,
    r#type: String,
    price: WooV3Decimal,
    quantity: WooV3Decimal,
    amount: WooV3Decimal,
    #[serde(rename = "clientOrderId")]
    client_order_id: f64,
    #[serde(rename = "bidAskLevel")]
    bid_ask_level: f64,
}
impl Serialize for WooV3PlaceOrderResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3PlaceOrderResultWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3PlaceOrderResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3PlaceOrderResultWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3ListenKeyRequest")]
struct WooV3ListenKeyRequestWire {
    r#type: WooV3ListenKeyRequestType,
}
impl Serialize for WooV3ListenKeyRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3ListenKeyRequestWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3ListenKeyRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3ListenKeyRequestWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3ListenKeyResult")]
struct WooV3ListenKeyResultWire {
    #[serde(rename = "authKey")]
    auth_key: String,
    #[serde(rename = "expiredTime")]
    expired_time: f64,
}
impl Serialize for WooV3ListenKeyResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3ListenKeyResultWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3ListenKeyResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3ListenKeyResultWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

// Explicit registrations keep the complete output graph visible to parity indexing.
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MigrationRoute {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, legacy_method, legacy_path, v3_method, v3_path, gate, consumer, runtime_lifecycle });
    }
}
impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for WooV3Response<T>
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { success, data, timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PageMeta {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { total, records_per_page, current_page });
    }
}
impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for WooV3Rows<T>
{
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { rows, meta });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3Instrument {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, status, base_asset, quote_asset, base_asset_multiplier, base_min, base_max, base_tick, quote_tick, min_notional, base_imr, funding_interval_hours, is_allowed_rpi });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3Balance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { token, holding, frozen, available_balance, pending_short_qty, pending_long_qty, average_open_price, mark_price, timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3Position {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, holding, position_side, pending_long_qty, pending_short_qty, settle_price, average_open_price, mark_price, est_liq_price, timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { application_id, account, alias, account_mode, position_mode, leverage, margin_ratio, total_collateral, total_account_value, account_type });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, side, r#type, client_order_id, order_tag, position_side, price, quantity, amount, reduce_only, visible_quantity, margin_mode, bid_ask_level, post_only_adjusted });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, r#type, price, quantity, amount, client_order_id, bid_ask_level });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3ListenKeyRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { r#type });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3ListenKeyResult {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { auth_key, expired_time });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3Endpoint {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { PublicInstruments, PublicFutures, PublicOrderbook, PublicKline, PublicKlineHistory, TradeOrder, TradeOrders, TradeAlgoOrder, TradeAlgoOrders, TradeAllOrders, TradeCancelAllAfter, TransactionHistory, AccountInfo, AccountListenKey, AssetBalances, FuturesPositions, FuturesPositionMode, FuturesLeverage, SpotMarginLeverage });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3HttpMethod {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Get, Post, Put, Delete });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MigrationGate {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { PublicReadParity, PrivateReadParity, RegularWriteDemoLifecycle, AlgoWriteDemoLifecycle, CancelAllAfterDemoLifecycle, AccountWriteDemoLifecycle, PublicWebsocketParity, PrivateWebsocketParity });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MigrationRouteRuntimeLifecycle {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { AlreadyV3, Ownerless });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentBaseAssetMultiplier {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentBaseMin {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentBaseMax {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentBaseTick {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentQuoteTick {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentMinNotional {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3InstrumentBaseIMR {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Decimal(value), Number(value) });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PositionPositionSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Both, Long, Short });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountInfoPositionMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { OneWay, HedgeMode });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequestSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequestType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market, Ioc, Fok, PostOnly, Ask, Bid, Ac, Rpi });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequestPositionSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Both, Long, Short });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequestMarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Cross, Isolated });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3PlaceOrderRequestBidAskLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Level1, Level2, Level3, Level4, Level5 });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3ListenKeyRequestType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Websocket });
    }
}
