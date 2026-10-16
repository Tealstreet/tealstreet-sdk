//! Constants from orderly.types.ts. Tables retain source entry order and duplicates.
use safe_cex_types::{OrderSide, OrderType};

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BROKER_ID: &str = "ab82cb09-cfec-4473-80a3-b740779d0644";
pub const TESTNET_BROKER_ID: &str = "e708a644-9ce5-46c6-b50c-74b15f62d8ca";
pub const BASE_URL: [(&str, &str); 2] = [
    ("livenet", "https://api-evm.orderly.org"),
    ("testnet", "https://testnet-api-evm.orderly.org"),
];
pub const BASE_WS_URL: [(&str, [(&str, &str); 2]); 2] = [
    (
        "public",
        [
            ("livenet", "wss://ws-evm.orderly.org/ws/stream"),
            ("testnet", "wss://testnet-ws-evm.orderly.org/ws/stream"),
        ],
    ),
    (
        "private",
        [
            (
                "livenet",
                "wss://ws-private-evm.orderly.org/v2/ws/private/stream",
            ),
            (
                "testnet",
                "wss://testnet-ws-private-evm.orderly.org/v2/ws/private/stream",
            ),
        ],
    ),
];
pub const ENDPOINTS: [(&str, &str); 20] = [
    ("ACCOUNT", "/v1/client/info"),
    ("BALANCE", "/v1/client/holding"),
    ("MARKETS", "/v1/public/info"),
    ("TICKERS", "/v1/public/futures"),
    ("POSITIONS", "/v1/positions"),
    ("POSITION", "/v1/position/{symbol}"),
    ("ALGO_ORDERS", "/v1/algo/orders"),
    ("UPDATE_ORDER", "/v1/order"),
    ("ALGO_ORDER", "/v1/algo/order"),
    ("CANCEL_ORDERS", "/v1/orders"),
    ("CANCEL_ALGO_ORDERS", "/v1/algo/orders"),
    ("CANCEL_SYMBOL_ORDERS", "/v1/orders"),
    ("ORDERS", "/v1/orders"),
    ("KLINE", "/v1/kline"),
    ("CANCEL_ORDER", "/v1/order"),
    ("PLACE_ORDER", "/v1/order"),
    ("LEVERAGE", "/v1/client/leverages"),
    ("MARGIN_MODE", "/v1/client/margin_mode"),
    ("MARGIN_MODES", "/v1/client/margin_modes"),
    ("ORDERBOOK", "/v1/orderbook"),
];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 9] = [
    ENDPOINTS[8].1,
    ENDPOINTS[5].1,
    ENDPOINTS[7].1,
    ENDPOINTS[9].1,
    ENDPOINTS[10].1,
    ENDPOINTS[11].1,
    ENDPOINTS[14].1,
    ENDPOINTS[15].1,
    ENDPOINTS[16].1,
];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 6] = [
    ENDPOINTS[0].1,
    ENDPOINTS[1].1,
    ENDPOINTS[4].1,
    ENDPOINTS[6].1,
    ENDPOINTS[12].1,
    ENDPOINTS[18].1,
];
// These are own entries. Dynamic JS property reads must use the shared
// safe-cex-num table lookup, including inherited Object.prototype members.
pub const ORDER_TYPE: [(&str, OrderType); 5] = [
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("TAKE_PROFIT", OrderType::TakeProfit),
    ("STOP_LOSS", OrderType::StopLoss),
    ("TRAILING_STOP", OrderType::TrailingStopLoss),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
