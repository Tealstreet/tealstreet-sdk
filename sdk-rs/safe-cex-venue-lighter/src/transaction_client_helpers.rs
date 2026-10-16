//! Pure helpers from TransactionClient. Transaction building and submission are separate.
use num_bigint::BigInt;

// ts: TransactionClient#bytesToHex
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
// ts: TransactionClient#utf8ToHex
/// TextEncoder replaces each unpaired UTF16 unit with U+FFFD.
pub fn utf8_to_hex(value: &[u16]) -> String {
    bytes_to_hex(String::from_utf16_lossy(value).as_bytes())
}
// ts: TransactionClient#toHexNoPrefix
pub fn to_hex_no_prefix(value: &[u16]) -> Vec<u16> {
    value
        .strip_prefix(&[u16::from(b'0'), u16::from(b'x')])
        .unwrap_or(value)
        .to_vec()
}
fn range_error(field_name: &[u16], suffix: &str) -> Vec<u16> {
    field_name
        .iter()
        .copied()
        .chain(suffix.encode_utf16())
        .collect()
}
// ts: TransactionClient#toUint32FieldHex
pub fn to_uint32_field_hex(value: f64, field_name: &[u16]) -> Result<String, Vec<u16>> {
    if !value.is_finite() || value.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&value) {
        return Err(range_error(field_name, " is out of uint32 range"));
    }
    let mut buffer = [0u8; 8];
    buffer[..4].copy_from_slice(&(value as u32).to_le_bytes());
    Ok(bytes_to_hex(&buffer))
}
// ts: TransactionClient#toInt64FieldHex
pub fn to_int64_field_hex(value: &BigInt, field_name: &[u16]) -> Result<String, Vec<u16>> {
    let value =
        i64::try_from(value).map_err(|_| range_error(field_name, " is out of int64 range"))?;
    Ok(bytes_to_hex(&value.to_le_bytes()))
}
fn equals(value: Option<&[u16]>, ascii: &str) -> bool {
    value.is_some_and(|value| value.iter().copied().eq(ascii.encode_utf16()))
}
// ts: TransactionClient#mapOrderTypeValue
pub fn map_order_type_value(order_type: Option<&[u16]>) -> f64 {
    if equals(order_type, "market") {
        return 1.0;
    }
    if equals(order_type, "stop_loss") {
        return 2.0;
    }
    if equals(order_type, "stop_loss_limit") {
        return 3.0;
    }
    if equals(order_type, "take_profit") {
        return 4.0;
    }
    if equals(order_type, "take_profit_limit") {
        return 5.0;
    }
    0.0
}
// ts: TransactionClient#mapTimeInForceValue
pub fn map_time_in_force_value(
    order_type: Option<&[u16]>,
    post_only: Option<bool>,
    time_in_force: Option<&[u16]>,
) -> f64 {
    if equals(order_type, "market") {
        return 0.0;
    }
    if post_only == Some(true) || equals(time_in_force, "PostOnly") {
        return 2.0;
    }
    if equals(time_in_force, "ImmediateOrCancel") || equals(time_in_force, "FillOrKill") {
        return 0.0;
    }
    1.0
}
// ts: TransactionClient#isStopLossOrderType
pub fn is_stop_loss_order_type(value: f64) -> bool {
    value == 2.0 || value == 3.0
}
// ts: TransactionClient#isTakeProfitOrderType
pub fn is_take_profit_order_type(value: f64) -> bool {
    value == 4.0 || value == 5.0
}
// ts: TransactionClient#isChildGroupedOrderType
pub fn is_child_grouped_order_type(value: f64) -> bool {
    is_stop_loss_order_type(value) || is_take_profit_order_type(value)
}
