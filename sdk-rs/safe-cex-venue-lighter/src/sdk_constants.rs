//! Additional protocol constants from the Lighter SDK.
use num_bigint::BigInt;
use std::sync::LazyLock;

// ts: ChainId
pub const CHAIN_ID: [(&str, f64); 10] = [
    ("Mainnet", 1.0),
    ("Sepolia", 11155111.0),
    ("ArbitrumOne", 42161.0),
    ("ArbitrumSepolia", 421614.0),
    ("Optimism", 10.0),
    ("OptimismSepolia", 11155420.0),
    ("Polygon", 137.0),
    ("PolygonMumbai", 80001.0),
    ("Base", 8453.0),
    ("BaseSepolia", 84532.0),
];

// ts: ApiEndpoints
pub const API_ENDPOINTS: [(&str, &str); 4] = [
    ("Production", "https://api.lighter.xyz"),
    ("Staging", "https://api-staging.lighter.xyz"),
    ("Testnet", "https://api-testnet.lighter.xyz"),
    ("Development", "http://localhost:8080"),
];

// ts: WsEndpoints
pub const WS_ENDPOINTS: [(&str, &str); 4] = [
    ("Production", "wss://ws.lighter.xyz"),
    ("Staging", "wss://ws-staging.lighter.xyz"),
    ("Testnet", "wss://ws-testnet.lighter.xyz"),
    ("Development", "ws://localhost:8081"),
];

// ts: Defaults
pub const DEFAULTS: [(&str, f64); 11] = [
    ("RequestTimeout", 30000.0),
    ("WsPingInterval", 30000.0),
    ("WsReconnectDelay", 5000.0),
    ("WsMaxReconnectAttempts", 10.0),
    ("MaxRetryAttempts", 3.0),
    ("RetryDelay", 1000.0),
    ("PageSize", 50.0),
    ("MaxPageSize", 1000.0),
    ("AuthTokenValidity", 3600.0),
    ("OrderExpiry", 86400.0),
    ("MinAuthTokenFuture", 60.0),
];

// ts: RateLimits
pub const RATE_LIMITS: [(&str, f64); 4] = [
    ("Public", 60.0),
    ("Authenticated", 300.0),
    ("Trading", 600.0),
    ("WebSocket", 100.0),
];

// ts: HttpStatus
pub const HTTP_STATUS: [(&str, f64); 14] = [
    ("OK", 200.0),
    ("Created", 201.0),
    ("Accepted", 202.0),
    ("NoContent", 204.0),
    ("BadRequest", 400.0),
    ("Unauthorized", 401.0),
    ("Forbidden", 403.0),
    ("NotFound", 404.0),
    ("Conflict", 409.0),
    ("TooManyRequests", 429.0),
    ("InternalServerError", 500.0),
    ("BadGateway", 502.0),
    ("ServiceUnavailable", 503.0),
    ("GatewayTimeout", 504.0),
];

// ts: ErrorCode
pub const ERROR_CODE: [(&str, &str); 15] = [
    ("NetworkError", "NETWORK_ERROR"),
    ("Timeout", "TIMEOUT"),
    ("InvalidParameter", "INVALID_PARAMETER"),
    ("InvalidSignature", "INVALID_SIGNATURE"),
    ("InsufficientBalance", "INSUFFICIENT_BALANCE"),
    ("OrderNotFound", "ORDER_NOT_FOUND"),
    ("MarketNotFound", "MARKET_NOT_FOUND"),
    ("NonceTooLow", "NONCE_TOO_LOW"),
    ("NonceTooHigh", "NONCE_TOO_HIGH"),
    ("TransactionExpired", "TX_EXPIRED"),
    ("RateLimitExceeded", "RATE_LIMIT_EXCEEDED"),
    ("AuthenticationFailed", "AUTH_FAILED"),
    ("CryptoNotInitialized", "CRYPTO_NOT_INITIALIZED"),
    ("InternalError", "INTERNAL_ERROR"),
    ("Unknown", "UNKNOWN"),
];

