//! Protocol values mirrored from bitget.types.ts; lookup semantics belong to callers.
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide};

// ts: RECV_WINDOW
pub const RECV_WINDOW: u64 = 20000;

// ts: BROKER_ID
pub const BROKER_ID: &str = "Tealstreet";

// ts: BASE_URL
pub const BASE_URL: &str = "https://api.bitget.com";

// ts: BASE_WS_URL
pub const BASE_WS_URL: &str = "wss://ws.bitget.com/v2/ws/public";

// ts: BASE_WS_PRIVATE_URL
pub const BASE_WS_PRIVATE_URL: &str = "wss://ws.bitget.com/v2/ws/private";

// ts: BASE_WS_PRIVATE_UTA_URL
pub const BASE_WS_PRIVATE_UTA_URL: &str = "wss://ws.bitget.com/v3/ws/private";

// ts: ENDPOINTS
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ApiGeneration {
    #[serde(rename = "v2")]
    ClassicV2,
    #[serde(rename = "v3")]
    UnifiedV3,
}

// ts: ENDPOINTS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub name: &'static str,
    pub path: &'static str,
    pub generation: ApiGeneration,
}

// ts: ENDPOINTS
pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        name: "ACCOUNT_SPOT",
        path: "/api/v2/spot/account/info",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ACCOUNT",
        path: "/api/v2/mix/account/account",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "BALANCE",
        path: "/api/v2/mix/account/accounts",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ACCOUNT_BILL",
        path: "/api/v2/mix/account/bill",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "MARKETS",
        path: "/api/v2/mix/market/contracts",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "SYMBOL_LEVERAGE",
        path: "/api/v2/mix/market/symbol-leverage",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "TICKERS",
        path: "/api/v2/mix/market/tickers",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "POSITIONS",
        path: "/api/v2/mix/position/all-position",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "POSITIONS_V2",
        path: "/api/v2/mix/position/all-position",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "KLINE",
        path: "/api/v2/mix/market/candles",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ORDERS",
        path: "/api/v2/mix/order/orders-pending",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "CANCEL_ALGO_ORDER",
        path: "/api/v2/mix/order/cancel-plan-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "CANCEL_ORDERS",
        path: "/api/v2/mix/order/batch-cancel-orders",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "CANCEL_ALL_ORDERS",
        path: "/api/v2/mix/order/cancel-all-orders",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "PLACE_ORDER",
        path: "/api/v2/mix/order/place-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "BATCH_ORDERS",
        path: "/api/v2/mix/order/batch-place-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ALGO_ORDERS",
        path: "/api/v2/mix/order/orders-plan-pending",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "PLACE_ALGO_ORDER",
        path: "/api/v2/mix/order/place-tpsl-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "UPDATE_ORDER",
        path: "/api/v2/mix/order/modify-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "UPDATE_ALGO_ORDER",
        path: "/api/v2/mix/order/modify-tpsl-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "SET_LEVERAGE",
        path: "/api/v2/mix/account/set-leverage",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "SET_LEVERAGE_V3",
        path: "/api/v3/account/set-leverage",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "ACCOUNT_SETTINGS_V3",
        path: "/api/v3/account/settings",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_ACCOUNT_ASSETS",
        path: "/api/v3/account/assets",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_POSITIONS",
        path: "/api/v3/position/current-position",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_ORDERS",
        path: "/api/v3/trade/unfilled-orders",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_STRATEGY_ORDERS",
        path: "/api/v3/trade/unfilled-strategy-orders",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_ORDER_FILLS",
        path: "/api/v3/trade/fills",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_PLACE_ORDER",
        path: "/api/v3/trade/place-order",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_PLACE_STRATEGY_ORDER",
        path: "/api/v3/trade/place-strategy-order",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_CANCEL_ORDER",
        path: "/api/v3/trade/cancel-order",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_CANCEL_STRATEGY_ORDER",
        path: "/api/v3/trade/cancel-strategy-order",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_CANCEL_BATCH",
        path: "/api/v3/trade/cancel-batch",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_MODIFY_ORDER",
        path: "/api/v3/trade/modify-order",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "UTA_SET_HOLD_MODE",
        path: "/api/v3/account/set-hold-mode",
        generation: ApiGeneration::UnifiedV3,
    },
    Endpoint {
        name: "POSITION_LEVERAGE",
        path: "/api/v2/mix/position/single-position",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "SET_POSITION_MODE",
        path: "/api/v2/mix/account/set-position-mode",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "SET_MARGIN_MODE",
        path: "/api/v2/mix/account/set-margin-mode",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "PLACE_TRAILING_STOP_ORDER",
        path: "/api/v2/mix/order/place-plan-order",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "FETCH_SINGLE_ACCOUNT",
        path: "/api/v2/mix/account/account",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ORDER_HISTORY",
        path: "/api/v2/mix/order/orders-history",
        generation: ApiGeneration::ClassicV2,
    },
    Endpoint {
        name: "ORDER_FILLS",
        path: "/api/v2/mix/order/fills",
        generation: ApiGeneration::ClassicV2,
    },
];

