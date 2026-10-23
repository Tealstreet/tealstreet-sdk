use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use safe_cex_crypto::es256::Es256SigningKey;
use safe_cex_engine::Method;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

const MAX_CREDENTIAL_BYTES: usize = 16_384;
const MAX_URI_BYTES: usize = 4_096;
const KEY_ERROR: &str = "Coinbase API key must be organizations/{org_id}/apiKeys/{key_id}";
const SECRET_ERROR: &str =
    "Coinbase secret must be a P-256 private-key PEM or base64 Ed25519 seed/keypair";

pub struct Credentials {
    key: String,
    signer: Signer,
}

enum Signer {
    Es256(Es256SigningKey),
    Ed25519(Zeroizing<[u8; 32]>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CredentialField {
    Text(String),
    Other(serde::de::IgnoredAny),
}

impl CredentialField {
    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Other(_) => None,
        }
    }
}

impl Drop for CredentialField {
    fn drop(&mut self) {
        if let Self::Text(value) = self {
            value.zeroize();
        }
    }
}

#[derive(Default, Deserialize)]
struct DownloadedKey {
    name: Option<CredentialField>,
    #[serde(rename = "privateKey")]
    private_key_camel: Option<CredentialField>,
    private_key: Option<CredentialField>,
    #[serde(rename = "keySecret")]
    key_secret_camel: Option<CredentialField>,
    key_secret: Option<CredentialField>,
    secret: Option<CredentialField>,
}

impl DownloadedKey {
    fn parse(value: &str) -> Option<Self> {
        value
            .trim()
            .starts_with('{')
            .then(|| serde_json::from_str(value).ok())
            .flatten()
    }

    fn secret(&self) -> Option<&str> {
        [
            &self.private_key_camel,
            &self.private_key,
            &self.key_secret_camel,
            &self.key_secret,
            &self.secret,
        ]
        .into_iter()
        .find_map(|field| field.as_ref().and_then(CredentialField::text))
    }
}

#[derive(Serialize)]
struct Header<'a> {
    alg: &'static str,
    typ: &'static str,
    kid: &'a str,
    nonce: &'a str,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    aud: Option<[&'static str; 1]>,
    nbf: u64,
    exp: u64,
    sub: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<&'a str>,
}

