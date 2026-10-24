//! The venue signs sorted parameter names and their unseparated values.
//! `Envelope` is the dispatch recipe: the shared signer owns credentials and
//! the HMAC. `Credentials` is the legacy local signer, removed when the managed
//! request conversion adopts the recipe.
use safe_cex_crypto::{Encoding, hmac_sha256};
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

pub struct Credentials {
    pub key: String,
    pub secret: String,
}
impl Credentials {
    pub fn envelope(&self, method: &str, params: &Params, now: f64, auth: bool) -> String {
        let nonce = safe_cex_engine::time::wire_stamp_ms(now) as u64;
        let payload = format!(
            "{method}{nonce}{}{}{nonce}",
            self.key,
            params.signing_text()
        );
        let sig = hmac_sha256(self.secret.as_bytes(), payload.as_bytes(), Encoding::Hex);
        envelope_json(method, &self.key, (!auth).then_some(params), &sig, nonce)
    }
}
safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{Identity, SignError, SignResult};
    use safe_cex_engine::{Action, SignStep};
    #[test]
    fn sorted_fields_and_array_values_match_hmac_oracle() {
        let params = Params::default()
            .text("type", "MARKET")
            .text("side", "SELL")
            .text("quantity", "0.01")
            .text("instrument_name", "BTCUSD-PERP")
            .strings("exec_inst", vec!["REDUCE_ONLY".into()]);
        let credentials = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
        };
        let body = credentials.envelope("private/create-order", &params, 1610000000000.0, false);
        // Independent Python hmac.sha256 oracle, using the documented signing formula.
        assert!(body.contains("e9f8f06c03ce0214c03b5157d0a71fe3407d018fd0d96966a17c250bf93bd8bf"));
        assert!(body.contains(r#""nonce":1610000000000"#));
        assert!(
            !credentials
                .envelope("public/auth", &Params::default(), 1.0, true)
                .contains("params")
        );
    }

    #[test]
    fn recipe_apply_refuses_malformed_provider_results_and_foreign_actions() {
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
