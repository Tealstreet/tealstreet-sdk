//! Sorted HMAC query signing; POST parameters retain their native JSON types.
#[path = "sign/dispatch.rs"]
pub mod dispatch;
use safe_cex_engine::url::encode;
use safe_cex_engine::{Method, num};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct Credentials {
    pub read: Option<safe_cex_engine::signing::Identity>,
    pub primary: safe_cex_engine::signing::Identity,
}
impl Credentials {
    pub fn identity(&self, method: Method) -> &safe_cex_engine::signing::Identity {
        self.read
            .as_ref()
            .filter(|_| method == Method::Get)
            .unwrap_or(&self.primary)
    }
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

/// Native public transport bytes, shared with managed unsigned intent. API-key
/// and signature material are deliberately absent from this static declaration.
pub fn unsigned_headers(method: Method) -> &'static [(&'static str, &'static str)] {
    if method == Method::Post {
        &[
            ("X-SOURCE-KEY", "Tealstreet"),
            ("Content-Type", "application/json"),
        ]
    } else {
        &[
            ("X-SOURCE-KEY", "Tealstreet"),
            ("Content-Type", "application/json, charset=utf-8"),
        ]
    }
}

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
    /// Native wire shape, shared by legacy construction and dispatch recipes.
    pub(crate) fn render(
        self,
        method: Method,
        path: &str,
        mut headers: Vec<(&'static str, String)>,
    ) -> RequestParts {
        let public = unsigned_headers(method);
        headers.push((public[1].0, public[1].1.into()));
        let url = format!("{}{path}", crate::paths::REST);
        if method == Method::Post {
            let body = (!self.0.is_empty())
                .then(|| serde_json::to_string(&self.0).expect("finite request parameters"));
            (url, headers, body)
        } else {
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
