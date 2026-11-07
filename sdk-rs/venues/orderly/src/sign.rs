use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use safe_cex_engine::Method;
use zeroize::{Zeroize, Zeroizing};

pub struct Credentials {
    pub key: String,
    pub account: String,
    seed: [u8; 32],
}
impl Credentials {
    pub fn new(key: &str, secret: &str, account: &str) -> Result<Self, String> {
        let key = key.strip_prefix("ed25519:").unwrap_or(key);
        let secret = secret.strip_prefix("ed25519:").unwrap_or(secret);
        if key.is_empty()
            || account.is_empty()
            || account
                .bytes()
                .any(|c| c.is_ascii_control() || matches!(c, b'/' | b'?' | b'#'))
        {
            return Err("Orderly key and applicationId are required".into());
        }
        if bs58::decode(key)
            .into_vec()
            .map_err(|_| "Orderly key must be base58")?
            .len()
            != 32
        {
            return Err("Orderly public key must be 32 bytes".into());
        }
        let decoded = Zeroizing::new(
            bs58::decode(secret)
                .into_vec()
                .map_err(|_| "Orderly secret must be base58")?,
        );
        let seed = decoded
            .as_slice()
            .try_into()
            .map_err(|_| "Orderly seed must be 32 bytes")?;
        Ok(Self {
            key: format!("ed25519:{key}"),
            account: account.into(),
            seed,
        })
    }
    pub fn signature(&self, payload: &str) -> Result<String, String> {
        safe_cex_crypto::ed25519::sign(payload.as_bytes(), &self.seed).map(|s| URL_SAFE.encode(s))
    }
    pub fn headers(
        &self,
        method: Method,
        path: &str,
        body: Option<&str>,
        now: f64,
    ) -> Result<Vec<(&'static str, String)>, String> {
        if !now.is_finite() || now < 0.0 {
            return Err("Orderly invalid clock".into());
        }
        let timestamp = (safe_cex_engine::time::wire_stamp_ms(now) as u64).to_string();
        let text = canonical_text(&timestamp, method, path, body.unwrap_or(""));
        let signature = self.signature(&text)?;
        Ok(headers(
            self.key.clone(),
            self.account.clone(),
            timestamp,
            signature,
            text,
            method,
        ))
    }
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

impl safe_cex_crypto::secrets::EraseSecrets for Credentials {
    fn erase_secrets(&mut self) {
        self.seed.zeroize();
        self.key.zeroize();
        self.account.zeroize();
    }
}
impl Drop for Credentials {
    fn drop(&mut self) {
        safe_cex_crypto::secrets::EraseSecrets::erase_secrets(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn credentials() -> Credentials {
        Credentials::new(
            "11111111111111111111111111111111",
            &bs58::encode([7u8; 32]).into_string(),
            "account-id",
        )
        .unwrap()
    }
    #[test]
    fn exact_method_query_and_body_bytes_are_signed_with_padding() {
        let c = credentials();
        let body = r#"{"symbol":"PERP_BTC_USDC","client_order_id":"caller-42"}"#;
        let h = c
            .headers(Method::Post, "/v1/order?z=last&a=first", Some(body), 1234.0)
            .unwrap();
        let text = h
            .iter()
            .find(|(k, _)| *k == "orderly-textsign")
            .unwrap()
            .1
            .as_str();
        assert_eq!(text, format!("1234POST/v1/order?z=last&a=first{body}"));
        let signature = &h.iter().find(|(k, _)| *k == "orderly-signature").unwrap().1;
        assert!(signature.ends_with("=="));
        assert_eq!(&c.signature(text).unwrap(), signature);
        for changed in [
            text.replacen("POST", "PUT", 1),
            text.replace("?z=last&a=first", "?a=first&z=last"),
            text.replace("caller-42", "caller-43"),
        ] {
            assert_ne!(c.signature(&changed).unwrap(), *signature);
        }
        assert_eq!(
            h.iter().find(|(k, _)| *k == "orderly-key").unwrap().1,
            "ed25519:11111111111111111111111111111111"
        );
    }
    #[test]
    fn malformed_credentials_and_clock_fail_before_signing() {
        for secret in [
            "0INVALID".to_owned(),
            bs58::encode([1; 31]).into_string(),
            bs58::encode([1; 64]).into_string(),
        ] {
            assert!(Credentials::new("11111111111111111111111111111111", &secret, "a").is_err());
        }
        assert!(
            credentials()
                .headers(Method::Get, "/v1/orders", None, f64::NAN)
                .is_err()
        );
        assert!(
            Credentials::new("bad\nheader", &bs58::encode([7; 32]).into_string(), "a").is_err()
        );
    }
}

#[cfg(test)]
mod security_erasure {
    use super::*;
    use safe_cex_crypto::secrets::EraseSecrets;
    #[test]
    fn stored_seed_and_original_options_are_both_scrubbed() {
        let mut c = Credentials::new(
            "11111111111111111111111111111111",
            &bs58::encode([7u8; 32]).into_string(),
            "account-id",
        )
        .unwrap();
        assert_eq!(c.seed, [7; 32]);
        c.erase_secrets();
        assert_eq!(c.seed, [0; 32]);
        assert!(c.key.is_empty() && c.account.is_empty());
    }
    #[test]
    fn invalid_decoded_seed_lengths_fail_without_returning_secret_bytes() {
        for length in [0, 1, 31, 33, 64] {
            let encoded = bs58::encode(vec![7; length]).into_string();
            let error = match Credentials::new(
                "11111111111111111111111111111111",
                &encoded,
                "account-id",
            ) {
                Ok(_) => panic!("invalid seed accepted"),
                Err(error) => error,
            };
            assert_eq!(error, "Orderly seed must be 32 bytes");
            assert!(!error.contains(&format!("{encoded:?}")));
        }
    }
}
