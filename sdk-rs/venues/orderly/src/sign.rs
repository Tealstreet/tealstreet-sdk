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
        let text = canonical_text(&timestamp, method.as_str(), path, body.unwrap_or(""));
        Ok(vec![
            ("orderly-key", self.key.clone()),
            ("orderly-account-id", self.account.clone()),
            ("orderly-timestamp", timestamp),
            ("orderly-signature", self.signature(&text)?),
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
        ])
    }
}

/// The exact `orderly-textsign` bytes; local signing and the remote recipe share it.
fn canonical_text(timestamp: &str, method: &str, path: &str, body: &str) -> String {
    format!("{timestamp}{method}{path}{body}")
}

/// Private REST paths this venue signs; anything else is refused remotely.
const SIGNED_PATHS: &[&str] = &[
    "/v1/client/info",
    "/v1/client/margin_modes",
    "/v1/client/margin_mode",
    "/v1/client/leverages",
    "/v1/client/leverage",
    "/v1/positions",
    "/v1/orders",
    "/v1/algo/orders",
    "/v1/order",
    "/v1/algo/order",
    "/v1/trades",
    "/v1/funding_fee/history",
];

fn signed_path(path: &str) -> bool {
    let route = path.split_once('?').map_or(path, |(route, _)| route);
    SIGNED_PATHS.contains(&route)
        || route
            .strip_prefix("/v1/order/")
            .and_then(|rest| rest.strip_suffix("/trades"))
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// `orderly.rest.1`: ed25519 over `timestamp ‖ METHOD ‖ path?query ‖ body`.
/// `orderly.rest.1` inputs; any other field is refused.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RestInputs<'a> {
    #[serde(borrow)]
    timestamp: std::borrow::Cow<'a, str>,
    #[serde(borrow)]
    method: std::borrow::Cow<'a, str>,
    #[serde(borrow)]
    path: std::borrow::Cow<'a, str>,
    #[serde(borrow, default)]
    body: std::borrow::Cow<'a, str>,
}

pub struct Rest;
impl safe_cex_engine::signing_remote::RemoteRecipe for Rest {
    fn id(&self) -> &'static str {
        "orderly.rest.1"
    }
    fn algorithm(&self) -> safe_cex_engine::signing::Algorithm {
        safe_cex_engine::signing::Algorithm::Ed25519
    }
    fn role(&self) -> Option<safe_cex_engine::signing::ExtraKey> {
        None
    }
    fn canonicalize(&self, inputs: &str) -> Result<Vec<u8>, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError::Refused;
        let inputs: RestInputs = safe_cex_engine::signing_remote::parse_inputs(inputs)?;
        if !matches!(&*inputs.method, "GET" | "POST" | "PUT" | "DELETE")
            || inputs.timestamp.is_empty()
            || inputs.timestamp.len() > 16
            || !inputs.timestamp.bytes().all(|b| b.is_ascii_digit())
            || !signed_path(&inputs.path)
            || inputs
                .path
                .bytes()
                .any(|b| b.is_ascii_control() || b == b'#')
        {
            return Err(Refused);
        }
        Ok(canonical_text(
            &inputs.timestamp,
            &inputs.method,
            &inputs.path,
            &inputs.body,
        )
        .into_bytes())
    }
    fn stamp_ms(&self, inputs: &str) -> Result<f64, safe_cex_engine::signing::SignError> {
        let inputs: RestInputs = safe_cex_engine::signing_remote::parse_inputs(inputs)?;
        inputs
            .timestamp
            .parse::<u64>()
            .map(|stamp| stamp as f64)
            .map_err(|_| safe_cex_engine::signing::SignError::Refused)
    }
}
pub static REST: Rest = Rest;

