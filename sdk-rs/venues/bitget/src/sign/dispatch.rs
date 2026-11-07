//! Canonical Bitget work. The injected provider owns all credential material.
use safe_cex_engine::signing::seconds_suffix_payload;
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, SignStep, num};

fn work(
    identity: &Identity,
    payload: Vec<PayloadPart>,
    context: Context,
    stamp_ms: f64,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let passphrase = identity
        .passphrase
        .clone()
        .ok_or(SignError::InvalidRequest)?;
    let mut work = hmac_sha256_work(
        &identity.credential,
        identity.signing_ref()?,
        payload,
        context,
        SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        },
    );
    work.operations.push(Operation::Present { key: passphrase });
    Ok(work)
}

fn values(result: &SignResult) -> Result<(&str, String, &str), SignError> {
    fn text(value: &[u8]) -> Result<&str, SignError> {
        std::str::from_utf8(value)
            .ok()
            .filter(|text| !text.is_empty() && !text.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)
    }
    if result.values.len() != 3 {
        return Err(SignError::InvalidResult);
    }
    let signature = result.values[1].as_slice();
    if signature.len() != 32 {
        return Err(SignError::InvalidResult);
    }
    Ok((
        text(&result.values[0])?,
        safe_cex_crypto::encode(signature, safe_cex_crypto::Encoding::Base64),
        text(&result.values[2])?,
    ))
}

fn context(origin: &str, method: &str, path: &str) -> Context {
    Context {
        venue: "bitget".into(),
        method: method.into(),
        host: origin
            .split_once("://")
            .map_or(origin, |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default()
            .into(),
        path: path.into(),
    }
}

pub(crate) struct Http {
    identity: Option<Identity>,
    path: String,
    stamp: String,
}
impl Http {
    pub fn new(identity: Option<Identity>, path: String) -> Self {
        Self {
            identity,
            path,
            stamp: String::new(),
        }
    }
}
impl SignStep for Http {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (_, authority_path) = http
            .url
            .split_once("://")
            .ok_or(SignError::InvalidRequest)?;
        let path_start = authority_path.find('/').ok_or(SignError::InvalidRequest)?;
        if authority_path[path_start..] != self.path {
            return Err(SignError::InvalidRequest);
        }
        let stamp_ms = dispatch.stamp_ms();
        self.stamp = num::format(stamp_ms);
        work(
            self.identity.as_ref().ok_or(SignError::Unavailable)?,
            vec![PayloadPart::Bytes {
                bytes: format!(
                    "{}{}{}{}",
                    self.stamp,
                    http.method.as_str(),
                    self.path,
                    http.body.as_deref().unwrap_or_default()
                )
                .into_bytes(),
            }],
            context(&http.url, http.method.as_str(), &self.path),
            stamp_ms,
        )
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (key, signature, passphrase) = values(result)?;
        if !http.headers.is_empty() {
            return Err(SignError::InvalidRequest);
        }
        http.headers = vec![
            ("ACCESS-KEY", key.into()),
            ("ACCESS-PASSPHRASE", passphrase.into()),
            ("ACCESS-SIGN", signature),
            ("ACCESS-TIMESTAMP", self.stamp.clone()),
            ("X-CHANNEL-API-CODE", "Tealstreet".into()),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

pub(crate) struct WsLogin {
    identity: Identity,
    origin: String,
    stamp: String,
}
impl WsLogin {
    pub fn new(identity: Identity, origin: String) -> Self {
        Self {
            identity,
            origin,
            stamp: String::new(),
        }
    }
}
impl SignStep for WsLogin {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if !matches!(outbound, Action::WsSend { .. }) {
            return Err(SignError::InvalidRequest);
        }
        let seconds = (dispatch.stamp_ms() / 1000.).floor();
        self.stamp = num::format(seconds);
        work(
            &self.identity,
            // Same typed grammar as OKX; the remote wire declares this suffix.
            seconds_suffix_payload(&self.stamp, "GET", "/user/verify"),
            context(&self.origin, "WS", "/user/verify"),
            seconds * 1000.,
        )
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (key, signature, passphrase) = values(result)?;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Login<'a> {
            api_key: &'a str,
            passphrase: &'a str,
            timestamp: &'a str,
            sign: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            op: &'static str,
            args: [Login<'a>; 1],
        }
        *text = serde_json::to_string(&Payload {
            op: "login",
            args: [Login {
                api_key: key,
                passphrase,
                timestamp: &self.stamp,
                sign: &signature,
            }],
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
