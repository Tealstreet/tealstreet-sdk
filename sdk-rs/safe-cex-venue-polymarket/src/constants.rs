//! Literal declarations from polymarket.constants.ts. Dictionary tables preserve
//! own-entry order; dynamic lookups must use safe_cex_num::js_table_lookup.
use safe_cex_types::{OrderSide, OrderType};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PolymarketContracts {
    pub exchange_v2: &'static str,
    pub neg_risk_exchange_v2: &'static str,
    pub pusd: &'static str,
    pub ctf: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PolymarketFees {
    pub default_fee_bps: &'static str,
    pub taker_fee_bps: f64,
    pub maker_fee_bps: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PolymarketConfig {
    pub rest_base: &'static str,
    pub ws_clob: &'static str,
    pub ws_market: &'static str,
    pub ws_user: &'static str,
    pub ws_live: &'static str,
    pub gamma_api: &'static str,
    pub data_api: &'static str,
    pub chain_id: f64,
    pub contracts: PolymarketContracts,
    pub fees: PolymarketFees,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip712Domain {
    pub name: &'static str,
    pub version: &'static str,
    pub chain_id: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct WsConfig {
    pub ping_interval: f64,
    pub pong_timeout: f64,
    pub reconnect_delay: f64,
    pub max_reconnect_attempts: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct RateLimits {
    pub public_rps: f64,
    pub private_rps: f64,
    pub orders_per_second: f64,
}

pub const POLYMARKET_CONFIG: PolymarketConfig = PolymarketConfig {
    rest_base: "https://clob.polymarket.com",
    ws_clob: "wss://ws-subscriptions-clob.polymarket.com",
    ws_market: "wss://ws-subscriptions-clob.polymarket.com/ws/market",
    ws_user: "wss://ws-subscriptions-clob.polymarket.com/ws/user",
    ws_live: "wss://ws-live-data.polymarket.com",
    gamma_api: "https://gamma-api.polymarket.com",
    data_api: "https://data-api.polymarket.com",
    chain_id: 137.0,
    contracts: PolymarketContracts {
        exchange_v2: "0xE111180000d2663C0091e4f400237545B87B996B",
        neg_risk_exchange_v2: "0xe2222d279d744050d28e00520010520000310F59",
        pusd: "0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB",
        ctf: "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045",
    },
    fees: PolymarketFees {
        default_fee_bps: "0",
        taker_fee_bps: 0.0,
        maker_fee_bps: 0.0,
    },
};

pub const EIP712_DOMAIN: Eip712Domain = Eip712Domain {
    name: "Polymarket CTF Exchange",
    version: "2",
    chain_id: 137.0,
};

pub const WS_CONFIG: WsConfig = WsConfig {
    ping_interval: 10000.0,
    pong_timeout: 10000.0,
    reconnect_delay: 1000.0,
    max_reconnect_attempts: 10.0,
};

pub const RATE_LIMITS: RateLimits = RateLimits {
    public_rps: 10.0,
    private_rps: 5.0,
    orders_per_second: 5.0,
};

pub const ENDPOINTS: &[(&str, &str)] = &[
    ("MARKETS", "/markets"),
    ("SIMPLIFIED_MARKETS", "/simplified-markets"),
    ("ORDER_BOOK", "/book"),
    ("ORDER_BOOKS", "/books"),
    ("PRICE", "/price"),
    ("PRICES", "/prices"),
    ("MIDPOINT", "/midpoint"),
    ("MIDPOINTS", "/midpoints"),
    ("SPREAD", "/spread"),
    ("SPREADS", "/spreads"),
    ("LAST_TRADE_PRICE", "/last-trade-price"),
    ("PRICE_HISTORY", "/prices-history"),
    ("TIME", "/time"),
    ("ORDER", "/order"),
    ("ORDERS", "/orders"),
    ("CANCEL", "/order"),
    ("CANCEL_ORDERS", "/orders"),
    ("CANCEL_ALL", "/cancel-all"),
    ("CANCEL_MARKET_ORDERS", "/cancel-market-orders"),
    ("OPEN_ORDERS", "/data/orders"),
    ("GET_ORDER", "/data/order/"),
    ("TRADES", "/data/trades"),
    ("POSITIONS", "/positions"),
    ("DERIVE_API_KEY", "/auth/derive-api-key"),
    ("DELETE_API_KEY", "/auth/api-key"),
    ("API_KEYS", "/auth/api-keys"),
    ("NONCE", "/auth/nonce"),
    ("BALANCE_ALLOWANCE", "/balance-allowance"),
];

pub const ORDER_TYPE_MAP: &[(&str, OrderType)] = &[
    ("GTC", OrderType::Limit),
    ("GTD", OrderType::Limit),
    ("FOK", OrderType::Market),
    ("FAK", OrderType::Market),
];

pub const ORDER_SIDE_MAP: &[(&str, OrderSide)] =
    &[("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];

pub const ORDER_SIDE_REVERSE: &[(OrderSide, &str)] =
    &[(OrderSide::Buy, "BUY"), (OrderSide::Sell, "SELL")];

pub const ORDER_STATUS_MAP: &[(&str, &str)] = &[
    ("live", "open"),
    ("LIVE", "open"),
    ("matched", "closed"),
    ("MATCHED", "closed"),
    ("cancelled", "canceled"),
    ("canceled", "canceled"),
    ("CANCELLED", "canceled"),
    ("CANCELED", "canceled"),
    ("delayed", "open"),
    ("DELAYED", "open"),
    ("unmatched", "open"),
    ("UNMATCHED", "open"),
];

pub const WS_MESSAGE_TYPES: &[(&str, &str)] = &[
    ("USER", "user"),
    ("MARKET", "market"),
    ("SUBSCRIBE", "subscribe"),
    ("UNSUBSCRIBE", "unsubscribe"),
    ("PRICE_CHANGE", "price_change"),
    ("ORDER_BOOK_UPDATE", "order_book_update"),
    ("TRADE", "trade"),
    ("ORDER_UPDATE", "order_update"),
    ("POSITION_UPDATE", "position_update"),
];

pub const TIMEFRAME_MAP: &[(&str, &str)] = &[
    ("1m", "1m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("1h", "1h"),
    ("4h", "4h"),
    ("1d", "1d"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignatureType {
    Eoa = 0,
    PolyProxy = 1,
    PolyGnosisSafe = 2,
    Poly1271 = 3,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TypedDataField {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub field_type: &'static str,
}

pub const EIP712_ORDER_TYPES: &[(&str, &[TypedDataField])] = &[(
    "Order",
    &[
        TypedDataField {
            name: "salt",
            field_type: "uint256",
        },
        TypedDataField {
            name: "maker",
            field_type: "address",
        },
        TypedDataField {
            name: "signer",
            field_type: "address",
        },
        TypedDataField {
            name: "tokenId",
            field_type: "uint256",
        },
        TypedDataField {
            name: "makerAmount",
            field_type: "uint256",
        },
        TypedDataField {
            name: "takerAmount",
            field_type: "uint256",
        },
        TypedDataField {
            name: "side",
            field_type: "uint8",
        },
        TypedDataField {
            name: "signatureType",
            field_type: "uint8",
        },
        TypedDataField {
            name: "timestamp",
            field_type: "uint256",
        },
        TypedDataField {
            name: "metadata",
            field_type: "bytes32",
        },
        TypedDataField {
            name: "builder",
            field_type: "bytes32",
        },
    ],
)];