/// Verifier for `orderly.rest.1` results from the account's public `ed25519:` key.
pub fn remote_verifier(
    key: &str,
) -> Result<safe_cex_engine::signing_remote::RemoteVerifier, String> {
    let public: [u8; 32] = bs58::decode(key.strip_prefix("ed25519:").unwrap_or(key))
        .into_vec()
        .map_err(|_| "Orderly key must be base58")?
        .try_into()
        .map_err(|_| "Orderly public key must be 32 bytes")?;
    Ok(Box::new(move |payload, signature| {
        safe_cex_crypto::ed25519::verify(payload, &public, signature).unwrap_or(false)
    }))
}
pub static REMOTE_RECIPES: &[&dyn safe_cex_engine::signing_remote::RemoteRecipe] = &[&REST];

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
    fn remote_recipe_rebuilds_the_exact_local_preimage_and_signature() {
        use safe_cex_engine::signing_remote::RemoteRecipe;
        let c = credentials();
        let body = r#"{"symbol":"PERP_BTC_USDC","client_order_id":"caller-42"}"#;
        let h = c
            .headers(Method::Post, "/v1/order", Some(body), 1234.0)
            .unwrap();
        let header = |name: &str| h.iter().find(|(k, _)| *k == name).unwrap().1.clone();
        let bytes = REST
            .canonicalize(
                &serde_json::json!({
                    "timestamp": "1234", "method": "POST", "path": "/v1/order", "body": body
                })
                .to_string(),
            )
            .unwrap();
        assert_eq!(bytes, header("orderly-textsign").into_bytes());
        assert_eq!(
            c.signature(std::str::from_utf8(&bytes).unwrap()).unwrap(),
            header("orderly-signature")
        );
        let read = REST
            .canonicalize(
                &serde_json::json!({
                    "timestamp": "1", "method": "GET", "path": "/v1/order/123/trades"
                })
                .to_string(),
            )
            .unwrap();
        assert_eq!(read, b"1GET/v1/order/123/trades");
    }

    #[test]
    fn remote_verifier_accepts_only_the_account_key_over_the_exact_bytes() {
        // RFC 8032 test 1: a published seed/public-key pair, not account material.
        let seed = [
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ];
        let public = [
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64,
            0x07, 0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68,
            0xf7, 0x07, 0x51, 0x1a,
        ];
        let text = b"1GET/v1/positions";
        let signature = safe_cex_crypto::ed25519::sign(text, &seed).unwrap();
        let key = format!("ed25519:{}", bs58::encode(public).into_string());
        let verify = remote_verifier(&key).unwrap();
        assert!(verify(text, &signature));
        assert!(!verify(b"1GET/v1/orders", &signature));
        assert!(!verify(text, &signature[..63]));
        assert!(!remote_verifier(&credentials().key).unwrap()(
            text, &signature
        ));
        assert!(remote_verifier("ed25519:not-base58-0OIl").is_err());
    }

    #[test]
    fn remote_recipe_refuses_paths_methods_and_fields_outside_its_allowlist() {
        use safe_cex_engine::signing::SignError::Refused;
        use safe_cex_engine::signing_remote::RemoteRecipe;
        let ok = serde_json::json!({ "timestamp": "1", "method": "GET", "path": "/v1/positions" });
        assert!(REST.canonicalize(&ok.to_string()).is_ok());
        for (field, value) in [
            ("path", serde_json::json!("/v1/withdraw")),
            ("path", serde_json::json!("/v1/order/../withdraw/trades")),
            ("path", serde_json::json!("/v1/positions#x")),
            ("path", serde_json::json!("/v1/public/info")),
            ("method", serde_json::json!("PATCH")),
            ("timestamp", serde_json::json!("12a")),
            ("timestamp", serde_json::json!(1)),
            ("body", serde_json::json!({})),
            ("extra", serde_json::json!("x")),
        ] {
            let mut inputs = ok.clone();
            inputs[field] = value;
            assert_eq!(
                REST.canonicalize(&inputs.to_string()),
                Err(Refused),
                "{field}"
            );
        }
        assert_eq!(REMOTE_RECIPES.len(), 1);
        assert_eq!(REMOTE_RECIPES[0].id(), "orderly.rest.1");
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
