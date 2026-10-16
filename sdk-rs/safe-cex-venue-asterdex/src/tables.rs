//! Protocol table values from asterdex.types.ts; live oracle checks every leaf.
pub const ASTERDEX_ENDPOINTS: &[(&str, &str)] = &[
    ("REST.PRODUCTION", "https://fapi.asterdex.com"),
    ("REST.TESTNET", "https://fapi.asterdex-testnet.com"),
    ("WS.PUBLIC.PRODUCTION", "wss://fstream.asterdex.com/stream"),
    (
        "WS.PUBLIC.TESTNET",
        "wss://fstream5.asterdex-testnet.com/stream",
    ),
    ("WS.PRIVATE.PRODUCTION", "wss://fstream.asterdex.com/stream"),
    (
        "WS.PRIVATE.TESTNET",
        "wss://fstream5.asterdex-testnet.com/stream",
    ),
];
pub const ASTERDEX_STP_MODES: &[&str] = &["EXPIRE_TAKER", "EXPIRE_MAKER", "EXPIRE_BOTH"];
pub const ASTERDEX_MAIN_WALLET_REQUIRED_HINT: &str =
    "Aster API Wallet/vault keys require the Main Wallet Address";
pub const ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE: &str = "Aster API Wallet/vault keys require the Main Wallet Address. Fill Main Wallet Address with your login wallet — do not replace the main wallet with the API/vault wallet.";
pub const ASTERDEX_RATE_LIMIT_ENDPOINTS: &[(&str, &str)] = &[
    ("HEAVY.0", "/order"),
    ("HEAVY.1", "/batch"),
    ("HEAVY.2", "/position"),
    ("MEDIUM.0", "/account"),
    ("MEDIUM.1", "/balance"),
    ("MEDIUM.2", "/trades"),
    ("LIGHT.0", "/ticker"),
    ("LIGHT.1", "/klines"),
    ("LIGHT.2", "/depth"),
];
pub const ASTERDEX_ORDER_TYPE_MAP: &[(&str, &str)] = &[
    ("limit", "LIMIT"),
    ("market", "MARKET"),
    ("stop_market", "STOP_MARKET"),
    ("take_profit_market", "TAKE_PROFIT_MARKET"),
    ("trailing_stop_market", "TRAILING_STOP_MARKET"),
];
pub const ASTERDEX_ORDER_SIDE_MAP: &[(&str, &str)] = &[("buy", "BUY"), ("sell", "SELL")];
pub const ASTERDEX_POSITION_SIDE_MAP: &[(&str, &str)] =
    &[("LONG", "LONG"), ("SHORT", "SHORT"), ("BOTH", "BOTH")];
pub const ASTERDEX_TIME_IN_FORCE_MAP: &[(&str, &str)] = &[
    ("GoodTillCancel", "GTC"),
    ("ImmediateOrCancel", "IOC"),
    ("FillOrKill", "FOK"),
    ("PostOnly", "GTX"),
];
pub const ASTERDEX_ORDER_STATUS_MAP: &[(&str, &str)] = &[
    ("NEW", "NEW"),
    ("PARTIALLY_FILLED", "PARTIALLY_FILLED"),
    ("FILLED", "FILLED"),
    ("CANCELED", "CANCELED"),
    ("REJECTED", "REJECTED"),
    ("EXPIRED", "EXPIRED"),
];
