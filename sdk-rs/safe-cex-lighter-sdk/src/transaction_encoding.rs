//! Error and BigInt coercion shared by the retained amount utility bodies.
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingError {
    pub name: &'static str,
    pub message: Vec<u16>,
}
impl EncodingError {
    pub(crate) fn error(message: impl AsRef<str>) -> Self {
        Self {
            name: "Error",
            message: message.as_ref().encode_utf16().collect(),
        }
    }
}
pub(crate) fn bigint_string(input: &[u16]) -> Result<BigInt, EncodingError> {
    if let Ok(text) = String::from_utf16(input) {
        return js_bigint_primitive(&JsPrimitive::String(text)).map_err(|e| EncodingError {
            name: e.kind.name(),
            message: e.message_utf16,
        });
    }
    let mut message: Vec<u16> = "Cannot convert ".encode_utf16().collect();
    message.extend(input.iter().take(1000));
    if input.len() > 1000 {
        message.push(0x2026);
    }
    message.extend(" to a BigInt".encode_utf16());
    Err(EncodingError {
        name: "SyntaxError",
        message,
    })
}
