//! BloFin signs the exact request bytes, encoding the HMAC's hex as base64.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::{Encoding, hmac_sha256};

pub fn signature(
    secret: &str,
    path: &str,
    method: &str,
    timestamp: &str,
    nonce: &str,
    body: &str,
) -> String {
    let text = format!("{path}{method}{timestamp}{nonce}{body}");
    STANDARD.encode(hmac_sha256(
        secret.as_bytes(),
        text.as_bytes(),
        Encoding::Hex,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_encodes_hex_text_rather_than_raw_digest() {
        let result = signature(
            "secret",
            "/api/v1/account/positions?instId=BTC-USDT",
            "GET",
            "1700000000000",
            "nonce",
            "",
        );
        assert_eq!(STANDARD.decode(result).unwrap().len(), 64);
        assert_eq!(
            signature("secret", "/test", "POST", "1", "n", "{}"),
            "ZmIwNzhmM2MzZDRjMmI2Y2M3ZjlkZWEwMDUwNGE5ZDFiMTY3MzcwZjMyODliOTgxYjZjMTA4OTAzNTJmYjJkZA=="
        );
    }
}
