//! Mirrors weex.types.ts constants in declaration and entry order.
use safe_cex_types::{
    MarginType, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType,
    PositionMode, PositionSide,
};

pub const RECV_WINDOW: f64 = 20_000.0;

pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api-contract.weex.com"),
    ("testnet", "https://api-contract.weex.com"),
];

#[allow(clippy::type_complexity)]
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://ws-contract.weex.com/v3/ws/public"),
            ("testnet", "wss://ws-contract.weex.com/v3/ws/public"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://ws-contract.weex.com/v3/ws/private"),
            ("testnet", "wss://ws-contract.weex.com/v3/ws/private"),
        ],
    ),
];

pub const PRIVATE_WS_REQUEST_PATH: &str = "/v3/ws/private";

pub const ENDPOINTS: &[(&str, &str)] = &[
    ("EXCHANGE_INFO", "/capi/v3/market/exchangeInfo"),
    ("TICKER_24HR", "/capi/v3/market/ticker/24hr"),
    ("BOOK_TICKER", "/capi/v3/market/ticker/bookTicker"),
    ("DEPTH", "/capi/v3/market/depth"),
    ("KLINES", "/capi/v3/market/klines"),
    ("HISTORY_KLINES", "/capi/v3/market/historyKlines"),
    ("PUBLIC_TRADES", "/capi/v3/market/trades"),
    ("ACCOUNT_CONFIG", "/capi/v3/account/accountConfig"),
    ("BALANCE", "/capi/v3/account/balance"),
    ("SYMBOL_CONFIG", "/capi/v3/account/symbolConfig"),
    ("INCOME", "/capi/v3/account/income"),
    ("POSITIONS", "/capi/v3/account/position/allPosition"),
    (
        "SINGLE_POSITION",
        "/capi/v3/account/position/singlePosition",
    ),
    ("SET_MARGIN_TYPE", "/capi/v3/account/marginType"),
    ("SET_LEVERAGE", "/capi/v3/account/leverage"),
    ("ORDER", "/capi/v3/order"),
    ("BATCH_ORDERS", "/capi/v3/batchOrders"),
    ("OPEN_ORDERS", "/capi/v3/openOrders"),
    ("CANCEL_ALL_OPEN_ORDERS", "/capi/v3/allOpenOrders"),
    ("ORDER_HISTORY", "/capi/v3/order/history"),
    ("USER_TRADES", "/capi/v3/userTrades"),
    ("ALGO_ORDER", "/capi/v3/algoOrder"),
    ("OPEN_ALGO_ORDERS", "/capi/v3/openAlgoOrders"),
    ("CANCEL_ALL_ALGO_ORDERS", "/capi/v3/algoOpenOrders"),
    ("PLACE_TP_SL_ORDER", "/capi/v3/placeTpSlOrder"),
    ("MODIFY_TP_SL_ORDER", "/capi/v3/modifyTpSlOrder"),
    ("CLOSE_POSITIONS", "/capi/v3/closePositions"),
];

pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/capi/v3/market/exchangeInfo",
    "/capi/v3/market/ticker/24hr",
    "/capi/v3/market/ticker/bookTicker",
    "/capi/v3/market/depth",
    "/capi/v3/market/klines",
    "/capi/v3/market/historyKlines",
    "/capi/v3/market/trades",
];

pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/capi/v3/order",
    "/capi/v3/batchOrders",
    "/capi/v3/account/marginType",
    "/capi/v3/account/leverage",
    "/capi/v3/algoOrder",
    "/capi/v3/placeTpSlOrder",
    "/capi/v3/modifyTpSlOrder",
    "/capi/v3/closePositions",
];

pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/capi/v3/account/accountConfig",
    "/capi/v3/account/balance",
    "/capi/v3/account/symbolConfig",
    "/capi/v3/account/position/allPosition",
    "/capi/v3/openOrders",
    "/capi/v3/openAlgoOrders",
];

