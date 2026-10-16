//! Coinbase's decimal-string guards used before order submission.
use serde_json::Value;

// ts: isCoinbaseUnsignedDecimal
pub fn is_coinbase_unsigned_decimal(value: Option<&Value>) -> bool {
    let Some(Value::String(value)) = value else {
        return false;
    };
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (value.as_str(), None),
    };
    let whole_valid = whole == "0"
        || whole.as_bytes().split_first().is_some_and(|(first, rest)| {
            (b'1'..=b'9').contains(first) && rest.iter().all(u8::is_ascii_digit)
        });
    let fraction_valid = fraction.is_none_or(|digits| {
        !digits.is_empty() && digits.as_bytes().iter().all(u8::is_ascii_digit)
    });
    whole_valid && fraction_valid
}

// ts: isCoinbaseZeroDecimal
pub fn is_coinbase_zero_decimal(value: Option<&Value>) -> bool {
    let Some(Value::String(value)) = value else {
        return false;
    };
    value == "0"
        || value.strip_prefix("0.").is_some_and(|fraction| {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte == b'0')
        })
}
