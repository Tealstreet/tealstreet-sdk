//! Own entries retain source order. Use `lookup` for ordinary-object reads,
//! including inherited Object.prototype identities; do not use an own-only find.
pub use safe_cex_num::js_table_lookup as lookup;
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, PositionSide};

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BASE_URL: [(&str, &str); 2] = [
    ("livenet", "https://api.phemex.com"),
    ("testnet", "https://testnet-api.phemex.com"),
];
pub const BASE_WS_URL: [(&str, [(&str, &str); 2]); 2] = [
    (
        "public",
        [
            ("livenet", "wss://ws.phemex.com"),
            ("testnet", "wss://testnet-api.phemex.com/ws"),
        ],
    ),
    (
        "private",
        [
            ("livenet", "wss://ws.phemex.com"),
            ("testnet", "wss://testnet-api.phemex.com/ws"),
        ],
    ),
];
pub const ENDPOINTS: [(&str, &str); 14] = [
    ("MARKETS", "/public/products"),
    ("BATCH_ORDERS", "/g-orders"),
    ("CANCEL_ORDER", "/g-orders/cancel"),
    ("TICKERS", "/md/v3/ticker/24hr/all"),
    ("ACCOUNT", "/g-accounts/accountPositions"),
    ("ORDER", "/g-orders/create"),
    ("USER_TRADES", "/exchange/order/v2/tradingList"),
    ("USER_TRADES_LEGACY", "/exchange/order/trade"),
    ("KLINE", "/exchange/public/md/v2/kline/list"),
    ("ORDERBOOK", "/md/v2/fullbook"),
    ("SET_LEVERAGE", "/g-positions/leverage"),
    ("SET_LEVERAGE_CONTRACT", "/positions/leverage"),
    ("POSITIONS", "/g-accounts/accountPositions"),
    ("SET_POSITION_MODE", "/g-positions/switch-pos-mode-sync"),
];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 2] = [ENDPOINTS[12].1, ENDPOINTS[4].1];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 6] = [
    ENDPOINTS[1].1,
    ENDPOINTS[2].1,
    ENDPOINTS[5].1,
    ENDPOINTS[10].1,
    ENDPOINTS[11].1,
    ENDPOINTS[13].1,
];
pub const PUBLIC_ENDPOINTS: [&str; 4] = [
    ENDPOINTS[0].1,
    ENDPOINTS[3].1,
    ENDPOINTS[8].1,
    ENDPOINTS[9].1,
];
pub const ORDER_STATUS: [(&str, OrderStatus); 8] = [
    ("new", OrderStatus::Open),
    ("untriggered", OrderStatus::Open),
    ("partially_filled", OrderStatus::Open),
    ("partiallyfilled", OrderStatus::Open),
    ("triggered", OrderStatus::Closed),
    ("filled", OrderStatus::Closed),
    ("rejected", OrderStatus::Canceled),
    ("canceled", OrderStatus::Canceled),
];
pub const ORDER_TYPE: [(&str, &str); 6] = [
    ("market", "Market"),
    ("limit", "Limit"),
    ("conditional", "Stop"),
    ("stop_market", "Stop"),
    ("take_profit_market", "MarketIfTouched"),
    ("trailing_stop_market", "MarketIfTouched"),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
pub const POSITION_SIDE: [(&str, PositionSide); 2] =
    [("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];
pub const TIME_IN_FORCE: [(&str, OrderTimeInForce); 4] = [
    ("GoodTillCancel", OrderTimeInForce::GoodTillCancel),
    ("ImmediateOrCancel", OrderTimeInForce::ImmediateOrCancel),
    ("FillOrKill", OrderTimeInForce::FillOrKill),
    ("PostOnly", OrderTimeInForce::PostOnly),
];

/// phemex.api.ts broker identifier; order formatters add their own underscore.
pub const BROKER_ID: &str = "Tealstreet";
