//! MEXC signs the exact POST body, or the sorted GET query, in REST headers.
pub mod dispatch;
#[cfg(test)]
use safe_cex_crypto::{Encoding, hmac_sha256};
#[cfg(test)]
use safe_cex_engine::num;
use safe_cex_engine::url::encode;
use std::collections::BTreeMap;

#[cfg(test)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}
#[cfg(test)]
impl Credentials {
    pub fn signature(&self, now: f64, payload: &str) -> String {
        hmac_sha256(
            self.secret.as_bytes(),
            format!(
                "{}{}{payload}",
                self.key,
                num::format(safe_cex_engine::time::wire_stamp_ms(now))
            )
            .as_bytes(),
            Encoding::Hex,
        )
    }
    #[cfg(test)]
    pub fn headers(&self, now: f64, payload: &str) -> Vec<(&'static str, String)> {
        self.headers_window(now, payload, None)
    }
    pub fn headers_window(
        &self,
        now: f64,
        payload: &str,
        window: Option<u32>,
    ) -> Vec<(&'static str, String)> {
        let window = window.unwrap_or(20_000).clamp(1, 20_000);
        vec![
            ("ApiKey", self.key.clone()),
            (
                "Request-Time",
                num::format(safe_cex_engine::time::wire_stamp_ms(now)),
            ),
            ("Signature", self.signature(now, payload)),
            ("Recv-Window", window.to_string()),
            ("Content-Type", "application/json".into()),
        ]
    }
}
pub fn query(params: impl IntoIterator<Item = (&'static str, String)>) -> String {
    params
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(k, v)| format!("{k}={}", encode(&v)))
        .collect::<Vec<_>>()
        .join("&")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_protocol_time_uses_the_same_integer_in_header_and_preimage() {
        let c = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
        };
        let headers = c.headers(1700000000000.875, "[\"739113577038255616\"]");
        assert!(headers.contains(&("Request-Time", "1700000000000".into())));
        assert!(headers.contains(&(
            "Signature",
            "59e9ebb06b0e27069eec29158b3682a9f279f1e3790b0fa09a403d4965270557".into()
        )));
    }
    #[test]
    fn signs_exact_post_bytes_and_sorted_get_parameters() {
        let c = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
        };
        assert_eq!(
            query([("symbol", "BTC_USDT".into()), ("page_num", "1".into())]),
            "page_num=1&symbol=BTC_USDT"
        );
        assert_eq!(
            c.signature(1700000000000.0, "[\"739113577038255616\"]"),
            "59e9ebb06b0e27069eec29158b3682a9f279f1e3790b0fa09a403d4965270557"
        );
        assert_ne!(
            c.signature(1.0, r#"{"vol":1}"#),
            c.signature(1.0, r#"{ "vol":1}"#)
        );
    }
}

#[cfg(test)]
safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);

#[cfg(test)]
mod security_receive_window {
    use super::*;

    #[test]
    fn smaller_receive_window_reaches_header_and_cannot_be_unbounded() {
        let c = Credentials {
            key: "k".into(),
            secret: "s".into(),
        };
        for (window, expected) in [
            (None, 20_000),
            (Some(500), 500),
            (Some(0), 1),
            (Some(u32::MAX), 20_000),
        ] {
            let headers = c.headers_window(1000.0, "symbol=BTCUSDT", window);
            assert!(
                headers
                    .iter()
                    .any(|(k, v)| *k == "Recv-Window" && v == &expected.to_string())
            );
        }
    }
}
