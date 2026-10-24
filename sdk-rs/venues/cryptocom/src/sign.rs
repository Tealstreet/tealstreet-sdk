//! The venue signs sorted parameter names and their unseparated values.
//! The shared provider owns credentials and the HMAC; Envelope binds the wire.
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Param {
    Text(String),
    Number(u64),
    Strings(Vec<String>),
}

#[derive(Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Params(BTreeMap<&'static str, Param>);
impl Params {
    pub fn text(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.0.insert(key, Param::Text(value.into()));
        self
    }
    pub fn number(mut self, key: &'static str, value: u64) -> Self {
        self.0.insert(key, Param::Number(value));
        self
    }
    pub fn strings(mut self, key: &'static str, values: Vec<String>) -> Self {
        self.0.insert(key, Param::Strings(values));
        self
    }
    pub(crate) fn signing_text(&self) -> String {
        let mut text = String::new();
        for (key, value) in &self.0 {
            text.push_str(key);
            match value {
                Param::Text(value) => text.push_str(value),
                Param::Number(value) => text.push_str(&value.to_string()),
                Param::Strings(values) => {
                    for value in values {
                        text.push_str(value);
                    }
                }
            }
        }
        text
    }
}

pub mod dispatch;
pub use dispatch::Envelope;

/// Field order and spelling are the venue wire contract.
fn envelope_json(
    method: &str,
    api_key: &str,
    params: Option<&Params>,
    sig: &str,
    nonce: u64,
) -> String {
    #[derive(Serialize)]
    struct Request<'a> {
        id: u64,
        method: &'a str,
        api_key: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<&'a Params>,
        sig: &'a str,
        nonce: u64,
    }
    serde_json::to_string(&Request {
        id: nonce,
        method,
        api_key,
        params,
        sig,
        nonce,
    })
    .expect("envelope fields serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{Identity, SignError, SignResult};
    use safe_cex_engine::{Action, SignStep};
    #[test]
    fn recipe_apply_without_preparation_and_foreign_transports_are_refused() {
        use safe_cex_engine::signing::SignReceipt;
        use safe_cex_engine::{HttpRequest, RequestId};
        let identity: Identity = serde_json::from_str(
            r#"{"credential":"primary.credential","signing":"primary.signing"}"#,
        )
        .unwrap();
        let mut step = Envelope::http(
            identity,
            "private/get-accounts",
            Params::default(),
            "https://api.crypto.com/exchange/v1/private/get-accounts",
        );
        let mut http = Action::Http(HttpRequest {
            id: RequestId(1),
            method: safe_cex_engine::Method::Post,
            route: None,
            timeout_ms: None,
            url: String::new(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        });
        let result = |key: &[u8], mac: usize| {
            SignResult::new(vec![key.to_vec(), vec![7; mac]], SignReceipt::default())
        };
        for bad in [
            result(b"", 32),
            result(b"key\r\n", 32),
            result(b"key", 31),
            result(&[0xff], 32),
        ] {
            assert_eq!(step.apply(&mut http, &bad), Err(SignError::InvalidResult));
        }
        let Action::Http(sent) = &http else {
            unreachable!()
        };
        assert!(sent.body.is_none(), "no partial body on refusal");
        assert_eq!(
            step.apply(&mut http, &result(b"key", 32)),
            Err(SignError::InvalidResult)
        );
        let mut timer = Action::ClearTimer {
            timer: safe_cex_engine::TimerId(1),
        };
        assert_eq!(
            step.apply(&mut timer, &result(b"key", 32)),
            Err(SignError::InvalidRequest)
        );
        let mut auth = Envelope::ws_auth(
            serde_json::from_str(
                r#"{"credential":"primary.credential","signing":"primary.signing"}"#,
            )
            .unwrap(),
            "wss://stream.crypto.com/exchange/v1/user",
        );
        assert_eq!(
            auth.apply(&mut http, &result(b"key", 32)),
            Err(SignError::InvalidRequest),
            "the WS auth recipe never writes an HTTP body"
        );
    }
}
