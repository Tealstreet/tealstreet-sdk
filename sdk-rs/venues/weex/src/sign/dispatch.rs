//! Opaque native canonical work; production builder adoption is a later phase.
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

pub const UNSIGNED_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("User-Agent", "Tealstreet"),
];
fn public_headers() -> Vec<(&'static str, String)> {
    UNSIGNED_HEADERS
        .iter()
        .map(|&(name, value)| (name, value.into()))
        .collect()
}
pub struct Http {
    identity: Identity,
    method: Method,
    path_query: String,
    body: Option<String>,
    stamp: Option<f64>,
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
            stamp: None,
        }
    }
    pub fn unsigned(&self) -> (String, Vec<(&'static str, String)>, Option<String>) {
        (url(&self.path_query), public_headers(), self.body.clone())
    }
}
/// The unsigned request URL, before any identity signs it.
pub fn url(path_query: &str) -> String {
    format!("{}{}", crate::paths::REST, path_query)
}
impl Http {
    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // Native endpoints are slash-separated alphanumerics; query values
        // already use the shared RFC3986 encoder. Refuse non-native input,
        // rather than normalize bytes after canonical signing.
        let (path, query) = self
            .path_query
            .split_once('?')
            .unwrap_or((&self.path_query, ""));
        if !path.starts_with("/capi/v3/")
            || !path
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/_-".contains(&byte))
            || !query
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.~%&=".contains(&byte))
            || http.method != self.method
            || http.url != format!("{}{}", crate::paths::REST, self.path_query)
            || http.body != self.body
            || http.headers != public_headers()
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
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp = None;
        self.attest(outbound)?;
        let stamp = context.stamp_ms();
        let work = work(
            &self.identity,
            format!(
                "{stamp:.0}{}{}{}",
                self.method.as_str(),
                self.path_query,
                self.body.as_deref().unwrap_or("")
            ),
            Context {
                venue: "weex".into(),
                method: self.method.as_str().into(),
                host: "api-contract.weex.com".into(),
                path: self.path_query.clone(),
            },
            stamp,
        )?;
        self.stamp = Some(stamp);
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let headers = signed_headers(self.stamp, result)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.headers = headers;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
pub struct Upgrade {
    identity: Identity,
    stamp: Option<f64>,
}
impl Upgrade {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            stamp: None,
        }
    }
    pub fn unsigned(&self) -> safe_cex_engine::SocketSpec {
        safe_cex_engine::SocketSpec::new("wss://ws-contract.weex.com/v3/ws/private")
            .headers(public_headers())
            .channel(safe_cex_engine::SocketChannel::Private)
    }
}
impl Upgrade {
    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        match outbound {
            Action::WsConnect {
                binary: false,
                url,
                headers,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } if url == "wss://ws-contract.weex.com/v3/ws/private"
                && *headers == public_headers() =>
            {
                Ok(())
            }
            _ => Err(SignError::InvalidRequest),
        }
    }
}
impl SignStep for Upgrade {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp = None;
        self.attest(outbound)?;
        let stamp = context.stamp_ms();
        let work = work(
            &self.identity,
            format!("{stamp:.0}/v3/ws/private"),
            Context {
                venue: "weex".into(),
                method: "WS".into(),
                host: "ws-contract.weex.com".into(),
                path: "/v3/ws/private".into(),
            },
            stamp,
        )?;
        self.stamp = Some(stamp);
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let signed = signed_headers(self.stamp, result)?;
        let Action::WsConnect { headers, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        *headers = signed;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

fn work(
    identity: &Identity,
    payload: String,
    context: Context,
    stamp: f64,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let passphrase = identity
        .passphrase
        .clone()
        .ok_or(SignError::InvalidRequest)?;
    let receipt = SignReceipt {
        nonce: None,
        stamp_ms: Some(stamp),
        expiry_ms: None,
    };
    receipt.validate()?;
    let mut request = hmac_sha256_work(
        identity,
        vec![PayloadPart::Bytes {
            bytes: payload.into_bytes(),
        }],
        context,
        receipt,
    );
    request
        .operations
        .push(Operation::Present { key: passphrase });
    Ok(request)
}

fn signed_headers(
    stamp: Option<f64>,
    result: &SignResult,
) -> Result<Vec<(&'static str, String)>, SignError> {
    let stamp = stamp.ok_or(SignError::InvalidRequest)?;
    if result.receipt
        != (SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp),
            expiry_ms: None,
        })
        || result.values.len() != 3
        || result.values[1].len() != 32
    {
        return Err(SignError::InvalidResult);
    }
    safe_cex_engine::signing_headers::ResultHeaderPolicy::Deny.validate(&result.headers)?;
    let key = credential(&result.values[0])?;
    let passphrase = credential(&result.values[2])?;
    const NAMES: [&str; 6] = [
        "ACCESS-KEY",
        "ACCESS-PASSPHRASE",
        "ACCESS-TIMESTAMP",
        "ACCESS-SIGN",
        "Content-Type",
        "User-Agent",
    ];
    // Validate the one owned outbound copy, then move it into the native shape.
    // No extra credential/passphrase copy survives validation or its failure.
    let values = [
        key.to_owned(),
        passphrase.to_owned(),
        format!("{stamp:.0}"),
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64),
        UNSIGNED_HEADERS[0].1.into(),
        UNSIGNED_HEADERS[1].1.into(),
    ];
    let mut headers = NAMES
        .into_iter()
        .zip(values)
        .map(|(name, value)| (name.to_owned(), value))
        .collect::<Vec<_>>();
    if let Err(error) = safe_cex_engine::signing_headers::validate(&headers) {
        for (_, value) in &mut headers {
            safe_cex_crypto::secrets::erase(value);
        }
        return Err(error);
    }
    Ok(NAMES
        .into_iter()
        .zip(headers)
        .map(|(name, (_, value))| (name, value))
        .collect())
}
fn credential(bytes: &[u8]) -> Result<&str, SignError> {
    std::str::from_utf8(bytes)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or(SignError::InvalidResult)
}
