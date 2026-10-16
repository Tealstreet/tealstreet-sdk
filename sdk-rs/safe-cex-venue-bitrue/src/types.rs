//! Mirrors the configuration exports in `bitrue.types.ts`.

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BROKER_ID: &str = "Tealstreet";
pub const BASE_URL: &str = "https://fapi.bitrue.com";
pub const BASE_WS_URL: [(&str, &str); 2] = [
    ("public", "wss://fmarket-ws.bitrue.com/kline-api/ws"),
    ("private", "wss://fapiws.bitrue.com/stream"),
];
pub const ENDPOINTS: [(&str, &str); 11] = [
    ("ACCOUNT", "/fapi/v2/account"),
    ("KLINE", "/fapi/v1/klines"),
    ("MARKETS", "/fapi/v1/contracts"),
    ("TICKERS", "/fapi/v1/ticker"),
    ("LEVERAGE_BRACKET", "/fapi/v2/leverageBracket"),
    ("ORDERS", "/fapi/v2/openOrders"),
    ("MY_TRADES", "/fapi/v2/myTrades"),
    ("PLACE_ORDER", "/fapi/v2/order"),
    ("CANCEL_ORDER", "/fapi/v2/cancel"),
    (
        "LISTEN_KEY",
        "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey",
    ),
    ("SET_LEVERAGE", "/fapi/v2/level_edit"),
];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 1] = [ENDPOINTS[7].1];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 2] = [ENDPOINTS[5].1, ENDPOINTS[0].1];

use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_types::{OrderSide, OrderStatus, OrderType};

pub const INTERVAL: [(&str, &str); 7] = [
    ("1m", "1min"),
    ("5m", "5min"),
    ("15m", "15min"),
    ("30m", "30min"),
    ("1h", "60min"),
    ("1d", "1day"),
    ("1w", "1week"),
];
pub const ORDER_TYPE: [(&str, OrderType); 4] = [
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
];
pub const ORDER_STATUS: [(&str, OrderStatus); 11] = [
    ("INIT", OrderStatus::Open),
    ("PENDING_CREATE", OrderStatus::Open),
    ("NEW", OrderStatus::Open),
    ("PARTIALLY_FILLED", OrderStatus::Open),
    ("PART_FILLED", OrderStatus::Open),
    ("FILLED", OrderStatus::Closed),
    ("CANCELED", OrderStatus::Canceled),
    ("CANCELLED", OrderStatus::Canceled),
    ("PENDING_CANCEL", OrderStatus::Canceling),
    ("REJECTED", OrderStatus::Canceled),
    ("EXPIRED", OrderStatus::Canceled),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

// Preserve source prototype hits until each consumer chooses its own branch/output policy.
pub fn interval(key: &str) -> TableHit<&&'static str> {
    js_table_lookup(&INTERVAL, key)
}
pub fn order_type(key: &str) -> TableHit<&OrderType> {
    js_table_lookup(&ORDER_TYPE, key)
}
pub fn order_status(key: &str) -> TableHit<&OrderStatus> {
    js_table_lookup(&ORDER_STATUS, key)
}
pub fn order_side(key: &str) -> TableHit<&OrderSide> {
    js_table_lookup(&ORDER_SIDE, key)
}
