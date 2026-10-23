//! Canonical BloFin dispatch work. The injected provider owns all credential
//! material; production adoption arrives with the custody cutover, so until
//! then only `tests/dispatch_recipe.rs` compiles this module.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, num};

/// Static public codec data; signed headers are inserted ahead of it.
pub const UNSIGNED_HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];
/// The private WS login signs this fixed GET target.
pub const VERIFY_PATH: &str = "/users/self/verify";

fn unsigned_headers() -> Vec<(&'static str, String)> {
    UNSIGNED_HEADERS
        .iter()
        .map(|&(name, value)| (name, value.into()))
        .collect()
}

/// The shared nanoid mapping over one fresh 21-byte signing-entropy draw.
fn nonce(dispatch: &DispatchContext<'_>) -> Result<String, SignError> {
    let mut failure = None;
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
        dispatch.signing_bytes(bytes).map_err(|error| {
            failure = Some(error);
            String::new()
        })
    })
    .map_err(|_| failure.unwrap_or(SignError::Unavailable))
}

/// Typed preimage parts in native order: path+query, METHOD, stamp, nonce, body.
/// The provider concatenates exact bytes; a remote codec splits on these parts.
fn work(
    identity: &Identity,
    parts: [&str; 5],
    context: Context,
    stamp_ms: f64,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let passphrase = identity
        .passphrase
        .clone()
        .ok_or(SignError::InvalidRequest)?;
    let receipt = SignReceipt {
        stamp_ms: Some(stamp_ms),
        expiry_ms: None,
    };
    receipt.validate()?;
    let mut request = hmac_sha256_work(
        identity,
        parts
            .into_iter()
            .map(|part| PayloadPart::Bytes {
                bytes: part.as_bytes().to_vec(),
            })
            .collect(),
        context,
        receipt,
    );
    request
        .operations
        .push(Operation::Present { key: passphrase });
    Ok(request)
}

/// Presented key, base64 of the MAC's ASCII lowercase hex, presented passphrase.
fn values(stamp_ms: Option<f64>, result: &SignResult) -> Result<(&str, String, &str), SignError> {
    fn text(value: &[u8]) -> Result<&str, SignError> {
        std::str::from_utf8(value)
            .ok()
            .filter(|text| !text.is_empty() && !text.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)
    }
    let stamp_ms = stamp_ms.ok_or(SignError::InvalidRequest)?;
    if result.receipt
        != (SignReceipt {
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        })
        || result.values.len() != 3
        || result.values[1].len() != 32
    {
        return Err(SignError::InvalidResult);
    }
    let hex = safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex);
    Ok((
        text(&result.values[0])?,
        STANDARD.encode(hex),
        text(&result.values[2])?,
    ))
}

fn context(url: &str, method: &str, path: &str) -> Context {
    Context {
        venue: "blofin".into(),
        method: method.into(),
        host: url
            .split_once("://")
            .map_or(url, |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default()
            .into(),
        path: path.into(),
    }
}

/// One signed REST request. The caller's credential selection supplies the
/// identity; this recipe never chooses a slot from the method.
pub struct Http {
    identity: Identity,
    method: Method,
    path_query: String,
    body: Option<String>,
    stamp_ms: Option<f64>,
    nonce: String,
}
impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        path_query: String,
        body: Option<String>,
    ) -> Self {
        Self {
            identity,
            method,
            path_query,
            body,
            stamp_ms: None,
            nonce: String::new(),
        }
    }
    /// Refuse anything but the exact unsigned intent this recipe signs.
    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (scheme, authority_path) = http
            .url
            .split_once("://")
            .ok_or(SignError::InvalidRequest)?;
        let path_start = authority_path.find('/').ok_or(SignError::InvalidRequest)?;
        if scheme != "https"
            || authority_path[path_start..] != self.path_query
            || http.method != self.method
            || http.body != self.body
            || http.headers != unsigned_headers()
            || !http.signer_headers.is_empty()
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}
impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        UNSIGNED_HEADERS
    }

    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp_ms = None;
        self.nonce.clear();
        self.attest(outbound)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let stamp_ms = dispatch.stamp_ms();
        let nonce = nonce(dispatch)?;
        let request = work(
            &self.identity,
            [
                &self.path_query,
                self.method.as_str(),
                &num::format(stamp_ms),
                &nonce,
                self.body.as_deref().unwrap_or_default(),
            ],
            context(&http.url, self.method.as_str(), &self.path_query),
            stamp_ms,
        )?;
        self.stamp_ms = Some(stamp_ms);
        self.nonce = nonce;
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let (key, signature, passphrase) = values(self.stamp_ms, result)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let mut headers = vec![
            ("ACCESS-KEY", key.into()),
            ("ACCESS-SIGN", signature),
            ("ACCESS-NONCE", self.nonce.clone()),
            (
                "ACCESS-TIMESTAMP",
                num::format(self.stamp_ms.ok_or(SignError::InvalidRequest)?),
            ),
            ("ACCESS-PASSPHRASE", passphrase.into()),
        ];
        headers.append(&mut http.headers);
        http.headers = headers;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

/// The private WS login, staged through `send_signed` with an empty text.
pub struct Login {
    identity: Identity,
    origin: String,
    stamp_ms: Option<f64>,
    nonce: String,
}
impl Login {
    pub fn new(identity: Identity, origin: String) -> Self {
        Self {
            identity,
            origin,
            stamp_ms: None,
            nonce: String::new(),
        }
    }
}
impl SignStep for Login {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp_ms = None;
        self.nonce.clear();
        if !matches!(outbound, Action::WsSend { text, .. } if text.is_empty()) {
            return Err(SignError::InvalidRequest);
        }
        let stamp_ms = dispatch.stamp_ms();
        let nonce = nonce(dispatch)?;
        let request = work(
            &self.identity,
            [VERIFY_PATH, "GET", &num::format(stamp_ms), &nonce, ""],
            context(&self.origin, "WS", VERIFY_PATH),
            stamp_ms,
        )?;
        self.stamp_ms = Some(stamp_ms);
        self.nonce = nonce;
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let (key, signature, passphrase) = values(self.stamp_ms, result)?;
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            api_key: &'a str,
            passphrase: &'a str,
            timestamp: u64,
            sign: &'a str,
            nonce: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            op: &'static str,
            args: [Args<'a>; 1],
        }
        // The receipt validated as a nonnegative safe integer; the wire is a JSON integer.
        let timestamp = self.stamp_ms.ok_or(SignError::InvalidRequest)? as u64;
        *text = serde_json::to_string(&Payload {
            op: "login",
            args: [Args {
                api_key: key,
                passphrase,
                timestamp,
                sign: &signature,
                nonce: &self.nonce,
            }],
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
