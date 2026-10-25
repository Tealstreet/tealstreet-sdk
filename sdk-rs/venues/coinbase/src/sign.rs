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
        let mut parts = key.split('/');
        if parts.next() != Some("organizations")
            || !parts.next().is_some_and(|part| !part.is_empty())
            || parts.next() != Some("apiKeys")
            || !parts.next().is_some_and(|part| !part.is_empty())
            || parts.next().is_some()
            || key.chars().any(char::is_whitespace)
            || key.chars().any(char::is_control)
        {
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
        let algorithm = match self.signer {
            Signer::Es256(_) => JwtAlgorithm::Es256,
            Signer::Ed25519(_) => JwtAlgorithm::EdDsa,
        };
        let input = signing_input(algorithm, &self.key, nonce, seconds as u64, uri)?;
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
        if !valid_request_path(path) {
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

/// JWT signature algorithm, fixed by the key's type at setup (non-secret).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JwtAlgorithm {
    Es256,
    EdDsa,
}

/// `base64url(header).base64url(claims)`: the exact JWS signing input every
/// Coinbase signer (legacy, dispatch recipe, remote recipe) signs.
fn signing_input(
    algorithm: JwtAlgorithm,
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
    let (alg, iss, aud) = match algorithm {
        JwtAlgorithm::Es256 => ("ES256", "coinbase-cloud", None),
        JwtAlgorithm::EdDsa => ("EdDSA", "cdp", Some(["cdp_service"])),
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

const REST_ORIGIN: &str = "https://api.coinbase.com";

/// Signed request path rules shared with the legacy signer: rooted, not
/// protocol-relative, bounded and control-free.
fn valid_request_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= MAX_URI_BYTES
        && !path.chars().any(char::is_control)
}

/// One JWT at release: the nonce comes from engine signing entropy and `nbf`
/// from the dispatch stamp; `exp` is fixed at `nbf + 120` and declared as the
/// receipt expiry so a late result is never released under the old token.
fn prepare_jwt(
    dispatch: &safe_cex_engine::DispatchContext<'_>,
    identity: &safe_cex_engine::signing::Identity,
    key_name: &str,
    algorithm: JwtAlgorithm,
    uri: Option<&str>,
    context: safe_cex_engine::signing::Context,
) -> Result<(safe_cex_engine::signing::SignRequest, String), safe_cex_engine::signing::SignError> {
    use safe_cex_engine::signing::{
        Algorithm, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    };
    let stamp = dispatch.stamp_ms();
    let nbf = (stamp / 1_000.0).floor();
    if !nbf.is_finite() || nbf < 0.0 || nbf >= (u64::MAX / 2000) as f64 {
        return Err(SignError::InvalidRequest);
    }
    let nonce = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
        dispatch
            .signing_bytes(bytes)
            .map_err(|_| "Coinbase signing entropy unavailable".to_owned())
    })
    .map_err(|_| SignError::Unavailable)?;
    let nbf = nbf as u64;
    let input = signing_input(algorithm, key_name, &nonce, nbf, uri)
        .map_err(|_| SignError::InvalidRequest)?;
    let request = SignRequest {
        receipt: SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp),
            expiry_ms: Some(((nbf + 120) * 1000) as f64),
        },
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                algorithm: match algorithm {
                    JwtAlgorithm::Es256 => Algorithm::Es256,
                    JwtAlgorithm::EdDsa => Algorithm::Ed25519,
                },
                key: identity.signing.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: input.clone().into_bytes(),
                }],
            },
        ],
        context,
    };
    Ok((request, input))
}

