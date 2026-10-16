//! GRVT protocol tables, measured against the live TypeScript exports.

/// Exact TypeScript `BASE_URLS` entries, retaining declaration order.
pub const BASE_URLS: &[(&str, &str)] = &[
    ("PRODUCTION_MDG", "https://market-data.grvt.io"),
    ("PRODUCTION_TDG", "https://trades.grvt.io"),
    ("PRODUCTION_EDGE", "https://edge.grvt.io"),
    ("TESTNET_MDG", "https://market-data.testnet.grvt.io"),
    ("TESTNET_TDG", "https://trades.testnet.grvt.io"),
    ("TESTNET_EDGE", "https://edge.testnet.grvt.io"),
    (
        "STAGING_MDG",
        "https://market-data.staging.gravitymarkets.io",
    ),
    ("STAGING_TDG", "https://trades.staging.gravitymarkets.io"),
    ("STAGING_EDGE", "https://edge.staging.gravitymarkets.io"),
    ("DEV_MDG", "https://market-data.dev.gravitymarkets.io"),
    ("DEV_TDG", "https://trades.dev.gravitymarkets.io"),
    ("DEV_EDGE", "https://edge.dev.gravitymarkets.io"),
];

/// Exact TypeScript `WSS_URLS` entries, retaining declaration order.
pub const WSS_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "wss://market-data.grvt.io/ws/full"),
    ("TESTNET", "wss://market-data.testnet.grvt.io/ws/full"),
    ("STAGING", "wss://market-data.stg.gravitymarkets.io/ws/full"),
    ("DEV", "wss://market-data.dev.gravitymarkets.io/ws/full"),
];

/// Exact TypeScript `ENDPOINTS` entries, retaining declaration order.
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("INSTRUMENTS", "/lite/v1/all_instruments"),
    ("INSTRUMENT", "/lite/v1/instrument"),
    ("TICKER", "/lite/v1/ticker"),
    ("MINI_TICKER", "/lite/v1/mini"),
    ("ORDERBOOK", "/lite/v1/book"),
    ("TRADES", "/lite/v1/trade_history"),
    ("KLINES", "/lite/v1/kline"),
    ("FUNDING", "/lite/v1/funding"),
    ("AUTH", "/auth/api_key/login"),
    ("ACCOUNT_SUMMARY", "/lite/v1/account_summary"),
    ("POSITIONS", "/lite/v1/positions"),
    ("OPEN_ORDERS", "/lite/v1/open_orders"),
    ("CREATE_ORDER", "/lite/v1/create_order"),
    ("CANCEL_ORDER", "/lite/v1/cancel_order"),
    ("CANCEL_ALL_ORDERS", "/lite/v1/cancel_all_orders"),
    ("ORDER_HISTORY", "/lite/v1/order_history"),
    ("FILL_HISTORY", "/lite/v1/fill_history"),
    ("GET_LEVERAGE", "/lite/v1/get_all_initial_leverage"),
    ("SET_LEVERAGE", "/lite/v1/set_initial_leverage"),
    ("AUTHORIZE_BUILDER", "/full/v1/authorize_builder"),
    (
        "GET_AUTHORIZED_BUILDERS",
        "/full/v1/get_authorized_builders",
    ),
    ("BUILDER_FILL_HISTORY", "/full/v1/builder_fill_history"),
];

/// Exact TypeScript `GRAFANA_ENDPOINTS_EVERY` endpoint sequence.
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/lite/v1/create_order",
    "/lite/v1/cancel_order",
    "/lite/v1/cancel_all_orders",
];

/// Exact TypeScript `GRAFANA_ENDPOINTS_PERIODIC` endpoint sequence.
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/lite/v1/account_summary",
    "/lite/v1/positions",
    "/lite/v1/open_orders",
];
