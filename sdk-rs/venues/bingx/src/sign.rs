//! Sorted HMAC query signing; POST parameters retain their native JSON types.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Method, num};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Param {
    Text(String),
    Number(f64),
    Integer(u64),
    Boolean(bool),
}

impl Param {
    fn text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Number(n) => num::format(*n),
            Self::Integer(n) => n.to_string(),
            Self::Boolean(b) => b.to_string(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Params(BTreeMap<String, Param>);

pub type RequestParts = (String, Vec<(&'static str, String)>, Option<String>);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn str(mut self, name: &str, value: &str) -> Self {
        self.0.insert(name.into(), Param::Text(value.into()));
        self
    }
    pub fn num(mut self, name: &str, value: f64) -> Self {
        self.0.insert(name.into(), Param::Number(value));
        self
    }
    pub fn integer(mut self, name: &str, value: u64) -> Self {
        self.0.insert(name.into(), Param::Integer(value));
        self
    }
    pub fn boolean(mut self, name: &str, value: bool) -> Self {
        self.0.insert(name.into(), Param::Boolean(value));
        self
    }
    pub fn opt_num(self, name: &str, value: Option<f64>) -> Self {
        match value {
            Some(value) => self.num(name, value),
            None => self,
        }
    }
    pub fn opt_str(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(name, value),
            None => self,
        }
    }
    fn query(&self) -> String {
        self.0
            .iter()
            .map(|(key, value)| format!("{}={}", encode(key), encode(&value.text())))
            .collect::<Vec<_>>()
            .join("&")
    }
    pub fn into_request(
        mut self,
        method: Method,
        path: &str,
        credentials: Option<&Credentials>,
        now: f64,
    ) -> RequestParts {
        let mut headers = vec![("X-SOURCE-KEY", "Tealstreet".into())];
        if let Some(c) = credentials {
            headers.push(("X-BX-APIKEY", c.key.clone()));
            if path != "/openApi/user/auth/userDataStream" {
                self = self.num("timestamp", now.floor());
                let signature =
                    hmac_sha256(c.secret.as_bytes(), self.query().as_bytes(), Encoding::Hex);
                self = self.str("signature", &signature);
            }
        }
        let url = format!("{}{path}", crate::paths::REST);
        if method == Method::Post {
            headers.push(("Content-Type", "application/json".into()));
            let body = (!self.0.is_empty())
                .then(|| serde_json::to_string(&self.0).expect("finite request parameters"));
            (url, headers, body)
        } else {
            headers.push(("Content-Type", "application/json, charset=utf-8".into()));
            let query = self.query();
            (
                if query.is_empty() {
                    url
                } else {
                    format!("{url}?{query}")
                },
                headers,
                None,
            )
        }
    }
}

fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char)
        } else {
            out.push_str(&format!("%{byte:02X}"))
        }
    }
    out
}