impl Credentials {
    pub fn new(key: &str, secret: &str) -> Result<Self, String> {
        if key.len() > MAX_CREDENTIAL_BYTES || secret.len() > MAX_CREDENTIAL_BYTES {
            return Err("Coinbase credentials exceed the supported size".into());
        }
        let key_json = DownloadedKey::parse(key);
        let secret_json = DownloadedKey::parse(secret);
        let key = key_json
            .as_ref()
            .and_then(|json| json.name.as_ref())
            .and_then(CredentialField::text)
            .unwrap_or(key)
            .trim();
        if !valid_key_name(key) {
            return Err(KEY_ERROR.into());
        }
        let secret = secret_json
            .as_ref()
            .and_then(DownloadedKey::secret)
            .or_else(|| key_json.as_ref().and_then(DownloadedKey::secret))
            .unwrap_or(secret);
        let secret = Zeroizing::new(secret.replace("\\n", "\n"));
        let secret = secret.trim();
        let signer = if secret.starts_with("-----BEGIN ") {
            Signer::Es256(import_pem(secret)?)
        } else {
            let secret = secret
                .get(..8)
                .filter(|prefix| prefix.eq_ignore_ascii_case("ed25519:"))
                .map_or(secret, |_| &secret[8..]);
            let decoded = decode_base64(secret)?;
            if !matches!(decoded.len(), 32 | 64) {
                return Err(SECRET_ERROR.into());
            }
            // CDP keypairs are seed || public key; TS and the official SDK use the seed.
            let mut seed = Zeroizing::new([0; 32]);
            seed.copy_from_slice(&decoded[..32]);
            Signer::Ed25519(seed)
        };
        Ok(Self {
            key: key.into(),
            signer,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn jwt(
        &self,
        now: f64,
        uri: Option<&str>,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<String, String> {
        self.jwt_with_nonce(now, uri, &safe_cex_crypto::uuid::uuid_with_entropy(fill)?)
    }

    fn jwt_with_nonce(&self, now: f64, uri: Option<&str>, nonce: &str) -> Result<String, String> {
        let seconds = (now / 1_000.0).floor();
        if !now.is_finite() || now < 0.0 || seconds >= (u64::MAX - 120) as f64 {
            return Err("Coinbase invalid clock".into());
        }
        let kind = match self.signer {
            Signer::Es256(_) => JwtKind::Es256,
            Signer::Ed25519(_) => JwtKind::EdDsa,
        };
        let input = signing_input(kind, &self.key, nonce, seconds as u64, uri)?;
        let signature = match &self.signer {
            Signer::Es256(key) => key.sign(input.as_bytes())?,
            Signer::Ed25519(seed) => {
                safe_cex_crypto::ed25519::sign(input.as_bytes(), seed.as_ref())?
            }
        };
        Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
    }

    pub fn headers(
        &self,
        method: Method,
        path: &str,
        now: f64,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<Vec<(&'static str, String)>, String> {
        if !path.starts_with('/')
            || path.starts_with("//")
            || path.len() > MAX_URI_BYTES
            || path.chars().any(char::is_control)
        {
            return Err("Coinbase invalid request path".into());
        }
        let pathname = path.split(['?', '#']).next().unwrap_or(path);
        let uri = format!("{} api.coinbase.com{pathname}", method.as_str());
        Ok(vec![
            (
                "Authorization",
                format!("Bearer {}", self.jwt(now, Some(&uri), fill)?),
            ),
            ("Content-Type", "application/json".into()),
        ])
    }
}

#[derive(Clone, Copy)]
enum JwtKind {
    Es256,
    EdDsa,
}

/// `base64url(header).base64url(claims)`: the exact JWS signing input the local
/// signer and the remote JWT recipes share.
fn signing_input(
    kind: JwtKind,
    kid: &str,
    nonce: &str,
    nbf: u64,
    uri: Option<&str>,
) -> Result<String, String> {
    if uri.is_some_and(|value| {
        value.is_empty() || value.len() > MAX_URI_BYTES || value.chars().any(char::is_control)
    }) {
        return Err("Coinbase invalid JWT URI".into());
    }
    if nonce.is_empty() || nonce.len() > 128 || nonce.chars().any(char::is_control) {
        return Err("Coinbase invalid JWT nonce".into());
    }
    let exp = nbf.checked_add(120).ok_or("Coinbase invalid clock")?;
    // Keep the active TS claim contract; current Coinbase examples disagree.
    let (alg, iss, aud) = match kind {
        JwtKind::Es256 => ("ES256", "coinbase-cloud", None),
        JwtKind::EdDsa => ("EdDSA", "cdp", Some(["cdp_service"])),
    };
    let header = serde_json::to_vec(&Header {
        alg,
        typ: "JWT",
        kid,
        nonce,
    })
    .map_err(|_| "Coinbase JWT header encoding failed")?;
    let claims = serde_json::to_vec(&Claims {
        iss,
        aud,
        nbf,
        exp,
        sub: kid,
        uri,
    })
    .map_err(|_| "Coinbase JWT claims encoding failed")?;
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header),
        URL_SAFE_NO_PAD.encode(claims)
    ))
}

fn valid_key_name(key: &str) -> bool {
    let mut parts = key.split('/');
    parts.next() == Some("organizations")
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next() == Some("apiKeys")
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none()
        && !key.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Brokerage routes this venue signs, relative to `/api/v3/brokerage/`;
/// `*` is one UUID segment, so a named route such as `portfolios/move_funds`
/// never matches it. Fund movement and every other route are refused.
const SIGNED_ROUTES: &[&str] = &[
    "key_permissions",
    "portfolios",
    "portfolios/*",
    "cfm/balance_summary",
    "cfm/positions",
    "intx/portfolio/*",
    "intx/positions/*",
    "orders",
    "orders/edit",
    "orders/batch_cancel",
    "orders/historical/*",
    "orders/historical/batch",
    "orders/historical/fills",
];

/// `METHOD api.coinbase.com/api/v3/brokerage/<route>`, pathname only.
fn signed_uri(uri: &str) -> bool {
    let Some((method, target)) = uri.split_once(' ') else {
        return false;
    };
    let Some(route) = target.strip_prefix("api.coinbase.com/api/v3/brokerage/") else {
        return false;
    };
    let id = |segment: &str| {
        let groups: Vec<_> = segment.split('-').collect();
        groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12])
            && groups
                .iter()
                .all(|group| group.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    matches!(method, "GET" | "POST" | "PUT" | "DELETE")
        && SIGNED_ROUTES.iter().any(|pattern| {
            let mut wanted = pattern.split('/');
            let mut given = route.split('/');
            loop {
                match (wanted.next(), given.next()) {
                    (None, None) => return true,
                    (Some("*"), Some(segment)) if id(segment) => {}
                    (Some(a), Some(b)) if a == b => {}
                    _ => return false,
                }
            }
        })
}

/// `coinbase.restJwt*.1`: the JWS signing input from non-secret claims. The
/// header nonce comes from engine signing entropy; `exp - nbf` is fixed at 120 s.
pub struct RestJwt(JwtKind);
impl safe_cex_engine::signing_remote::RemoteRecipe for RestJwt {
    fn id(&self) -> &'static str {
        match self.0 {
            JwtKind::Es256 => "coinbase.restJwtEs256.1",
            JwtKind::EdDsa => "coinbase.restJwtEd25519.1",
        }
    }
    fn algorithm(&self) -> safe_cex_engine::signing::Algorithm {
        match self.0 {
            JwtKind::Es256 => safe_cex_engine::signing::Algorithm::Es256,
            JwtKind::EdDsa => safe_cex_engine::signing::Algorithm::Ed25519,
        }
    }
    fn role(&self) -> Option<safe_cex_engine::signing::ExtraKey> {
        None
    }
    fn bound_credential_field(&self) -> Option<&'static str> {
        Some("kid")
    }
    fn canonicalize(
        &self,
        inputs: &serde_json::Value,
    ) -> Result<Vec<u8>, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError::Refused;
        let object = inputs.as_object().ok_or(Refused)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "kid" | "nonce" | "nbf" | "uri"))
        {
            return Err(Refused);
        }
        let kid = object
            .get("kid")
            .and_then(serde_json::Value::as_str)
            .ok_or(Refused)?;
        let nonce = object
            .get("nonce")
            .and_then(serde_json::Value::as_str)
            .ok_or(Refused)?;
        let nbf = object
            .get("nbf")
            .and_then(serde_json::Value::as_u64)
            .ok_or(Refused)?;
        let uri = match object.get("uri") {
            None => None,
            Some(uri) => Some(uri.as_str().filter(|uri| signed_uri(uri)).ok_or(Refused)?),
        };
        if !valid_key_name(kid) || nbf > u64::MAX / 2 {
            return Err(Refused);
        }
        signing_input(self.0, kid, nonce, nbf, uri)
            .map(String::into_bytes)
            .map_err(|_| Refused)
    }
    fn stamp_ms(
        &self,
        inputs: &serde_json::Value,
    ) -> Result<f64, safe_cex_engine::signing::SignError> {
        inputs
            .get("nbf")
            .and_then(serde_json::Value::as_u64)
            .filter(|nbf| *nbf <= u64::MAX / 2000)
            .map(|nbf| (nbf * 1000) as f64)
            .ok_or(safe_cex_engine::signing::SignError::Refused)
    }
}
pub static REST_JWT_ES256: RestJwt = RestJwt(JwtKind::Es256);
pub static REST_JWT_ED25519: RestJwt = RestJwt(JwtKind::EdDsa);
pub static REMOTE_RECIPES: &[&dyn safe_cex_engine::signing_remote::RemoteRecipe] =
    &[&REST_JWT_ES256, &REST_JWT_ED25519];

