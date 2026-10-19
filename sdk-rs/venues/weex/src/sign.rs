//! WEEX signs the exact bytes sent; callers construct sorted, encoded queries.
use safe_cex_crypto::{Encoding, hmac_sha256};

pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub passphrase: String,
}
impl Credentials {
    fn auth(&self, timestamp: String, payload: String) -> Vec<(&'static str, String)> {
        vec![
            ("ACCESS-KEY", self.key.clone()),
            ("ACCESS-PASSPHRASE", self.passphrase.clone()),
            ("ACCESS-TIMESTAMP", timestamp),
            (
                "ACCESS-SIGN",
                hmac_sha256(self.secret.as_bytes(), payload.as_bytes(), Encoding::Base64),
            ),
            ("Content-Type", "application/json".into()),
            ("User-Agent", "Tealstreet".into()),
        ]
    }
    pub fn headers(
        &self,
        method: &str,
        path_query: &str,
        body: &str,
        now: f64,
    ) -> Vec<(&'static str, String)> {
        let timestamp = format!("{:.0}", now.floor());
        let payload = format!(
            "{timestamp}{}{path_query}{body}",
            method.to_ascii_uppercase()
        );
        self.auth(timestamp, payload)
    }
    pub fn ws_headers(&self, now: f64) -> Vec<(&'static str, String)> {
        let timestamp = format!("{:.0}", now.floor());
        let payload = format!("{timestamp}/v3/ws/private");
        self.auth(timestamp, payload)
    }
}
/// RFC 3986 query component encoding, with spaces represented by `%20`.
pub fn encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::new();
    for c in value.bytes() {
        if c.is_ascii_alphanumeric() || b"-._~".contains(&c) {
            out.push(c as char);
        } else {
            out.push('%');
            out.push(HEX[(c >> 4) as usize] as char);
            out.push(HEX[(c & 15) as usize] as char);
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_rest_and_upgrade_payloads() {
        let c = Credentials {
            key: "k".into(),
            secret: "secret".into(),
            passphrase: "p".into(),
        };
        let h = c.headers("get", "/capi/v3/openOrders?limit=100&page=0", "", 1234.9);
        assert_eq!(h[2].1, "1234");
        // Fixed vectors generated independently with Python hashlib/hmac.
        assert_eq!(h[3].1, "bCU4mL/nryWPHh2ijQKaUlRR81jLGrRazQ5ZFQPfR+E=");
        let h = c.headers("POST", "/capi/v3/order", r#"{"symbol":"BTCUSDT"}"#, 1234.0);
        assert_eq!(h[3].1, "LGImWmNEu6Q2o2LhDIPtwbUT0PBJLHsWBbY7mB0+0JY=");
        assert_eq!(
            c.ws_headers(1234.0)[3].1,
            "8IL8D6LBPFWJeTejIF/swS90xLfKkMPm++NyP0LDezA="
        );
        assert_ne!(c.ws_headers(1234.0)[3].1, c.ws_headers(2234.0)[3].1);
        assert_eq!(encode("a b/+"), "a%20b%2F%2B");
    }
}
