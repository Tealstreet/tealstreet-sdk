//! Bitunix signs compact body bytes and sorted, delimiter-free query parameters.
//! `Dispatch` is the dispatch recipe: the shared provider owns the key and the
//! secret-suffix digest. `Credentials` is the legacy local signer, removed when
//! the managed request conversion adopts the recipe.
use safe_cex_crypto::{hex, sha256, uuid::uuid_with_entropy};
use safe_cex_engine::signing::{
    Algorithm, Context, Encoding, Identity, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{Action, DispatchContext, SignStep, num};
#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}
pub fn signature(c: &Credentials, nonce: &str, timestamp: &str, query: &str, body: &str) -> String {
    let digest = hex(&sha256(
        format!("{nonce}{timestamp}{}{query}{body}", c.key).as_bytes(),
    ));
    hex(&sha256(format!("{digest}{}", c.secret).as_bytes()))
}
#[derive(Default, Clone)]
pub struct Params(Vec<(String, String)>);
impl Params {
    pub fn str(mut self, k: &str, v: &str) -> Self {
        self.0.push((k.into(), v.into()));
        self
    }
    pub fn num(self, k: &str, v: f64) -> Self {
        self.str(k, &num::format(v))
    }
    pub fn opt_str(self, k: &str, v: Option<&str>) -> Self {
        if let Some(v) = v {
            self.str(k, v)
        } else {
            self
        }
    }
    pub fn opt_num(self, k: &str, v: Option<f64>) -> Self {
        if let Some(v) = v {
            self.num(k, v)
        } else {
            self
        }
    }
    pub fn query(&self) -> String {
        let mut rows = self.0.clone();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
    pub(crate) fn signing(&self) -> String {
        let mut rows = self.0.clone();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.iter()
            .map(|(k, v)| format!("{k}{v}").replace(['=', '&'], ""))
            .collect()
    }
}
pub fn encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
pub fn headers(
    fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    c: &Credentials,
    params: &Params,
    body: &str,
    now: f64,
) -> Result<Vec<(&'static str, String)>, String> {
    let nonce = uuid_with_entropy(fill)?;
    let timestamp = num::format(safe_cex_engine::time::wire_stamp_ms(now));
    let sign = signature(c, &nonce, &timestamp, &params.signing(), body);
    Ok(vec![
        ("api-key", c.key.clone()),
        ("nonce", nonce),
        ("timestamp", timestamp),
        ("sign", sign),
        ("Content-Type", "application/json".into()),
    ])
}
/// Where a dispatch recipe writes its signature.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Http,
    Login,
}

/// Dispatch recipe for one private REST request or the private WS login.
/// Each preparation draws a fresh UUID nonce from signing entropy and the
/// protocol stamp; the provider computes
/// `hex(sha256(hex(sha256(nonce‖timestamp‖key‖query‖body))‖secret))`.
pub struct Dispatch {
    identity: Identity,
    target: Target,
    query: String,
    body: String,
    context: Context,
    nonce: String,
    timestamp: String,
}
impl Dispatch {
    /// `body` is the exact compact JSON sent, or empty.
    pub fn http(identity: Identity, method: &str, url: &str, params: &Params, body: &str) -> Self {
        Self::new(
            identity,
            Target::Http,
            method,
            url,
            params.signing(),
            body.into(),
        )
    }
    pub fn login(identity: Identity, url: &str) -> Self {
        Self::new(
            identity,
            Target::Login,
            "WS",
            url,
            String::new(),
            String::new(),
        )
    }
    fn new(
        identity: Identity,
        target: Target,
        method: &str,
        url: &str,
        query: String,
        body: String,
    ) -> Self {
        let (host, path) = url
            .split_once("://")
            .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
            .unwrap_or(("", ""));
        Self {
            identity,
            target,
            query,
            body,
            context: Context {
                venue: "bitunix".into(),
                method: method.into(),
                host: host.into(),
                path: format!("/{}", path.split('?').next().unwrap_or_default()),
            },
            nonce: String::new(),
            timestamp: String::new(),
        }
    }
    fn matches(&self, outbound: &Action) -> bool {
        match self.target {
            Target::Http => matches!(outbound, Action::Http(_)),
            Target::Login => matches!(outbound, Action::WsSend { .. }),
        }
    }
}
impl SignStep for Dispatch {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        self.nonce =
            uuid_with_entropy(|bytes| dispatch.signing_bytes(bytes).map_err(|e| e.to_string()))
                .map_err(|_| SignError::Unavailable)?;
        let stamp = dispatch.stamp_ms();
        self.timestamp = num::format(stamp);
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(stamp),
                expiry_ms: None,
            },
            operations: vec![
                Operation::Present {
                    key: self.identity.credential.clone(),
                },
                Operation::Sha256 {
                    payload: vec![
                        PayloadPart::Bytes {
                            bytes: format!("{}{}", self.nonce, self.timestamp).into_bytes(),
                        },
                        PayloadPart::Credential {
                            key: self.identity.credential.clone(),
                        },
                        PayloadPart::Bytes {
                            bytes: format!("{}{}", self.query, self.body).into_bytes(),
                        },
                    ],
                },
                Operation::Sign {
                    algorithm: Algorithm::Sha256SecretSuffix,
                    key: self.identity.signing.clone(),
                    payload: vec![PayloadPart::Result {
                        index: 1,
                        encoding: Encoding::Hex,
                    }],
                },
            ],
            context: self.context.clone(),
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let sign = result
            .values
            .get(2)
            .filter(|sign| sign.len() == 32)
            .map(|sign| hex(sign))
            .ok_or(SignError::InvalidResult)?;
        match outbound {
            Action::Http(http) => {
                http.headers = vec![
                    ("api-key", key.into()),
                    ("nonce", self.nonce.clone()),
                    ("timestamp", self.timestamp.clone()),
                    ("sign", sign),
                    ("Content-Type", "application/json".into()),
                ];
            }
            Action::WsSend { text, .. } => {
                *text = login_frame(key, &self.timestamp, &self.nonce, &sign)
            }
            _ => return Err(SignError::InvalidRequest),
        }
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// The private login frame; field order is the venue wire contract.
pub fn login_frame(api_key: &str, timestamp: &str, nonce: &str, sign: &str) -> String {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Login<'a> {
        api_key: &'a str,
        timestamp: &'a str,
        nonce: &'a str,
        sign: &'a str,
    }
    let args = [Login {
        api_key,
        timestamp,
        nonce,
        sign,
    }];
    format!(
        r#"{{"op":"login","args":{}}}"#,
        serde_json::to_string(&args).expect("login fields serialize")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_example_and_sorted_query() {
        // bitunix.com/api-docs/futures/common/sign.html
        let c = Credentials {
            key: "yourApiKey".into(),
            secret: "yourSecretKey".into(),
        };
        let body = r#"{"uid":"2899","arr":[{"id":1,"name":"maple"},{"id":2,"name":"lily"}]}"#;
        assert_eq!(
            signature(&c, "123456", "20241120123045", "id1uid200", body),
            "00397cd1e52c7dce3258067324363b6361fabc9178a0912b330c138db8745655"
        );
        assert_eq!(
            Params::default().str("uid", "200").str("id", "1").signing(),
            "id1uid200"
        );
    }
}

safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);