/// The presented credential must be this recipe's key name, so a provider
/// presenting another key cannot sign for it; the signature is raw r||s or
/// Ed25519, 64 bytes either way.
fn finish_jwt(
    key_name: &str,
    input: &str,
    result: &safe_cex_engine::signing::SignResult,
) -> Result<String, safe_cex_engine::signing::SignError> {
    use safe_cex_engine::signing::SignError;
    if result.values.first().map(Vec::as_slice) != Some(key_name.as_bytes()) {
        return Err(SignError::InvalidResult);
    }
    let signature = result
        .values
        .get(1)
        .filter(|signature| signature.len() == 64)
        .ok_or(SignError::InvalidResult)?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

/// Dispatch recipe for one private Coinbase REST request. `path` includes the
/// query; the outbound URL must be exactly `https://api.coinbase.com` + path.
/// The `uri` claim keeps only the pathname, as legacy does.
pub struct Http {
    identity: safe_cex_engine::signing::Identity,
    key_name: String,
    algorithm: JwtAlgorithm,
    method: Method,
    path: String,
    prepared: Option<String>,
}
impl Http {
    pub fn new(
        identity: safe_cex_engine::signing::Identity,
        key_name: &str,
        algorithm: JwtAlgorithm,
        method: Method,
        path: &str,
    ) -> Self {
        Self {
            identity,
            key_name: key_name.into(),
            algorithm,
            method,
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
        use safe_cex_engine::signing::SignError;
        let safe_cex_engine::Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if http.method != self.method
            || !valid_request_path(&self.path)
            || http.url.strip_prefix(REST_ORIGIN) != Some(self.path.as_str())
        {
            return Err(SignError::InvalidRequest);
        }
        let pathname = self.path.split(['?', '#']).next().unwrap_or(&self.path);
        let uri = format!("{} api.coinbase.com{pathname}", self.method.as_str());
        let (request, input) = prepare_jwt(
            dispatch,
            &self.identity,
            &self.key_name,
            self.algorithm,
            Some(&uri),
            safe_cex_engine::signing::Context {
                venue: "coinbase".into(),
                method: self.method.as_str().into(),
                host: "api.coinbase.com".into(),
                path: pathname.into(),
            },
        )?;
        self.prepared = Some(input);
        Ok(request)
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
        let input = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let jwt = finish_jwt(&self.key_name, &input, result)?;
        http.headers = vec![
            ("Authorization", format!("Bearer {jwt}")),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// Dispatch recipe for one private-socket subscribe frame (`user` or
/// `heartbeats`), with a JWT that carries no `uri` claim.
pub struct WsSubscribe {
    identity: safe_cex_engine::signing::Identity,
    key_name: String,
    algorithm: JwtAlgorithm,
    channel: String,
    prepared: Option<String>,
}
impl WsSubscribe {
    pub fn new(
        identity: safe_cex_engine::signing::Identity,
        key_name: &str,
        algorithm: JwtAlgorithm,
        channel: &str,
    ) -> Self {
        Self {
            identity,
            key_name: key_name.into(),
            algorithm,
            channel: channel.into(),
            prepared: None,
        }
    }
}
impl safe_cex_engine::SignStep for WsSubscribe {
    fn prepare(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        dispatch: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError;
        if !matches!(outbound, safe_cex_engine::Action::WsSend { .. })
            || !matches!(self.channel.as_str(), "user" | "heartbeats")
        {
            return Err(SignError::InvalidRequest);
        }
        let (request, input) = prepare_jwt(
            dispatch,
            &self.identity,
            &self.key_name,
            self.algorithm,
            None,
            safe_cex_engine::signing::Context {
                venue: "coinbase".into(),
                method: "WS".into(),
                host: "advanced-trade-ws-user.coinbase.com".into(),
                path: self.channel.clone(),
            },
        )?;
        self.prepared = Some(input);
        Ok(request)
    }
    fn apply(
        &mut self,
        outbound: &mut safe_cex_engine::Action,
        result: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::SignError;
        let safe_cex_engine::Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let input = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let jwt = finish_jwt(&self.key_name, &input, result)?;
        *text = subscribe_frame(&self.channel, jwt).map_err(|_| SignError::InvalidRequest)?;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

/// The private-socket subscribe frame, in the legacy field order.
pub fn subscribe_frame(channel: &str, jwt: String) -> Result<String, String> {
    #[derive(Serialize)]
    struct Sub<'a> {
        #[serde(rename = "type")]
        kind: &'static str,
        channel: &'a str,
        jwt: String,
    }
    serde_json::to_string(&Sub {
        kind: "subscribe",
        channel,
        jwt,
    })
    .map_err(|_| "Coinbase subscribe encoding failed".to_owned())
}

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
