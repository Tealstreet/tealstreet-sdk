//! Phemex authenticates the exact path, query (without `?`), expiry and body.
use safe_cex_crypto::{Encoding, hmac_sha256};
use serde::Serialize;

pub struct Credentials {
    pub key: String,
    pub secret: String,
}
impl Credentials {
    pub fn expiry(now: f64) -> u64 {
        ((now + 60_000.0) / 1000.0 + 0.5).floor() as u64
    }
    pub fn headers(
        &self,
        path: &str,
        query: &str,
        body: &str,
        now: f64,
    ) -> Vec<(&'static str, String)> {
        let expiry = Self::expiry(now).to_string();
        let query = decoded_query(query);
        let payload = format!("{path}{query}{expiry}{body}");
        vec![
            ("X-PHEMEX-ACCESS-TOKEN", self.key.clone()),
            ("X-PHEMEX-REQUEST-EXPIRY", expiry),
            (
                "X-PHEMEX-REQUEST-SIGNATURE",
                hmac_sha256(self.secret.as_bytes(), payload.as_bytes(), Encoding::Hex),
            ),
            ("Content-Type", "application/json".into()),
        ]
    }
    pub fn websocket_auth(&self, now: f64, id: u64) -> String {
        #[derive(Serialize)]
        struct Auth<'a> {
            method: &'static str,
            params: (&'static str, &'a str, String, u64),
            id: u64,
        }
        let expiry = Self::expiry(now);
        let payload = format!("{}{expiry}", self.key);
        let signature = hmac_sha256(self.secret.as_bytes(), payload.as_bytes(), Encoding::Hex);
        serde_json::to_string(&Auth {
            method: "user.auth",
            params: ("API", &self.key, signature, expiry),
            id,
        })
        .expect("serializable auth")
    }
}
// TS signs decodeURIComponent(qs.stringify(params)); a plus remains a plus.
fn decoded_query(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            result.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        result.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| query.to_owned())
}

safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);
