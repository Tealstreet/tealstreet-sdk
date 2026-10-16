//! Amount utility bodies over required ECMAScript allocation/arithmetic primitives.
use crate::transaction_encoding::{EncodingError, bigint_string};
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive, js_number_string};

/// Bindings must preserve BigInt exponentiation and String padStart/padEnd semantics.
pub trait TextIntrinsics {
    fn pow10(&self, exponent: &BigInt) -> Result<BigInt, EncodingError>;
    fn pad_zeros(&self, text: &[u16], target: f64, start: bool) -> Result<Vec<u16>, EncodingError>;
}
pub fn big_int_to_hex<R: TextIntrinsics>(
    runtime: &R,
    value: &BigInt,
    bytes: Option<f64>,
) -> Result<Vec<u16>, EncodingError> {
    let mut hex: Vec<u16> = value.to_str_radix(16).encode_utf16().collect();
    if let Some(bytes) = bytes {
        hex = runtime.pad_zeros(&hex, bytes * 2.0, true)?;
    }
    let mut output: Vec<u16> = "0x".encode_utf16().collect();
    output.extend(hex);
    Ok(output)
}
pub fn format_amount<R: TextIntrinsics>(
    runtime: &R,
    amount: &BigInt,
    decimals: f64,
) -> Result<Vec<u16>, EncodingError> {
    if decimals < 0.0 {
        return Err(EncodingError::error("Decimals must be non-negative"));
    }
    if decimals == 0.0 {
        return Ok(amount.to_string().encode_utf16().collect());
    }
    let exponent =
        js_bigint_primitive(&JsPrimitive::Number(decimals)).map_err(|e| EncodingError {
            name: e.kind.name(),
            message: e.message_utf16,
        })?;
    let divisor = runtime.pow10(&exponent)?;
    let whole = amount / &divisor;
    let fractional = amount % &divisor;
    if fractional == BigInt::from(0) {
        return Ok(format!("{whole}.0").encode_utf16().collect());
    }
    let mut fraction = runtime.pad_zeros(
        &fractional.to_string().encode_utf16().collect::<Vec<_>>(),
        decimals,
        true,
    )?;
    while fraction.last() == Some(&48) {
        fraction.pop();
    }
    let mut output: Vec<u16> = format!("{whole}.").encode_utf16().collect();
    output.extend(fraction);
    Ok(output)
}
pub fn parse_amount<R: TextIntrinsics>(
    runtime: &R,
    amount: &[u16],
    decimals: f64,
) -> Result<BigInt, EncodingError> {
    if decimals < 0.0 {
        return Err(EncodingError::error("Decimals must be non-negative"));
    }
    let at = amount.iter().position(|u| *u == 46).unwrap_or(amount.len());
    let has_dot = at < amount.len();
    let whole = &amount[..at];
    let fraction = if has_dot { &amount[at + 1..] } else { &[] };
    if whole.is_empty()
        || !whole.iter().all(|u| matches!(u, 48..=57))
        || !fraction.iter().all(|u| matches!(u, 48..=57))
    {
        let mut error = EncodingError::error("Invalid amount format: ");
        error.message.extend_from_slice(amount);
        return Err(error);
    }
    if fraction.len() as f64 > decimals {
        return Err(EncodingError::error(format!(
            "Amount has too many decimal places. Expected {}, got {}",
            js_number_string(decimals),
            fraction.len()
        )));
    }
    let padded = runtime.pad_zeros(fraction, decimals, false)?;
    let mut combined = whole.to_vec();
    combined.extend(padded);
    bigint_string(&combined)
}
