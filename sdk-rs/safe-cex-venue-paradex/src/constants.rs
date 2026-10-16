//! Own entries in TypeScript declaration order; use the shared table lookup for inherited reads.
use safe_cex_types::{OrderSide, OrderType};

// ts: RECV_WINDOW
pub const RECV_WINDOW: f64 = 20_000.0;

// ts: BASE_URL
pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.prod.paradex.trade/v1"),
    ("testnet", "https://api.testnet.paradex.trade/v1"),
];

// ts: BASE_WS_URL
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            (
                "livenet",
                "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false",
            ),
            (
                "testnet",
                "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false",
            ),
        ],
    ),
    (
        "private",
        &[
            (
                "livenet",
                "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false",
            ),
            (
                "testnet",
                "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false",
            ),
        ],
    ),
];

// ts: ENDPOINTS
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("AUTH", "/auth"),
    ("ACCOUNT", "/account"),
    ("FILLS", "/fills"),
    ("ONBOARDING", "/onboarding"),
    ("MARKETS", "/markets"),
    ("TICKERS", "/markets/summary"),
    ("ORDERS", "/orders"),
    ("ORDERS_HISTORY", "/orders-history"),
    ("ORDER", "/orders/{id}"),
    ("MODIFY_ORDER", "/orders/{id}"),
    ("CANCEL_ORDER", "/orders/{id}"),
    (
        "CANCEL_ORDER_BY_CLIENT_ID",
        "/orders/by_client_id/{client_id}",
    ),
    ("CANCEL_ORDERS_BATCH", "/orders/batch"),
    ("CANCEL_ALL_ORDERS", "/orders"),
    ("PLACE_ORDER", "/orders"),
    ("PLACE_ORDERS_BATCH", "/orders/batch"),
    ("POSITIONS", "/positions"),
    ("POSITION", "/positions/{market}"),
    ("ORDERBOOK", "/orderbook/{market}"),
    ("OHLCV", "/markets/klines"),
    ("TRADINGVIEW_HISTORY", "/tradingview/history"),
    ("MARGIN", "/account/margin/{market}"),
    ("ALL_MARGINS", "/account/margin"),
    ("SET_MARGIN", "/account/margin/{market}"),
    (
        "MAX_SLIPPAGE",
        "/account/profile/market_max_slippage/{market}",
    ),
];

// ts: ORDER_TYPE
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_LIMIT", OrderType::StopLoss),
    ("STOP_MARKET", OrderType::StopLoss),
    ("STOP_LOSS_LIMIT", OrderType::StopLoss),
    ("STOP_LOSS_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_LIMIT", OrderType::TakeProfit),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
];

// ts: ORDER_SIDE
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

// ts: isParadexTriggerOrderType
pub fn is_paradex_trigger_order_type(kind: &str) -> bool {
    [
        "STOP_LIMIT",
        "STOP_MARKET",
        "STOP_LOSS_LIMIT",
        "STOP_LOSS_MARKET",
        "TAKE_PROFIT_LIMIT",
        "TAKE_PROFIT_MARKET",
    ]
    .contains(&kind)
}
// ts: isParadexMarketOrderType
pub fn is_paradex_market_order_type(kind: &str) -> bool {
    [
        "MARKET",
        "STOP_MARKET",
        "STOP_LOSS_MARKET",
        "TAKE_PROFIT_MARKET",
    ]
    .contains(&kind)
}
