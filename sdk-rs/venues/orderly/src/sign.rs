use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use safe_cex_engine::Method;
use zeroize::Zeroizing;

/// Whether `key` and the base58 seed have the 32-byte Ed25519 shapes native
/// accepts; either may carry the `ed25519:` prefix. Nothing is retained.
pub fn keys_valid(key: &str, secret: &str) -> bool {
    let key = key.strip_prefix("ed25519:").unwrap_or(key);
    let seed = secret.strip_prefix("ed25519:").unwrap_or(secret);
    let decoded = |text: &str| bs58::decode(text).into_vec().ok().map(Zeroizing::new);
    !key.is_empty()
        && decoded(key).is_some_and(|bytes| bytes.len() == 32)
        && decoded(seed).is_some_and(|bytes| bytes.len() == 32)
}

/// The exact `orderly-textsign` bytes; the local and dispatch signers share it.
fn canonical_text(timestamp: &str, method: Method, path: &str, body: &str) -> String {
    format!("{timestamp}{}{path}{body}", method.as_str())
}

/// Header names and order for every signed Orderly request.
fn headers(
    key: String,
    account: String,
    timestamp: String,
    signature: String,
    text: String,
    method: Method,
) -> Vec<(&'static str, String)> {
    vec![
        ("orderly-key", key),
        ("orderly-account-id", account),
        ("orderly-timestamp", timestamp),
        ("orderly-signature", signature),
        ("orderly-textsign", text),
        (
            "Content-Type",
            if matches!(method, Method::Get | Method::Delete) {
                "application/x-www-form-urlencoded"
            } else {
                "application/json"
            }
            .into(),
        ),
        ("Accept", "*/*".into()),
    ]
}

/// Dispatch recipe for one private Orderly REST request: the provider presents
/// the `ed25519:` public key and signs the exact textsign bytes at release.
/// `origin` is the REST base (scheme://host, no path) and `path` includes the
/// query; the outbound URL must be exactly `origin + path`. The body is read
/// from the outbound request.
pub struct Http {
    identity: safe_cex_engine::signing::Identity,
    account: String,
    method: Method,
    origin: String,
    path: String,
    prepared: Option<(String, String)>,
}
impl Http {
    pub fn new(
        identity: safe_cex_engine::signing::Identity,
        account: &str,
        method: Method,
        origin: &str,
        path: &str,
    ) -> Self {
        Self {
            identity,
            account: account.into(),
            method,
            origin: origin.into(),
            path: path.into(),
            prepared: None,
        }
    }
}
impl safe_cex_engine::SignStep for Http {
    fn prepare(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        dispatch: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::{
            Algorithm, Context, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
        };
        let safe_cex_engine::Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // The outbound must be exactly the request this recipe canonicalises.
        // The origin is scheme://host only, so the signed path is the real
        // URL path from its root and the split cannot be shifted.
        let bare_origin = self.origin.split_once("://").is_some_and(|(_, host)| {
            !host.is_empty() && !host.contains(['/', '?', '#', '@', '\\'])
        });
        if http.method != self.method
            || !bare_origin
            || !self.path.starts_with('/')
            || http.url.strip_prefix(&self.origin) != Some(self.path.as_str())
        {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let timestamp = (stamp as u64).to_string();
        let text = canonical_text(
            &timestamp,
            self.method,
            &self.path,
            http.body.as_deref().unwrap_or(""),
        );
        self.prepared = Some((timestamp, text.clone()));
        let host = http
            .url
            .split_once("://")
            .map_or("", |(_, rest)| rest.split('/').next().unwrap_or(""));
        Ok(SignRequest {
            receipt: SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp),
                expiry_ms: None,
            },
            operations: vec![
                Operation::Present {
                    key: self.identity.credential.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Ed25519,
                    key: self.identity.signing_ref()?.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: text.into_bytes(),
                    }],
                },
            ],
            context: Context {
                venue: "orderly".into(),
                method: self.method.as_str().into(),
                host: host.into(),
                path: self.path.clone(),
            },
        })
    }
    fn apply(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        result: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError;
        let safe_cex_engine::Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| key.starts_with("ed25519:") && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let signature = result
            .values
            .get(1)
            .filter(|signature| signature.len() == 64)
            .ok_or(SignError::InvalidResult)?;
        let (timestamp, text) = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        http.headers = headers(
            key.into(),
            self.account.clone(),
            timestamp,
            URL_SAFE.encode(signature),
            text,
            self.method,
        );
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// Dispatch recipe for the private socket's `auth` frame: the provider presents
/// the `ed25519:` public key and signs the whole-millisecond stamp's decimal text,
/// exactly as native does.
pub struct WsAuth {
    identity: safe_cex_engine::signing::Identity,
    host: String,
    stamp: Option<u64>,
}
impl WsAuth {
    pub fn new(identity: safe_cex_engine::signing::Identity, url: &str) -> Self {
        let host = url
            .split_once("://")
            .map_or("", |(_, rest)| rest.split('/').next().unwrap_or(""))
            .into();
        Self {
            identity,
            host,
            stamp: None,
        }
    }
}
impl safe_cex_engine::SignStep for WsAuth {
    fn prepare(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        dispatch: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::{
            Algorithm, Context, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
        };
        self.stamp = None;
        if !matches!(outbound, safe_cex_engine::Action::WsSend { .. }) {
            return Err(SignError::InvalidRequest);
        }
        let stamp_ms = dispatch.stamp_ms().floor();
        let receipt = SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        };
        receipt.validate()?;
        let stamp = stamp_ms as u64;
        let request = SignRequest {
            receipt,
            operations: vec![
                Operation::Present {
                    key: self.identity.credential.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Ed25519,
                    key: self.identity.signing_ref()?.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: stamp.to_string().into_bytes(),
                    }],
                },
            ],
            context: Context {
                venue: "orderly".into(),
                method: "WS".into(),
                host: self.host.clone(),
                path: "auth".into(),
            },
        };
        self.stamp = Some(stamp);
        Ok(request)
    }
    fn apply(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        result: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError;
        let timestamp = self.stamp.take().ok_or(SignError::InvalidRequest)?;
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| key.starts_with("ed25519:"))
            .ok_or(SignError::InvalidResult)?;
        let signature = result
            .values
            .get(1)
            .filter(|signature| signature.len() == 64)
            .ok_or(SignError::InvalidResult)?;
        let safe_cex_engine::Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        #[derive(serde::Serialize)]
        struct Params<'a> {
            orderly_key: &'a str,
            sign: String,
            timestamp: u64,
        }
        #[derive(serde::Serialize)]
        struct Auth<'a> {
            event: &'a str,
            params: Params<'a>,
        }
        *text = serde_json::to_string(&Auth {
            event: "auth",
            params: Params {
                orderly_key: key,
                sign: URL_SAFE.encode(signature),
                timestamp,
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
mod tests {
    use super::*;
    #[test]
    fn key_shapes_match_native_acceptance() {
        let seed = bs58::encode([7u8; 32]).into_string();
        assert!(keys_valid("11111111111111111111111111111111", &seed));
        assert!(keys_valid(
            "ed25519:11111111111111111111111111111111",
            &format!("ed25519:{seed}")
        ));
        for bad in [
            "0INVALID".to_owned(),
            bs58::encode([1; 31]).into_string(),
            bs58::encode([1; 64]).into_string(),
        ] {
            assert!(!keys_valid("11111111111111111111111111111111", &bad));
            assert!(!keys_valid(&bad, &seed));
        }
        assert!(!keys_valid("", &seed));
    }
}
