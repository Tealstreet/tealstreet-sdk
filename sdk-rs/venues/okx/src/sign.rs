//! OKX canonical work uses the shared injected provider.
pub(crate) use crate::dispatch_signing::{DEMO_HEADERS, Http, WsLogin};

#[cfg(test)]
mod oracle {
    //! OKX signs the exact path/query and serialized body it sends.
    use safe_cex_crypto::{Encoding, hmac_sha256};

    pub struct Credentials {
        pub key: String,
        pub secret: String,
        pub passphrase: String,
    }

    impl Credentials {
        pub fn signature(&self, timestamp: &str, method: &str, path: &str, body: &str) -> String {
            hmac_sha256(
                self.secret.as_bytes(),
                format!("{timestamp}{method}{path}{body}").as_bytes(),
                Encoding::Base64,
            )
        }

        pub fn headers(
            &self,
            now: f64,
            method: &str,
            path: &str,
            body: &str,
        ) -> Vec<(&'static str, String)> {
            let timestamp = iso_timestamp(now);
            vec![
                ("OK-ACCESS-KEY", self.key.clone()),
                (
                    "OK-ACCESS-SIGN",
                    self.signature(&timestamp, method, path, body),
                ),
                ("OK-ACCESS-TIMESTAMP", timestamp),
                ("OK-ACCESS-PASSPHRASE", self.passphrase.clone()),
            ]
        }
    }

    fn iso_timestamp(ms: f64) -> String {
        safe_cex_engine::time::iso_millis(safe_cex_engine::time::wire_stamp_ms(ms) as i64)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn signs_exact_get_query_post_body_and_ws_preimages() {
            let c = Credentials {
                key: "test-key".into(),
                secret: "test-secret".into(),
                passphrase: "test-passphrase".into(),
            };
            assert_eq!(
                c.headers(0., "GET", "/api/v5/account/config", ""),
                vec![
                    ("OK-ACCESS-KEY", "test-key".into()),
                    (
                        "OK-ACCESS-SIGN",
                        "29klY97actzxrGqk1y/O9sExvXx4Rs9/13y6nq9tfi0=".into()
                    ),
                    ("OK-ACCESS-TIMESTAMP", "1970-01-01T00:00:00.000Z".into()),
                    ("OK-ACCESS-PASSPHRASE", "test-passphrase".into()),
                ]
            );
            assert_eq!(
                c.signature(
                    "1970-01-01T00:00:00.000Z",
                    "GET",
                    "/api/v5/account/config",
                    ""
                ),
                "29klY97actzxrGqk1y/O9sExvXx4Rs9/13y6nq9tfi0="
            );
            assert_eq!(
                c.signature("1234567890", "GET", "/users/self/verify", ""),
                "qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="
            );
            assert_eq!(
                c.signature(
                    "1970-01-01T00:00:00.000Z",
                    "POST",
                    "/api/v5/trade/batch-orders",
                    r#"[{"instId":"BTC-USDT-SWAP"}]"#
                ),
                "J5gNN8urKafdLakUupoqtac2yY2yQukwhRQPZYuj1ss="
            );
        }
        #[test]
        fn utc_milliseconds_and_leap_day() {
            assert_eq!(iso_timestamp(0.0), "1970-01-01T00:00:00.000Z");
            assert_eq!(
                iso_timestamp(1_709_209_849_123.9),
                "2024-02-29T12:30:49.123Z"
            );
            assert_eq!(iso_timestamp(-1.0), "1969-12-31T23:59:59.999Z");
        }
    }

    safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret, passphrase);
}
