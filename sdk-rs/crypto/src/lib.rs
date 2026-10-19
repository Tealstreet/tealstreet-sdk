//! Signing primitives shared by venue engines. Pure functions: no I/O, no host.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::Sha256;

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

pub mod ed25519;
pub mod eip712;
pub mod es256;
pub mod ethereum;
pub mod lighter;
pub mod starknet;
pub mod uuid;
pub mod zklink;

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
