//! Tealstreet proxy JWT verification, as `getVerifiedProxyUserId` in NX
//! `services/exchange-server/src/server.ts`.
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

pub(crate) const TOKEN_EXPIRED: &str = "Tealstreet proxy token expired";

pub const PROXY_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAuQe6F/ux1LfEBUmfzqnc
slvMkUU2fr8jk6g1CotUpdCkIPjDzID4tVy+wmin+A9D7XqBicpRaE4utz/+CMmW
+VRBU7xPWqIPjWYuocV5uLlRGl2QPP9rjq0KHSGy1PepeRLaAaYR+zpYQEJL2m4i
DHnoiqdka5A3fn8CPkwc/xaoZyBh2q/qK+s8P+KlnLc4SnuArJ/xRqnZAtAl0EEG
uWoRey8IW4okSSvjqoallzji8i5wBjc0r8eMYTmbPrhWzfASHqnkGew+zEFgKg3S
Cyh5qJX24ue3s7rD2vQd7/aF0+YYIk7LdqJlNScBRoMnwb+fwW9JZX126tIfNbkg
iDSP0wI/qFMTxWQRyVewpzO0xEgoyfI/l4PGSJ76DLzuRpH+jFiVs4bM/mquyS4c
UR4XYvl5bX13jNQoMdJ1K3RB4NjuFgMm2MDwpw1uqZ8/ymYryfv83InsH7TxBPD1
DBqSEapHHsa1KIQnr+g5NpROYU7PAtp4YR7xlwR3A69nX0i8inj6ETXQYHpnHe+b
zJulL1PpiIbHVE9ct7EPtQjEGFTj8NghON8Wm6igo4TmcPS/JNNCj6MDAae0r9u2
J/oXFIM7efD4gC6HoRJa+OkoMsFwoMtJ+hDWXotO3qLejMFR2T2bgqu9g5EHtIle
FBzrmart0nDEAIIY+92qGqUCAwEAAQ==
-----END PUBLIC KEY-----";

/// A validated public chat handle, never an email or an account name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatHandle(String);

impl ChatHandle {
    pub fn parse(value: &str) -> Option<Self> {
        (!value.is_empty()
            && value.len() <= 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
        .then(|| Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A proxy token that verified: its user and its expiry in Unix milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub user_id: String,
    pub chat_handle: Option<ChatHandle>,
    pub expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    user_id: String,
    #[serde(default, deserialize_with = "chat_handle")]
    chat_handle: Option<ChatHandle>,
    exp: u64,
}

fn chat_handle<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ChatHandle>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_str().and_then(ChatHandle::parse))
}

pub struct Verifier {
    key: DecodingKey,
    #[cfg(feature = "test-support")]
    unsigned: bool,
}

impl Verifier {
    /// Recheck the expiry captured from a verified token. Public sockets have
    /// no token; authenticated sockets have no expiry leeway.
    pub(crate) fn check_expiry(
        expires_at_ms: Option<u64>,
        now_ms: u64,
    ) -> Result<(), &'static str> {
        if expires_at_ms.is_some_and(|expires| now_ms >= expires) {
            Err(TOKEN_EXPIRED)
        } else {
            Ok(())
        }
    }

    pub fn new(public_key_pem: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            key: DecodingKey::from_rsa_pem(public_key_pem.as_bytes())?,
            #[cfg(feature = "test-support")]
            unsigned: false,
        })
    }

    /// Loopback test servers: claims are checked as ever, the signature is not,
    /// so tests need no private key ([`unsigned_token`]).
    #[cfg(feature = "test-support")]
    pub fn unsigned() -> Self {
        Self {
            unsigned: true,
            ..Self::new(PROXY_PUBLIC_KEY).expect("the bundled key parses")
        }
    }

    /// RS256 only, `exp` required and unexpired, `userId` a nonempty string.
    pub fn verify(&self, token: &str, now_ms: u64) -> Option<Verified> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 0;
        validation.set_required_spec_claims(&["exp"]);
        #[cfg(feature = "test-support")]
        if self.unsigned {
            validation.insecure_disable_signature_validation();
        }
        let claims = decode::<Claims>(token, &self.key, &validation).ok()?.claims;
        let expires_at_ms = claims.exp.checked_mul(1000)?;
        (!claims.user_id.is_empty() && Self::check_expiry(Some(expires_at_ms), now_ms).is_ok())
            .then_some(Verified {
                user_id: claims.user_id,
                chat_handle: claims.chat_handle,
                expires_at_ms,
            })
    }
}

