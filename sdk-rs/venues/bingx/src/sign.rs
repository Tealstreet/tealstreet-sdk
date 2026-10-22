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
    fn render(
        self,
        method: Method,
        path: &str,
        mut headers: Vec<(&'static str, String)>,
    ) -> RequestParts {
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

/// Stage native intent; only shared dispatch can materialize credentials.
pub fn stage<P>(
    cx: &mut safe_cex_engine::Cx<'_, P>,
    credentials: Option<&Credentials>,
    method: Method,
    path: &'static str,
    params: Params,
    at: Option<safe_cex_engine::json::ArrayAt>,
    pending: P,
) -> safe_cex_engine::RequestId {
    let recipe = credentials
        .map(|c| dispatch::Http::new(c.identity(method).clone(), method, path, params.clone()));
    let (url, headers, body) =
        params.render(method, path, vec![("X-SOURCE-KEY", "Tealstreet".into())]);
    let id = match at {
        Some(at) => cx.request_streamed_body(method, url, headers, body, at, pending),
        None => cx.request(method, url, headers, body, pending),
    };
    if let Some(recipe) = recipe {
        cx.sign_http(id, Box::new(recipe));
    }
    id
}
