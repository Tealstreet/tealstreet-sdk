//! Normal local provider. Construction consumes credentials before venue setup.
//! This module alone owns key material; engine/venue state receives opaque refs.
use hmac::{Hmac, Mac};
use safe_cex_engine::signing::{
    Algorithm, Encoding, KeyRef, Operation, PayloadPart, SignError, SignFuture, SignRequest,
    SignResult, Signer,
};
use sha2::{Digest, Sha256, Sha512};
use std::collections::HashMap;
use zeroize::Zeroizing;
mod key_material;
pub use key_material::KeyEncoding;

/// Trusted provider setup only. Never place these values in venue/engine options.
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSetup {
    pub primary: Option<LocalIdentity>,
    pub secondary: Option<LocalIdentity>,
}
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalIdentity {
    #[serde(default)]
    pub key: Secret,
    #[serde(default)]
    pub secret: Secret,
    pub passphrase: Option<Secret>,
    /// Explicit caller account/credential mapping; no key-derived fallback.
    pub stable_id: Option<String>,
    #[serde(default)]
    pub secret_encoding: KeyEncoding,
    #[serde(default)]
    pub trim_credential: bool,
}
#[derive(Default)]
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut *self.0).into_bytes()
    }
}
impl<'de> serde::Deserialize<'de> for Secret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <String as serde::Deserialize>::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Default)]
pub struct LocalSigner {
    keys: HashMap<KeyRef, Zeroizing<Vec<u8>>>,
}
impl LocalSigner {
    pub fn new() -> Self {
        Self::default()
    }
    /// Consumes bytes; neither this provider nor its keys implement Debug/export.
    pub fn insert(&mut self, reference: KeyRef, bytes: Vec<u8>) {
        self.keys.insert(reference, Zeroizing::new(bytes));
    }
    pub fn from_setup(
        setup: LocalSetup,
    ) -> Result<(Self, safe_cex_engine::signing::Identities), SignError> {
        let mut signer = Self::new();
        let mut install = |slot: &str, identity: Option<LocalIdentity>| {
            identity
                .map(|identity| {
                    if identity
                        .stable_id
                        .as_deref()
                        .is_some_and(|id| !safe_cex_engine::signing::valid_stable_id(id))
                    {
                        return Err(SignError::InvalidRequest);
                    }
                    let mut key = key_material::decode(identity.key, KeyEncoding::Utf8)?;
                    if identity.trim_credential {
                        key = Zeroizing::new(
                            std::str::from_utf8(&key)
                                .map_err(|_| SignError::InvalidRequest)?
                                .trim()
                                .as_bytes()
                                .to_vec(),
                        );
                    }
                    let secret = key_material::decode(identity.secret, identity.secret_encoding)?;
                    let passphrase = identity
                        .passphrase
                        .map(|value| key_material::decode(value, KeyEncoding::Utf8))
                        .transpose()?;
                    let credential = KeyRef(if key.is_empty() {
                        String::new()
                    } else {
                        format!("{slot}.credential")
                    });
                    let signing = KeyRef(if secret.is_empty() {
                        String::new()
                    } else {
                        format!("{slot}.signing")
                    });
                    if !key.is_empty() {
                        signer.keys.insert(credential.clone(), key);
                    }
                    if !secret.is_empty() {
                        signer.keys.insert(signing.clone(), secret);
                    }
                    let passphrase = passphrase.map(|bytes| {
                        let reference = KeyRef(format!("{slot}.passphrase"));
                        signer.keys.insert(reference.clone(), bytes);
                        reference
                    });
                    Ok(safe_cex_engine::signing::Identity {
                        credential,
                        signing,
                        passphrase,
                        stable_id: identity.stable_id,
                    })
                })
                .transpose()
        };
        let identities = safe_cex_engine::signing::Identities {
            primary: install("primary", setup.primary)?,
            secondary: install("secondary", setup.secondary)?,
        };
        Ok((signer, identities))
    }
    /// Own and wipe the setup input on both successful and malformed construction.
    pub fn from_json(
        input: String,
    ) -> Result<(Self, safe_cex_engine::signing::Identities), SignError> {
        let input = Zeroizing::new(input);
        if input.len() > 4 * 1024 * 1024 {
            return Err(SignError::InvalidRequest);
        }
        let setup: LocalSetup =
            serde_json::from_str(&input).map_err(|_| SignError::InvalidRequest)?;
        Self::from_setup(setup)
    }
    pub fn remove(&mut self, reference: &KeyRef) {
        self.keys.remove(reference);
    }
    fn key(&self, reference: &KeyRef) -> Result<&[u8], SignError> {
        self.keys
            .get(reference)
            .map(|key| key.as_slice())
            .ok_or(SignError::Unavailable)
    }
    fn expand(
        &self,
        payload: &[PayloadPart],
        results: &SignResult,
    ) -> Result<Zeroizing<Vec<u8>>, SignError> {
        if payload.len() > 256 {
            return Err(SignError::InvalidRequest);
        }
        let mut length = 0usize;
        for part in payload {
            let n = match part {
                PayloadPart::Bytes { bytes } => bytes.len(),
                PayloadPart::Credential { key } => self.key(key)?.len(),
                PayloadPart::Result { index, encoding } => {
                    let n = results
                        .0
                        .get(*index)
                        .ok_or(SignError::InvalidRequest)?
                        .len();
                    match encoding {
                        Encoding::Raw => n,
                        Encoding::Hex => n.checked_mul(2).ok_or(SignError::InvalidRequest)?,
                        Encoding::Base64 => n
                            .div_ceil(3)
                            .checked_mul(4)
                            .ok_or(SignError::InvalidRequest)?,
                        Encoding::Base64Hex => n
                            .checked_mul(2)
                            .ok_or(SignError::InvalidRequest)?
                            .div_ceil(3)
                            .checked_mul(4)
                            .ok_or(SignError::InvalidRequest)?,
                    }
                }
            };
            length = length
                .checked_add(n)
                .filter(|n| *n <= 1024 * 1024)
                .ok_or(SignError::InvalidRequest)?;
        }
        // One allocation: growing a secret-bearing Vec would free an unwiped buffer.
        let mut message = Zeroizing::new(Vec::with_capacity(length));
        for part in payload {
            match part {
                PayloadPart::Bytes { bytes } => message.extend_from_slice(bytes),
                PayloadPart::Credential { key } => message.extend_from_slice(self.key(key)?),
                PayloadPart::Result { index, encoding } => {
                    let bytes = results.0.get(*index).ok_or(SignError::InvalidRequest)?;
                    match encoding {
                        Encoding::Raw => message.extend_from_slice(bytes),
                        Encoding::Hex => {
                            message.extend_from_slice(Zeroizing::new(crate::hex(bytes)).as_bytes())
                        }
                        Encoding::Base64 => message.extend_from_slice(
                            Zeroizing::new(crate::encode(bytes, crate::Encoding::Base64))
                                .as_bytes(),
                        ),
                        Encoding::Base64Hex => {
                            let hex = Zeroizing::new(crate::hex(bytes));
                            let encoded = Zeroizing::new(crate::encode(
                                hex.as_bytes(),
                                crate::Encoding::Base64,
                            ));
                            message.extend_from_slice(encoded.as_bytes());
                        }
                    }
                }
            }
        }
        Ok(message)
    }
    /// Also used by synchronous local hosts; async providers share the same types.
    pub fn sign_now(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        request.validate()?;
        let mut result = SignResult(Vec::with_capacity(request.operations.len()));
        for operation in &request.operations {
            let value = match operation {
                Operation::Sha256 { payload } => {
                    Sha256::digest(&*self.expand(payload, &result)?).to_vec()
                }
                Operation::Present { key } => {
                    let value = self.key(key)?;
                    if value.len() > 1024 * 1024 {
                        return Err(SignError::InvalidRequest);
                    }
                    value.to_vec()
                }
                Operation::Sign {
                    algorithm,
                    key,
                    payload,
                } => {
                    let message = self.expand(payload, &result)?;
                    let key = self.key(key)?;
                    match algorithm {
                        Algorithm::HmacSha256 => {
                            let mut mac = Hmac::<Sha256>::new_from_slice(key)
                                .map_err(|_| SignError::InvalidRequest)?;
                            mac.update(&message);
                            mac.finalize().into_bytes().to_vec()
                        }
                        Algorithm::HmacSha512 => {
                            let mut mac = Hmac::<Sha512>::new_from_slice(key)
                                .map_err(|_| SignError::InvalidRequest)?;
                            mac.update(&message);
                            mac.finalize().into_bytes().to_vec()
                        }
                        Algorithm::Sha256SecretSuffix => {
                            let mut hash = Sha256::new();
                            hash.update(&message);
                            hash.update(key);
                            hash.finalize().to_vec()
                        }
                        // Vocabulary is not a claim that the local provider implements it.
                        Algorithm::Ed25519
                        | Algorithm::Es256
                        | Algorithm::Secp256k1Digest
                        | Algorithm::StarkDigest
                        | Algorithm::SchnorrDigest => return Err(SignError::Unsupported),
                    }
                }
            };
            result.0.push(value);
        }
        Ok(result)
    }
}
impl Signer for LocalSigner {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.sign_now(request))
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_now(&request);
        Box::pin(std::future::ready(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vector() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/hmac/binance.json")).unwrap()
    }

    fn pem(label: &str, body: &str) -> String {
        let boundary = "-----";
        format!("{boundary}BEGIN {label}{boundary}\n{body}\n{boundary}END {label}{boundary}\n")
    }

    #[test]
    fn trusted_formats_decode_inside_provider_without_changing_signature_bytes() {
        let vector = vector();
        let secret = vector["secret"].as_str().unwrap();
        let base64 = crate::encode(secret.as_bytes(), crate::Encoding::Base64);
        for (encoding, material) in [
            ("utf8", secret.to_owned()),
            ("base64", base64.clone()),
            ("hex", crate::hex(secret.as_bytes())),
            ("pem", pem("PRIVATE KEY", &base64)),
        ] {
            let (signer, identities) = LocalSigner::from_json(
                serde_json::json!({
                    "primary": {
                        "key": "public-identifier", "secret": material, "secretEncoding": encoding,
                        "stableId": "account/opaque-17", "passphrase": vector["payload"],
                    }
                })
                .to_string(),
            )
            .unwrap();
            let identity = identities.primary.clone().unwrap();
            assert_eq!(identity.require_stable_id(), Ok("account/opaque-17"));
            let request = SignRequest {
                context: Default::default(),
                operations: vec![
                    Operation::Present {
                        key: identity.passphrase.unwrap(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: identity.signing,
                        payload: vec![PayloadPart::Bytes {
                            bytes: vector["payload"].as_str().unwrap().as_bytes().to_vec(),
                        }],
                    },
                ],
            };
            let result = signer.sign_now(&request).unwrap();
            assert_eq!(result.0[0], vector["payload"].as_str().unwrap().as_bytes());
            assert_eq!(
                crate::hex(&result.0[1]),
                vector["signature"].as_str().unwrap(),
                "{encoding}"
            );
            assert!(!format!("{identities:?}{request:?}{result:?}").contains(secret));
        }
    }

    #[test]
    fn stable_ids_are_explicit_and_independent_of_keys_or_normalization() {
        let setup = |key: &str, stable: Option<&str>| {
            serde_json::json!({
            "primary": { "key": key, "secret": vector()["secret"], "trimCredential": true, "stableId": stable },
            "secondary": { "key": key, "secret": vector()["secret"], "stableId": stable },
        }).to_string()
        };
        let (signer, first) =
            LocalSigner::from_json(setup(" public-identifier ", Some("same/credential"))).unwrap();
        let (_, second) = LocalSigner::from_json(setup(
            "different-public-identifier",
            Some("same/credential"),
        ))
        .unwrap();
        assert_eq!(
            first.primary.as_ref().unwrap().require_stable_id(),
            Ok("same/credential")
        );
        assert_eq!(
            first.primary.as_ref().unwrap().stable_id,
            second.primary.unwrap().stable_id
        );
        assert_eq!(
            first.primary.as_ref().unwrap().stable_id,
            first.secondary.as_ref().unwrap().stable_id
        );
        let request = SignRequest {
            context: Default::default(),
            operations: vec![
                Operation::Present {
                    key: first.primary.unwrap().credential,
                },
                Operation::Present {
                    key: first.secondary.unwrap().credential,
                },
            ],
        };
        let result = signer.sign_now(&request).unwrap();
        assert_eq!(
            result.0,
            [
                b"public-identifier".to_vec(),
                b" public-identifier ".to_vec()
            ]
        );
        let (_, legacy) = LocalSigner::from_json(setup("public-identifier", None)).unwrap();
        assert_eq!(
            legacy.primary.unwrap().require_stable_id(),
            Err(SignError::InvalidRequest)
        );
    }

    #[test]
    fn missing_and_explicit_empty_passphrases_remain_distinct() {
        let (_, absent) = LocalSigner::from_json(r#"{"primary":{}}"#.into()).unwrap();
        assert!(absent.primary.unwrap().passphrase.is_none());
        let (signer, present) =
            LocalSigner::from_json(r#"{"primary":{"passphrase":""}}"#.into()).unwrap();
        let request = SignRequest {
            context: Default::default(),
            operations: vec![Operation::Present {
                key: present.primary.unwrap().passphrase.unwrap(),
            }],
        };
        assert_eq!(signer.sign_now(&request).unwrap().0, [Vec::<u8>::new()]);
    }

    #[test]
    fn malformed_or_oversized_setup_material_has_only_closed_errors() {
        for (encoding, value) in [
            ("base64", "not base64!".to_owned()),
            ("hex", "123".to_owned()),
            ("hex", "0g".to_owned()),
            (
                "pem",
                pem("PRIVATE KEY", "AQ==").replace("END PRIVATE KEY", "END EC PRIVATE KEY"),
            ),
            ("pem", format!("{}trailing", pem("PRIVATE KEY", "AQ=="))),
            ("pem", pem("CERTIFICATE", "AQ==")),
            ("utf8", "x".repeat(1024 * 1024 + 1)),
        ] {
            let result = LocalSigner::from_json(
                serde_json::json!({ "primary": { "secret": value, "secretEncoding": encoding } })
                    .to_string(),
            );
            assert!(
                matches!(result, Err(SignError::InvalidRequest)),
                "{encoding}"
            );
        }
        for id in ["".to_owned(), "bad\nidentity".to_owned(), "x".repeat(257)] {
            assert!(matches!(
                LocalSigner::from_json(
                    serde_json::json!({ "primary": { "stableId": id } }).to_string()
                ),
                Err(SignError::InvalidRequest)
            ));
        }
        assert!(matches!(
            LocalSigner::from_json(" ".repeat(4 * 1024 * 1024 + 1)),
            Err(SignError::InvalidRequest)
        ));
        assert!(matches!(
            LocalSigner::from_json(r#"{"primary":{"secretEncodng":"base64"}}"#.into()),
            Err(SignError::InvalidRequest)
        ));
    }

    fn fixture() -> (LocalSigner, KeyRef) {
        // Reuse the existing public Binance documentation vector; no new keys.
        let key = KeyRef("primary".into());
        let mut signer = LocalSigner::new();
        let vector: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/hmac/binance.json")).unwrap();
        let secret = vector["secret"].as_str().unwrap();
        signer.insert(key.clone(), secret.as_bytes().to_vec());
        (signer, key)
    }
    #[test]
    fn atomic_credential_interpolation_and_exact_hmac() {
        let (mut signer, key) = fixture();
        let public = KeyRef("api-key".into());
        signer.insert(public.clone(), b"public-identifier".to_vec());
        let request = SignRequest {
            context: Default::default(),
            operations: vec![
                Operation::Present {
                    key: public.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: key.clone(),
                    payload: vec![
                        PayloadPart::Bytes {
                            bytes: b"apiKey=".to_vec(),
                        },
                        PayloadPart::Credential { key: public },
                    ],
                },
            ],
        };
        let result = signer.sign_now(&request).unwrap();
        assert_eq!(result.0[0], b"public-identifier");
        assert_eq!(
            crate::hex(&result.0[1]),
            crate::hmac_sha256(
                signer.key(&key).unwrap(),
                b"apiKey=public-identifier",
                crate::Encoding::Hex
            )
        );
        assert!(!format!("{request:?}{result:?}").contains("public-identifier"));
        signer.remove(&key);
        assert_eq!(signer.sign_now(&request), Err(SignError::Unavailable));
    }
    #[test]
    fn composite_digest_and_secret_suffix_stay_inside_one_batch() {
        let (mut signer, key) = fixture();
        let api = KeyRef("public".into());
        signer.insert(api.clone(), b"public-identifier".to_vec());
        let request = SignRequest {
            context: Default::default(),
            operations: vec![
                Operation::Sha256 {
                    payload: vec![
                        PayloadPart::Bytes {
                            bytes: b"nonce+timestamp+".to_vec(),
                        },
                        PayloadPart::Credential { key: api },
                    ],
                },
                Operation::Sign {
                    algorithm: Algorithm::Sha256SecretSuffix,
                    key: key.clone(),
                    payload: vec![PayloadPart::Result {
                        index: 0,
                        encoding: Encoding::Hex,
                    }],
                },
            ],
        };
        let result = signer.sign_now(&request).unwrap();
        let digest = crate::sha256(b"nonce+timestamp+public-identifier");
        assert_eq!(result.0[0], digest);
        let mut expected = Sha256::new();
        expected.update(crate::hex(&digest));
        expected.update(signer.key(&key).unwrap());
        assert_eq!(result.0[1], &expected.finalize()[..]);
        assert!(!format!("{request:?}{result:?}").contains("public-identifier"));
    }
    #[test]
    fn failed_batch_never_returns_partial_credentials_or_provider_text() {
        let (signer, key) = fixture();
        let request = SignRequest {
            context: Default::default(),
            operations: vec![
                Operation::Present { key: key.clone() },
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key,
                    payload: vec![PayloadPart::Result {
                        index: 9,
                        encoding: Encoding::Raw,
                    }],
                },
            ],
        };
        assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
        assert_eq!(
            SignError::Refused.to_string(),
            "Signing provider refused the request"
        );
    }
}
