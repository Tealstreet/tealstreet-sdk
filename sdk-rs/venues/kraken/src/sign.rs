//! Kraken Futures signs the exact encoded bytes sent; Spot uses a different scheme.
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
/// JS encodeURIComponent used by the batch JSON special case.
pub fn encode_uri(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!'()*".contains(&byte) {
            out.push(byte as char);
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
    nonce: u64,
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
            secret: Zeroizing::new(bytes),
            nonce: 0,
        })
    }
    pub fn nonce(&mut self, now: f64) -> Result<String, String> {
        if !now.is_finite() || now < 0.0 || now.floor() > (u64::MAX / 1000) as f64 {
            return Err("Kraken invalid nonce clock".into());
        }
        let clock = (now.floor() as u64)
            .checked_mul(1000)
            .ok_or("Kraken nonce overflow")?;
        self.nonce = clock.max(self.nonce.checked_add(1).ok_or("Kraken nonce exhausted")?);
        Ok(self.nonce.to_string())
    }
    fn digest(&self, bytes: &[u8]) -> String {
        let hash = Sha256::digest(bytes);
        let mut mac =
            Hmac::<Sha512>::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        mac.update(&hash);
        STANDARD.encode(mac.finalize().into_bytes())
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
