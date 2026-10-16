//! Mirrors bitunix.utils.ts using the workspace SHA-256 primitive.
use sha2::{Digest, Sha256};

/// UTF-8 bytes, SHA-256 digest, lowercase hexadecimal, in source order.
pub fn sha256_hex(input_string: &str) -> String {
    Sha256::digest(input_string.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
