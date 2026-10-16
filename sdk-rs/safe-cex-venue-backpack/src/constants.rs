//! Backpack source tables, retaining TypeScript own-property order.

// ts: BASE_URLS
pub const BASE_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "https://api.backpack.exchange"),
    ("TESTNET", "https://api.backpack.exchange"),
];

// ts: WSS_URLS
pub const WSS_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "wss://ws.backpack.exchange"),
    ("TESTNET", "wss://ws.backpack.exchange"),
];

// ts: ENDPOINTS
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("TIME", "/api/v1/time"),
    ("STATUS", "/api/v1/status"),
    ("ASSETS", "/api/v1/assets"),
    ("MARKETS", "/api/v1/markets"),
    ("MARKET", "/api/v1/market"),
    ("TICKER", "/api/v1/ticker"),
    ("TICKERS", "/api/v1/tickers"),
    ("DEPTH", "/api/v1/depth"),
    ("KLINES", "/api/v1/klines"),
    ("TRADES", "/api/v1/trades"),
    ("TRADES_HISTORY", "/api/v1/trades/history"),
    ("MARK_PRICES", "/api/v1/markPrices"),
    ("OPEN_INTEREST", "/api/v1/openInterest"),
    ("FUNDING_RATES", "/api/v1/fundingRates"),
    ("CAPITAL", "/api/v1/capital"),
    ("CAPITAL_COLLATERAL", "/api/v1/capital/collateral"),
    ("ORDER", "/api/v1/order"),
    ("ORDERS", "/api/v1/orders"),
    ("POSITIONS", "/api/v1/position"),
    ("ACCOUNT", "/api/v1/account"),
    ("FILLS_HISTORY", "/wapi/v1/history/fills"),
    ("ORDERS_HISTORY", "/wapi/v1/history/orders"),
    ("DEPOSITS", "/wapi/v1/capital/deposits"),
    ("WITHDRAWALS", "/wapi/v1/capital/withdrawals"),
    ("DEPOSIT_ADDRESS", "/wapi/v1/capital/deposit/address"),
];

// ts: INSTRUCTIONS
pub const INSTRUCTIONS: &[(&str, &str)] = &[
    ("BALANCE_QUERY", "balanceQuery"),
    ("COLLATERAL_QUERY", "collateralQuery"),
    ("ORDER_QUERY", "orderQuery"),
    ("ORDER_QUERY_ALL", "orderQueryAll"),
    ("ORDER_EXECUTE", "orderExecute"),
    ("ORDER_CANCEL", "orderCancel"),
    ("ORDER_CANCEL_ALL", "orderCancelAll"),
    ("FILL_HISTORY_QUERY_ALL", "fillHistoryQueryAll"),
    ("ORDER_HISTORY_QUERY_ALL", "orderHistoryQueryAll"),
    ("POSITION_QUERY", "positionQuery"),
    ("ACCOUNT_QUERY", "accountQuery"),
    ("ACCOUNT_UPDATE", "accountUpdate"),
    ("DEPOSIT_QUERY_ALL", "depositQueryAll"),
    ("WITHDRAWAL_QUERY_ALL", "withdrawalQueryAll"),
    ("DEPOSIT_ADDRESS_QUERY", "depositAddressQuery"),
    ("SUBSCRIBE", "subscribe"),
];

// ts: GRAFANA_ENDPOINTS_EVERY
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[ENDPOINTS[16].1, ENDPOINTS[17].1];

// ts: GRAFANA_ENDPOINTS_PERIODIC
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[ENDPOINTS[14].1, ENDPOINTS[18].1];