fn decode_base64(value: &str) -> Result<Zeroizing<Vec<u8>>, String> {
    let normalized = Zeroizing::new(
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '+' => '-',
                '/' => '_',
                _ => c,
            })
            .collect::<String>(),
    );
    let decoded = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    )
    .decode(normalized.as_bytes())
    .map_err(|_| SECRET_ERROR)?;
    Ok(Zeroizing::new(decoded))
}

fn import_pem(pem: &str) -> Result<Es256SigningKey, String> {
    let (begin, end, sec1) = if pem.starts_with("-----BEGIN EC PRIVATE KEY-----") {
        (
            "-----BEGIN EC PRIVATE KEY-----",
            "-----END EC PRIVATE KEY-----",
            true,
        )
    } else {
        (
            "-----BEGIN PRIVATE KEY-----",
            "-----END PRIVATE KEY-----",
            false,
        )
    };
    let body = pem
        .strip_prefix(begin)
        .and_then(|value| value.strip_suffix(end))
        .ok_or(SECRET_ERROR)?;
    let der = decode_base64(body)?;
    if !sec1 {
        return Es256SigningKey::from_pkcs8_der(&der).map_err(|_| SECRET_ERROR.into());
    }
    // PKCS8 PrivateKeyInfo: version 0, ecPublicKey/P-256 OIDs, SEC1 octet string.
    let mut inner = Zeroizing::new(vec![
        0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06,
        0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
    ]);
    inner.extend_from_slice(&der_wrap(0x04, &der));
    Es256SigningKey::from_pkcs8_der(&der_wrap(0x30, &inner)).map_err(|_| SECRET_ERROR.into())
}

