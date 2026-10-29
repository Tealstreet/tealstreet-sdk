//! Tealstreet proxy JWT verification, as `getVerifiedProxyUserId` in NX
//! `services/exchange-server/src/server.ts`.
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

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

/// A proxy token that verified: its user and its expiry in Unix milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub user_id: String,
    pub expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claims {
    user_id: String,
    exp: u64,
}

pub struct Verifier {
    key: DecodingKey,
}

impl Verifier {
    pub fn new(public_key_pem: &str) -> Result<Self, jsonwebtoken::errors::Error> {
        Ok(Self {
            key: DecodingKey::from_rsa_pem(public_key_pem.as_bytes())?,
        })
    }

    /// RS256 only, `exp` required and unexpired, `userId` a nonempty string.
    pub fn verify(&self, token: &str, now_ms: u64) -> Option<Verified> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 0;
        validation.set_required_spec_claims(&["exp"]);
        let claims = decode::<Claims>(token, &self.key, &validation).ok()?.claims;
        let expires_at_ms = claims.exp.checked_mul(1000)?;
        (!claims.user_id.is_empty() && expires_at_ms > now_ms).then_some(Verified {
            user_id: claims.user_id,
            expires_at_ms,
        })
    }
}
