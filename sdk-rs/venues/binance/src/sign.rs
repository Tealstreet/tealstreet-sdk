//! Binance canonicalisation recipes. Cryptographic operations and credentials
//! are owned by the injected Signer; this module never receives a secret.
pub use safe_cex_engine::signing::Identity as Credentials;
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
pub use safe_cex_engine::url::Query;
use safe_cex_engine::{Action, SignStep, num};
pub const RECV_WINDOW_MS: u32 = 20_000;

/// Exact-byte work for the trusted hosted-signing facade. Serialization and
/// identity selection belong to the caller; these helpers never execute crypto.
pub mod hosted_signing {
    use super::*;

    pub fn http_work(
        identity: &Identity,
        exact_encoded_query: String,
        stamp_ms: f64,
        context: Context,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &identity.credential,
            identity.signing_ref()?,
            vec![bytes(exact_encoded_query.into_bytes())],
            context,
            SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
            },
        ))
    }

    pub fn ws_work(
        identity: &Identity,
        exact_preimage: String,
        context: Context,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &identity.credential,
            identity.signing_ref()?,
            vec![bytes(exact_preimage.into_bytes())],
            context,
            SignReceipt::default(),
        ))
    }
}

pub(crate) struct Http {
    credentials: Credentials,
    query: Query,
    recv_window: Option<f64>,
    signed: bool,
    header_policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
}
impl Http {
    pub fn new(
        credentials: Credentials,
        query: Query,
        recv_window: Option<f64>,
        signed: bool,
    ) -> Self {
        Self {
            credentials,
            query,
            recv_window,
            signed,
            header_policy: Default::default(),
        }
    }
    pub fn with_header_policy(
        mut self,
        policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
    ) -> Self {
        if self.signed {
            self.header_policy = policy;
        }
        self
    }
}
fn bytes(bytes: impl Into<Vec<u8>>) -> PayloadPart {
    PayloadPart::Bytes {
        bytes: bytes.into(),
    }
}
fn request_context(url: &str, method: &str) -> Context {
    let (host, path) = url
        .split_once("://")
        .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
        .unwrap_or(("", ""));
    Context {
        venue: "binance".into(),
        method: method.into(),
        host: host.into(),
        path: format!("/{path}"),
    }
}
fn credential(value: &[u8]) -> Result<&str, SignError> {
    let value = std::str::from_utf8(value).map_err(|_| SignError::InvalidResult)?;
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    Ok(value)
}
fn hex(bytes: &[u8]) -> String {
    safe_cex_crypto::hex(bytes)
}
impl SignStep for Http {
    fn result_header_policy(&self) -> safe_cex_engine::signing_headers::ResultHeaderPolicy {
        self.header_policy
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let now = context.stamp_ms();
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let context = request_context(&http.url, http.method.as_str());
        if self.signed {
            let query = self
                .query
                .clone()
                .num("timestamp", now)
                .num(
                    "recvWindow",
                    self.recv_window.unwrap_or(f64::from(RECV_WINDOW_MS)),
                )
                .into_string();
            http.url.push('?');
            http.url.push_str(&query);
            return hosted_signing::http_work(&self.credentials, query, now, context);
        }
        Ok(SignRequest {
            receipt: SignReceipt::default(),
            operations: vec![Operation::Present {
                key: self.credentials.credential.clone(),
            }],
            context,
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = credential(&result.values[0])?;
        if self.signed {
            http.url.push_str("&signature=");
            http.url.push_str(&hex(&result.values[1]));
        }
        http.headers.push(("X-MBX-APIKEY", key.into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        let window = self.recv_window.unwrap_or(f64::from(RECV_WINDOW_MS));
        if window.is_nan() {
            1.
        } else {
            window.clamp(1., 5000.)
        }
    }
}

pub(crate) struct SpotSubscribe {
    credentials: Credentials,
    id: String,
    timestamp: f64,
}
impl SpotSubscribe {
    pub fn new(credentials: Credentials, id: String) -> Self {
        Self {
            credentials,
            id,
            timestamp: 0.,
        }
    }
}
impl SignStep for SpotSubscribe {
    fn prepare(
        &mut self,
        _: &mut Action,
        context: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let now = context.stamp_ms();
        self.timestamp = now;
        Ok(hmac_sha256_work(
            &self.credentials.credential,
            self.credentials.signing_ref()?,
            vec![
                bytes(b"apiKey=".to_vec()),
                PayloadPart::Credential {
                    key: self.credentials.credential.clone(),
                },
                bytes(
                    format!("&recvWindow=5000&timestamp={}", num::format(self.timestamp))
                        .into_bytes(),
                ),
            ],
            Context {
                venue: "binancespot".into(),
                method: "WS".into(),
                path: "userDataStream.subscribe.signature".into(),
                ..Default::default()
            },
            SignReceipt {
                nonce: None,
                stamp_ms: Some(self.timestamp),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // Struct declaration order preserves the existing serialized wire bytes.
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            api_key: &'a str,
            recv_window: u32,
            timestamp: f64,
            signature: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            id: &'a str,
            method: &'a str,
            params: Params<'a>,
        }
        let signature = hex(&result.values[1]);
        *text = serde_json::to_string(&Subscription {
            id: &self.id,
            method: "userDataStream.subscribe.signature",
            params: Params {
                api_key: credential(&result.values[0])?,
                recv_window: 5000,
                timestamp: self.timestamp,
                signature: &signature,
            },
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

#[cfg(test)]
#[path = "../tests/unit/hosted_signing.rs"]
mod hosted_signing_tests;
