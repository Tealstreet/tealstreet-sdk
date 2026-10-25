//! Canonical Bybit recipes. The shared dispatch signer owns credentials and HMACs.
pub use safe_cex_engine::signing::Identity as Credentials;
use safe_cex_engine::signing::{Context, SignError, SignReceipt, SignRequest, SignResult};
use safe_cex_engine::{Action, DispatchContext, SignStep, num};
pub const RECV_WINDOW_MS: u32 = 20_000;
pub const BROKER_ID: &str = "Tealstreet";
const WS_AUTH_EXPIRY_MS: f64 = 10_000.0;
fn credential(value: &[u8]) -> Result<&str, SignError> {
    let value = std::str::from_utf8(value).map_err(|_| SignError::InvalidResult)?;
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    Ok(value)
}
fn context(url: &str, method: &str) -> Context {
    let (host, path) = url
        .split_once("://")
        .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
        .unwrap_or(("", ""));
    Context {
        venue: "bybit".into(),
        method: method.into(),
        host: host.into(),
        path: format!("/{path}"),
    }
}
pub(crate) struct Http {
    credentials: Credentials,
    payload: String,
    window: u32,
    stamp: String,
    header_policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
}
impl Http {
    pub fn new(credentials: Credentials, payload: String, window: Option<u32>) -> Self {
        Self {
            credentials,
            payload,
            window: window.unwrap_or(RECV_WINDOW_MS).clamp(1, RECV_WINDOW_MS),
            stamp: String::new(),
            header_policy: Default::default(),
        }
    }
}
impl Http {
    pub fn with_header_policy(
        mut self,
        policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
    ) -> Self {
        self.header_policy = policy;
        self
    }
}
impl SignStep for Http {
    fn result_header_policy(&self) -> safe_cex_engine::signing_headers::ResultHeaderPolicy {
        self.header_policy
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let stamp = dispatch.stamp_ms();
        self.stamp = num::format(stamp);
        Ok(crate::hosted_signing::http_work(
            &self.credentials,
            self.payload.clone(),
            stamp,
            self.window.to_string(),
            context(&http.url, http.method.as_str()),
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = credential(result.values.first().ok_or(SignError::InvalidResult)?)?;
        let mac = result
            .values
            .get(1)
            .filter(|mac| mac.len() == 32)
            .ok_or(SignError::InvalidResult)?;
        http.headers = vec![
            ("X-BAPI-API-KEY", key.into()),
            ("X-BAPI-TIMESTAMP", self.stamp.clone()),
            ("X-BAPI-RECV-WINDOW", self.window.to_string()),
            ("X-BAPI-SIGN", safe_cex_crypto::hex(mac)),
            ("X-Referer", BROKER_ID.into()),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        f64::from(self.window).min(5000.)
    }
}
pub(crate) struct WsAuth {
    credentials: Credentials,
    request_id: Option<String>,
    origin: String,
    expires: String,
}
impl WsAuth {
    pub fn new(credentials: Credentials, request_id: Option<String>, origin: String) -> Self {
        Self {
            credentials,
            request_id,
            origin,
            expires: String::new(),
        }
    }
}
impl SignStep for WsAuth {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if !matches!(outbound, Action::WsSend { .. }) {
            return Err(SignError::InvalidRequest);
        }
        let expiry = dispatch.stamp_ms() + WS_AUTH_EXPIRY_MS;
        self.expires = num::format(expiry);
        let mut work = crate::hosted_signing::ws_work(
            &self.credentials,
            format!("GET/realtime{}", self.expires),
            context(&self.origin, "WS"),
        );
        // Native auth generates this expiry; hosted auth preserves caller bytes.
        work.receipt = SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(expiry),
        };
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = credential(result.values.first().ok_or(SignError::InvalidResult)?)?;
        let mac = result
            .values
            .get(1)
            .filter(|mac| mac.len() == 32)
            .ok_or(SignError::InvalidResult)?;
        #[derive(serde::Serialize)]
        struct Auth<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            req_id: Option<&'a str>,
            op: &'a str,
            args: [&'a str; 3],
        }
        let signature = safe_cex_crypto::hex(mac);
        *text = serde_json::to_string(&Auth {
            req_id: self.request_id.as_deref(),
            op: "auth",
            args: [key, &self.expires, &signature],
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

#[cfg(test)]
pub(crate) mod oracle {
    use super::{BROKER_ID, RECV_WINDOW_MS, WS_AUTH_EXPIRY_MS};
    use safe_cex_crypto::{Encoding, hmac_sha256};
    use safe_cex_engine::num;
    #[derive(Clone)]
    pub struct Credentials {
        pub key: String,
        pub secret: String,
    }

    impl Credentials {
        /// The auth headers for one request at `timestamp_ms`.
        pub fn headers(&self, timestamp_ms: f64, payload: &str) -> Vec<(&'static str, String)> {
            self.headers_window(timestamp_ms, payload, None)
        }

        pub fn headers_window(
            &self,
            timestamp_ms: f64,
            payload: &str,
            window: Option<u32>,
        ) -> Vec<(&'static str, String)> {
            let window = window.unwrap_or(RECV_WINDOW_MS).clamp(1, RECV_WINDOW_MS);
            let timestamp = num::format(timestamp_ms.floor());
            let signed = format!("{timestamp}{}{window}{payload}", self.key);
            let signature = hmac_sha256(self.secret.as_bytes(), signed.as_bytes(), Encoding::Hex);
            vec![
                ("X-BAPI-API-KEY", self.key.clone()),
                ("X-BAPI-TIMESTAMP", timestamp),
                ("X-BAPI-RECV-WINDOW", window.to_string()),
                ("X-BAPI-SIGN", signature),
                ("X-Referer", BROKER_ID.to_owned()),
                ("Content-Type", "application/json".to_owned()),
            ]
        }

        /// `{"op":"auth","args":[key, expires, HMAC("GET/realtime" + expires)]}`.
        pub fn ws_auth(&self, now_ms: f64) -> String {
            let expires = num::format((now_ms + WS_AUTH_EXPIRY_MS).floor());
            let signature = hmac_sha256(
                self.secret.as_bytes(),
                format!("GET/realtime{expires}").as_bytes(),
                Encoding::Hex,
            );
            let key = serde_json::to_string(&self.key).unwrap_or_default();
            format!(r#"{{"op":"auth","args":[{key},"{expires}","{signature}"]}}"#)
        }
    }

    safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);
}

pub use safe_cex_engine::url::Query;

/// A JSON object written in field order, so the signed bytes are the sent bytes.
#[derive(Clone, Default)]
pub struct Body(String);

impl Body {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(&mut self, name: &str) {
        self.0.push(if self.0.is_empty() { '{' } else { ',' });
        self.0.push('"');
        self.0.push_str(name);
        self.0.push_str("\":");
    }

    pub fn str(mut self, name: &str, value: &str) -> Self {
        self.key(name);
        self.0
            .push_str(&serde_json::to_string(value).unwrap_or_default());
        self
    }

    pub fn opt_str(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(name, value),
            None => self,
        }
    }

    pub fn int(mut self, name: &str, value: i64) -> Self {
        self.key(name);
        self.0.push_str(&value.to_string());
        self
    }

    pub fn bool(mut self, name: &str, value: bool) -> Self {
        self.key(name);
        self.0.push_str(if value { "true" } else { "false" });
        self
    }

    /// A field whose value is already JSON.
    pub fn raw(mut self, name: &str, json: &str) -> Self {
        self.key(name);
        self.0.push_str(json);
        self
    }

    pub fn finish(mut self) -> String {
        if self.0.is_empty() {
            self.0.push('{');
        }
        self.0.push('}');
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::oracle::Credentials;
    use super::*;

    #[test]
    fn signs_timestamp_key_window_and_payload() {
        // developers' guide, "Create A Request": the plain text for a GET.
        let credentials = Credentials {
            key: "XXXXXXXXXX".into(),
            secret: "secret".into(),
        };
        let headers = credentials.headers(
            1658384314791.4,
            "category=option&symbol=BTC-29JUL22-25000-C",
        );
        let get = |name: &str| headers.iter().find(|(k, _)| *k == name).unwrap().1.clone();
        assert_eq!(get("X-BAPI-TIMESTAMP"), "1658384314791");
        assert_eq!(get("X-BAPI-RECV-WINDOW"), "20000");
        // HMAC-SHA256 of that plain text under "secret", computed with openssl.
        assert_eq!(
            get("X-BAPI-SIGN"),
            "5d43cb726beb406b53df5b66126578ec6013c25ea8927b3708a9482d6ef18bb3"
        );
        // "Authentication": expires is now + 10 s.
        assert_eq!(
            credentials.ws_auth(1662350400000.0),
            r#"{"op":"auth","args":["XXXXXXXXXX","1662350410000","d29d2c1ba514324abd09ebb5de55032b88be8769b16a263d718e752f892e5e3a"]}"#
        );
    }

    #[test]
    fn writes_bodies_in_field_order() {
        let body = Body::new()
            .str("category", "linear")
            .int("positionIdx", 0)
            .bool("reduceOnly", false)
            .raw("request", "[]")
            .finish();
        assert_eq!(
            body,
            r#"{"category":"linear","positionIdx":0,"reduceOnly":false,"request":[]}"#
        );
        assert_eq!(Body::new().finish(), "{}");
        assert_eq!(
            Query::new().str("cursor", "page_args%3Dx").into_string(),
            "cursor=page_args%253Dx"
        );
    }
}

#[cfg(test)]
mod security_receive_window {
    use super::oracle::Credentials;
    use safe_cex_crypto::{Encoding, hmac_sha256};

    #[test]
    fn smaller_receive_window_is_in_both_signed_preimage_and_header() {
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
            let header = |key| &headers.iter().find(|(k, _)| *k == key).unwrap().1;
            assert_eq!(header("X-BAPI-RECV-WINDOW"), &expected.to_string());
            assert_eq!(
                header("X-BAPI-SIGN"),
                &hmac_sha256(
                    b"s",
                    format!("1000k{expected}symbol=BTCUSDT").as_bytes(),
                    Encoding::Hex
                )
            );
        }
    }
}
