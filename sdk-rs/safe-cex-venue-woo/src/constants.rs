//! Legacy endpoint and mapping tables, in TypeScript own-property order.
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderType};

// ts: RECV_WINDOW
pub const RECV_WINDOW: u64 = 20000;

// ts: BROKER_ID
pub const BROKER_ID: &str = "ab82cb09-cfec-4473-80a3-b740779d0644";

// ts: TESTNET_BROKER_ID
pub const TESTNET_BROKER_ID: &str = "e708a644-9ce5-46c6-b50c-74b15f62d8ca";

// ts: BASE_URL
pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.woox.io"),
    ("testnet", "https://api.staging.woox.io"),
];

// ts: BASE_WS_URL
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://wss.woox.io/ws/stream/"),
            ("testnet", "wss://wss.staging.woox.io/ws/stream/"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://wss.woox.io/v2/ws/private/stream/"),
            ("testnet", "wss://wss.staging.woox.io/v2/ws/private/stream/"),
        ],
    ),
];

// ts: ENDPOINTS
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("ACCOUNT", "/v3/accountinfo"),
    ("BALANCE", "/v3/balances"),
    ("POSITIONS", "/v3/positions"),
    ("POSITION", "/v3/position/{symbol}"),
    ("TRANSACTION_HISTORY", "/v3/trade/transactionHistory"),
    ("ALGO_ORDERS", "/v3/algo/orders"),
    ("UPDATE_ORDER", "/v3/order"),
    ("ALGO_ORDER", "/v3/algo/order"),
    ("CANCEL_ORDERS", "/v3/orders/pending"),
    ("CANCEL_ALGO_ORDERS", "/v3/algo/orders/pending"),
    ("CANCEL_SYMBOL_ORDERS", "/v1/orders"),
    ("MARKETS", "/v1/public/info"),
    ("TICKERS", "/v1/public/futures"),
    ("FUNDING_RATES", "/v1/public/funding_rates"),
    ("CLIENT_TRADES", "/v1/client/trades"),
    ("HIST_TRADES", "/v1/client/hist_trades"),
    ("ORDERS", "/v1/orders"),
    ("KLINE", "/v1/kline"),
    ("KLINE_HISTORY", "https://api-pub.woox.io/v1/hist/kline"),
    ("CANCEL_ORDER", "/v1/order"),
    ("PLACE_ORDER", "/v1/order"),
    ("LEVERAGE", "/v1/client/leverage"),
    ("ORDERBOOK", "/v1/public/orderbook"),
    ("POSITION_MODE", "/v1/client/position_mode"),
    ("FUTURES_LEVERAGE", "/v1/client/futures_leverage"),
];

// ts: GRAFANA_ENDPOINTS_EVERY
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/v3/algo/order",
    "/v3/position/{symbol}",
    "/v3/order",
    "/v3/orders/pending",
    "/v3/algo/orders/pending",
    "/v1/orders",
    "/v1/order",
    "/v1/order",
    "/v1/client/leverage",
    "/v1/client/position_mode",
    "/v1/client/futures_leverage",
];

// ts: GRAFANA_ENDPOINTS_PERIODIC
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/v3/accountinfo",
    "/v3/balances",
    "/v3/positions",
    "/v3/algo/orders",
];

// ts: ORDER_TYPE
// Own entries; binding::globals adds the source Object.prototype at runtime.
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("POST_ONLY", OrderType::Limit),
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("ASK", OrderType::Market),
    ("BID", OrderType::Market),
    ("TAKE_PROFIT", OrderType::TakeProfit),
    ("STOP_LOSS", OrderType::StopLoss),
    ("TRAILING_STOP", OrderType::TrailingStopLoss),
];

// ts: ORDER_SIDE
// Own entries; binding::globals adds the source Object.prototype at runtime.
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

// ts: ORDER_TRIGGER_TYPE
// Own entries; binding::globals adds the source Object.prototype at runtime.
pub const ORDER_TRIGGER_TYPE: &[(&str, OrderStopTriggerType)] = &[
    ("MARK_PRICE", OrderStopTriggerType::MarkPrice),
    ("MARKET_PRICE", OrderStopTriggerType::LastPrice),
];
