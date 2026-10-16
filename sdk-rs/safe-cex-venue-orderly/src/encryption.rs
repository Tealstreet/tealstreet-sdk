//! Source encryption adapters; Ed25519 signing is owned by safe-cex-crypto.
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha512};

pub fn sha512_for_ed25519(message: &[u8]) -> [u8; 64] {
    Sha512::digest(message).into()
}

pub fn base64url(value: &str) -> String {
    value.replace('+', "-").replace('/', "_")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderlySignError {
    pub name: &'static str,
    pub message: String,
}

pub fn orderly_sign(text_sign: &str, secret: &str) -> Result<String, OrderlySignError> {
    // Source takes exactly split(':')[1], ignoring both the prefix and later parts.
    let encoded_seed = secret.split(':').nth(1).ok_or_else(|| OrderlySignError {
        name: "TypeError",
        message: "Expected String".into(),
    })?;
    let seed = bs58::decode(encoded_seed)
        .into_vec()
        .map_err(|_| OrderlySignError {
            name: "Error",
            message: "Non-base58 character".into(),
        })?;
    if seed.len() != 32 {
        return Err(OrderlySignError {
            name: "RangeError",
            message: format!(
                "\"secretKey\" expected Uint8Array of length 32, got length={}",
                seed.len()
            ),
        });
    }
    let signature =
        safe_cex_crypto::ed25519::sign(text_sign.as_bytes(), &seed).map_err(|message| {
            OrderlySignError {
                name: "Error",
                message,
            }
        })?;
    // Padding survives: the actual helper replaces only '+' and '/'.
    Ok(base64url(&STANDARD.encode(signature)))
}
