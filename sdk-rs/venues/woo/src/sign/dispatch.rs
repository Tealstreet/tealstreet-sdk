//! Exact WOO canonical work; the provider owns credential material.
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, num};

/// V1 mutations send the sorted query as a form body; everything else is JSON.
const FORM_HEADERS: &[(&str, &str)] = &[("Content-Type", "application/x-www-form-urlencoded")];
const JSON_HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];

fn headers(fixed: &'static [(&'static str, &'static str)]) -> Vec<(&'static str, String)> {
    fixed
        .iter()
        .map(|&(name, value)| (name, value.into()))
        .collect()
}

fn context(origin: &str, method: &str, path: &str) -> Context {
    Context {
        venue: "woo".into(),
        method: method.into(),
        host: origin
            .split_once("://")
            .map_or(origin, |(_, rest)| rest)
            .into(),
        path: path.into(),
    }
}

fn receipt(stamp_ms: f64) -> SignReceipt {
    SignReceipt {
        stamp_ms: Some(stamp_ms),
        expiry_ms: None,
        nonce: None,
    }
}

/// Presented key and the MAC as lowercase hex.
fn values(stamp_ms: f64, result: &SignResult) -> Result<(&str, String), SignError> {
    if result.receipt != receipt(stamp_ms)
        || result.values.len() != 2
        || result.values[1].len() != 32
        || !result.headers.is_empty()
    {
        return Err(SignError::InvalidResult);
    }
    let key = std::str::from_utf8(&result.values[0])
        .ok()
        .filter(|key| {
            !key.is_empty()
                && key.len() <= safe_cex_engine::signing_headers::MAX_VALUE_BYTES
                && !key.bytes().any(|b| b < 0x20 || b == 0x7f)
        })
        .ok_or(SignError::InvalidResult)?;
    Ok((
        key,
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex),
    ))
}

/// One signed REST request, over exactly the bytes sent. The caller's credential
/// selection supplies the identity; this recipe never chooses a slot.
pub struct Http {
    identity: Identity,
    origin: &'static str,
    method: Method,
    /// Path plus its query, as sent.
    path_query: String,
    body: Option<String>,
    /// V1 signs `{sorted query}|{stamp}`; V3 signs `{stamp}{METHOD}{path?query}{body}`.
    v1_query: Option<String>,
    stamp_ms: Option<f64>,
}

impl Http {
    /// Lays out the request as WOO expects it: V1 sorts the query and sends a
    /// mutation's query as its form body; V3 keeps the caller's order and body.
    pub fn new(
        identity: Identity,
        origin: &'static str,
        method: Method,
        path: &str,
        params: &[(String, String)],
        body: Option<String>,
    ) -> Self {
        let v1 = path.contains("/v1/");
        let serialized = super::query(params, v1);
        let mutation = method != Method::Get;
        let path_query = if !serialized.is_empty() && (!v1 || !mutation) {
            format!("{path}?{serialized}")
        } else {
            path.to_owned()
        };
        let body = if v1 && mutation {
            Some(serialized.clone())
        } else {
            body
        };
        Self {
            identity,
            origin,
            method,
            path_query,
            body,
            v1_query: v1.then_some(serialized),
            stamp_ms: None,
        }
    }

    pub fn url(&self) -> String {
        format!("{}{}", self.origin, self.path_query)
    }

    pub fn body(&self) -> Option<String> {
        self.body.clone()
    }

    fn fixed_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.v1_query.is_some() && self.method != Method::Get {
            FORM_HEADERS
        } else {
            JSON_HEADERS
        }
    }

    /// Refuse anything but the exact unsigned intent this recipe signs.
    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if !self.path_query.starts_with('/')
            || self.path_query.contains(['#', '\r', '\n'])
            || http.url != self.url()
            || http.method != self.method
            || http.body != self.body
            || http.headers != headers(self.fixed_headers())
            || !http.signer_headers.is_empty()
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}

impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        self.fixed_headers()
    }

    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp_ms = None;
        self.attest(outbound)?;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        let stamp_ms = safe_cex_engine::time::wire_stamp_ms(dispatch.stamp_ms());
        receipt(stamp_ms).validate()?;
        let stamp = num::format(stamp_ms);
        let parts: Vec<&str> = match &self.v1_query {
            Some(query) => vec![query, "|", &stamp],
            None => vec![
                &stamp,
                self.method.as_str(),
                &self.path_query,
                self.body.as_deref().unwrap_or_default(),
            ],
        };
        let request = hmac_sha256_work(
            &self.identity.credential,
            self.identity.signing_ref()?,
            parts
                .into_iter()
                .map(|part| PayloadPart::Bytes {
                    bytes: part.as_bytes().to_vec(),
                })
                .collect(),
            context(self.origin, self.method.as_str(), &self.path_query),
            receipt(stamp_ms),
        );
        self.stamp_ms = Some(stamp_ms);
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let stamp_ms = self.stamp_ms.take().ok_or(SignError::InvalidRequest)?;
        self.attest(outbound)?;
        let (key, signature) = values(stamp_ms, result)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let mut signed = vec![
            ("x-api-key", key.into()),
            ("x-api-timestamp", num::format(stamp_ms)),
            ("x-api-signature", signature),
        ];
        signed.append(&mut http.headers);
        http.headers = signed;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

/// The private socket auth: the HMAC of `|{stamp}`, signed by the identity the
/// caller selected (the Primary).
pub struct WsAuth {
    identity: Identity,
    url: String,
    stamp_ms: Option<f64>,
}

impl WsAuth {
    pub fn new(identity: Identity, url: impl Into<String>) -> Self {
        Self {
            identity,
            url: url.into(),
            stamp_ms: None,
        }
    }
}

impl SignStep for WsAuth {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp_ms = None;
        if !matches!(outbound, Action::WsSend { .. }) {
            return Err(SignError::InvalidRequest);
        }
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        let stamp_ms = safe_cex_engine::time::wire_stamp_ms(dispatch.stamp_ms());
        receipt(stamp_ms).validate()?;
        let request = hmac_sha256_work(
            &self.identity.credential,
            self.identity.signing_ref()?,
            vec![PayloadPart::Bytes {
                bytes: format!("|{}", num::format(stamp_ms)).into_bytes(),
            }],
            context(&self.url, "WS", "auth"),
            receipt(stamp_ms),
        );
        self.stamp_ms = Some(stamp_ms);
        Ok(request)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let stamp_ms = self.stamp_ms.take().ok_or(SignError::InvalidRequest)?;
        let (key, sign) = values(stamp_ms, result)?;
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        #[derive(serde::Serialize)]
        struct AuthParams<'a> {
            apikey: &'a str,
            sign: String,
            timestamp: f64,
        }
        #[derive(serde::Serialize)]
        struct Auth<'a> {
            event: &'static str,
            params: AuthParams<'a>,
        }
        *text = serde_json::to_string(&Auth {
            event: "auth",
            params: AuthParams {
                apikey: key,
                sign,
                timestamp: stamp_ms,
            },
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