/// A proxy token for `user` that only [`Verifier::unsigned`] accepts.
#[cfg(feature = "test-support")]
pub fn unsigned_token(user: &str, exp_seconds: u64) -> String {
    use base64::Engine as _;
    let part = |value: serde_json::Value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
    };
    let header = part(serde_json::json!({"alg": "RS256", "typ": "JWT"}));
    let claims = part(serde_json::json!({"userId": user, "exp": exp_seconds}));
    format!("{header}.{claims}.unsigned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use serde_json::{Value, json};

    fn token(handle: Option<Value>) -> (String, u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut claims = json!({"userId": "auth0|opaque-user", "exp": now + 3600,
            "email": "private@example.invalid", "nickname": "not-the-public-handle"});
        if let Some(handle) = handle {
            claims["chatHandle"] = handle;
        }
        let part = |value: Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
        };
        (
            format!(
                "{}.{}.unsigned",
                part(json!({"alg":"RS256","typ":"JWT"})),
                part(claims)
            ),
            now * 1000,
        )
    }

    #[test]
    fn proxy_claim_keeps_only_a_valid_public_chat_handle() {
        for handle in [
            "Trader-063249",
            "Renolo_the_Delusional_Bear",
            "a",
            &"X".repeat(32),
        ] {
            let (token, now) = token(Some(json!(handle)));
            let verified = Verifier::unsigned().verify(&token, now).unwrap();
            assert_eq!(verified.user_id, "auth0|opaque-user");
            assert_eq!(
                verified.chat_handle.as_ref().map(ChatHandle::as_str),
                Some(handle)
            );
            assert!(!format!("{verified:?}").contains("private@example.invalid"));
            assert!(!format!("{verified:?}").contains("not-the-public-handle"));
            assert!(
                Verifier::new(PROXY_PUBLIC_KEY)
                    .unwrap()
                    .verify(&token, now)
                    .is_none(),
                "the handle never bypasses signature verification"
            );
        }
    }

    #[test]
    fn invalid_or_absent_chat_handle_claim_falls_back_without_refusing_the_user() {
        let invalid = [
            None,
            Some(Value::Null),
            Some(json!(42)),
            Some(json!(true)),
            Some(json!([])),
            Some(json!({"username":"Trader-063249"})),
            Some(json!("")),
            Some(json!("X".repeat(33))),
            Some(json!("private@example.invalid")),
            Some(json!("Trader\nforged log")),
            Some(json!("Trader\r\nforged")),
            Some(json!("Trader\tname")),
            Some(json!("Trader\u{001b}[31m")),
            Some(json!("Trader\u{202e}")),
            Some(json!("交易者")),
            Some(json!(" Trader")),
            Some(json!("Trader ")),
            Some(json!("chat=forged")),
            Some(json!("Daxx (web)")),
        ];
        for handle in invalid {
            let (token, now) = token(handle.clone());
            let verified = Verifier::unsigned().verify(&token, now).unwrap();
            assert_eq!(verified.user_id, "auth0|opaque-user");
            assert_eq!(verified.chat_handle, None, "{handle:?}");
        }
    }

    #[test]
    fn token_expiry_uses_the_verified_exp_without_leeway() {
        let (token, now) = token(None);
        let verifier = Verifier::unsigned();
        let expires = verifier.verify(&token, now).unwrap().expires_at_ms;
        assert!(verifier.verify(&token, expires - 1).is_some());
        assert!(verifier.verify(&token, expires).is_none());
        assert!(verifier.verify(&token, expires + 1).is_none());
    }
}
