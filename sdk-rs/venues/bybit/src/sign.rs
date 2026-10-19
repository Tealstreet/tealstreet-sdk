//! Signed REST and private socket auth. Bybit HMACs
//! `timestamp + key + recvWindow + payload`, where the payload is the query
//! string of a GET and the JSON body of a POST, and sends the result in headers.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::num;

pub const RECV_WINDOW_MS: u32 = 20_000;
pub const BROKER_ID: &str = "Tealstreet";
/// A socket auth signature stays valid this long.
const WS_AUTH_EXPIRY_MS: f64 = 10_000.0;

#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}

impl Credentials {
    /// The auth headers for one request at `timestamp_ms`.
    pub fn headers(&self, timestamp_ms: f64, payload: &str) -> Vec<(&'static str, String)> {
        let timestamp = num::format(timestamp_ms.floor());
        let signed = format!("{timestamp}{}{RECV_WINDOW_MS}{payload}", self.key);
        let signature = hmac_sha256(self.secret.as_bytes(), signed.as_bytes(), Encoding::Hex);
        vec![
            ("X-BAPI-API-KEY", self.key.clone()),
            ("X-BAPI-TIMESTAMP", timestamp),
            ("X-BAPI-RECV-WINDOW", RECV_WINDOW_MS.to_string()),
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

/// A query string under construction, percent-encoded as it is built.
#[derive(Clone, Default)]
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

    pub fn into_string(self) -> String {
        self.0
    }
}

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

/// RFC 3986: everything but unreserved characters is escaped, as `qs` does.
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
