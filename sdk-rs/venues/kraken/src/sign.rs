//! Kraken Futures signs the exact encoded bytes sent; Spot uses a different scheme.
pub mod dispatch;
pub mod setup;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroizing;

pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~()".contains(&byte) {
            out.push(byte as char);
        } else if byte == b' ' {
            out.push('+');
        } else {
            use std::fmt::Write;
            write!(out, "%{byte:02X}").unwrap();
        }
    }
    out
}
pub fn form(params: &[(&str, String)]) -> String {
    let mut sorted: Vec<_> = params.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    sorted
        .into_iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
pub struct Signer {
    secret: Zeroizing<Vec<u8>>,
    identity: [u8; 32],
}
impl Signer {
    pub fn new(secret: &str) -> Result<Self, String> {
        let bytes = STANDARD
            .decode(secret)
            .map_err(|_| "Kraken secret is not valid base64")?;
        if bytes.is_empty() {
            return Err("Kraken secret is empty".into());
        }
        Ok(Self {
            identity: safe_cex_crypto::sha256(&bytes),
            secret: Zeroizing::new(bytes),
        })
    }
    pub fn nonce(&mut self, now: f64) -> Result<String, String> {
        if !now.is_finite()
            || now < 0.0
            || safe_cex_engine::time::wire_stamp_ms(now) > (u64::MAX / 1000) as f64
        {
            return Err("Kraken invalid nonce clock".into());
        }
        let clock = (safe_cex_engine::time::wire_stamp_ms(now) as u64)
            .checked_mul(1000)
            .ok_or("Kraken nonce overflow")?;
        safe_cex_crypto::nonce::kraken(self.identity, clock).map(|nonce| nonce.to_string())
    }
    fn digest(&self, bytes: &[u8]) -> String {
        let hash = Sha256::digest(bytes);
        let mut mac =
            Hmac::<Sha512>::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        mac.update(&hash);
        STANDARD.encode(mac.finalize().into_bytes())
    }
    pub fn spot_signature(&self, body: &str, nonce: &str, path: &str) -> Result<String, String> {
        if !path.starts_with("/0/private/") || path.contains('?') {
            return Err("Kraken Spot unsupported signing path".into());
        }
        let hash = Sha256::digest(format!("{nonce}{body}").as_bytes());
        let mut mac =
            Hmac::<Sha512>::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        mac.update(path.as_bytes());
        mac.update(&hash);
        Ok(STANDARD.encode(mac.finalize().into_bytes()))
    }
    pub fn challenge(&self, challenge: &str) -> String {
        self.digest(challenge.as_bytes())
    }
    pub fn signature(&self, form: &str, nonce: &str, path: &str) -> Result<String, String> {
        let signed_path = if let Some(rest) = path.strip_prefix("/derivatives") {
            rest
        } else {
            path
        };
        if !(signed_path.starts_with("/api/v3/") || signed_path.starts_with("/api/history/"))
            || path.contains('?')
        {
            return Err("Kraken unsupported signing path".into());
        }
        Ok(self.digest(format!("{form}{nonce}{signed_path}").as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separate_signers_and_recreated_signers_never_reuse_nonces() {
        let mut a = Signer::new("c2VjdXJpdHktbm9uY2UtdGVzdA==").unwrap();
        let mut b = Signer::new("c2VjdXJpdHktbm9uY2UtdGVzdA==").unwrap();
        let first: u64 = a.nonce(1700000000000.0).unwrap().parse().unwrap();
        let second: u64 = b.nonce(1700000000000.0).unwrap().parse().unwrap();
        drop(a);
        let third: u64 = Signer::new("c2VjdXJpdHktbm9uY2UtdGVzdA==")
            .unwrap()
            .nonce(1.0)
            .unwrap()
            .parse()
            .unwrap();
        assert!(second > first && third > second);
    }
    #[test]
    fn nonce_remains_unique_across_clock_regression_and_more_than_thousand_calls() {
        let mut s = Signer::new("c2VjcmV0").unwrap();
        let mut last = 0;
        for i in 0..2002 {
            let next = s
                .nonce(if i < 2000 { 1700000000000.0 } else { 1.0 })
                .unwrap()
                .parse::<u64>()
                .unwrap();
            assert!(next > last);
            last = next;
        }
        assert_eq!(last, 1700000000002001);
    }
    #[test]
    fn rfc1738_sorted_form_preserves_utf8_and_literal_plus() {
        assert_eq!(
            form(&[("symbol", "PF_XBTUSD".into()), ("note", "é + /~".into())]),
            "note=%C3%A9+%2B+%2F~&symbol=PF_XBTUSD"
        );
        assert!(Signer::new("*").is_err());
        assert!(Signer::new("").is_err());
    }
}