fn der_wrap(tag: u8, bytes: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(vec![tag]);
    match bytes.len() {
        length if length < 128 => out.push(length as u8),
        length if length < 256 => out.extend_from_slice(&[0x81, length as u8]),
        length => out.extend_from_slice(&[0x82, (length >> 8) as u8, length as u8]),
    }
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::{
        ecdsa::{Signature, SigningKey, VerifyingKey, signature::Verifier},
        pkcs8::EncodePrivateKey,
    };

    const KEY: &str = "organizations/org-id/apiKeys/key-id";

    #[derive(Deserialize)]
    struct ReadHeader {
        alg: String,
        typ: String,
        kid: String,
        nonce: String,
    }
    #[derive(Deserialize)]
    struct ReadClaims {
        iss: String,
        aud: Option<Vec<String>>,
        nbf: u64,
        exp: u64,
        sub: String,
        uri: Option<String>,
    }

    fn decode(token: &str) -> (ReadHeader, ReadClaims, Vec<u8>, String) {
        let parts: Vec<_> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|part| !part.contains(['=', '+', '/'])));
        let header = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        let claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        (
            header,
            claims,
            URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
            format!("{}.{}", parts[0], parts[1]),
        )
    }

    fn ec_credentials(sec1: bool) -> (Credentials, SigningKey) {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let (label, der) = if sec1 {
            let secret = p256::SecretKey::from_slice(&[7; 32]).unwrap();
            ("EC PRIVATE KEY", secret.to_sec1_der().unwrap().to_vec())
        } else {
            (
                "PRIVATE KEY",
                p256::SecretKey::from_slice(&key.to_bytes())
                    .unwrap()
                    .to_pkcs8_der()
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            )
        };
        let pem = format!(
            "-----BEGIN {label}-----\n{}\n-----END {label}-----",
            base64::engine::general_purpose::STANDARD.encode(der)
        );
        (Credentials::new(KEY, &pem).unwrap(), key)
    }

    #[test]
    fn rest_jwt_recipes_rebuild_the_exact_local_signing_input() {
        use safe_cex_engine::signing_remote::RemoteRecipe;
        // RFC 8032 test 1 seed: a published vector, not account material.
        let rfc8032 = base64::engine::general_purpose::STANDARD.encode([
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ]);
        let uri = "POST api.coinbase.com/api/v3/brokerage/orders";
        for (credentials, recipe) in [
            (ec_credentials(false).0, &REST_JWT_ES256),
            (Credentials::new(KEY, &rfc8032).unwrap(), &REST_JWT_ED25519),
        ] {
            for uri in [Some(uri), None] {
                let token = credentials.jwt_with_nonce(1_234_567.9, uri, "n-1").unwrap();
                let (_, _, _, input) = decode(&token);
                let mut inputs = serde_json::json!({ "kid": KEY, "nonce": "n-1", "nbf": 1_234 });
                if let Some(uri) = uri {
                    inputs["uri"] = uri.into();
                }
                assert_eq!(recipe.canonicalize(&inputs).unwrap(), input.into_bytes());
            }
        }
        assert_eq!(
            REMOTE_RECIPES.iter().map(|r| r.id()).collect::<Vec<_>>(),
            ["coinbase.restJwtEs256.1", "coinbase.restJwtEd25519.1"]
        );
    }

    #[test]
    fn rest_jwt_recipes_refuse_routes_keys_and_fields_outside_the_allowlist() {
        use safe_cex_engine::signing::SignError::Refused;
        use safe_cex_engine::signing_remote::RemoteRecipe;
        let ok = serde_json::json!({
            "kid": KEY, "nonce": "n", "nbf": 1,
            "uri": "GET api.coinbase.com/api/v3/brokerage/orders/historical/0b9e2c4a-1f3d-4c5e-8a7b-6d5c4b3a2f10"
        });
        assert!(REST_JWT_ES256.canonicalize(&ok).is_ok());
        for (field, value) in [
            (
                "uri",
                serde_json::json!("POST api.coinbase.com/api/v3/brokerage/portfolios/move_funds"),
            ),
            (
                "uri",
                serde_json::json!("POST api.coinbase.com/v2/accounts/x/transactions"),
            ),
            (
                "uri",
                serde_json::json!("GET evil.example/api/v3/brokerage/orders"),
            ),
            (
                "uri",
                serde_json::json!("GET api.coinbase.com/api/v3/brokerage/orders?x=1"),
            ),
            (
                "uri",
                serde_json::json!("GET api.coinbase.com/api/v3/brokerage/orders/historical/../x"),
            ),
            (
                "uri",
                serde_json::json!("PATCH api.coinbase.com/api/v3/brokerage/orders"),
            ),
            (
                "uri",
                serde_json::json!("GET api.coinbase.com/api/v3/brokerage/orders/historical/abc-1"),
            ),
            ("kid", serde_json::json!("organizations/o/apiKeys/k/extra")),
            ("nonce", serde_json::json!("")),
            ("nbf", serde_json::json!("1")),
            ("exp", serde_json::json!(999_999)),
        ] {
            let mut inputs = ok.clone();
            inputs[field] = value;
            assert_eq!(
                REST_JWT_ES256.canonicalize(&inputs),
                Err(Refused),
                "{field}"
            );
        }
    }

    #[test]
    fn both_p256_formats_sign_jws_raw_sha256_and_mutations_fail() {
        for sec1 in [true, false] {
            let (credentials, signing) = ec_credentials(sec1);
            let token = credentials
                .jwt_with_nonce(
                    1_234_567.9,
                    Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
                    "test-nonce",
                )
                .unwrap();
            let (header, claims, signature, input) = decode(&token);
            assert_eq!(
                (
                    header.alg.as_str(),
                    header.typ.as_str(),
                    header.kid.as_str(),
                    header.nonce.as_str()
                ),
                ("ES256", "JWT", KEY, "test-nonce")
            );
            assert_eq!(
                (
                    claims.iss.as_str(),
                    claims.aud,
                    claims.nbf,
                    claims.exp,
                    claims.sub.as_str()
                ),
                ("coinbase-cloud", None, 1_234, 1_354, KEY)
            );
            assert_eq!(signature.len(), 64);
            let verifier = VerifyingKey::from(&signing);
            let signature = Signature::from_slice(&signature).unwrap();
            assert!(verifier.verify(input.as_bytes(), &signature).is_ok());
            assert!(
                verifier
                    .verify(format!("{input}x").as_bytes(), &signature)
                    .is_err()
            );
            let (encoded_header, encoded_claims) = input.split_once('.').unwrap();
            let header_text =
                String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_header).unwrap()).unwrap();
            let claims_text =
                String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_claims).unwrap()).unwrap();
            for (before, after) in [
                ("test-nonce", "other-nonce"),
                ("key-id", "wrong-id"),
                ("ES256", "EdDSA"),
            ] {
                let changed = format!(
                    "{}.{encoded_claims}",
                    URL_SAFE_NO_PAD.encode(header_text.replace(before, after))
                );
                assert!(verifier.verify(changed.as_bytes(), &signature).is_err());
            }
            for (before, after) in [
                ("1234", "1235"),
                ("1354", "1355"),
                ("GET api", "POST api"),
                ("/accounts", "/orders"),
                ("coinbase-cloud", "cdp"),
            ] {
                let changed = format!(
                    "{encoded_header}.{}",
                    URL_SAFE_NO_PAD.encode(claims_text.replace(before, after))
                );
                assert!(verifier.verify(changed.as_bytes(), &signature).is_err());
            }
            let changed = SigningKey::from_slice(&[8; 32]).unwrap();
            assert!(
                VerifyingKey::from(&changed)
                    .verify(input.as_bytes(), &signature)
                    .is_err()
            );
        }
    }

    #[test]
    fn ed25519_seed_keypair_prefix_url_base64_and_signature_verification() {
        // RFC 8032 test 1: public key for this independently published seed.
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
        let mut pair = seed.to_vec();
        pair.extend_from_slice(&public);
        for secret in [
            base64::engine::general_purpose::STANDARD.encode(seed),
            URL_SAFE_NO_PAD.encode(&pair),
            format!("Ed25519: {}\n", URL_SAFE_NO_PAD.encode(seed)),
        ] {
            let credentials = Credentials::new(KEY, &secret).unwrap();
            let (header, claims, signature, input) =
                decode(&credentials.jwt_with_nonce(0.0, None, "nonce").unwrap());
            assert_eq!(header.alg, "EdDSA");
            assert_eq!(
                (
                    claims.iss.as_str(),
                    claims.aud,
                    claims.nbf,
                    claims.exp,
                    claims.uri
                ),
                ("cdp", Some(vec!["cdp_service".into()]), 0, 120, None)
            );
            assert!(
                safe_cex_crypto::ed25519::verify(input.as_bytes(), &public, &signature).unwrap()
            );
            assert!(
                !safe_cex_crypto::ed25519::verify(
                    format!("{input}x").as_bytes(),
                    &public,
                    &signature
                )
                .unwrap()
            );
        }
    }

    #[test]
    fn bounded_downloaded_json_normalizes_aliases_escaped_pem_and_precedence() {
        let secret = URL_SAFE_NO_PAD.encode([7; 32]);
        for field in [
            "privateKey",
            "private_key",
            "keySecret",
            "key_secret",
            "secret",
        ] {
            let json = format!("{{\"name\":\"{KEY}\",\"{field}\":\"{secret}\"}}");
            assert_eq!(Credentials::new(&json, "").unwrap().key(), KEY);
            assert_eq!(Credentials::new(KEY, &json).unwrap().key(), KEY);
        }
        let secret_json = format!("{{\"privateKey\":false,\"private_key\":\"{secret}\"}}");
        let key_json = format!("{{\"name\":\"{KEY}\",\"secret\":\"bad\"}}");
        assert!(Credentials::new(&key_json, &secret_json).is_ok());
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let body = base64::engine::general_purpose::STANDARD.encode(
            p256::SecretKey::from_slice(&key.to_bytes())
                .unwrap()
                .to_pkcs8_der()
                .unwrap()
                .as_bytes(),
        );
        let pem = format!("-----BEGIN PRIVATE KEY-----\\n{body}\\n-----END PRIVATE KEY-----");
        assert!(Credentials::new(KEY, &pem).is_ok());
        assert!(Credentials::new(KEY, &"a".repeat(MAX_CREDENTIAL_BYTES + 1)).is_err());
    }

    #[test]
    fn rest_uri_uses_real_host_and_path_only_and_tokens_are_fresh() {
        let credentials = Credentials::new(KEY, &URL_SAFE_NO_PAD.encode([7; 32])).unwrap();
        let mut counter = 0u8;
        let mut fill = |bytes: &mut [u8]| {
            counter += 1;
            bytes.fill(counter);
            Ok(())
        };
        let headers = credentials
            .headers(
                Method::Get,
                "/api/v3/brokerage/orders/historical/batch?cursor=abc#ignored",
                1_000.0,
                &mut fill,
            )
            .unwrap();
        let token = headers
            .iter()
            .find(|(name, _)| *name == "Authorization")
            .unwrap()
            .1
            .strip_prefix("Bearer ")
            .unwrap();
        let (header, claims, _, _) = decode(token);
        assert_eq!(
            claims.uri.as_deref(),
            Some("GET api.coinbase.com/api/v3/brokerage/orders/historical/batch")
        );
        let (second, ws_claims, _, _) = decode(&credentials.jwt(1_000.0, None, &mut fill).unwrap());
        assert_ne!(header.nonce, second.nonce);
        assert!(ws_claims.uri.is_none());
        assert_eq!(
            headers.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["Authorization", "Content-Type"]
        );
    }

    #[test]
    fn invalid_keys_paths_clocks_and_secrets_fail_without_secret_echo() {
        let secret = URL_SAFE_NO_PAD.encode([7; 32]);
        for key in [
            "",
            "api-key",
            "organizations//apiKeys/k",
            "organizations/o/apiKeys/",
            "organizations/o/apiKeys/k/extra",
            "organizations/o/apiKeys/k\nheader",
        ] {
            assert!(Credentials::new(key, &secret).is_err());
        }
        for secret in [
            "sensitive-secret",
            "-----BEGIN PRIVATE KEY-----AAAA-----END EC PRIVATE KEY-----",
            "-----BEGIN PRIVATE KEY-----AAAA-----END PRIVATE KEY-----",
        ] {
            let error = Credentials::new(KEY, secret).err().unwrap();
            assert_eq!(error, SECRET_ERROR);
            assert!(!error.contains(secret));
        }
        for length in [0, 1, 31, 33, 63, 65] {
            assert!(Credentials::new(KEY, &URL_SAFE_NO_PAD.encode(vec![1; length])).is_err());
        }
        let credentials = Credentials::new(KEY, &secret).unwrap();
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, f64::MAX] {
            assert!(
                credentials
                    .jwt(now, None, |bytes| {
                        bytes.fill(1);
                        Ok(())
                    })
                    .is_err()
            );
        }
        for path in [
            "",
            "https://api.coinbase.com/path",
            "//other-host/path",
            "/path\nheader",
        ] {
            assert!(
                credentials
                    .headers(Method::Get, path, 0.0, |bytes| {
                        bytes.fill(1);
                        Ok(())
                    })
                    .is_err()
            );
        }
        for nonce in ["", "bad\nnonce"] {
            assert!(credentials.jwt_with_nonce(0.0, None, nonce).is_err());
        }
        assert!(
            credentials
                .jwt(0.0, Some(""), |bytes| {
                    bytes.fill(1);
                    Ok(())
                })
                .is_err()
        );
        assert!(
            credentials
                .jwt(0.0, Some("GET api\nheader"), |bytes| {
                    bytes.fill(1);
                    Ok(())
                })
                .is_err()
        );
    }
}
