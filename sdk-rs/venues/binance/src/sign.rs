//! Signed REST requests: Binance HMACs the query string and takes every
//! parameter, including POST/PUT/DELETE data, in the query.
use safe_cex_crypto::{Encoding, hmac_sha256};

pub const RECV_WINDOW_MS: u32 = 20_000;

#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}

pub use safe_cex_engine::url::Query;

pub trait Signed {
    /// Append `timestamp`, `recvWindow` and the HMAC-SHA256 `signature`.
    fn signed(self, credentials: &Credentials, now_ms: f64) -> String;
}

impl Signed for Query {
    fn signed(self, credentials: &Credentials, now_ms: f64) -> String {
        let mut query = self
            .num("timestamp", now_ms.floor())
            .num("recvWindow", f64::from(RECV_WINDOW_MS))
            .into_string();
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
        let signed = Query::new()
            .str("symbol", "BTCUSDT")
            .signed(&credentials, 1591702613943.7);
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
