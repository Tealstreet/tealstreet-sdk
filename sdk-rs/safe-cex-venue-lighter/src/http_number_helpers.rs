//! Pure api-client bigint guards. No HTTP transport is implied.
use super::transaction_builders::TransactionBuildError;
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_number_primitive};
// ts: isSafeIntegerBigInt
pub fn is_safe_integer_bigint(value: &BigInt) -> bool {
    let max = BigInt::from(9007199254740991u64);
    value <= &max && value >= &(-max)
}
// ts: toSafeNumber
pub fn to_safe_number(value: &BigInt, field_name: &[u16]) -> Result<f64, TransactionBuildError> {
    if !is_safe_integer_bigint(value) {
        return Err(TransactionBuildError {
            name: "Error",
            message: field_name
                .iter()
                .copied()
                .chain(" exceeds Number safe integer range".encode_utf16())
                .collect(),
        });
    }
    // The guard leaves only exactly representable integers; no narrowing cast.
    js_number_primitive(&JsPrimitive::BigInt(value.clone())).map_err(|error| {
        TransactionBuildError {
            name: error.name,
            message: error.message.encode_utf16().collect(),
        }
    })
}
