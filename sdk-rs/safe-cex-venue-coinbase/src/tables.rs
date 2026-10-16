//! Protocol tables from coinbase.types.ts.

use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType, PositionSide};

// ts: RECV_WINDOW

pub const RECV_WINDOW: u64 = 20000;

// ts: BASE_URL

pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.coinbase.com"),
    ("testnet", "https://api.coinbase.com"),
];

// ts: BASE_WS_URL

pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://advanced-trade-ws.coinbase.com"),
            ("testnet", "wss://advanced-trade-ws.coinbase.com"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://advanced-trade-ws-user.coinbase.com"),
            ("testnet", "wss://advanced-trade-ws-user.coinbase.com"),
        ],
    ),
];

// ts: ENDPOINTS

pub const ENDPOINTS: &[(&str, &str)] = &[
    ("MARKETS", "/api/v3/brokerage/products"),
    ("LIST_ACCOUNTS", "/api/v3/brokerage/accounts"),
    ("LIST_PORTFOLIOS", "/api/v3/brokerage/portfolios"),
    (
        "LIST_POSITIONS_INTX",
        "/api/v3/brokerage/intx/positions/{portfolio_uuid}",
    ),
    ("LIST_POSITIONS_CFM", "/api/v3/brokerage/cfm/positions"),
    ("KLINE", "/api/v3/brokerage/products/{product_id}/candles"),
    ("ORDER_BOOK", "/api/v3/brokerage/product_book"),
    ("LIST_ORDERS", "/api/v3/brokerage/orders/historical/batch"),
    (
        "INTX_INSTRUMENTS",
        "https://api.international.coinbase.com/api/v1/instruments/",
    ),
    (
        "BALANCE",
        "/api/v3/brokerage/intx/portfolio/{portfolio_uuid}",
    ),
    ("BALANCE_CFM", "/api/v3/brokerage/cfm/balance_summary"),
    (
        "PORTFOLIO_BREAKDOWN",
        "/api/v3/brokerage/portfolios/{portfolio_uuid}",
    ),
    ("ORDER", "/api/v3/brokerage/orders"),
    ("EDIT_ORDER", "/api/v3/brokerage/orders/edit"),
    (
        "CANCEL_ORDER_BATCH",
        "/api/v3/brokerage/orders/batch_cancel",
    ),
];

// ts: GRAFANA_ENDPOINTS_EVERY

pub static GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/api/v3/brokerage/orders",
    "/api/v3/brokerage/orders/edit",
    "/api/v3/brokerage/orders/batch_cancel",
];

// ts: GRAFANA_ENDPOINTS_PERIODIC

pub static GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/api/v3/brokerage/portfolios",
    "/api/v3/brokerage/intx/positions/{portfolio_uuid}",
    "/api/v3/brokerage/cfm/positions",
    "/api/v3/brokerage/orders/historical/batch",
    "/api/v3/brokerage/intx/portfolio/{portfolio_uuid}",
    "/api/v3/brokerage/cfm/balance_summary",
    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
];

// ts: PUBLIC_ENDPOINTS

pub const PUBLIC_ENDPOINTS: &[&str] =
    &["https://api.international.coinbase.com/api/v1/instruments/"];

// ts: ORDER_TYPE

pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
];

// ts: ORDER_SIDE

pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

// ts: POSITION_SIDE

pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];

// ts: TIME_IN_FORCE

pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("GTX", OrderTimeInForce::PostOnly),
];
