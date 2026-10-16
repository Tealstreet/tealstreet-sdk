//! Protocol constants from lighter-sdk/src/types/constants.ts.
use num_bigint::BigInt;
use std::sync::LazyLock;

// ts: TxType
pub const TX_TYPE: [(&str, f64); 9] = [
    ("L2CreateOrder", 14.0),
    ("L2CancelOrder", 15.0),
    ("L2Transfer", 16.0),
    ("L2Withdraw", 17.0),
    ("L2UpdateApiKey", 18.0),
    ("L2Liquidation", 19.0),
    ("L2Deleverage", 20.0),
    ("L2FundingFee", 21.0),
    ("L2CreateGroupedOrders", 28.0),
];

// ts: MaxAccountIndex
pub const MAX_ACCOUNT_INDEX: f64 = 255.0;

// ts: MaxApiKeyIndex
pub const MAX_API_KEY_INDEX: f64 = 255.0;

// ts: MaxNonce
pub const MAX_NONCE: f64 = 4294967295.0;

// ts: MaxExpiry
pub const MAX_EXPIRY: f64 = 4294967295.0;

// ts: MinOrderSize
pub static MIN_ORDER_SIZE: LazyLock<BigInt> = LazyLock::new(|| BigInt::from(1_u8));

// ts: MaxOrderSize
pub static MAX_ORDER_SIZE: LazyLock<BigInt> = LazyLock::new(|| (BigInt::from(1_u8) << 64) - 1);

// ts: PricePrecision
pub const PRICE_PRECISION: f64 = 8.0;

// ts: QuantityPrecision
pub const QUANTITY_PRECISION: f64 = 8.0;

// ts: DefaultGasLimit
pub const DEFAULT_GAS_LIMIT: f64 = 300000.0;

// ts: DefaultGasPriceMultiplier
pub const DEFAULT_GAS_PRICE_MULTIPLIER: f64 = 1.2;

// ts: ApiVersion
pub const API_VERSION: &str = "v1";
