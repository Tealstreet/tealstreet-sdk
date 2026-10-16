//! Protocol values mirror the live TypeScript tables.
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide};
// ts: RECV_WINDOW
pub const RECV_WINDOW: u64 = 20_000;
// ts: BROKER_ID
pub const BROKER_ID: &str = "3zexp";
// ts: BASE_URL
pub const BASE_URL: &str = "https://api-futures.kucoin.com";
// ts: ENDPOINTS
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("ACCOUNT_SPOT", "/api/v1/account-overview"),
    ("ACCOUNT", "/api/v1/account-overview"),
    ("BALANCE", "/api/v1/account-overview"),
    ("MARKETS", "/api/v1/contracts/active"),
    ("SYMBOL_LEVERAGE", "/api/v1/position/risk/limit"),
    ("TICKERS", "/api/v1/allTickers"),
    ("ORDERBOOK", "/api/v1/level2/snapshot"),
    ("POSITIONS", "/api/v1/positions"),
    ("POSITIONS_V2", "/api/v1/positions"),
    ("KLINE", "/api/v1/kline/query"),
    ("ORDERS", "/api/v1/orders"),
    ("CANCEL_ALGO_ORDER", "/api/v1/stopOrders/{orderId}"),
    ("CANCEL_ORDERS", "/api/v1/orders"),
    ("CANCEL_ALL_ORDERS", "/api/v3/orders"),
    ("CANCEL_ALL_STOP_ORDERS", "/api/v1/stopOrders"),
    ("CANCEL_SYMBOL_ORDERS", "/api/v3/orders"),
    ("PLACE_ORDER", "/api/v1/orders"),
    ("BATCH_ORDERS", "/api/v1/orders/multi"),
    ("ALGO_ORDERS", "/api/v1/stopOrders"),
    ("PLACE_ALGO_ORDER", "/api/v1/st-orders"),
    ("UPDATE_ORDER", "/api/v1/orders"),
    ("UPDATE_ALGO_ORDER", "/api/v1/stopOrders"),
    ("SET_LEVERAGE", "/api/v2/changeCrossUserLeverage"),
    ("POSITION_LEVERAGE", "/api/v2/getCrossUserLeverage"),
    ("SET_POSITION_MODE", "/api/v2/position/switchPositionMode"),
    ("SET_MARGIN_MODE", "/api/v2/position/changeMarginMode"),
    ("PLACE_TRAILING_STOP_ORDER", "/api/v1/stop-order"),
    ("GET_MARGIN_MODE", "/api/v2/position/getMarginMode"),
    ("GET_POSITION_MODE", "/api/v2/position/getPositionMode"),
    ("FETCH_SINGLE_ACCOUNT", "/api/v2/position"),
    ("GET_PUBLIC_WS_TOKEN", "/api/v1/bullet-public"),
    ("GET_PRIVATE_WS_TOKEN", "/api/v1/bullet-private"),
];
// ts: GRAFANA_ENDPOINTS_EVERY
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    ENDPOINTS[4].1,
    ENDPOINTS[11].1,
    ENDPOINTS[12].1,
    ENDPOINTS[13].1,
    ENDPOINTS[14].1,
    ENDPOINTS[15].1,
    ENDPOINTS[16].1,
    ENDPOINTS[17].1,
    ENDPOINTS[18].1,
    ENDPOINTS[19].1,
    ENDPOINTS[20].1,
    ENDPOINTS[21].1,
    ENDPOINTS[22].1,
    ENDPOINTS[24].1,
    ENDPOINTS[25].1,
    ENDPOINTS[26].1,
];
// ts: GRAFANA_ENDPOINTS_PERIODIC
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    ENDPOINTS[1].1,
    ENDPOINTS[0].1,
    ENDPOINTS[2].1,
    ENDPOINTS[6].1,
    ENDPOINTS[7].1,
    ENDPOINTS[8].1,
    ENDPOINTS[10].1,
    ENDPOINTS[29].1,
    ENDPOINTS[27].1,
    ENDPOINTS[28].1,
    ENDPOINTS[23].1,
];
// ts: INTERVAL
pub const INTERVAL: &[(&str, &str)] = &[
    ("1m", "1"),
    ("3m", "3"),
    ("5m", "5"),
    ("15m", "15"),
    ("30m", "30"),
    ("1h", "60"),
    ("2h", "120"),
    ("4h", "240"),
    ("6h", "360"),
    ("12h", "720"),
    ("1d", "1440"),
    ("1w", "10080"),
];
// ts: WS_INTERVAL
pub const WS_INTERVAL: &[(&str, &str)] = &[
    ("1m", "1min"),
    ("3m", "3min"),
    ("5m", "5min"),
    ("15m", "15min"),
    ("30m", "30min"),
    ("1h", "1hour"),
    ("2h", "2hour"),
    ("4h", "4hour"),
    ("6h", "6hour"),
    ("12h", "12hour"),
    ("1d", "1day"),
    ("1w", "1week"),
];
// ts: POSITION_SIDE
pub const POSITION_SIDE: &[(&str, PositionSide)] =
    &[("long", PositionSide::Long), ("short", PositionSide::Short)];
// ts: ORDER_STATUS
pub const ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("init", OrderStatus::Open),
    ("new", OrderStatus::Open),
    ("open", OrderStatus::Open),
    ("match", OrderStatus::Open),
    ("not_trigger", OrderStatus::Open),
    ("partially_filled", OrderStatus::Open),
    ("filled", OrderStatus::Closed),
    ("canceled", OrderStatus::Canceled),
    ("cancelled", OrderStatus::Canceled),
    ("done", OrderStatus::Closed),
];
// ts: ORDER_TYPE
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("limit", OrderType::Limit),
    ("market", OrderType::Market),
    ("pos_profit", OrderType::TakeProfit),
    ("pos_loss", OrderType::StopLoss),
    ("loss_plan", OrderType::StopLoss),
    ("profit_plan", OrderType::TakeProfit),
    ("track_plan", OrderType::TrailingStopLoss),
    ("tp", OrderType::TakeProfit),
    ("sl", OrderType::StopLoss),
    ("psl", OrderType::StopLoss),
    ("ptp", OrderType::TakeProfit),
    ("track", OrderType::TrailingStopLoss),
];
// ts: ORDER_SIDE
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[
    ("open_long", OrderSide::Buy),
    ("open_short", OrderSide::Sell),
    ("close_long", OrderSide::Sell),
    ("close_short", OrderSide::Buy),
    ("reduce_close_long", OrderSide::Sell),
    ("reduce_close_short", OrderSide::Buy),
    ("offset_close_long", OrderSide::Sell),
    ("offset_close_short", OrderSide::Buy),
    ("burst_close_long", OrderSide::Sell),
    ("burst_close_short", OrderSide::Buy),
    ("delivery_close_long", OrderSide::Sell),
    ("delivery_close_short", OrderSide::Buy),
    ("buy_single", OrderSide::Buy),
    ("sell_single", OrderSide::Sell),
    ("buy", OrderSide::Buy),
    ("sell", OrderSide::Sell),
];
// ts: TIME_IN_FORCE
pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("normal", OrderTimeInForce::GoodTillCancel),
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("post_only", OrderTimeInForce::PostOnly),
    ("fok", OrderTimeInForce::FillOrKill),
    ("ioc", OrderTimeInForce::ImmediateOrCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("FOK", OrderTimeInForce::FillOrKill),
];