// ts: GRAFANA_ENDPOINTS_EVERY
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/api/v2/mix/market/symbol-leverage",
    "/api/v2/mix/order/cancel-plan-order",
    "/api/v2/mix/order/batch-cancel-orders",
    "/api/v2/mix/order/cancel-all-orders",
    "/api/v2/mix/order/place-order",
    "/api/v2/mix/order/batch-place-order",
    "/api/v2/mix/order/orders-plan-pending",
    "/api/v2/mix/order/place-tpsl-order",
    "/api/v2/mix/order/modify-order",
    "/api/v2/mix/order/modify-tpsl-order",
    "/api/v3/trade/place-order",
    "/api/v3/trade/place-strategy-order",
    "/api/v3/trade/cancel-order",
    "/api/v3/trade/cancel-strategy-order",
    "/api/v3/trade/cancel-batch",
    "/api/v3/trade/modify-order",
    "/api/v2/mix/account/set-leverage",
    "/api/v3/account/set-leverage",
    "/api/v3/account/set-hold-mode",
    "/api/v2/mix/position/single-position",
    "/api/v2/mix/account/set-position-mode",
    "/api/v2/mix/account/set-margin-mode",
    "/api/v2/mix/order/place-plan-order",
];

// ts: GRAFANA_ENDPOINTS_PERIODIC
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/api/v2/mix/account/account",
    "/api/v2/spot/account/info",
    "/api/v2/mix/account/accounts",
    "/api/v3/account/settings",
    "/api/v3/account/assets",
    "/api/v3/position/current-position",
    "/api/v3/trade/unfilled-orders",
    "/api/v3/trade/unfilled-strategy-orders",
    "/api/v3/trade/fills",
    "/api/v2/mix/position/all-position",
    "/api/v2/mix/position/all-position",
    "/api/v2/mix/order/orders-pending",
    "/api/v2/mix/account/account",
];

// ts: INTERVAL
pub const INTERVAL: &[(&str, &str)] = &[
    ("1m", "1m"),
    ("3m", "3m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "1H"),
    ("2h", "2H"),
    ("4h", "4H"),
    ("6h", "6Hutc"),
    ("12h", "12Hutc"),
    ("1d", "1Dutc"),
    ("1w", "1Wutc"),
];

// ts: POSITION_SIDE
pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("long", PositionSide::Long), ("short", PositionSide::Short)];

// ts: ORDER_STATUS
pub const ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("init", OrderStatus::Open),
    ("new", OrderStatus::Open),
    ("not_trigger", OrderStatus::Open),
    ("partial_fill", OrderStatus::Open),
    ("partially_fill", OrderStatus::Open),
    ("partially_filled", OrderStatus::Open),
    ("triggered", OrderStatus::Closed),
    ("full_fill", OrderStatus::Closed),
    ("filled", OrderStatus::Closed),
    ("fail_trigger", OrderStatus::Canceled),
    ("cancel", OrderStatus::Canceled),
    ("cancelled", OrderStatus::Canceled),
    ("canceled", OrderStatus::Canceled),
    ("live", OrderStatus::Open),
    ("pending", OrderStatus::Open),
    ("submitting", OrderStatus::Open),
    ("success", OrderStatus::Closed),
    ("failed", OrderStatus::Canceled),
    ("fail_execute", OrderStatus::Canceled),
    ("executed", OrderStatus::Closed),
];

// ts: ORDER_TYPE
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("limit", OrderType::Limit),
    ("market", OrderType::Market),
    ("pos_profit", OrderType::TakeProfit),
    ("pos_loss", OrderType::StopLoss),
    ("loss_plan", OrderType::StopLoss),
    ("profit_plan", OrderType::TakeProfit),
    ("track_plan", OrderType::TrailingStopLoss),
    ("tp", OrderType::TakeProfit),
    ("sl", OrderType::StopLoss),
    ("psl", OrderType::StopLoss),
    ("ptp", OrderType::TakeProfit),
    ("track", OrderType::TrailingStopLoss),
];

// ts: ORDER_SIDE
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[
    ("open_long", OrderSide::Buy),
    ("open_short", OrderSide::Sell),
    ("close_long", OrderSide::Sell),
    ("close_short", OrderSide::Buy),
    ("reduce_close_long", OrderSide::Sell),
    ("reduce_close_short", OrderSide::Buy),
    ("offset_close_long", OrderSide::Sell),
    ("offset_close_short", OrderSide::Buy),
    ("burst_close_long", OrderSide::Sell),
    ("burst_close_short", OrderSide::Buy),
    ("delivery_close_long", OrderSide::Sell),
    ("delivery_close_short", OrderSide::Buy),
    ("buy_single", OrderSide::Buy),
    ("sell_single", OrderSide::Sell),
    ("buy", OrderSide::Buy),
    ("sell", OrderSide::Sell),
];

// ts: TIME_IN_FORCE
pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("normal", OrderTimeInForce::GoodTillCancel),
    ("gtc", OrderTimeInForce::GoodTillCancel),
    ("post_only", OrderTimeInForce::PostOnly),
    ("fok", OrderTimeInForce::FillOrKill),
    ("ioc", OrderTimeInForce::ImmediateOrCancel),
];
