use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use safe_cex_engine::Method;
use zeroize::Zeroize;

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
        let seed = bs58::decode(secret)
            .into_vec()
            .map_err(|_| "Orderly secret must be base58")?
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
        let timestamp = (now as u64).to_string();
        let text = format!("{timestamp}{}{path}{}", method.as_str(), body.unwrap_or(""));
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

impl Drop for Credentials {
    fn drop(&mut self) {
        self.seed.zeroize();
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
