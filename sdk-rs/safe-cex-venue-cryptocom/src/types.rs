//! Mirrors cryptocom.types.ts configuration; implemented wire records live in wire.rs.
use safe_cex_num::prototype_realm::{PrototypeRealm, PrototypeValue};

const CANDLESTICK_INTERVALS: &[(&str, &str)] = &[
    ("1m", "1m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "1h"),
    ("2h", "2h"),
    ("4h", "4h"),
    ("12h", "12h"),
    ("1d", "1D"),
    ("1w", "7D"),
];

/// Source lookup with retained builtin referents, not metadata copies.
pub fn map_candlestick_interval(interval: &str) -> Result<PrototypeValue, String> {
    map_candlestick_interval_units(&interval.encode_utf16().collect::<Vec<_>>()).map_err(|units| {
        String::from_utf16(&units).expect("UTF-8 input gives a well-formed diagnostic")
    })
}

/// Lossless input/error ABI; primitive mappings and inherited referents share one realm.
pub fn map_candlestick_interval_units(interval: &[u16]) -> Result<PrototypeValue, Vec<u16>> {
    let text = String::from_utf16(interval).ok();
    let mapped = text.as_deref().map_or(PrototypeValue::Undefined, |key| {
        CANDLESTICK_INTERVALS
            .iter()
            .find(|(name, _)| *name == key)
            .map_or_else(
                || PrototypeRealm::current().inherited(key),
                |(_, value)| PrototypeValue::String((*value).into()),
            )
    });
    if !mapped.truthy() {
        let mut message = "Unsupported Crypto.com candlestick interval: "
            .encode_utf16()
            .collect::<Vec<_>>();
        message.extend_from_slice(interval);
        return Err(message);
    }
    Ok(mapped)
}

pub const BASE_URLS: &[(&str, &str)] = &[
    ("PRODUCTION", "https://api.crypto.com/exchange/v1"),
    ("TESTNET", "https://uat-api.3ona.co/exchange/v1"),
];

pub const WSS_URLS: &[(&str, &[(&str, &str)])] = &[
    (
        "PRODUCTION",
        &[
            ("USER", "wss://stream.crypto.com/exchange/v1/user"),
            ("MARKET", "wss://stream.crypto.com/exchange/v1/market"),
        ],
    ),
    (
        "TESTNET",
        &[
            ("USER", "wss://uat-stream.3ona.co/exchange/v1/user"),
            ("MARKET", "wss://uat-stream.3ona.co/exchange/v1/market"),
        ],
    ),
];

pub const CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS: f64 = 30_000.0;
pub const CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS: f64 = 90_000.0;

pub const ENDPOINTS: &[(&str, &str)] = &[
    ("INSTRUMENTS", "/public/get-instruments"),
    ("BOOK", "/public/get-book"),
    ("TICKERS", "/public/get-tickers"),
    ("TICKER", "/public/get-ticker"),
    ("TRADES", "/public/get-trades"),
    ("CANDLESTICK", "/public/get-candlestick"),
    ("VALUATIONS", "/public/get-valuations"),
    (
        "EXPIRED_SETTLEMENT_PRICE",
        "/public/get-expired-settlement-price",
    ),
    ("INSURANCE", "/public/get-insurance"),
    ("ACCOUNTS", "/private/get-accounts"),
    ("CREATE_ORDER", "/private/create-order"),
    ("ADVANCED_CREATE_ORDER", "/private/advanced/create-order"),
    ("ADVANCED_AMEND_ORDER", "/private/advanced/amend-order"),
    ("ADVANCED_CANCEL_ORDER", "/private/advanced/cancel-order"),
    ("ADVANCED_OPEN_ORDERS", "/private/advanced/get-open-orders"),
    ("AMEND_ORDER", "/private/amend-order"),
    ("CANCEL_ORDER", "/private/cancel-order"),
    ("CANCEL_ALL_ORDERS", "/private/cancel-all-orders"),
    ("CLOSE_POSITION", "/private/close-position"),
    ("ORDER_DETAIL", "/private/get-order-detail"),
    ("OPEN_ORDERS", "/private/get-open-orders"),
    ("ORDER_HISTORY", "/private/get-order-history"),
    ("TRADES_HISTORY", "/private/get-trades"),
    ("POSITIONS", "/private/get-positions"),
    ("CHANGE_LEVERAGE", "/private/change-account-leverage"),
    ("BALANCE", "/private/user-balance"),
    ("BALANCE_HISTORY", "/private/user-balance-history"),
    ("POSITION_BALANCE", "/private/get-position-balance"),
    ("CREATE_WITHDRAWAL", "/private/create-withdrawal"),
    ("GET_DEPOSIT_ADDRESS", "/private/get-deposit-address"),
    ("GET_WITHDRAWAL_HISTORY", "/private/get-withdrawal-history"),
    ("GET_DEPOSIT_HISTORY", "/private/get-deposit-history"),
    ("GET_CURRENCY_NETWORKS", "/private/get-currency-networks"),
    ("SUBACCOUNT_BALANCES", "/private/get-subaccount-balances"),
    (
        "CREATE_SUBACCOUNT_TRANSFER",
        "/private/create-subaccount-transfer",
    ),
    (
        "GET_SUBACCOUNT_TRANSFER_HISTORY",
        "/private/get-subaccount-transfer-history",
    ),
];

pub const PUBLIC_ENDPOINTS: crate::public_endpoints::PublicEndpoints =
    crate::public_endpoints::PublicEndpoints::module();

pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    ENDPOINTS[10].1,
    ENDPOINTS[15].1,
    ENDPOINTS[16].1,
    ENDPOINTS[17].1,
    ENDPOINTS[18].1,
];

pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    ENDPOINTS[9].1,
    ENDPOINTS[25].1,
    ENDPOINTS[23].1,
    ENDPOINTS[20].1,
];
