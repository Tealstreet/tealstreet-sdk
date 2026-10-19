//! Signing primitives shared by venue engines. Pure functions: no I/O, no host.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

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

pub fn hmac_sha512(key: &[u8], payload: &[u8], encoding: Encoding) -> String {
    let mut mac = Hmac::<Sha512>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload);
    encode(&mac.finalize().into_bytes(), encoding)
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lenient base64 decode: skips unknown characters and accepts URL-safe digits,
/// matching how venues hand out secrets (Kraken secrets decode this way).
pub fn base64_decode_lenient(input: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    output
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

    #[test]
    fn lenient_base64_accepts_url_safe_and_noise() {
        assert_eq!(base64_decode_lenient("aGVs bG8="), b"hello");
        assert_eq!(base64_decode_lenient("-_8="), vec![0xfb, 0xff]);
    }
}