// ts: TransactionType
pub const TRANSACTION_TYPE: [(&str, &str); 5] = [
    ("CreateOrder", "create_order"),
    ("CancelOrder", "cancel_order"),
    ("Transfer", "transfer"),
    ("Withdraw", "withdraw"),
    ("UpdateApiKey", "update_api_key"),
];

// ts: TokenDecimals
pub const TOKEN_DECIMALS: [(&str, f64); 5] = [
    ("USDT", 6.0),
    ("USDC", 6.0),
    ("DAI", 18.0),
    ("ETH", 18.0),
    ("BTC", 8.0),
];

// ts: FeePrecision
pub const FEE_PRECISION: f64 = 6.0;

// ts: MaxSlippageBps
pub const MAX_SLIPPAGE_BPS: f64 = 10000.0;

// ts: MinSlippageBps
pub const MIN_SLIPPAGE_BPS: f64 = 1.0;

// ts: DefaultSlippageBps
pub const DEFAULT_SLIPPAGE_BPS: f64 = 50.0;

// ts: BasisPoints
pub const BASIS_POINTS: f64 = 10000.0;

// ts: ProtocolVersion
pub const PROTOCOL_VERSION: &str = "1.0.0";

// ts: SdkVersion
// ts-bug: SdkVersion is 1.0.0 while the package metadata says 0.0.1.
pub const SDK_VERSION: &str = "1.0.0";

// ts: UserAgent
// ts-bug: UserAgent retains the source SdkVersion mismatch.
pub const USER_AGENT: &str = "lighter-sdk/1.0.0";

// ts: WsProtocolVersion
pub const WS_PROTOCOL_VERSION: &str = "1.0";

// ts: MaxWsMessageSize
pub const MAX_WS_MESSAGE_SIZE: f64 = 1048576.0;

// ts: OrderBookDepth
pub const ORDER_BOOK_DEPTH: [(&str, f64); 4] = [
    ("Shallow", 10.0),
    ("Medium", 50.0),
    ("Deep", 100.0),
    ("Full", 1000.0),
];

// ts: KlineInterval
pub const KLINE_INTERVAL: [(&str, &str); 9] = [
    ("OneMinute", "1m"),
    ("FiveMinutes", "5m"),
    ("FifteenMinutes", "15m"),
    ("ThirtyMinutes", "30m"),
    ("OneHour", "1h"),
    ("FourHours", "4h"),
    ("OneDay", "1d"),
    ("OneWeek", "1w"),
    ("OneMonth", "1M"),
];

// ts: ZeroAddress
pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

// ts: MaxUint256
pub static MAX_UINT256: LazyLock<BigInt> = LazyLock::new(|| (BigInt::from(1_u8) << 256) - 1);

// ts: MaxUint128
pub static MAX_UINT128: LazyLock<BigInt> = LazyLock::new(|| (BigInt::from(1_u8) << 128) - 1);

// ts: MaxUint64
pub static MAX_UINT64: LazyLock<BigInt> = LazyLock::new(|| (BigInt::from(1_u8) << 64) - 1);

// ts: MaxUint32
pub const MAX_UINT32: f64 = 4294967295.0;

// ts: MaxUint16
pub const MAX_UINT16: f64 = 65535.0;

// ts: MaxUint8
pub const MAX_UINT8: f64 = 255.0;

// ts: EnvVars
pub const ENV_VARS: [(&str, &str); 7] = [
    ("ApiUrl", "LIGHTER_API_URL"),
    ("WsUrl", "LIGHTER_WS_URL"),
    ("ChainId", "LIGHTER_CHAIN_ID"),
    ("PrivateKey", "LIGHTER_PRIVATE_KEY"),
    ("AccountIndex", "LIGHTER_ACCOUNT_INDEX"),
    ("ApiKeyIndex", "LIGHTER_API_KEY_INDEX"),
    ("Debug", "LIGHTER_DEBUG"),
];
