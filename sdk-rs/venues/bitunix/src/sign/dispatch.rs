//! Exact native bytes; the provider alone interpolates, signs and presents
//! material: `hex(sha256(hex(sha256(nonce ‖ ts ‖ key ‖ query ‖ body)) ‖ secret))`.
use safe_cex_engine::signing::{
    Context, Encoding, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

pub const HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];
pub const TESTNET_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("x-simulated-trading", "1"),
];
const HOST: &str = "fapi.bitunix.com";

/// Fresh per physical attempt: the nonce and stamp are drawn in `prepare`.
struct Attempt {
    nonce: String,
    receipt: SignReceipt,
}

fn work(
    identity: &Identity,
    dispatch: &DispatchContext<'_>,
    signed: &str,
    context: Context,
) -> Result<(SignRequest, Attempt), SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let nonce = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
        dispatch
            .signing_bytes(bytes)
            .map_err(|_| "signing entropy unavailable".to_owned())
    })
    .map_err(|_| SignError::Unavailable)?;
    let stamp = dispatch.stamp_after(None)?;
    let receipt = SignReceipt {
        nonce: None,
        stamp_ms: Some(stamp),
        expiry_ms: None,
    };
    receipt.validate()?;
    let request = SignRequest {
        receipt,
        operations: vec![
            Operation::Sha256 {
                payload: vec![
                    PayloadPart::Bytes {
                        bytes: format!("{nonce}{stamp:.0}").into_bytes(),
                    },
                    PayloadPart::Credential {
                        key: identity.credential.clone(),
                    },
                    PayloadPart::Bytes {
                        bytes: signed.as_bytes().to_vec(),
                    },
                ],
            },
            Operation::Sign {
                algorithm: safe_cex_engine::signing::Algorithm::Sha256SecretSuffix,
                key: identity.signing_ref()?.clone(),
                payload: vec![PayloadPart::Result {
                    index: 0,
                    encoding: Encoding::Hex,
                }],
            },
            Operation::Present {
                key: identity.credential.clone(),
            },
        ],
        context,
    };
    Ok((request, Attempt { nonce, receipt }))
}

/// The presented key and the hex signature, after checking the exact shape.
fn values(attempt: &Attempt, result: &SignResult) -> Result<(String, String), SignError> {
    if result.receipt != attempt.receipt
        || result.values.len() != 3
        || result.values[0].len() != 32
        || result.values[1].len() != 32
    {
        return Err(SignError::InvalidResult);
    }
    safe_cex_engine::signing_headers::ResultHeaderPolicy::Deny.validate(&result.headers)?;
    let key = std::str::from_utf8(&result.values[2])
        .ok()
        .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
        .ok_or(SignError::InvalidResult)?;
    Ok((
        key.to_owned(),
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex),
    ))
}

pub struct Http {
    identity: Identity,
    method: Method,
    url: String,
    body: Option<String>,
    /// Sorted, delimiter-free query pairs followed by the exact body.
    signed: String,
    headers: &'static [(&'static str, &'static str)],
    attempt: Option<Attempt>,
}

impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        url: String,
        query: String,
        body: Option<String>,
        testnet: bool,
    ) -> Self {
        let signed = format!("{query}{}", body.as_deref().unwrap_or(""));
        Self {
            identity,
            method,
            url,
            body,
            signed,
            headers: if testnet { TESTNET_HEADERS } else { HEADERS },
            attempt: None,
        }
    }

    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if http.method != self.method
            || http.url != self.url
            || http.body != self.body
            || !http.signer_headers.is_empty()
            || http.headers.len() != self.headers.len()
            || http
                .headers
                .iter()
                .zip(self.headers)
                .any(|((name, value), (n, v))| name != n || value != v)
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}

impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        self.headers
    }

    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.attempt = None;
        self.attest(outbound)?;
        let path = self
            .url
            .strip_prefix(&format!("https://{HOST}"))
            .ok_or(SignError::InvalidRequest)?;
        let path = path.split_once('?').map_or(path, |(path, _)| path);
        let (request, attempt) = work(
            &self.identity,
            dispatch,
            &self.signed,
            Context {
                venue: "bitunix".into(),
                method: self.method.as_str().into(),
                host: HOST.into(),
                path: path.into(),
            },
        )?;
        self.attempt = Some(attempt);
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let attempt = self.attempt.take().ok_or(SignError::InvalidRequest)?;
        let (key, sign) = values(&attempt, result)?;
        let stamp = attempt.receipt.stamp_ms.ok_or(SignError::InvalidResult)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // Native order: the signed headers lead the static ones.
        http.headers.splice(
            0..0,
            [
                ("api-key", key),
                ("nonce", attempt.nonce),
                ("timestamp", format!("{stamp:.0}")),
                ("sign", sign),
            ],
        );
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

/// The private socket login: the same batch over an empty query and body.
pub struct WsLogin {
    identity: Identity,
    attempt: Option<Attempt>,
}

impl WsLogin {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            attempt: None,
        }
    }
}

impl SignStep for WsLogin {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.attempt = None;
        if !matches!(outbound, Action::WsSend { text, .. } if text.is_empty()) {
            return Err(SignError::InvalidRequest);
        }
        let (request, attempt) = work(
            &self.identity,
            dispatch,
            "",
            Context {
                venue: "bitunix".into(),
                method: "WS".into(),
                host: HOST.into(),
                path: "/private/".into(),
            },
        )?;
        self.attempt = Some(attempt);
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let attempt = self.attempt.take().ok_or(SignError::InvalidRequest)?;
        let (key, sign) = values(&attempt, result)?;
        let stamp = attempt.receipt.stamp_ms.ok_or(SignError::InvalidResult)?;
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Login<'a> {
            api_key: &'a str,
            timestamp: &'a str,
            nonce: &'a str,
            sign: &'a str,
        }
        let args = [Login {
            api_key: &key,
            timestamp: &format!("{stamp:.0}"),
            nonce: &attempt.nonce,
            sign: &sign,
        }];
        *text = format!(
            r#"{{"op":"login","args":{}}}"#,
            serde_json::to_string(&args).map_err(|_| SignError::InvalidResult)?
        );
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
