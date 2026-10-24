//! Exact BloFin HTTP canonical work; the provider owns credential material.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, num};

/// Static public codec data; signed headers are inserted ahead of it.
pub const UNSIGNED_HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];

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
/// Refuse unusable identities before drawing nonce entropy, as legacy did.
fn ready(identity: &Identity) -> Result<(), SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    identity
        .passphrase
        .as_ref()
        .map(|_| ())
        .ok_or(SignError::InvalidRequest)
}

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
            .filter(|text| {
                !text.is_empty()
                    && text.len() <= safe_cex_engine::signing_headers::MAX_VALUE_BYTES
                    && !text.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
            })
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
        || !result.headers.is_empty()
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
    origin: &'static str,
    method: Method,
    path_query: String,
    body: Option<String>,
    stamp_ms: Option<f64>,
    nonce: String,
}
impl Http {
    pub fn new(
        identity: Identity,
        testnet: bool,
        method: Method,
        path_query: String,
        body: Option<String>,
    ) -> Self {
        Self {
            identity,
            origin: if testnet {
                "https://demo-trading-openapi.blofin.com"
            } else {
                "https://openapi.blofin.com"
            },
            method,
            path_query,
            body,
            stamp_ms: None,
            nonce: String::new(),
        }
    }
    pub fn unsigned(&self) -> (String, Vec<(&'static str, String)>, Option<String>) {
        (
            format!("{}{}", self.origin, self.path_query),
            unsigned_headers(),
            self.body.clone(),
        )
    }
    /// Refuse anything but the exact unsigned intent this recipe signs.
    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if !self.path_query.starts_with('/')
            || self.path_query.contains(['#', '\r', '\n'])
            || http.url != format!("{}{}", self.origin, self.path_query)
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
        ready(&self.identity)?;
        SignReceipt {
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        }
        .validate()?;
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
        let stamp_ms = self.stamp_ms.take().ok_or(SignError::InvalidRequest)?;
        let nonce = std::mem::take(&mut self.nonce);
        self.attest(outbound)?;
        let (key, signature, passphrase) = values(Some(stamp_ms), result)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let mut headers = vec![
            ("ACCESS-KEY", key.into()),
            ("ACCESS-SIGN", signature),
            ("ACCESS-NONCE", nonce),
            ("ACCESS-TIMESTAMP", num::format(stamp_ms)),
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
