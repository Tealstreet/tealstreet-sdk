//! Signed REST requests: Binance HMACs the query string and takes every
//! parameter, including POST/PUT/DELETE data, in the query.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::num;

pub const RECV_WINDOW_MS: u32 = 20_000;

#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}

/// A query string under construction, percent-encoded as it is built.
#[derive(Default)]
pub struct Query(String);

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn str(mut self, name: &str, value: &str) -> Self {
        if !self.0.is_empty() {
            self.0.push('&');
        }
        self.0.push_str(name);
        self.0.push('=');
        encode_into(&mut self.0, value);
        self
    }

    pub fn num(self, name: &str, value: f64) -> Self {
        self.str(name, &num::format(value))
    }

    pub fn opt_str(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(name, value),
            None => self,
        }
    }

    pub fn opt_num(self, name: &str, value: Option<f64>) -> Self {
        match value {
            Some(value) => self.num(name, value),
            None => self,
        }
    }

    pub fn unsigned(self) -> String {
        self.0
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Append `timestamp`, `recvWindow` and the HMAC-SHA256 `signature`.
    pub fn signed(
        self,
        credentials: &Credentials,
        now_ms: f64,
        recv_window: Option<f64>,
    ) -> String {
        let mut query = self
            .num("timestamp", now_ms.floor())
            .num(
                "recvWindow",
                recv_window.unwrap_or(f64::from(RECV_WINDOW_MS)),
            )
            .0;
        let signature = hmac_sha256(
            credentials.secret.as_bytes(),
            query.as_bytes(),
            Encoding::Hex,
        );
        query.push_str("&signature=");
        query.push_str(&signature);
        query
    }
}

/// Percent-encode one query value.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    encode_into(&mut out, value);
    out
}

/// RFC 3986: everything but unreserved characters is escaped.
fn encode_into(out: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xf) as usize] as char);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_the_documented_example() {
        // developers.binance.com, "SIGNED Endpoint Examples for POST /fapi/v1/order".
        let credentials = Credentials {
            key: String::new(),
            secret: "2b5eb11e18796d12d88f13dc27dbbd02c2cc51ff7059765ed9821957d82bb4d9".into(),
        };
        let payload = "symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=1&price=9000&timeInForce=GTC&recvWindow=5000&timestamp=1591702613943";
        assert_eq!(
            hmac_sha256(
                credentials.secret.as_bytes(),
                payload.as_bytes(),
                Encoding::Hex
            ),
            "3c661234138461fcc7a7d8746c6558c9842d4e10870d2ecbedf7777cad694af9"
        );
        let signed =
            Query::new()
                .str("symbol", "BTCUSDT")
                .signed(&credentials, 1591702613943.7, None);
        assert!(
            signed
                .starts_with("symbol=BTCUSDT&timestamp=1591702613943&recvWindow=20000&signature=")
        );
    }

    #[test]
    fn percent_encodes_json_parameters() {
        let query = Query::new().str("batchOrders", r#"[{"a":"b c"}]"#);
        assert_eq!(
            query.as_str(),
            "batchOrders=%5B%7B%22a%22%3A%22b%20c%22%7D%5D"
        );
    }
}
