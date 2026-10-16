//! BingX protocol constants, in TypeScript own-property order.
//!
//! Mapping consumers must use `safe_cex_num::js_table_lookup` when the source
//! performs an ordinary object lookup, so inherited identities remain visible.
use safe_cex_types::{
    OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionSide,
};

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BASE_URL: [(&str, &str); 2] = [
    ("livenet", "https://open-api.bingx.com"),
    ("testnet", "https://open-api.bingx.com"),
];
pub const BASE_WS_URL: [(&str, [(&str, &str); 2]); 2] = [
    (
        "public",
        [
            ("livenet", "wss://ws-market-swap.qq-os.com/ws"),
            ("testnet", "wss://ws-market-swap.qq-os.com/ws"),
        ],
    ),
    (
        "private",
        [
            ("livenet", "wss://open-api-swap.bingx.com/swap-market"),
            ("testnet", "wss://open-api-swap.bingx.com/swap-market"),
        ],
    ),
];
pub const ENDPOINTS: [(&str, &str); 19] = [
    ("BALANCE", "/openApi/swap/v3/user/balance"),
    ("MARKETS", "/openApi/swap/v2/quote/contracts"),
    ("POSITIONS", "/openApi/swap/v2/user/positions"),
    ("LEVERAGE", "/openApi/swap/v2/trade/leverage"),
    ("TICKERS_24H", "/openApi/swap/v2/quote/ticker"),
    ("TICKERS_PRICE", "/openApi/swap/v2/quote/premiumIndex"),
    ("HEDGE_MODE", "/openApi/swap/v1/positionSide/dual"),
    ("MARGIN_MODE", "/openApi/swap/v2/trade/marginType"),
    ("SET_LEVERAGE", "/openApi/swap/v2/trade/leverage"),
    ("OPEN_ORDERS", "/openApi/swap/v2/trade/openOrders"),
    (
        "CANCEL_SYMBOL_ORDERS",
        "/openApi/swap/v2/trade/allOpenOrders",
    ),
    ("ORDER", "/openApi/swap/v2/trade/order"),
    ("AMEND_ORDER", "/openApi/swap/v1/trade/amend"),
    (
        "CANCEL_REPLACE_ORDER",
        "/openApi/swap/v1/trade/cancelReplace",
    ),
    ("BATCH_ORDERS", "/openApi/swap/v2/trade/batchOrders"),
    ("KLINE", "/openApi/swap/v3/quote/klines"),
    ("LISTEN_KEY", "/openApi/user/auth/userDataStream"),
    ("ORDERBOOK", "/openApi/swap/v2/quote/depth"),
    ("USER_TRADES", "/openApi/swap/v2/trade/fillHistory"),
];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 7] = [
    ENDPOINTS[11].1, // ORDER
    ENDPOINTS[12].1, // AMEND_ORDER
    ENDPOINTS[13].1, // CANCEL_REPLACE_ORDER
    ENDPOINTS[14].1, // BATCH_ORDERS
    ENDPOINTS[8].1,  // SET_LEVERAGE
    ENDPOINTS[10].1, // CANCEL_SYMBOL_ORDERS
    ENDPOINTS[6].1,  // HEDGE_MODE
];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 4] = [
    ENDPOINTS[0].1, // BALANCE
    ENDPOINTS[2].1, // POSITIONS
    ENDPOINTS[9].1, // OPEN_ORDERS
    ENDPOINTS[3].1, // LEVERAGE
];
pub const ORDER_TYPE: [(&str, OrderType); 6] = [
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("TRIGGER_MARKET", OrderType::StopLoss),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
pub const POSITION_SIDE: [(&str, PositionSide); 2] =
    [("LONG", PositionSide::Long), ("SHORT", PositionSide::Short)];
pub const TIME_IN_FORCE: [(&str, OrderTimeInForce); 4] = [
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
    ("PostOnly", OrderTimeInForce::PostOnly),
];
pub const ORDER_STATUS: [(&str, OrderStatus); 8] = [
    ("CANCELED", OrderStatus::Canceled),
    ("FILLED", OrderStatus::Closed),
    ("NEW", OrderStatus::Open),
    ("OPEN", OrderStatus::Open),
    ("PARTIALLY_FILLED", OrderStatus::Open),
    ("PENDING", OrderStatus::Open),
    ("WORKING", OrderStatus::Open),
    ("EXPIRED", OrderStatus::Closed),
];
pub const ORDER_TRIGGER_TYPE: [(&str, OrderStopTriggerType); 2] = [
    ("MARK_PRICE", OrderStopTriggerType::MarkPrice),
    ("CONTRACT_PRICE", OrderStopTriggerType::LastPrice),
];
