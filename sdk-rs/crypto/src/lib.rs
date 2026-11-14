//! Signing primitives and host-independent nonce coordination. No I/O.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Hex,
    Base64,
}

pub fn encode(bytes: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Hex => hex(bytes),
        Encoding::Base64 => STANDARD.encode(bytes),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}

pub fn hmac_sha256(key: &[u8], payload: &[u8], encoding: Encoding) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload);
    encode(&mac.finalize().into_bytes(), encoding)
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub mod ed25519;
pub mod eip712;
pub mod es256;
pub mod ethereum;
pub mod lighter;
pub mod nonce;
#[cfg(not(target_arch = "wasm32"))]
pub mod protected;
pub mod secrets;
pub mod signer;
pub mod starknet;
pub mod uuid;
#[cfg(feature = "zklink")]
pub mod zklink;

// Keep existing production declarations in place when adding admission helpers.
#[cfg(feature = "admission")]
pub mod admission;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_matches_rfc4231_case_2() {
        assert_eq!(
            hmac_sha256(b"Jefe", b"what do ya want for nothing?", Encoding::Hex),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "admission")))]
mod admission_feature_boundary {
    //! Existing venue closures leave the shared `admission` helpers disabled.
    //!
    //! ```compile_fail
    //! use safe_cex_crypto::admission::Statement;
    //! ```
    //!
    //! ```compile_fail
    //! use safe_cex_crypto::starknet::verify;
    //! ```
}
