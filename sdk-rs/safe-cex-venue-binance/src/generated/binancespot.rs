// Generated from the live TypeScript types source by scripts/types.mjs. Do not edit.
pub const RECV_WINDOW: f64 = 20000.0;
pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.binance.com"),
    ("testnet", "https://testnet.binance.vision"),
];
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://stream.binance.com:9443/ws"),
            ("testnet", "wss://stream.testnet.binance.vision/ws"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://ws-api.binance.com:443/ws-api/v3"),
            ("testnet", "wss://ws-api.testnet.binance.vision/ws-api/v3"),
        ],
    ),
];
pub const USER_DATA_RECV_WINDOW: f64 = 5000.0;
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("ACCOUNT", "/api/v3/account"),
    ("MARKETS", "/api/v3/exchangeInfo"),
    ("TICKERS_24H", "/api/v3/ticker/24hr"),
    ("ORDER", "/api/v3/order"),
    ("CANCEL_SYMBOL_ORDERS", "/api/v3/openOrders"),
    ("OPEN_ORDERS", "/api/v3/openOrders"),
    ("KLINE", "/api/v3/klines"),
    ("ORDERBOOK", "/api/v3/depth"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/api/v3/exchangeInfo",
    "/api/v3/ticker/24hr",
    "/api/v3/klines",
    "/api/v3/depth",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &["/api/v3/order", "/api/v3/openOrders"];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &["/api/v3/account", "/api/v3/openOrders"];
pub const ORDER_TYPE: &[(&str, safe_cex_types::OrderType)] = &[
    ("LIMIT", safe_cex_types::OrderType::Limit),
    ("MARKET", safe_cex_types::OrderType::Market),
    ("STOP_MARKET", safe_cex_types::OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", safe_cex_types::OrderType::TakeProfit),
    (
        "TRAILING_STOP_MARKET",
        safe_cex_types::OrderType::TrailingStopLoss,
    ),
];
pub const ORDER_TYPE_SPOT: &[(&str, safe_cex_types::OrderType)] = &[
    ("LIMIT", safe_cex_types::OrderType::Limit),
    ("MARKET", safe_cex_types::OrderType::Market),
    ("LIMIT_MAKER", safe_cex_types::OrderType::Limit),
    ("STOP_LOSS", safe_cex_types::OrderType::StopLoss),
    ("STOP_LOSS_LIMIT", safe_cex_types::OrderType::StopLoss),
    ("TAKE_PROFIT", safe_cex_types::OrderType::TakeProfit),
    ("TAKE_PROFIT_LIMIT", safe_cex_types::OrderType::TakeProfit),
];
pub const ORDER_SIDE: &[(&str, safe_cex_types::OrderSide)] = &[
    ("BUY", safe_cex_types::OrderSide::Buy),
    ("SELL", safe_cex_types::OrderSide::Sell),
];
pub const POSITION_SIDE: &[(&str, safe_cex_types::PositionSide)] = &[
    ("LONG", safe_cex_types::PositionSide::Long),
    ("SHORT", safe_cex_types::PositionSide::Short),
];
pub const TIME_IN_FORCE: &[(&str, safe_cex_types::OrderTimeInForce)] = &[
    ("GTC", safe_cex_types::OrderTimeInForce::GoodTillCancel),
    ("IOC", safe_cex_types::OrderTimeInForce::ImmediateOrCancel),
    ("FOK", safe_cex_types::OrderTimeInForce::FillOrKill),
    ("GTX", safe_cex_types::OrderTimeInForce::PostOnly),
];
