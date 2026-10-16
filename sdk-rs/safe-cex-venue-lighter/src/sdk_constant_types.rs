//! Closed literal aliases from lighter-sdk/src/constants/index.ts.
use serde::{Deserialize, Serialize};

// ts: ChainIdValue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ChainIdValue {
    Mainnet = 1,
    Sepolia = 11155111,
    ArbitrumOne = 42161,
    ArbitrumSepolia = 421614,
    Optimism = 10,
    OptimismSepolia = 11155420,
    Polygon = 137,
    PolygonMumbai = 80001,
    Base = 8453,
    BaseSepolia = 84532,
}
impl Serialize for ChainIdValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(*self as u32)
    }
}
impl<'de> Deserialize<'de> for ChainIdValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match f64::deserialize(deserializer)? {
            1.0 => Ok(Self::Mainnet),
            11155111.0 => Ok(Self::Sepolia),
            42161.0 => Ok(Self::ArbitrumOne),
            421614.0 => Ok(Self::ArbitrumSepolia),
            10.0 => Ok(Self::Optimism),
            11155420.0 => Ok(Self::OptimismSepolia),
            137.0 => Ok(Self::Polygon),
            80001.0 => Ok(Self::PolygonMumbai),
            8453.0 => Ok(Self::Base),
            84532.0 => Ok(Self::BaseSepolia),
            _ => Err(serde::de::Error::custom("expected ChainIdValue literal")),
        }
    }
}

// ts: ErrorCodeValue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCodeValue {
    #[serde(rename = "NETWORK_ERROR")]
    NetworkError,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "INVALID_PARAMETER")]
    InvalidParameter,
    #[serde(rename = "INVALID_SIGNATURE")]
    InvalidSignature,
    #[serde(rename = "INSUFFICIENT_BALANCE")]
    InsufficientBalance,
    #[serde(rename = "ORDER_NOT_FOUND")]
    OrderNotFound,
    #[serde(rename = "MARKET_NOT_FOUND")]
    MarketNotFound,
    #[serde(rename = "NONCE_TOO_LOW")]
    NonceTooLow,
    #[serde(rename = "NONCE_TOO_HIGH")]
    NonceTooHigh,
    #[serde(rename = "TX_EXPIRED")]
    TransactionExpired,
    #[serde(rename = "RATE_LIMIT_EXCEEDED")]
    RateLimitExceeded,
    #[serde(rename = "AUTH_FAILED")]
    AuthenticationFailed,
    #[serde(rename = "CRYPTO_NOT_INITIALIZED")]
    CryptoNotInitialized,
    #[serde(rename = "INTERNAL_ERROR")]
    InternalError,
    #[serde(rename = "UNKNOWN")]
    Unknown,
}
// Private remote companion keeps accepted-variant metadata behind a string-only gate.
#[derive(Deserialize)]
#[serde(remote = "ErrorCodeValue")]
enum ErrorCodeValueWire {
    #[serde(rename = "NETWORK_ERROR")]
    NetworkError,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "INVALID_PARAMETER")]
    InvalidParameter,
    #[serde(rename = "INVALID_SIGNATURE")]
    InvalidSignature,
    #[serde(rename = "INSUFFICIENT_BALANCE")]
    InsufficientBalance,
    #[serde(rename = "ORDER_NOT_FOUND")]
    OrderNotFound,
    #[serde(rename = "MARKET_NOT_FOUND")]
    MarketNotFound,
    #[serde(rename = "NONCE_TOO_LOW")]
    NonceTooLow,
    #[serde(rename = "NONCE_TOO_HIGH")]
    NonceTooHigh,
    #[serde(rename = "TX_EXPIRED")]
    TransactionExpired,
    #[serde(rename = "RATE_LIMIT_EXCEEDED")]
    RateLimitExceeded,
    #[serde(rename = "AUTH_FAILED")]
    AuthenticationFailed,
    #[serde(rename = "CRYPTO_NOT_INITIALIZED")]
    CryptoNotInitialized,
    #[serde(rename = "INTERNAL_ERROR")]
    InternalError,
    #[serde(rename = "UNKNOWN")]
    Unknown,
}
impl<'de> Deserialize<'de> for ErrorCodeValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        ErrorCodeValueWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

// ts: TransactionTypeValue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TransactionTypeValue {
    #[serde(rename = "create_order")]
    CreateOrder,
    #[serde(rename = "cancel_order")]
    CancelOrder,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "withdraw")]
    Withdraw,
    #[serde(rename = "update_api_key")]
    UpdateApiKey,
}
// Private remote companion keeps accepted-variant metadata behind a string-only gate.
#[derive(Deserialize)]
#[serde(remote = "TransactionTypeValue")]
enum TransactionTypeValueWire {
    #[serde(rename = "create_order")]
    CreateOrder,
    #[serde(rename = "cancel_order")]
    CancelOrder,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "withdraw")]
    Withdraw,
    #[serde(rename = "update_api_key")]
    UpdateApiKey,
}
impl<'de> Deserialize<'de> for TransactionTypeValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        TransactionTypeValueWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// ts: KlineIntervalValue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KlineIntervalValue {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "1d")]
    OneDay,
    #[serde(rename = "1w")]
    OneWeek,
    #[serde(rename = "1M")]
    OneMonth,
}
// Private remote companion keeps accepted-variant metadata behind a string-only gate.
#[derive(Deserialize)]
#[serde(remote = "KlineIntervalValue")]
enum KlineIntervalValueWire {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "1d")]
    OneDay,
    #[serde(rename = "1w")]
    OneWeek,
    #[serde(rename = "1M")]
    OneMonth,
}
impl<'de> Deserialize<'de> for KlineIntervalValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        KlineIntervalValueWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
