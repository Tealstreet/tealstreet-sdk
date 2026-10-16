//! ts: bybitspot.types.ts
use serde_json::Value;
use std::sync::LazyLock;

pub const RECV_WINDOW: u64 = 20000;
pub const BROKER_ID: &str = "Tealstreet";
pub static BASE_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"livenet": "https://api.bybit.com", "testnet": "https://api-testnet.bybit.com"}),
);
pub static BASE_WS_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"public": {"spot": {"livenet": "wss://stream.bybit.com/v5/public/spot", "testnet": "wss://stream-testnet.bybit.com/v5/public/spot"}}, "private": {"livenet": "wss://stream.bybit.com/v5/private", "testnet": "wss://stream-testnet.bybit.com/v5/private"}}),
);
pub static DEMO_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"livenet": "https://api-demo.bybit.com", "testnet": "https://api-testnet.bybit.com"}),
);
pub static DEMO_WS_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"private": {"livenet": "wss://stream-demo.bybit.com/v5/private", "testnet": "wss://stream-testnet.bybit.com/v5/private"}}),
);
pub static ENDPOINTS: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"ACCOUNT_MARGIN": "/v5/account/info", "ACCOUNT": "/v5/user/query-api", "BALANCE": "/v5/account/wallet-balance", "FEE_RATE": "/v5/account/fee-rate", "UNFILLED_ORDERS": "/v5/order/realtime", "TICKERS": "/v5/market/tickers", "MARKETS": "/v5/market/instruments-info", "CANCEL_ORDER": "/v5/order/cancel", "CANCEL_ORDERS": "/v5/order/cancel-batch", "CANCEL_SYMBOL_ORDERS": "/v5/order/cancel-all", "POSITIONS": "/v5/position/list", "KLINE": "/v5/market/kline", "SET_LEVERAGE": "/v5/position/set-leverage", "SET_TRADING_STOP": "/v5/position/trading-stop", "CREATE_ORDER": "/v5/order/create", "REPLACE_ORDER": "/v5/order/amend", "SET_POSITION_MODE": "/v5/position/switch-mode", "SET_MARGIN_MODE": "/v5/account/set-margin-mode", "USER_ORDERS": "/v5/order/history", "USER_TRADES": "/v5/execution/list", "USER_CLOSED_PNL": "/v5/position/closed-pnl"}),
);
pub static GRAFANA_ENDPOINTS_EVERY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::json!([
        "/v5/order/create",
        "/v5/order/amend",
        "/v5/position/switch-mode",
        "/v5/account/set-margin-mode",
        "/v5/position/set-leverage",
        "/v5/order/cancel",
        "/v5/order/cancel-batch",
        "/v5/order/cancel-all",
        "/v5/position/trading-stop"
    ])
});
pub static GRAFANA_ENDPOINTS_PERIODIC: LazyLock<Value> = LazyLock::new(|| {
    serde_json::json!([
        "/v5/position/list",
        "/v5/order/realtime",
        "/v5/account/fee-rate",
        "/v5/user/query-api",
        "/v5/account/info",
        "/v5/account/wallet-balance"
    ])
});
pub static INTERVAL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"1m": "1", "3m": "3", "5m": "5", "15m": "15", "30m": "30", "1h": "60", "2h": "120", "4h": "240", "6h": "360", "12h": "720", "1d": "D", "1w": "W"}),
);
pub static ORDER_STATUS: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"Created": "open", "New": "open", "Active": "open", "Untriggered": "open", "PartiallyFilled": "open", "Rejected": "closed", "Filled": "closed", "Deactivated": "closed", "Triggered": "closed", "PendingCancel": "canceled", "Cancelled": "canceled"}),
);
pub static ORDER_TYPE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"Limit": "limit", "Market": "market", "Stop": "stop_market", "StopLoss": "stop_market", "TakeProfit": "take_profit_market", "PartialTakeProfit": "take_profit_market", "PartialStopLoss": "stop_market", "TrailingStop": "trailing_stop_market"}),
);
pub static ORDER_SIDE: LazyLock<Value> =
    LazyLock::new(|| serde_json::json!({"Buy": "buy", "Sell": "sell"}));
pub static POSITION_SIDE: LazyLock<Value> =
    LazyLock::new(|| serde_json::json!({"Buy": "long", "Sell": "short"}));
pub static ORDER_TIME_IN_FORCE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"GTC": "GoodTillCancel", "IOC": "ImmediateOrCancel", "FOK": "FillOrKill", "PostOnly": "PostOnly", "RPI": "PostOnly"}),
);
pub static ORDER_TIME_IN_FORCE_OUTBOUND: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"GoodTillCancel": "GTC", "ImmediateOrCancel": "IOC", "FillOrKill": "FOK", "PostOnly": "PostOnly"}),
);
pub static ORDER_TRIGGER_TYPE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"IndexPrice": "index_price", "MarkPrice": "mark_price", "LastPrice": "last_price"}),
);
