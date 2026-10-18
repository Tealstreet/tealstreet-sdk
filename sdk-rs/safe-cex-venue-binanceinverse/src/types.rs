//! binanceinverse.types.ts constants over the canonical Js graph.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://dapi.binance.com"),
    ("testnet", "https://demo-dapi.binance.com"),
];
pub const BASE_WS_URL: &[(&str, &str)] = &[
    ("livenet", "wss://dstream.binance.com/ws"),
    ("testnet", "wss://dstream.binancefuture.com/ws"),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("BALANCE", "/dapi/v1/balance"),
    ("MARKETS", "/dapi/v1/exchangeInfo"),
    ("ACCOUNT", "/dapi/v1/account"),
    ("LEVERAGE_BRACKET", "/dapi/v2/leverageBracket"),
    ("TICKERS_24H", "/dapi/v1/ticker/24hr"),
    ("TICKERS_BOOK", "/dapi/v1/ticker/bookTicker"),
    ("TICKERS_PRICE", "/dapi/v1/premiumIndex"),
    ("HEDGE_MODE", "/dapi/v1/positionSide/dual"),
    ("SET_LEVERAGE", "/dapi/v1/leverage"),
    ("OPEN_ORDERS", "/dapi/v1/openOrders"),
    ("CANCEL_SYMBOL_ORDERS", "/dapi/v1/allOpenOrders"),
    ("ORDER", "/dapi/v1/order"),
    ("BATCH_ORDERS", "/dapi/v1/batchOrders"),
    ("KLINE", "/dapi/v1/klines"),
    ("LISTEN_KEY", "/dapi/v1/listenKey"),
    ("ORDERBOOK", "/dapi/v1/depth"),
    ("ALL_ORDERS", "/dapi/v1/allOrders"),
    ("USER_TRADES", "/dapi/v1/userTrades"),
    ("SET_MARGIN_TYPE", "/dapi/v1/marginType"),
    ("POSITION_RISK", "/dapi/v1/positionRisk"),
    ("TRADES", "/dapi/v1/trades"),
    ("INCOME", "/dapi/v1/income"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "MARKETS",
    "TICKERS_24H",
    "TICKERS_BOOK",
    "TICKERS_PRICE",
    "KLINE",
    "ORDERBOOK",
    "TRADES",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "ORDER",
    "BATCH_ORDERS",
    "SET_LEVERAGE",
    "SET_MARGIN_TYPE",
    "CANCEL_SYMBOL_ORDERS",
    "HEDGE_MODE",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "BALANCE",
    "ACCOUNT",
    "LEVERAGE_BRACKET",
    "OPEN_ORDERS",
    "POSITION_RISK",
];
pub const ORDER_TYPE: &[(&str, &str)] = &[
    ("LIMIT", "limit"),
    ("MARKET", "market"),
    ("STOP_MARKET", "stop_market"),
    ("TAKE_PROFIT_MARKET", "take_profit_market"),
    ("TRAILING_STOP_MARKET", "trailing_stop_market"),
];
pub const ORDER_SIDE: &[(&str, &str)] = &[("BUY", "buy"), ("SELL", "sell")];
pub const POSITION_SIDE: &[(&str, &str)] = &[("LONG", "long"), ("SHORT", "short")];
pub const TIME_IN_FORCE: &[(&str, &str)] = &[
    ("GTC", "GoodTillCancel"),
    ("IOC", "ImmediateOrCancel"),
    ("FOK", "FillOrKill"),
    ("GTX", "PostOnly"),
];
pub const ORDER_TRIGGER_TYPE: &[(&str, &str)] = &[
    ("MARK_PRICE", "mark_price"),
    ("CONTRACT_PRICE", "last_price"),
];

fn object(rows: &[(&str, &str)]) -> Js {
    Js::object(
        rows.iter()
            .map(|(key, value)| (*key, Js::str(value)))
            .collect(),
    )
}
pub(crate) fn install(globals: &Js) -> Result<(), Thrown> {
    globals.define("RECV_WINDOW", Js::Number(RECV_WINDOW))?;
    for (name, rows) in [
        ("BASE_URL", BASE_URL),
        ("BASE_WS_URL", BASE_WS_URL),
        ("ENDPOINTS", ENDPOINTS),
        ("ORDER_TYPE", ORDER_TYPE),
        ("ORDER_SIDE", ORDER_SIDE),
        ("POSITION_SIDE", POSITION_SIDE),
        ("TIME_IN_FORCE", TIME_IN_FORCE),
        ("ORDER_TRIGGER_TYPE", ORDER_TRIGGER_TYPE),
    ] {
        globals.define(name, object(rows))?;
    }
    for (name, keys) in [
        ("PUBLIC_ENDPOINTS", PUBLIC_ENDPOINTS),
        ("GRAFANA_ENDPOINTS_EVERY", GRAFANA_ENDPOINTS_EVERY),
        ("GRAFANA_ENDPOINTS_PERIODIC", GRAFANA_ENDPOINTS_PERIODIC),
    ] {
        let values = keys
            .iter()
            .map(|key| {
                let (_, path) = ENDPOINTS
                    .iter()
                    .find(|(name, _)| name == key)
                    .expect("source endpoint");
                Js::str(path)
            })
            .collect();
        globals.define(name, Js::array(values))?;
    }
    Ok(())
}
