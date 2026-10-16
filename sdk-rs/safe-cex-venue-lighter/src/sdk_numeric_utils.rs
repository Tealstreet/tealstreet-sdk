//! Primitive-domain numeric utilities from lighter-sdk/src/utils/index.ts.
//! Number arguments retain binary64; strings retain all UTF-16 code units.
//! Uses the source's ordinary ECMAScript intrinsics (no reflective overrides).
use crate::{
    sdk_protocol::{MAX_ACCOUNT_INDEX, MAX_API_KEY_INDEX, MAX_NONCE},
    transaction_builders::TransactionBuildError,
};
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive, js_number_string, math_pow};
type Result<T> = std::result::Result<T, TransactionBuildError>;
fn error(message: impl AsRef<str>) -> TransactionBuildError {
    TransactionBuildError {
        name: "Error",
        message: message.as_ref().encode_utf16().collect(),
    }
}
fn number_bigint(value: f64) -> Result<BigInt> {
    js_bigint_primitive(&JsPrimitive::Number(value)).map_err(|e| TransactionBuildError {
        name: e.kind.name(),
        message: e.message_utf16,
    })
}
fn hex_digit(v: u16) -> bool {
    matches!(v,48..=57|65..=70|97..=102)
}
// ts: hexToBigInt
pub fn hex_to_big_int(hex: &[u16]) -> Result<BigInt> {
    let clean = if hex.starts_with(&[48, 120]) {
        &hex[2..]
    } else {
        hex
    };
    if !clean.iter().copied().all(hex_digit) {
        let mut message: Vec<u16> = "Invalid hex string: ".encode_utf16().collect();
        message.extend_from_slice(hex);
        return Err(TransactionBuildError {
            name: "Error",
            message,
        });
    }
    if clean.is_empty() {
        return Ok(BigInt::from(0));
    }
    // Validation leaves only ASCII digits, so this does not replace UTF16 units.
    let bytes: Vec<u8> = clean.iter().map(|v| *v as u8).collect();
    Ok(BigInt::parse_bytes(&bytes, 16).expect("validated source hex digits"))
}
// ts: validateAddress
pub fn validate_address(address: &[u16]) -> bool {
    if !address.starts_with(&[48, 120]) {
        return false;
    }
    if address.len() != 42 {
        return false;
    }
    address[2..].iter().copied().all(hex_digit)
}
// ts: clampBigInt
pub fn clamp_big_int(value: &BigInt, min: &BigInt, max: &BigInt) -> Result<BigInt> {
    if min > max {
        return Err(error("Min must be less than or equal to max"));
    }
    if value < min {
        return Ok(min.clone());
    }
    if value > max {
        return Ok(max.clone());
    }
    Ok(value.clone())
}
// ts: isInRange
pub fn is_in_range(value: &BigInt, min: &BigInt, max: &BigInt) -> bool {
    value >= min && value <= max
}
// ts: numberToBigInt
pub fn number_to_big_int(value: f64) -> Result<BigInt> {
    if !value.is_finite() || value.fract() != 0.0 {
        return Err(error(format!(
            "Cannot convert decimal number {} to BigInt",
            js_number_string(value)
        )));
    }
    if value.abs() > 9_007_199_254_740_991.0 {
        return Err(error(format!(
            "Number {} exceeds safe integer range",
            js_number_string(value)
        )));
    }
    number_bigint(value)
}
// ts: percentageOf
pub fn percentage_of(value: &BigInt, percentage: f64, decimals: Option<f64>) -> Result<BigInt> {
    let decimals = decimals.unwrap_or(0.0);
    if percentage < 0.0 {
        return Err(error("Percentage must be non-negative"));
    }
    if decimals < 0.0 {
        return Err(error("Decimals must be non-negative"));
    }
    // Retain the two binary64 powers/conversions in source order. In particular,
    // do not replace 10**decimals with exact BigInt exponentiation or round the
    // product differently: at large precision those yield different integers.
    let multiplier = number_bigint(math_pow(10.0, decimals))?;
    let percentage_bigint = number_bigint((percentage * math_pow(10.0, decimals)).floor())?;
    Ok((value * percentage_bigint) / (BigInt::from(100) * multiplier))
}
// ts: isValidAccountIndex
pub fn is_valid_account_index(value: &BigInt) -> bool {
    value >= &BigInt::from(0) && value <= &BigInt::from(MAX_ACCOUNT_INDEX as u32)
}
// ts: isValidApiKeyIndex
pub fn is_valid_api_key_index(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && (0.0..=MAX_API_KEY_INDEX).contains(&value)
}
// ts: isValidNonce
pub fn is_valid_nonce(value: &BigInt) -> bool {
    value >= &BigInt::from(0) && value <= &BigInt::from(MAX_NONCE as u64)
}
