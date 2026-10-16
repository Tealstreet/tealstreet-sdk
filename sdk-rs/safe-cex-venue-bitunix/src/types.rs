//! Constants from bitunix.types.ts. Use shared js_table_lookup for ordinary-object lookup.

use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide};

pub const RECV_WINDOW: f64 = 20_000.0;

pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://fapi.bitunix.com"),
    ("testnet", "https://testnet.bitunix.com"),
];

pub const ENDPOINTS: &[(&str, &str)] = &[
    ("MARKETS", "/api/v1/futures/market/trading_pairs"),
    ("ACCOUNT", "/api/v1/futures/account"),
    ("TICKERS", "/api/v1/futures/market/tickers"),
    (
        "LEVERAGE_MARGIN_MODE",
        "/api/v1/futures/account/get_leverage_margin_mode",
    ),
    ("HEDGE_MODE", "/api/v1/futures/account/change_position_mode"),
    ("SET_LEVERAGE", "/api/v1/futures/account/change_leverage"),
    ("OPEN_ORDERS", "/api/v1/futures/trade/get_pending_orders"),
    (
        "OPEN_TPSL_ORDERS",
        "/api/v1/futures/tpsl/get_pending_orders",
    ),
    ("ORDER", "/api/v1/futures/trade/place_order"),
    ("MODIFY_ORDER", "/api/v1/futures/trade/modify_order"),
    ("TP_SL_ORDER", "/api/v1/futures/tpsl/place_order"),
    (
        "POSITION_TP_SL_ORDER",
        "/api/v1/futures/tpsl/position/place_order",
    ),
    ("MODIFY_TP_SL_ORDER", "/api/v1/futures/tpsl/modify_order"),
    (
        "MODIFY_POSITION_TP_SL_ORDER",
        "/api/v1/futures/tpsl/position/modify_order",
    ),
    ("BATCH_ORDERS", "/api/v1/futures/trade/batch_order"),
    ("CANCEL_ORDER_TP_SL", "/api/v1/futures/tpsl/cancel_order"),
    ("CANCEL_ORDER", "/api/v1/futures/trade/cancel_orders"),
    ("KLINE", "/api/v1/futures/market/kline"),
    ("HISTORY_TRADES", "/api/v1/futures/trade/get_history_trades"),
    ("HISTORY_ORDERS", "/api/v1/futures/trade/get_history_orders"),
    (
        "HISTORY_POSITIONS",
        "/api/v1/futures/position/get_history_positions",
    ),
    (
        "SET_MARGIN_TYPE",
        "/api/v1/futures/account/change_margin_mode",
    ),
    (
        "POSITIONS",
        "/api/v1/futures/position/get_pending_positions",
    ),
];

pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://fapi.bitunix.com/public/"),
            ("testnet", "wss://fapi.bitunix.com/public/"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://fapi.bitunix.com/private/"),
            ("testnet", "wss://fapi.bitunix.com/private/"),
        ],
    ),
    (
        "frontendfutures",
        &[
            (
                "livenet",
                "wss://api.bitunix.com/ws-futures/?from=contract&compress=2",
            ),
            (
                "testnet",
                "wss://api.bitunix.com/ws-futures/?from=contract&compress=2",
            ),
        ],
    ),
];

pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/api/v1/futures/market/trading_pairs",
    "/api/v1/futures/market/tickers",
    "/api/v1/futures/market/kline",
];

pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/api/v1/futures/trade/place_order",
    "/api/v1/futures/trade/batch_order",
    "/api/v1/futures/tpsl/modify_order",
    "/api/v1/futures/tpsl/position/modify_order",
    "/api/v1/futures/account/change_leverage",
    "/api/v1/futures/account/change_position_mode",
];

pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/api/v1/futures/account",
    "/api/v1/futures/trade/get_pending_orders",
    "/api/v1/futures/position/get_pending_positions",
];

pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
];

pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];

pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("POST_ONLY", OrderTimeInForce::PostOnly),
];

pub const ORDER_TRIGGER_TYPE: &[(&str, OrderStopTriggerType)] = &[
    ("MARK", OrderStopTriggerType::MarkPrice),
    ("MARK_PRICE", OrderStopTriggerType::MarkPrice),
    ("LAST", OrderStopTriggerType::LastPrice),
    ("LAST_PRICE", OrderStopTriggerType::LastPrice),
    ("Mark", OrderStopTriggerType::MarkPrice),
    ("Last", OrderStopTriggerType::LastPrice),
];
