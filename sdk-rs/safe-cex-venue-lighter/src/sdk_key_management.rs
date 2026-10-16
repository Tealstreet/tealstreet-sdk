//! Typed key-management values and pure bodies from lighter-sdk/signer/key-manager.ts.
//! Contract: ordinary primitive strings and owned, unmodified Buffer/Uint8Array
//! byte values. Reflective prototype/global overrides and exotic iterators are
//! outside this adapter. Strings and error messages retain UTF-16 code units.
use crate::sdk_account_api::AccountValue;

// ts: KeyPair
pub struct KeyPair {
    pub private_key: Vec<u16>,
    pub public_key: Vec<u16>,
}

// ts: KeyManagerError
pub struct KeyManagerError {
    pub name: &'static str,
    pub message: Vec<u16>,
    pub cause: AccountValue,
}
impl KeyManagerError {
    // ts: KeyManagerError.constructor
    pub fn new(message: Vec<u16>, cause: Option<AccountValue>) -> Self {
        Self {
            name: "KeyManagerError",
            message,
            cause: cause.unwrap_or(AccountValue::Undefined),
        }
    }
}
/// Source union after ordinary native Buffer/Uint8Array classification. Buffer
/// classification precedes Uint8Array; both ordinary byte encodings are lowercase.
pub enum KeyInput<'a> {
    String(&'a [u16]),
    Buffer(&'a [u8]),
    Uint8Array(&'a [u8]),
    Unsupported,
}
fn hex_digit(v: u16) -> bool {
    matches!(v, 48..=57 | 65..=70 | 97..=102)
}
// ts: isValidPrivateKey
pub fn is_valid_private_key(private_key: &[u16]) -> bool {
    if private_key.len() != 80 {
        return false;
    }
    private_key.iter().copied().all(hex_digit)
}
// ts: isValidPublicKey
pub fn is_valid_public_key(public_key: &[u16]) -> bool {
    if public_key.len() != 80 {
        return false;
    }
    public_key.iter().copied().all(hex_digit)
}
fn normalize(key: KeyInput<'_>, private: bool) -> Result<Vec<u16>, KeyManagerError> {
    let kind = if private { "Private" } else { "Public" };
    let hex_key = match key {
        KeyInput::String(key) => {
            if key.starts_with(&[48, 120]) {
                key[2..].to_vec()
            } else {
                key.to_vec()
            }
        }
        KeyInput::Buffer(bytes) | KeyInput::Uint8Array(bytes) => {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            let mut result = Vec::with_capacity(bytes.len() * 2);
            for byte in bytes {
                result.push(HEX[(byte >> 4) as usize] as u16);
                result.push(HEX[(byte & 15) as usize] as u16);
            }
            result
        }
        KeyInput::Unsupported => {
            return Err(KeyManagerError::new(
                format!("{kind} key must be a string, Buffer, or Uint8Array")
                    .encode_utf16()
                    .collect(),
                None,
            ));
        }
    };
    let valid = if private {
        is_valid_private_key(&hex_key)
    } else {
        is_valid_public_key(&hex_key)
    };
    if !valid {
        return Err(KeyManagerError::new(
            format!(
                "Invalid {} key format after normalization. Expected 80 hex characters, got {}",
                kind.to_lowercase(),
                hex_key.len()
            )
            .encode_utf16()
            .collect(),
            None,
        ));
    }
    Ok(hex_key)
}
// ts: normalizePrivateKey
pub fn normalize_private_key(key: KeyInput<'_>) -> Result<Vec<u16>, KeyManagerError> {
    normalize(key, true)
}
// ts: normalizePublicKey
pub fn normalize_public_key(key: KeyInput<'_>) -> Result<Vec<u16>, KeyManagerError> {
    normalize(key, false)
}
// ts: secureCompareKeys
pub fn secure_compare_keys(key1: &[u16], key2: &[u16]) -> bool {
    if key1.len() != key2.len() {
        return false;
    }
    let mut result = 0u16;
    for i in 0..key1.len() {
        result |= key1[i] ^ key2[i];
    }
    result == 0
}
