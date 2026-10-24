#![allow(dead_code)]
//! Classic Futures API key version 2 signs the raw path and the passphrase.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::Method;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerOptions {
    pub partner: String,
    pub broker_key: String,
    pub broker_name: String,
    pub verify: Option<bool>,
}

pub struct Credentials {
    pub key: String,
    pub secret: String,
    pub passphrase: String,
    pub broker: Option<BrokerOptions>,
}

pub(crate) const PUBLIC_HEADERS: &[(&str, &str)] = &[
    ("X-CHANNEL-API-CODE", "3zexp"),
    ("Content-Type", "application/json"),
];
pub(crate) fn public_headers() -> Vec<(&'static str, String)> {
    PUBLIC_HEADERS
        .iter()
        .map(|(name, value)| (*name, (*value).into()))
        .collect()
}

pub(crate) fn public_request_headers(
    credentials: Option<&Credentials>,
    method: Method,
    path: &str,
    now: f64,
) -> Vec<(&'static str, String)> {
    credentials.map_or_else(public_headers, |credentials| {
        headers(credentials, method, path, "", now)
    })
}

pub fn headers(
    credentials: &Credentials,
    method: Method,
    path: &str,
    body: &str,
    now: f64,
) -> Vec<(&'static str, String)> {
    let timestamp = (safe_cex_engine::time::wire_stamp_ms(now) as u64).to_string();
    let payload = format!("{timestamp}{}{path}{body}", method.as_str());
    let mut headers = vec![
        ("KC-API-KEY", credentials.key.clone()),
        ("KC-API-TIMESTAMP", timestamp.clone()),
        ("KC-API-KEY-VERSION", "2".into()),
        (
            "KC-API-SIGN",
            hmac_sha256(
                credentials.secret.as_bytes(),
                payload.as_bytes(),
                Encoding::Base64,
            ),
        ),
        (
            "KC-API-PASSPHRASE",
            hmac_sha256(
                credentials.secret.as_bytes(),
                credentials.passphrase.as_bytes(),
                Encoding::Base64,
            ),
        ),
        ("Content-Type", "application/json".into()),
        ("X-CHANNEL-API-CODE", "3zexp".into()),
    ];
    if let Some(broker) = &credentials.broker {
        let partner = broker.partner.trim();
        let name = broker.broker_name.trim();
        let key = broker.broker_key.trim();
        if !partner.is_empty() && !name.is_empty() && !key.is_empty() {
            let payload = format!("{timestamp}{partner}{}", credentials.key);
            headers.extend([
                ("KC-API-PARTNER", partner.into()),
                (
                    "KC-API-PARTNER-SIGN",
                    hmac_sha256(key.as_bytes(), payload.as_bytes(), Encoding::Base64),
                ),
                ("KC-BROKER-NAME", name.into()),
                (
                    "KC-API-PARTNER-VERIFY",
                    broker.verify.unwrap_or(true).to_string(),
                ),
            ]);
        }
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broker_headers_share_the_request_timestamp_and_default_to_verify() {
        let mut credentials = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
            passphrase: "replay-passphrase".into(),
            broker: Some(BrokerOptions {
                partner: " partner-test ".into(),
                broker_key: " broker-test-key ".into(),
                broker_name: " Broker Name ".into(),
                verify: None,
            }),
        };
        let signed = headers(
            &credentials,
            Method::Post,
            "/api/v1/bullet-private",
            "",
            1700000000123.0,
        );
        assert!(signed.contains(&(
            "KC-API-PARTNER-SIGN",
            "qr2oPnOafgsiv3qWURUltiI7sSCTQIUK2rwlHEDADnY=".into()
        )));
        assert!(signed.contains(&("KC-API-PARTNER-VERIFY", "true".into())));
        assert!(signed.contains(&("KC-BROKER-NAME", "Broker Name".into())));
        credentials.broker.as_mut().unwrap().verify = Some(false);
        assert!(
            headers(
                &credentials,
                Method::Get,
                "/api/v1/positions",
                "",
                1700000000123.0
            )
            .contains(&("KC-API-PARTNER-VERIFY", "false".into()))
        );
    }
    #[test]
    fn version_two_signs_raw_query_and_passphrase_independently() {
        // Python hmac.sha256/base64 oracle from the documented prehash formula.
        let credentials = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
            passphrase: "passphrase!@#".into(),
            broker: None,
        };
        let get = headers(
            &credentials,
            Method::Get,
            "/api/v1/orders?symbol=XBTUSDTM&clientOid=hello!@#",
            "",
            1700000000123.0,
        );
        assert!(get.contains(&(
            "KC-API-SIGN",
            "pxv5Qv3DGyKlZAA4GNIcY8RVnp7FtaZmSmJx28Us0Ac=".into()
        )));
        assert!(get.contains(&(
            "KC-API-PASSPHRASE",
            "EjiYZdaPT/Nrmf+GmwgMlSoUxuCsxdZclZcIu/0i9Co=".into()
        )));
        assert!(get.contains(&("KC-API-KEY-VERSION", "2".into())));
        let post = headers(
            &credentials,
            Method::Post,
            "/api/v1/orders",
            r#"{"symbol":"XBTUSDTM","size":2}"#,
            1700000000123.0,
        );
        assert!(post.contains(&(
            "KC-API-SIGN",
            "wgkeALmBKfFSYSRNL+tgo/XH5cwGnkzyz5SQRRKgEnE=".into()
        )));
    }
}

safe_cex_crypto::redacted_debug!(BrokerOptions);
safe_cex_crypto::erase_credentials_on_drop!(BrokerOptions, broker_key);
impl safe_cex_crypto::secrets::EraseSecrets for Credentials {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        safe_cex_crypto::secrets::erase(&mut self.passphrase);
        if let Some(broker) = self.broker.as_mut() {
            safe_cex_crypto::secrets::EraseSecrets::erase_secrets(broker);
        }
    }
}
impl Drop for Credentials {
    fn drop(&mut self) {
        safe_cex_crypto::secrets::EraseSecrets::erase_secrets(self);
    }
}
