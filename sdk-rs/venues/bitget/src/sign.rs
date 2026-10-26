//! Native canonical work; credentials stay in the injected provider.
mod dispatch;
pub(crate) use dispatch::{Http, WsLogin};
pub use safe_cex_engine::url::encode;

#[cfg(test)]
pub(crate) mod oracle {
    use safe_cex_crypto::{Encoding, hmac_sha256};
    use safe_cex_engine::num;

    pub fn signature(
        secret: &str,
        timestamp: &str,
        method: &str,
        path: &str,
        body: &str,
    ) -> String {
        hmac_sha256(
            secret.as_bytes(),
            format!("{timestamp}{method}{path}{body}").as_bytes(),
            Encoding::Base64,
        )
    }

    pub fn headers(
        key: &str,
        secret: &str,
        passphrase: &str,
        now: f64,
        method: &str,
        path: &str,
        body: &str,
    ) -> Vec<(&'static str, String)> {
        let timestamp = num::format(safe_cex_engine::time::wire_stamp_ms(now));
        vec![
            ("ACCESS-KEY", key.to_owned()),
            ("ACCESS-PASSPHRASE", passphrase.to_owned()),
            (
                "ACCESS-SIGN",
                signature(secret, &timestamp, method, path, body),
            ),
            ("ACCESS-TIMESTAMP", timestamp),
            ("X-CHANNEL-API-CODE", "Tealstreet".to_owned()),
            ("Content-Type", "application/json".to_owned()),
        ]
    }

    #[test]
    fn fixed_native_header_oracle() {
        let headers = headers(
            "key",
            "material",
            "pass",
            1234567890999.9,
            "GET",
            "/user/verify",
            "",
        );
        assert_eq!(headers[3], ("ACCESS-TIMESTAMP", "1234567890999".into()));
        assert_eq!(
            signature("material", "1234567890", "GET", "/user/verify", ""),
            "bzh0vLZruCI40VGcKRLAHbocJlbUO8b2u+BNcAoIdfc="
        );
    }
}
