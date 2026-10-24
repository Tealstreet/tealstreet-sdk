//! The venue signs sorted parameter names and their unseparated values.
//! `Envelope` is the dispatch recipe: the shared signer owns credentials and
//! the HMAC. `Credentials` is the legacy local signer, removed when the managed
//! request conversion adopts the recipe.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, SignStep};
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

/// One signed JSON envelope: REST body or the private WS `public/auth` frame.
/// The nonce doubles as the envelope id and is drawn at each preparation.
pub struct Envelope {
    credentials: Identity,
    method: String,
    /// `None` for `public/auth`, whose envelope carries no params.
    params: Option<Params>,
    context: Context,
    nonce: u64,
}
impl Envelope {
    pub fn http(credentials: Identity, method: &str, params: Params, url: &str) -> Self {
        Self::new(credentials, method, Some(params), url, "POST")
    }
    pub fn ws_auth(credentials: Identity, url: &str) -> Self {
        Self::new(credentials, "public/auth", None, url, "WS")
    }
    fn new(
        credentials: Identity,
        method: &str,
        params: Option<Params>,
        url: &str,
        verb: &str,
    ) -> Self {
        let (host, path) = url
            .split_once("://")
            .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
            .unwrap_or(("", ""));
        Self {
            credentials,
            method: method.into(),
            params,
            context: Context {
                venue: "cryptocom".into(),
                method: verb.into(),
                host: host.into(),
                path: format!("/{path}"),
            },
            nonce: 0,
        }
    }
}
impl Envelope {
    /// REST recipes carry params; the WS auth recipe never does.
    fn transport(&self, outbound: &Action) -> Result<(), SignError> {
        match (outbound, self.params.is_some()) {
            (Action::Http(_), true) | (Action::WsSend { .. }, false) => Ok(()),
            _ => Err(SignError::InvalidRequest),
        }
    }
}
impl SignStep for Envelope {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.transport(outbound)?;
        let stamp = dispatch.stamp_ms();
        self.nonce = stamp as u64;
        let nonce = self.nonce;
        let signing_text = self
            .params
            .as_ref()
            .map(Params::signing_text)
            .unwrap_or_default();
        Ok(hmac_sha256_work(
            &self.credentials,
            vec![
                PayloadPart::Bytes {
                    bytes: format!("{}{nonce}", self.method).into_bytes(),
                },
                PayloadPart::Credential {
                    key: self.credentials.credential.clone(),
                },
                PayloadPart::Bytes {
                    bytes: format!("{signing_text}{nonce}").into_bytes(),
                },
            ],
            self.context.clone(),
            SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let mac = result
            .values
            .get(1)
            .filter(|mac| mac.len() == 32)
            .ok_or(SignError::InvalidResult)?;
        self.transport(outbound)?;
        let body = envelope_json(
            &self.method,
            key,
            self.params.as_ref(),
            &safe_cex_crypto::hex(mac),
            self.nonce,
        );
        match outbound {
            Action::Http(http) => http.body = Some(body),
            Action::WsSend { text, .. } => *text = body,
            _ => return Err(SignError::InvalidRequest),
        }
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

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
        step.apply(&mut http, &result(b"key", 32)).unwrap();
        let Action::Http(sent) = &http else {
            unreachable!()
        };
        assert!(sent.body.as_deref().unwrap().contains(r#""api_key":"key""#));
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
