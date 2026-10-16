//! `krakenspot.types.ts`: retains own-key order and inherited lookup identities.
use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_types::{OrderSide, OrderTimeInForce, OrderType};
use std::borrow::Cow;

pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://api.kraken.com"),
    ("testnet", "https://api.kraken.com"),
];
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://ws.kraken.com/v2"),
            ("testnet", "wss://ws.kraken.com/v2"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://ws-auth.kraken.com/v2"),
            ("testnet", "wss://ws-auth.kraken.com/v2"),
        ],
    ),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("ASSET_PAIRS", "/0/public/AssetPairs"),
    ("TICKER", "/0/public/Ticker"),
    ("OHLC", "/0/public/OHLC"),
    ("ORDERBOOK", "/0/public/Depth"),
    ("TRADES", "/0/public/Trades"),
    ("SERVER_TIME", "/0/public/Time"),
    ("BALANCE", "/0/private/Balance"),
    ("EXTENDED_BALANCE", "/0/private/BalanceEx"),
    ("TRADE_BALANCE", "/0/private/TradeBalance"),
    ("OPEN_ORDERS", "/0/private/OpenOrders"),
    ("CLOSED_ORDERS", "/0/private/ClosedOrders"),
    ("ADD_ORDER", "/0/private/AddOrder"),
    ("AMEND_ORDER", "/0/private/AmendOrder"),
    ("CANCEL_ORDER", "/0/private/CancelOrder"),
    ("CANCEL_ALL", "/0/private/CancelAll"),
    ("TRADES_HISTORY", "/0/private/TradesHistory"),
    ("OPEN_POSITIONS", "/0/private/OpenPositions"),
    ("WS_TOKEN", "/0/private/GetWebSocketsToken"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/0/public/AssetPairs",
    "/0/public/Ticker",
    "/0/public/OHLC",
    "/0/public/Depth",
    "/0/public/Trades",
    "/0/public/Time",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/0/private/AddOrder",
    "/0/private/AmendOrder",
    "/0/private/CancelOrder",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/0/private/Balance",
    "/0/private/OpenOrders",
    "/0/private/TradeBalance",
];
pub const INTERVAL: &[(&str, f64)] = &[
    ("1m", 1.0),
    ("3m", 1.0),
    ("5m", 5.0),
    ("15m", 15.0),
    ("30m", 30.0),
    ("1h", 60.0),
    ("2h", 60.0),
    ("4h", 240.0),
    ("6h", 240.0),
    ("12h", 240.0),
    ("1d", 1440.0),
    ("1w", 10080.0),
];
pub const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("limit", OrderType::Limit),
    ("market", OrderType::Market),
    ("stop-loss", OrderType::StopLoss),
    ("take-profit", OrderType::TakeProfit),
    ("stop-loss-limit", OrderType::StopLoss),
    ("take-profit-limit", OrderType::TakeProfit),
    ("trailing-stop", OrderType::TrailingStopLoss),
    ("trailing-stop-limit", OrderType::TrailingStopLoss),
];
pub const ORDER_SIDE: &[(&str, OrderSide)] = &[("buy", OrderSide::Buy), ("sell", OrderSide::Sell)];
pub const TIME_IN_FORCE: &[(&str, OrderTimeInForce)] = &[
    ("GTC", OrderTimeInForce::GoodTillCancel),
    ("IOC", OrderTimeInForce::ImmediateOrCancel),
    ("GTD", OrderTimeInForce::GoodTillCancel),
];
pub const ASSET_ALIASES: &[(&str, &str)] = &[
    ("XBT", "BTC"),
    ("XXBT", "BTC"),
    ("XETH", "ETH"),
    ("XXRP", "XRP"),
    ("XLTC", "LTC"),
    ("XXLM", "XLM"),
    ("XXMR", "XMR"),
    ("XZEC", "ZEC"),
    ("XREP", "REP"),
    ("XETC", "ETC"),
    ("XDAO", "DAO"),
    ("XMLN", "MLN"),
    ("ZUSD", "USD"),
    ("ZEUR", "EUR"),
    ("ZGBP", "GBP"),
    ("ZJPY", "JPY"),
    ("ZCAD", "CAD"),
    ("ZAUD", "AUD"),
];

/// JS `ASSET_ALIASES[asset] || asset`, including inherited non-string values.
/// UTF-16 keeps unmatched strings verbatim, including lone surrogate code units.
pub fn normalize_asset(asset: &[u16]) -> TableHit<Cow<'_, [u16]>> {
    if let Ok(key) = String::from_utf16(asset) {
        match js_table_lookup(ASSET_ALIASES, &key) {
            TableHit::Mapped(value) if !value.is_empty() => {
                return TableHit::Mapped(Cow::Owned(value.encode_utf16().collect()));
            }
            TableHit::Inherited(member) => return TableHit::Inherited(member),
            _ => {}
        }
    }
    TableHit::Mapped(Cow::Borrowed(asset))
}
