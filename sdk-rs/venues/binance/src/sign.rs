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
    fn signed(self, credentials: &Credentials, now_ms: f64, recv_window: Option<f64>) -> String;
}

impl Signed for Query {
    fn signed(self, credentials: &Credentials, now_ms: f64, recv_window: Option<f64>) -> String {
        let mut query = self
            .num("timestamp", now_ms.floor())
            .num(
                "recvWindow",
                recv_window.unwrap_or(f64::from(RECV_WINDOW_MS)),
            )
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
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../crypto/tests/fixtures/hmac/binance.json"
        ))
        .unwrap();
        let credentials = Credentials {
            key: String::new(),
            secret: vector["secret"].as_str().unwrap().into(),
        };
        let payload = vector["payload"].as_str().unwrap();
        assert_eq!(
            hmac_sha256(
                credentials.secret.as_bytes(),
                payload.as_bytes(),
                Encoding::Hex
            ),
            vector["signature"].as_str().unwrap()
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

safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);

#[cfg(test)]
mod security_receive_window {
    use super::*;

    #[test]
    fn configured_receive_window_is_signed_without_changing_supported_values() {
        let c = Credentials {
            key: "k".into(),
            secret: "s".into(),
        };
        for (window, expected) in [
            (None, 20_000.0),
            (Some(500.0), 500.0),
            (Some(0.0), 0.0),
            (Some(500.125), 500.125),
            (Some(60_000.0), 60_000.0),
        ] {
            let query = Query::new()
                .str("symbol", "BTCUSDT")
                .signed(&c, 1000.0, window);
            let (payload, signature) = query.split_once("&signature=").unwrap();
            assert!(payload.ends_with(&format!("recvWindow={expected}")));
            assert_eq!(
                signature,
                hmac_sha256(b"s", payload.as_bytes(), Encoding::Hex)
            );
        }
    }
}