pub const INTERVAL: &[(&str, &str)] = &[
    ("1m", "1m"),
    ("3m", "3m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "1h"),
    ("2h", "2h"),
    ("4h", "4h"),
    ("6h", "6h"),
    ("12h", "12h"),
    ("1d", "1d"),
    ("1w", "1w"),
];

pub const SUPPORTED_INTERVALS: &[&str] = &["1m", "5m", "15m", "30m", "1h", "4h", "12h", "1d", "1w"];

pub const SUPPORTED_WS_INTERVALS: &[&str] = &[
    "1m", "5m", "15m", "30m", "1h", "4h", "12h", "1d", "1w", "2h", "6h",
];

pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

pub const REVERSE_ORDER_SIDE: &[(OrderSide, &str)] =
    &[(OrderSide::Buy, "BUY"), (OrderSide::Sell, "SELL")];

pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];

pub const REVERSE_POSITION_SIDE: &[(PositionSide, &str)] =
    &[(PositionSide::Long, "LONG"), (PositionSide::Short, "SHORT")];

pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("CONDITIONAL", OrderType::Conditional),
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP", OrderType::StopLoss),
    ("STOP_MARKET", OrderType::StopLoss),
    ("STOP_LOSS", OrderType::StopLoss),
    ("TAKE_PROFIT", OrderType::TakeProfit),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
];

pub const REVERSE_ORDER_TYPE: &[(OrderType, &str)] = &[
    (OrderType::Limit, "LIMIT"),
    (OrderType::Market, "MARKET"),
    (OrderType::Conditional, "STOP_MARKET"),
    (OrderType::StopLoss, "STOP_MARKET"),
    (OrderType::TakeProfit, "TAKE_PROFIT_MARKET"),
    (OrderType::TrailingStopLoss, "TRAILING_STOP_MARKET"),
];

pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("POST_ONLY", OrderTimeInForce::PostOnly),
];

pub const REVERSE_TIME_IN_FORCE: &[(OrderTimeInForce, &str)] = &[
    (OrderTimeInForce::GoodTillCancel, "GTC"),
    (OrderTimeInForce::ImmediateOrCancel, "IOC"),
    (OrderTimeInForce::FillOrKill, "FOK"),
    (OrderTimeInForce::PostOnly, "POST_ONLY"),
];

pub const ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("NEW", OrderStatus::Open),
    ("PENDING", OrderStatus::Open),
    ("PARTIALLY_FILLED", OrderStatus::Open),
    ("UNTRIGGERED", OrderStatus::Open),
    ("FILLED", OrderStatus::Closed),
    ("CANCELED", OrderStatus::Canceled),
    ("CANCELLED", OrderStatus::Canceled),
    ("EXPIRED", OrderStatus::Canceled),
    ("REJECTED", OrderStatus::Canceled),
    ("CANCELING", OrderStatus::Canceling),
];

pub const MARGIN_TYPE: &[(&str, MarginType)] = &[
    ("CROSSED", MarginType::Cross),
    ("CROSS", MarginType::Cross),
    ("SHARED", MarginType::Cross),
    ("ISOLATED", MarginType::Isolated),
    ("ISOLATION", MarginType::Isolated),
];

pub const REVERSE_MARGIN_TYPE: &[(MarginType, &str)] = &[
    (MarginType::Cross, "CROSSED"),
    (MarginType::Isolated, "ISOLATED"),
];

pub const POSITION_MODE: &[(&str, PositionMode)] = &[
    ("COMBINED", PositionMode::OneWay),
    ("SEPARATED", PositionMode::Hedged),
];

pub const REVERSE_POSITION_MODE: &[(PositionMode, &str)] = &[
    (PositionMode::OneWay, "COMBINED"),
    (PositionMode::Hedged, "SEPARATED"),
];

pub const ORDER_TRIGGER_TYPE: &[(&str, OrderStopTriggerType)] = &[
    ("CONTRACT_PRICE", OrderStopTriggerType::LastPrice),
    ("LAST_PRICE", OrderStopTriggerType::LastPrice),
    ("MARK_PRICE", OrderStopTriggerType::MarkPrice),
];

pub const REVERSE_ORDER_TRIGGER_TYPE: &[(OrderStopTriggerType, &str)] = &[
    (OrderStopTriggerType::LastPrice, "CONTRACT_PRICE"),
    (OrderStopTriggerType::MarkPrice, "MARK_PRICE"),
    (OrderStopTriggerType::IndexPrice, "MARK_PRICE"),
];
