//! Sorted HMAC query signing; POST parameters retain their native JSON types.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::url::encode;
use safe_cex_engine::{Method, num};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Credentials {
    pub read: Option<safe_cex_engine::ReadCredentials>,
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
            let read = c.read.as_ref().filter(|_| method == Method::Get);
            let key = read.map_or(c.key.as_str(), |r| &r.key);
            let secret = read.map_or(c.secret.as_str(), |r| &r.secret);
            headers.push(("X-BX-APIKEY", key.to_owned()));
            if signs(path) {
                self = self.num("timestamp", safe_cex_engine::time::wire_stamp_ms(now));
                let signature =
                    hmac_sha256(secret.as_bytes(), self.query().as_bytes(), Encoding::Hex);
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

/// Listen-key requests carry only the API key; every other path is signed.
pub fn signs(path: &str) -> bool {
    path != "/openApi/user/auth/userDataStream"
}

impl Drop for Credentials {
    fn drop(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read.as_mut() {
            read.erase_secrets();
        }
    }
}
