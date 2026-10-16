//! Field-element and display-string bodies on the source primitive domains.
//! UTF16 is retained throughout; ordinary ECMAScript intrinsics are assumed.
use crate::transaction_builders::TransactionBuildError;
use num_bigint::BigInt;
use safe_cex_num::{js_number_string, js_trim_start};
type Result<T> = std::result::Result<T, TransactionBuildError>;
fn error(message: impl AsRef<str>) -> TransactionBuildError {
    TransactionBuildError {
        name: "Error",
        message: message.as_ref().encode_utf16().collect(),
    }
}
fn hex_bytes(bytes: &[u8]) -> Vec<u16> {
    bytes
        .iter()
        .flat_map(|v| format!("{v:02x}").encode_utf16().collect::<Vec<_>>())
        .collect()
}
// ts: uint32ToFieldElement
pub fn uint32_to_field_element(value: f64) -> Result<Vec<u16>> {
    if value < 0.0 || value > f64::from(u32::MAX) {
        return Err(error(format!(
            "Value {} out of range for uint32 (0 to {})",
            js_number_string(value),
            u32::MAX
        )));
    }
    // setUint32 truncates fractions and converts NaN/-0 to zero; the source
    // deliberately does not apply the TransactionClient helper's integer guard.
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&(value as u32).to_le_bytes());
    Ok(hex_bytes(&bytes))
}
fn checked_u64(value: &BigInt) -> Result<u64> {
    u64::try_from(value).map_err(|_| {
        error(format!(
            "Value {value} out of range for uint64 (0 to {})",
            u64::MAX
        ))
    })
}
// ts: uint64ToFieldElement
pub fn uint64_to_field_element(value: &BigInt) -> Result<Vec<u16>> {
    let n = checked_u64(value)?;
    Ok(hex_bytes(&n.to_le_bytes()))
}
// ts: int64ToFieldElement
pub fn int64_to_field_element(value: &BigInt) -> Result<Vec<u16>> {
    let n = i64::try_from(value).map_err(|_| {
        error(format!(
            "Value {value} out of range for int64 ({} to {})",
            i64::MIN,
            i64::MAX
        ))
    })?;
    Ok(hex_bytes(&n.to_le_bytes()))
}
// ts: splitUint64
pub fn split_uint64(value: &BigInt) -> Result<[Vec<u16>; 2]> {
    let n = checked_u64(value)?;
    let low = (n & 0xffffffff) as f64;
    let high = ((n >> 32) & 0xffffffff) as f64;
    Ok([
        uint32_to_field_element(low)?,
        uint32_to_field_element(high)?,
    ])
}
fn whitespace(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(|ch| {
        let mut buffer = [0u8; 4];
        js_trim_start(ch.encode_utf8(&mut buffer)).is_empty()
    })
}
fn digit(unit: u16) -> Option<i32> {
    match unit {
        48..=57 => Some(i32::from(unit - 48)),
        65..=70 => Some(i32::from(unit - 65 + 10)),
        97..=102 => Some(i32::from(unit - 97 + 10)),
        _ => None,
    }
}
/// parseInt(slice,16) where the actual source slice contains at most two UTF16
/// units. Retains leading ECMA whitespace/sign, optional 0x and prefix parsing.
fn pair_hex(pair: &[u16]) -> Vec<u16> {
    let mut at = 0;
    while at < pair.len() && whitespace(pair[at]) {
        at += 1;
    }
    let negative = pair.get(at) == Some(&45);
    if matches!(pair.get(at), Some(43 | 45)) {
        at += 1;
    }
    if pair.get(at) == Some(&48) && matches!(pair.get(at + 1), Some(120 | 88)) {
        at += 2;
    }
    let mut n = 0;
    let mut count = 0;
    while let Some(&unit) = pair.get(at) {
        let Some(d) = digit(unit) else { break };
        n = n * 16 + d;
        count += 1;
        at += 1;
    }
    if count == 0 {
        return "NaN".encode_utf16().collect();
    }
    let text = if negative && n != 0 {
        format!("-{n:x}")
    } else {
        format!("{n:02x}")
    };
    text.encode_utf16().collect()
}
// ts: hexToFieldElement
pub fn hex_to_field_element(hex: &[u16]) -> Vec<u16> {
    let clean = hex.strip_prefix(&[48, 120]).unwrap_or(hex);
    let mut padded = Vec::with_capacity(clean.len() + 1);
    if !clean.len().is_multiple_of(2) {
        padded.push(48);
    }
    padded.extend_from_slice(clean);
    let mut bytes: Vec<Vec<u16>> = padded.chunks(2).map(pair_hex).collect();
    while !bytes.len().is_multiple_of(8) {
        bytes.push(vec![48, 48]);
    }
    bytes.into_iter().flatten().collect()
}
// ts: concatenateFieldElements
/// Dense plain string-value arrays; no accessor/proxy interception contract.
pub fn concatenate_field_elements(elements: &[Vec<u16>]) -> Result<Vec<u16>> {
    for (i, element) in elements.iter().enumerate() {
        if element.len() != 16 {
            return Err(error(format!(
                "Field element at index {i} has invalid length {} (expected 16 hex chars)",
                element.len()
            )));
        }
    }
    Ok(elements.concat())
}
/// String.slice's ToIntegerOrInfinity and relative index, on a UTF16 length.
fn relative_index(index: f64, len: usize) -> usize {
    let index = if index.is_nan() { 0.0 } else { index.trunc() };
    if index < 0.0 {
        (len as f64 + index).max(0.0).min(len as f64) as usize
    } else {
        index.min(len as f64) as usize
    }
}
fn slice(value: &[u16], start: f64, end: f64) -> Vec<u16> {
    let start = relative_index(start, value.len());
    let end = relative_index(end, value.len());
    value[start..end.max(start)].to_vec()
}
// ts: truncate
pub fn truncate(value: &[u16], max_length: f64, ellipsis: Option<&[u16]>) -> Vec<u16> {
    if value.len() as f64 <= max_length {
        return value.to_vec();
    }
    let default: Vec<u16> = "...".encode_utf16().collect();
    let ellipsis = ellipsis.unwrap_or(&default);
    let truncate_length = max_length - ellipsis.len() as f64;
    if truncate_length < 0.0 {
        return slice(ellipsis, 0.0, max_length);
    }
    let mut result = slice(value, 0.0, truncate_length);
    result.extend_from_slice(ellipsis);
    result
}
// ts: formatAddress
pub fn format_address(
    address: &[u16],
    start_chars: Option<f64>,
    end_chars: Option<f64>,
) -> Vec<u16> {
    let start_chars = start_chars.unwrap_or(6.0);
    let end_chars = end_chars.unwrap_or(4.0);
    if address.len() as f64 <= start_chars + end_chars {
        return address.to_vec();
    }
    let mut result = slice(address, 0.0, start_chars);
    result.extend("...".encode_utf16());
    result.extend(slice(address, -end_chars, f64::INFINITY));
    result
}
