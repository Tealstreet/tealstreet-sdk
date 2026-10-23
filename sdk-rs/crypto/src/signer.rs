//! Normal local provider. Construction consumes credentials before venue setup.
//! This module alone owns key material; engine/venue state receives opaque refs.
use hmac::{Hmac, Mac};
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, Encoding, ExtraKey, Identities, Identity, KeyRef,
    Operation, PayloadPart, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use sha2::{Digest, Sha256, Sha512};
use std::collections::{BTreeMap, HashMap};
use zeroize::Zeroizing;
mod key_material;
mod primitives;
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
    /// Declared extra signing material, e.g. ApeX's zkLink seed.
    #[serde(default)]
    pub extra: LocalExtras,
    /// Secondary only: sign with the primary's ref instead of own material.
    #[serde(default)]
    pub signing_from_primary: bool,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalExtra {
    pub value: Secret,
    #[serde(default)]
    pub encoding: KeyEncoding,
}
/// Duplicate names are refused rather than silently keeping one value.
#[derive(Default)]
pub struct LocalExtras(pub BTreeMap<ExtraKey, LocalExtra>);
impl<'de> serde::Deserialize<'de> for LocalExtras {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = LocalExtras;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("extra signing material by name")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<LocalExtras, A::Error> {
                let mut extras = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<ExtraKey, LocalExtra>()? {
                    if extras.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate extra name"));
                    }
                }
                Ok(LocalExtras(extras))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
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

// Decoded buffers stay guarded on every failure, including the second slot.
struct DecodedIdentity {
    key: Zeroizing<Vec<u8>>,
    secret: Zeroizing<Vec<u8>>,
    passphrase: Option<Zeroizing<Vec<u8>>>,
    stable_id: Option<String>,
    extra: Vec<(ExtraKey, Zeroizing<Vec<u8>>)>,
}
impl DecodedIdentity {
    fn decode(identity: LocalIdentity) -> Result<Self, SignError> {
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
        Ok(Self {
            key,
            secret: key_material::decode(identity.secret, identity.secret_encoding)?,
            passphrase: identity
                .passphrase
                .map(|value| key_material::decode(value, KeyEncoding::Utf8))
                .transpose()?,
            stable_id: identity.stable_id,
            extra: identity
                .extra
                .0
                .into_iter()
                .map(|(name, extra)| Ok((name, key_material::decode(extra.value, extra.encoding)?)))
                .collect::<Result<_, SignError>>()?,
        })
    }
    fn extra_ref(slot: &str, name: ExtraKey) -> KeyRef {
        KeyRef(format!("{slot}.extra.{}", name.name()))
    }
    fn references(&self, slot: &str) -> Identity {
        Identity {
            credential: KeyRef(if self.key.is_empty() {
                String::new()
            } else {
                format!("{slot}.credential")
            }),
            signing: KeyRef(if self.secret.is_empty() {
                String::new()
            } else {
                format!("{slot}.signing")
            }),
            passphrase: self
                .passphrase
                .as_ref()
                .map(|_| KeyRef(format!("{slot}.passphrase"))),
            stable_id: self.stable_id.clone(),
            extra: self
                .extra
                .iter()
                .filter(|(_, bytes)| !bytes.is_empty())
                .map(|(name, _)| (*name, Self::extra_ref(slot, *name)))
                .collect(),
        }
    }
    fn install(self, signer: &mut LocalSigner, slot: &str) {
        if !self.key.is_empty() {
            signer
                .keys
                .insert(KeyRef(format!("{slot}.credential")), self.key);
        }
        if !self.secret.is_empty() {
            signer
                .keys
                .insert(KeyRef(format!("{slot}.signing")), self.secret);
        }
        if let Some(passphrase) = self.passphrase {
            signer
                .keys
                .insert(KeyRef(format!("{slot}.passphrase")), passphrase);
        }
        for (name, bytes) in self.extra {
            if !bytes.is_empty() {
                let reference = Self::extra_ref(slot, name);
                signer.roles.insert(reference.clone(), name);
                signer.keys.insert(reference, bytes);
            }
        }
    }
}

#[derive(Default)]
pub struct LocalSigner {
    keys: HashMap<KeyRef, Zeroizing<Vec<u8>>>,
    /// Extra refs are usable only by their own algorithm, never presented,
    /// interpolated or used as an ordinary MAC/digest key.
    roles: HashMap<KeyRef, ExtraKey>,
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
        Self::from_setup_with_policy(setup, CredentialSetupPolicy::default())
    }
    /// Existing policy APIs retain their private-context contract.
    pub fn from_setup_with_policy(
        setup: LocalSetup,
        policy: CredentialSetupPolicy,
    ) -> Result<(Self, Identities), SignError> {
        Self::from_setup_with_policy_and_mode(setup, policy, false)
    }
    /// Decode and validate all required material before installing provider refs.
    pub fn from_setup_with_policy_and_mode(
        mut setup: LocalSetup,
        policy: CredentialSetupPolicy,
        public_data_only: bool,
    ) -> Result<(Self, Identities), SignError> {
        for (identity, rule) in [
            (&mut setup.primary, policy.primary),
            (&mut setup.secondary, policy.secondary),
        ] {
            if let Some(identity) = identity {
                if let Some(trim) = rule.and_then(|rule| rule.trim_credential) {
                    identity.trim_credential = trim;
                }
                if let Some(encoding) = rule.and_then(|rule| rule.secret_encoding) {
                    identity.secret_encoding = encoding;
                }
                if let Some(extra) = rule.and_then(|rule| rule.extra)
                    && let Some(encoding) = extra.encoding
                    && let Some(material) = identity.extra.0.get_mut(&extra.key)
                {
                    material.encoding = encoding;
                }
            }
        }
        // The alias shares one installed copy, so both slots must declare the
        // same decoding and only the secondary may name the primary.
        let alias = setup
            .secondary
            .as_ref()
            .is_some_and(|id| id.signing_from_primary);
        if setup
            .primary
            .as_ref()
            .is_some_and(|id| id.signing_from_primary)
            || alias
                && (setup
                    .secondary
                    .as_ref()
                    .is_some_and(|id| !id.secret.0.is_empty())
                    || setup.primary.as_ref().is_none_or(|primary| {
                        primary.secret.0.is_empty()
                            || setup.secondary.as_ref().map(|id| id.secret_encoding)
                                != Some(primary.secret_encoding)
                    }))
        {
            return Err(SignError::InvalidRequest);
        }
        let primary = setup.primary.map(DecodedIdentity::decode).transpose()?;
        let secondary = setup.secondary.map(DecodedIdentity::decode).transpose()?;
        let mut identities = Identities {
            primary: primary.as_ref().map(|id| id.references("primary")),
            secondary: secondary.as_ref().map(|id| id.references("secondary")),
        };
        if alias {
            // A prefix-only hex key decodes to nothing; it cannot be aliased.
            let (Some(primary), Some(secondary)) =
                (identities.primary.as_ref(), identities.secondary.as_mut())
            else {
                return Err(SignError::InvalidRequest);
            };
            if primary.signing.0.is_empty() {
                return Err(SignError::InvalidRequest);
            }
            secondary.signing = primary.signing.clone();
        }
        // The same policy validates bytes and opaque refs. Empty decoded material
        // is represented only in temporary validation metadata as an invalid ref.
        // Optional explicit-empty passphrases retain their existing installed ref.
        let mut validation = identities.clone();
        for (material, identity) in [
            (primary.as_ref(), validation.primary.as_mut()),
            (secondary.as_ref(), validation.secondary.as_mut()),
        ] {
            if let (Some(material), Some(identity)) = (material, identity) {
                if material
                    .passphrase
                    .as_ref()
                    .is_some_and(|bytes| bytes.is_empty())
                {
                    identity.passphrase = Some(KeyRef(String::new()));
                }
                // Supplied-but-empty extras stay visible to declaration checks.
                for (name, bytes) in &material.extra {
                    if bytes.is_empty() {
                        identity.extra.insert(*name, KeyRef(String::new()));
                    }
                }
            }
        }
        policy.apply_to_identities(&mut validation, public_data_only)?;
        policy.apply_to_identities(&mut identities, public_data_only)?;
        let mut signer = Self::new();
        if let Some(primary) = primary {
            primary.install(&mut signer, "primary");
        }
        if let Some(secondary) = secondary {
            secondary.install(&mut signer, "secondary");
        }
        Ok((signer, identities))
    }
    /// Own and wipe the setup input on both successful and malformed construction.
    pub fn from_json(
        input: String,
    ) -> Result<(Self, safe_cex_engine::signing::Identities), SignError> {
        Self::from_json_with_policy(input, CredentialSetupPolicy::default())
    }
    /// Own and wipe the setup buffer under the same bounds as policy-free setup.
    pub fn from_json_with_policy(
        input: String,
        policy: CredentialSetupPolicy,
    ) -> Result<(Self, safe_cex_engine::signing::Identities), SignError> {
        Self::from_json_with_policy_and_mode(input, policy, false)
    }
    /// Setup mode is explicit; it is not inferred from stripped credentials.
    pub fn from_json_with_policy_and_mode(
        input: String,
        policy: CredentialSetupPolicy,
        public_data_only: bool,
    ) -> Result<(Self, Identities), SignError> {
        let input = Zeroizing::new(input);
        if input.len() > 4 * 1024 * 1024 {
            return Err(SignError::InvalidRequest);
        }
        let setup: LocalSetup =
            serde_json::from_str(&input).map_err(|_| SignError::InvalidRequest)?;
        Self::from_setup_with_policy_and_mode(setup, policy, public_data_only)
    }
    pub fn remove(&mut self, reference: &KeyRef) {
        self.keys.remove(reference);
        self.roles.remove(reference);
    }
    /// Ordinary material only; extra refs require `role_key`.
    fn key(&self, reference: &KeyRef) -> Result<&[u8], SignError> {
        if self.roles.contains_key(reference) {
            return Err(SignError::InvalidRequest);
        }
        self.raw_key(reference)
    }
    fn role_key(&self, reference: &KeyRef, role: ExtraKey) -> Result<&[u8], SignError> {
        if self.roles.get(reference) != Some(&role) {
            return Err(SignError::InvalidRequest);
        }
        self.raw_key(reference)
    }
    fn raw_key(&self, reference: &KeyRef) -> Result<&[u8], SignError> {
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
                        .values
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
                    let bytes = results
                        .values
                        .get(*index)
                        .ok_or(SignError::InvalidRequest)?;
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
        let mut result = SignResult::new(
            Vec::with_capacity(request.operations.len()),
            request.receipt,
        );
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
                    let key = match algorithm {
                        Algorithm::ZkLinkContract => self.role_key(key, ExtraKey::ZklinkSeed)?,
                        _ => self.key(key)?,
                    };
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
                        #[cfg(feature = "zklink")]
                        Algorithm::ZkLinkContract => {
                            if key.len() != 65 || message.len() != 36 {
                                return Err(SignError::InvalidRequest);
                            }
                            crate::zklink::sign(key, &message)
                                .map_err(|_| SignError::InvalidRequest)?
                                .signature
                                .to_vec()
                        }
                        #[cfg(not(feature = "zklink"))]
                        Algorithm::ZkLinkContract => return Err(SignError::Unsupported),
                        Algorithm::Ed25519 => primitives::ed25519(key, &message)?,
                        Algorithm::Es256 => primitives::es256(key, &message)?,
                        Algorithm::Secp256k1Digest => primitives::secp256k1_digest(key, &message)?,
                        Algorithm::StarkDigest => primitives::stark_digest(key, &message)?,
                        Algorithm::SchnorrDigest => primitives::schnorr_digest(key, &message)?,
                    }
                }
            };
            result.values.push(value);
        }
        Ok(result)
    }
}
/// Mirrors the `sign_now` arms; keep the two in step.
fn implemented(algorithm: Algorithm) -> bool {
    match algorithm {
        Algorithm::HmacSha256 | Algorithm::HmacSha512 | Algorithm::Sha256SecretSuffix => true,
        Algorithm::ZkLinkContract => cfg!(feature = "zklink"),
        Algorithm::Ed25519
        | Algorithm::Es256
        | Algorithm::Secp256k1Digest
        | Algorithm::StarkDigest
        | Algorithm::SchnorrDigest => false,
    }
}

impl Signer for LocalSigner {
    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        let usable = match algorithm {
            Algorithm::ZkLinkContract => self.role_key(key, ExtraKey::ZklinkSeed).is_ok(),
            _ => self.key(key).is_ok(),
        };
        if usable && implemented(algorithm) {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
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

    #[test]
    fn setup_policy_overrides_only_declared_trim_fields_before_present_and_hmac() {
        use safe_cex_engine::signing::IdentitySetupPolicy;
        let policy = CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                trim_credential: Some(true),
                ..Default::default()
            }),
            secondary: Some(IdentitySetupPolicy {
                trim_credential: Some(false),
                ..Default::default()
            }),
        };
        let setup = r#"{
            "primary":{"key":" primary-key ","secret":" primary-secret ","passphrase":" primary-pass ","stableId":"account/primary","trimCredential":false},
            "secondary":{"key":" secondary-key ","secret":" secondary-secret ","passphrase":" secondary-pass ","stableId":"account/secondary","trimCredential":true}
        }"#;
        let (signer, identities) =
            LocalSigner::from_json_with_policy(setup.into(), policy).unwrap();
        for (identity, key, passphrase, stable, mac) in [
            (
                identities.primary.unwrap(),
                "primary-key",
                " primary-pass ",
                "account/primary",
                "4f8d4471dddca1b3792fc9f83256ce90afab9d5c79a71336c6ff3a6d6b18cb95",
            ),
            (
                identities.secondary.unwrap(),
                " secondary-key ",
                " secondary-pass ",
                "account/secondary",
                "db697a69b671ef53c2dc18ee6491eaeecd3e428d152f41385efcc5f82289a7fb",
            ),
        ] {
            assert_eq!(identity.require_stable_id(), Ok(stable));
            let request = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![
                    Operation::Present {
                        key: identity.credential.clone(),
                    },
                    Operation::Present {
                        key: identity.passphrase.unwrap(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: identity.signing,
                        payload: vec![
                            PayloadPart::Bytes {
                                bytes: b"fixture:".to_vec(),
                            },
                            PayloadPart::Credential {
                                key: identity.credential,
                            },
                            PayloadPart::Bytes {
                                bytes: b":body".to_vec(),
                            },
                        ],
                    },
                ],
            };
            let result = signer.sign_now(&request).unwrap();
            assert_eq!(result.values[0], key.as_bytes());
            assert_eq!(result.values[1], passphrase.as_bytes());
            // Fixed Python stdlib HMAC oracle, including surrounding secret spaces.
            assert_eq!(crate::hex(&result.values[2]), mac);
        }
    }

    #[test]
    fn undeclared_setup_policy_preserves_caller_metadata_and_absent_slots() {
        use safe_cex_engine::signing::IdentitySetupPolicy;
        let (signer, identities) = LocalSigner::from_setup_with_policy(
            LocalSetup {
                primary: Some(LocalIdentity {
                    key: Secret::new(" primary-key ".into()),
                    secret: Secret::new("primary-secret".into()),
                    trim_credential: true,
                    ..Default::default()
                }),
                secondary: None,
            },
            CredentialSetupPolicy {
                primary: Some(IdentitySetupPolicy::default()),
                secondary: Some(IdentitySetupPolicy {
                    trim_credential: Some(true),
                    ..Default::default()
                }),
            },
        )
        .unwrap();
        assert!(identities.secondary.is_none());
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: identities.primary.unwrap().credential,
            }],
        };
        assert_eq!(
            signer.sign_now(&request).unwrap().values,
            [b"primary-key".to_vec()]
        );
        assert_eq!(
            serde_json::from_str::<CredentialSetupPolicy>("{}").unwrap(),
            CredentialSetupPolicy::default()
        );
        for invalid in [
            r#"{"primry":{}}"#,
            r#"{"primary":{"trimCredental":true}}"#,
            r#"{"secondary":{"trimCredential":"true"}}"#,
        ] {
            assert!(serde_json::from_str::<CredentialSetupPolicy>(invalid).is_err());
        }
    }

    #[test]
    fn policy_setup_retains_json_bounds_and_closed_failures() {
        let policy = CredentialSetupPolicy::default();
        for input in [
            " ".repeat(4 * 1024 * 1024 + 1),
            r#"{"primary":{"secret":"sentinel"},"unknown":"sentinel"}"#.into(),
            r#"{"primary":{"secret":"sentinel","secretEncoding":"hex"}}"#.into(),
            r#"{"primary":{"stableId":"bad\nmapping"}}"#.into(),
        ] {
            assert!(matches!(
                LocalSigner::from_json_with_policy(input, policy),
                Err(SignError::InvalidRequest)
            ));
        }
    }

    fn required_passphrase_policy() -> CredentialSetupPolicy {
        use safe_cex_engine::signing::{IdentitySetupPolicy, SetupFieldPolicy};
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                passphrase: Some(SetupFieldPolicy::RequiredForPrivate),
                ..Default::default()
            }),
            secondary: Some(IdentitySetupPolicy {
                passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn required_material_validates_before_install_and_defaults_remain_optional() {
        for input in [
            r#"{"primary":{"key":"key","secret":"secret"}}"#,
            r#"{"primary":{"key":"key","secret":"secret","passphrase":""}}"#,
            r#"{"primary":{"passphrase":""},"secondary":{"key":"read","secret":"secret"}}"#,
            r#"{"secondary":{"key":"read","secret":"secret"}}"#,
            r#"{"primary":{"passphrase":"usable"},"secondary":{"key":"read","secret":"secret","passphrase":""}}"#,
        ] {
            assert!(
                matches!(
                    LocalSigner::from_json_with_policy(input.into(), required_passphrase_policy()),
                    Err(SignError::InvalidRequest)
                ),
                "{input}"
            );
        }
        let missing = r#"{"primary":{"key":"key","secret":"secret"}}"#;
        assert!(LocalSigner::from_json(missing.into()).is_ok());
        assert!(
            LocalSigner::from_json_with_policy_and_mode(
                missing.into(),
                required_passphrase_policy(),
                true
            )
            .is_ok()
        );
        // Public mode relaxes only primary, not an existing complete secondary.
        assert!(matches!(LocalSigner::from_json_with_policy_and_mode(r#"{"primary":{"key":"key","secret":"secret"},"secondary":{"key":"read","secret":"secret"}}"#.into(), required_passphrase_policy(), true), Err(SignError::InvalidRequest)));
        // Incomplete slots keep their prior optional-field behavior.
        assert!(
            LocalSigner::from_json_with_policy(
                r#"{"primary":{"key":"key"},"secondary":{"secret":"secret"}}"#.into(),
                required_passphrase_policy()
            )
            .is_ok()
        );
    }

    #[test]
    fn fallback_aliases_primary_without_copying_or_normalizing_passphrase_material() {
        let passphrase = "\u{0085} \u{feff} exact passphrase ";
        let setup = serde_json::json!({"primary":{"passphrase":passphrase},"secondary":{"key":"read","secret":"secret","stableId":"account/read"}}).to_string();
        let (mut signer, ids) =
            LocalSigner::from_json_with_policy(setup, required_passphrase_policy()).unwrap();
        let primary = ids.primary.unwrap();
        let secondary = ids.secondary.unwrap();
        assert!(primary.credential.0.is_empty() && primary.signing.0.is_empty());
        assert_eq!(secondary.stable_id.as_deref(), Some("account/read"));
        assert_eq!(secondary.passphrase, primary.passphrase);
        assert!(
            !signer
                .keys
                .contains_key(&KeyRef("secondary.passphrase".into()))
        );
        let reference = secondary.passphrase.unwrap();
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: reference.clone(),
            }],
        };
        assert_eq!(
            signer.sign_now(&request).unwrap().values,
            [passphrase.as_bytes().to_vec()]
        );
        signer.remove(&reference);
        assert!(matches!(
            signer.sign_now(&request),
            Err(SignError::Unavailable)
        ));
        let own = serde_json::json!({"primary":{"passphrase":"primary"},"secondary":{"key":"read","secret":"secret","passphrase":"own"}}).to_string();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(own, required_passphrase_policy()).unwrap();
        let secondary = ids.secondary.unwrap();
        assert_eq!(
            secondary.passphrase,
            Some(KeyRef("secondary.passphrase".into()))
        );
        assert_eq!(
            signer.key(secondary.passphrase.as_ref().unwrap()).unwrap(),
            b"own"
        );
    }

    /// R1: the provider arm signs exactly what the legacy Lighter signer signs,
    /// with the payload split as entropy (first 32 bytes) then input.
    #[cfg(feature = "schnorr")]
    #[test]
    fn schnorr_digest_matches_the_legacy_hedged_lighter_signature() {
        let key =
            "01000000000000000000000000000000000000000000000000000000000000000000000000000000";
        let (signer, ids) = LocalSigner::from_json(
            serde_json::json!({"primary":{"key":"lighter-api-key","secret":format!("0x{key}"),"secretEncoding":"hex"}})
                .to_string(),
        )
        .unwrap();
        let signing = ids.primary.unwrap().signing;
        let sign = |entropy: [u8; 32], input: &[u8]| {
            let mut payload = entropy.to_vec();
            payload.extend_from_slice(input);
            signer.sign_now(&SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![Operation::Sign {
                    algorithm: Algorithm::SchnorrDigest,
                    key: signing.clone(),
                    payload: vec![PayloadPart::Bytes { bytes: payload }],
                }],
            })
        };
        let key_bytes: Vec<u8> = (0..40)
            .map(|i| u8::from_str_radix(&key[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        // A canonical word, then words at and above p, which legacy and Go reduce.
        for input in [
            vec![1, 0, 0, 0, 0, 0, 0, 0],
            vec![1, 0, 0, 0, 0xff, 0xff, 0xff, 0xff],
            vec![0xff; 16],
        ] {
            for entropy in [[0u8; 32], [0x5a; 32]] {
                let legacy = crate::lighter::schnorr_sign(
                    &input,
                    &key_bytes,
                    crate::lighter::hedged_nonce(&input, &key_bytes, &entropy).unwrap(),
                )
                .unwrap();
                assert_eq!(legacy.len(), 80);
                assert_eq!(sign(entropy, &input).unwrap().values, vec![legacy]);
            }
        }
        // Entropy alone, or input that is not whole words, is refused.
        assert_eq!(sign([0; 32], &[]), Err(SignError::InvalidRequest));
        assert_eq!(sign([0; 32], &[1, 2, 3]), Err(SignError::InvalidRequest));
    }

    fn schnorr_request(key: KeyRef, payload: Vec<u8>) -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key,
                payload: vec![PayloadPart::Bytes { bytes: payload }],
            }],
        }
    }

    fn schnorr_signer(key: &[u8]) -> (LocalSigner, KeyRef) {
        let hex: String = key.iter().map(|byte| format!("{byte:02x}")).collect();
        let (signer, ids) = LocalSigner::from_json(
            serde_json::json!({"primary":{"key":"k","secret":hex,"secretEncoding":"hex"}})
                .to_string(),
        )
        .unwrap();
        (signer, ids.primary.unwrap().signing)
    }

    /// R1: payloads without full entropy, or without whole input words, are refused.
    #[cfg(feature = "schnorr")]
    #[test]
    fn schnorr_digest_refuses_truncated_entropy_and_partial_words() {
        let mut key = [0u8; 40];
        key[0] = 1;
        let (signer, signing) = schnorr_signer(&key);
        for payload in [vec![], vec![7; 31], vec![7; 32], vec![7; 33], vec![7; 47]] {
            assert_eq!(
                signer.sign_now(&schnorr_request(signing.clone(), payload)),
                Err(SignError::InvalidRequest)
            );
        }
    }

    /// R1: an independent oracle. Signatures from the arm verify against the
    /// public keys the Go signer derived for the go-wasm.json scalars.
    #[cfg(feature = "schnorr")]
    #[test]
    fn schnorr_digest_verifies_against_go_derived_public_keys() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/lighter/go-wasm.json")).unwrap();
        let bytes = |case: &serde_json::Value, field: &str| -> Vec<u8> {
            let text = case[field].as_str().unwrap();
            (0..text.len() / 2)
                .map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap())
                .collect()
        };
        let mut checked = 0;
        for case in fixture["signatures"].as_array().unwrap() {
            let key = bytes(case, "key");
            let input = bytes(case, "payload");
            if input.is_empty() || !crate::lighter::canonical_scalar(&key).unwrap() {
                continue;
            }
            let (signer, signing) = schnorr_signer(&key);
            for entropy in [[0u8; 32], [0xa5; 32]] {
                let mut payload = entropy.to_vec();
                payload.extend_from_slice(&input);
                let result = signer
                    .sign_now(&schnorr_request(signing.clone(), payload))
                    .unwrap();
                let signature = &result.values[0];
                assert!(
                    crate::lighter::schnorr_verify(&input, &bytes(case, "publicKey"), signature)
                        .unwrap()
                );
                assert!(
                    !crate::lighter::schnorr_verify(
                        &bytes(case, "changedPayload"),
                        &bytes(case, "publicKey"),
                        signature
                    )
                    .unwrap()
                );
            }
            checked += 1;
        }
        assert!(
            checked > 0,
            "go-wasm.json must hold canonical signing vectors"
        );
    }

    /// R1: keys are refused, never reduced: zero, the group order and above,
    /// and 39/41-byte keys are refused; order - 1 signs. Legacy reduces a key at or above the
    /// order and signs; the arm's refusal is the recorded behaviour change.
    #[cfg(feature = "schnorr")]
    #[test]
    fn schnorr_digest_refuses_noncanonical_and_zero_keys() {
        use goldilocks_crypto::ScalarField;
        let mut input = [0u8; 40];
        input[0] = 1;
        let mut payload = vec![0x11; 32];
        payload.extend_from_slice(&input);
        let order = ScalarField::N.to_bytes_le();
        let mut short = vec![0u8; 39];
        short[0] = 1;
        let mut long = vec![0u8; 41];
        long[0] = 1;
        for key in [
            [0u8; 40].to_vec(),
            order.to_vec(),
            [0xff; 40].to_vec(),
            short,
            long,
        ] {
            let (signer, signing) = schnorr_signer(&key);
            assert_eq!(
                signer.sign_now(&schnorr_request(signing, payload.clone())),
                Err(SignError::InvalidRequest),
                "{}-byte key",
                key.len()
            );
        }
        for key in [order, [0xff; 40]] {
            let legacy = crate::lighter::hedged_nonce(&input, &key, &[0x11; 32])
                .and_then(|nonce| crate::lighter::schnorr_sign(&input, &key, nonce));
            assert_eq!(legacy.map(|signature| signature.len()), Ok(80));
        }
        let below_order = ScalarField::NEG_ONE.to_bytes_le();
        let (signer, signing) = schnorr_signer(&below_order);
        let result = signer.sign_now(&schnorr_request(signing, payload)).unwrap();
        let public = crate::lighter::public_key(&below_order).unwrap();
        assert!(crate::lighter::schnorr_verify(&input, &public, &result.values[0]).unwrap());
    }

    /// Without the schnorr feature the arm refuses as Unsupported.
    #[cfg(not(feature = "schnorr"))]
    #[test]
    fn schnorr_digest_is_unsupported_without_the_feature() {
        let mut key = [0u8; 40];
        key[0] = 1;
        let (signer, signing) = schnorr_signer(&key);
        let mut payload = vec![0; 32];
        payload.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            signer.sign_now(&schnorr_request(signing, payload)),
            Err(SignError::Unsupported)
        );
    }

    #[test]
    fn secondary_signing_alias_names_the_primary_ref_without_a_second_copy() {
        let hex_policy = |encoding| CredentialSetupPolicy {
            primary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Hex),
                ..Default::default()
            }),
            secondary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                secret_encoding: Some(encoding),
                ..Default::default()
            }),
        };
        let setup = |secondary: serde_json::Value| {
            serde_json::json!({"primary":{"key":"api-key","secret":"0x0102"},"secondary":secondary})
                .to_string()
        };
        let (mut signer, ids) = LocalSigner::from_json_with_policy(
            setup(serde_json::json!({"key":"read-key","signingFromPrimary":true})),
            hex_policy(KeyEncoding::Hex),
        )
        .unwrap();
        let (primary, secondary) = (ids.primary.unwrap(), ids.secondary.unwrap());
        assert_eq!(secondary.signing, primary.signing);
        assert_eq!(secondary.credential, KeyRef("secondary.credential".into()));
        assert!(
            !signer
                .keys
                .contains_key(&KeyRef("secondary.signing".into()))
        );
        assert_eq!(signer.key(&secondary.signing).unwrap(), [1, 2]);
        // One copy, one revocation point for both slots.
        signer.remove(&secondary.signing);
        assert!(signer.key(&primary.signing).is_err());
        for (secondary, policy) in [
            // A declared encoding mismatch is refused, not reinterpreted.
            (
                serde_json::json!({"key":"read-key","signingFromPrimary":true}),
                hex_policy(KeyEncoding::Utf8),
            ),
            // Own material and an alias together are ambiguous.
            (
                serde_json::json!({"key":"read-key","secret":"0x03","signingFromPrimary":true}),
                hex_policy(KeyEncoding::Hex),
            ),
        ] {
            assert!(matches!(
                LocalSigner::from_json_with_policy(setup(secondary), policy),
                Err(SignError::InvalidRequest)
            ));
        }
        // The primary cannot alias, and an alias needs primary signing material.
        for input in [
            r#"{"primary":{"key":"k","secret":"00","signingFromPrimary":true}}"#,
            r#"{"primary":{"key":"k"},"secondary":{"key":"r","signingFromPrimary":true}}"#,
            r#"{"secondary":{"key":"r","signingFromPrimary":true}}"#,
            // A prefix-only hex key decodes to nothing.
            r#"{"primary":{"key":"k","secret":"0x"},"secondary":{"key":"r","signingFromPrimary":true}}"#,
        ] {
            assert!(matches!(
                LocalSigner::from_json_with_policy(input.into(), hex_policy(KeyEncoding::Hex)),
                Err(SignError::InvalidRequest)
            ));
        }
    }

    #[test]
    fn opaque_policy_checks_presence_while_optional_empty_material_stays_explicit() {
        let (signer, mut ids) = LocalSigner::from_json(
            r#"{"primary":{"key":"key","secret":"secret","passphrase":""}}"#.into(),
        )
        .unwrap();
        // An external provider's nonempty ref does not prove nonempty bytes.
        required_passphrase_policy()
            .apply_to_identities(&mut ids, false)
            .unwrap();
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: ids.primary.unwrap().passphrase.unwrap(),
            }],
        };
        assert_eq!(
            signer.sign_now(&request).unwrap().values,
            [Vec::<u8>::new()]
        );
    }

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
            ("hex", format!("0x{}", crate::hex(secret.as_bytes()))),
            (
                "hex",
                format!("0X{}", crate::hex(secret.as_bytes()).to_uppercase()),
            ),
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
                receipt: safe_cex_engine::signing::SignReceipt {
                    stamp_ms: Some(1000.),
                    expiry_ms: Some(2000.),
                },
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
            assert_eq!(result.receipt, request.receipt);
            assert_eq!(
                result.values[0],
                vector["payload"].as_str().unwrap().as_bytes()
            );
            assert_eq!(
                crate::hex(&result.values[1]),
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
            receipt: Default::default(),
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
            result.values,
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
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present {
                key: present.primary.unwrap().passphrase.unwrap(),
            }],
        };
        assert_eq!(
            signer.sign_now(&request).unwrap().values,
            [Vec::<u8>::new()]
        );
    }

    #[test]
    fn malformed_or_oversized_setup_material_has_only_closed_errors() {
        for (encoding, value) in [
            ("base64", "not base64!".to_owned()),
            ("hex", "123".to_owned()),
            ("hex", "0g".to_owned()),
            ("hex", "0x123".to_owned()),
            ("hex", "0x0x00".to_owned()),
            ("hex", "x00".to_owned()),
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
            receipt: Default::default(),
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
        assert_eq!(result.values[0], b"public-identifier");
        assert_eq!(
            crate::hex(&result.values[1]),
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
            receipt: Default::default(),
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
        assert_eq!(result.values[0], digest);
        let mut expected = Sha256::new();
        expected.update(crate::hex(&digest));
        expected.update(signer.key(&key).unwrap());
        assert_eq!(result.values[1], &expected.finalize()[..]);
        assert!(!format!("{request:?}{result:?}").contains("public-identifier"));
    }
    #[test]
    fn failed_batch_never_returns_partial_credentials_or_provider_text() {
        let (signer, key) = fixture();
        let request = SignRequest {
            receipt: Default::default(),
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

    fn zklink_policy() -> CredentialSetupPolicy {
        use safe_cex_engine::signing::IdentitySetupPolicy;
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base64Text),
                extra: Some(safe_cex_engine::signing::ExtraKeyPolicy {
                    key: ExtraKey::ZklinkSeed,
                    field: safe_cex_engine::signing::ExtraFieldPolicy::RequiredForPrivate,
                    encoding: Some(KeyEncoding::Hex),
                }),
                ..Default::default()
            }),
            secondary: None,
        }
    }
    fn zklink_case() -> (String, Vec<u8>, String) {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/zklink/npm-wasm.json")).unwrap();
        let case = &corpus["cases"][0];
        let hex = |value: &str| value.trim_start_matches("0x").to_owned();
        (
            hex(case["seed"].as_str().unwrap()),
            (0..36)
                .map(|i| {
                    u8::from_str_radix(
                        &hex(case["payload"].as_str().unwrap())[i * 2..i * 2 + 2],
                        16,
                    )
                    .unwrap()
                })
                .collect(),
            case["signed"]["signature"]["signature"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
    }
    fn zklink_setup(seed: &str) -> String {
        format!(
            r#"{{"primary":{{"key":"apex-key","secret":"apex-secret","extra":{{"zklinkSeed":{{"value":"{seed}","encoding":"hex"}}}}}}}}"#
        )
    }
    #[cfg(feature = "zklink")]
    fn ids_signing(seed: &str) -> KeyRef {
        LocalSigner::from_json_with_policy(zklink_setup(seed), zklink_policy())
            .unwrap()
            .1
            .primary
            .unwrap()
            .signing
    }
    fn zklink_request(key: KeyRef, payload: Vec<u8>) -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::ZkLinkContract,
                key,
                payload: vec![PayloadPart::Bytes { bytes: payload }],
            }],
        }
    }

    #[test]
    fn declared_base64_text_encoding_matches_independent_node_hmac() {
        // Node: createHmac('sha256', Buffer.from('apex-secret').toString('base64')).
        let (seed, _, _) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let primary = ids.primary.unwrap();
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: primary.signing.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"1700000000000GET/api/v3/account".to_vec(),
                }],
            }],
        };
        assert_eq!(
            crate::hex(&signer.sign_now(&request).unwrap().values[0]),
            "754e7df89fca47eee8f6c2458bb820526604226248620adb40045b6b003d6ad0"
        );
        assert_eq!(signer.key(&primary.signing).unwrap(), b"YXBleC1zZWNyZXQ=");
    }

    #[test]
    fn zklink_extra_ref_is_installed_only_where_declared_and_redacted() {
        let (seed, _, _) = zklink_case();
        let (_, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let primary = ids.primary.unwrap();
        assert_eq!(
            primary.extra.get(&ExtraKey::ZklinkSeed),
            Some(&KeyRef("primary.extra.zklinkSeed".into()))
        );
        assert_eq!(
            primary.stable_id, None,
            "extra material never becomes an identity"
        );
        let serialized = serde_json::to_string(&primary).unwrap();
        assert!(!serialized.contains(&seed[..16]) && serialized.contains("zklinkSeed"));
        // Undeclared, duplicate, unknown and required-but-missing extras refuse.
        assert_eq!(
            LocalSigner::from_json(zklink_setup(&seed)).err(),
            Some(SignError::InvalidRequest)
        );
        let duplicate = zklink_setup(&seed).replace(
            r#""encoding":"hex"}}"#,
            &format!(r#""encoding":"hex"}},"zklinkSeed":{{"value":"{seed}"}}}}"#),
        );
        assert!(LocalSigner::from_json_with_policy(duplicate, zklink_policy()).is_err());
        let unknown = zklink_setup(&seed).replace("zklinkSeed", "l1Wallet");
        assert!(LocalSigner::from_json_with_policy(unknown, zklink_policy()).is_err());
        let missing = r#"{"primary":{"key":"apex-key","secret":"apex-secret"}}"#.to_owned();
        assert_eq!(
            LocalSigner::from_json_with_policy(missing.clone(), zklink_policy()).err(),
            Some(SignError::InvalidRequest)
        );
        assert!(
            LocalSigner::from_json_with_policy_and_mode(missing, zklink_policy(), true).is_ok(),
            "public mode bypasses only the primary requirement"
        );
        let empty = zklink_setup("");
        assert!(LocalSigner::from_json_with_policy(empty, zklink_policy()).is_err());
    }

    #[cfg(feature = "zklink")]
    #[test]
    fn zklink_contract_matches_npm_vector_with_exact_lengths() {
        let (seed, payload, expected) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let key = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
        let request = zklink_request(key.clone(), payload.clone());
        let result = signer.sign_now(&request).unwrap();
        request.validate_result(&result).unwrap();
        assert_eq!(crate::hex(&result.values[0]), expected);
        // The declared seed encoding wins over contrary caller metadata.
        let utf8 = zklink_setup(&seed).replace(r#""encoding":"hex""#, r#""encoding":"utf8""#);
        let (caller, ids) = LocalSigner::from_json_with_policy(utf8, zklink_policy()).unwrap();
        let caller_key = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
        assert_eq!(
            caller
                .sign_now(&zklink_request(caller_key, payload.clone()))
                .unwrap(),
            result
        );
        for bad in [payload[..35].to_vec(), [payload.clone(), vec![0]].concat()] {
            assert_eq!(
                signer.sign_now(&zklink_request(key.clone(), bad)),
                Err(SignError::InvalidRequest)
            );
        }
        let (short, _) =
            LocalSigner::from_json_with_policy(zklink_setup(&"01".repeat(64)), zklink_policy())
                .unwrap();
        assert_eq!(
            short.sign_now(&zklink_request(key.clone(), payload.clone())),
            Err(SignError::InvalidRequest)
        );
        // The seed is never ordinary material, nor is ordinary material a seed.
        let signing = ids_signing(&seed);
        assert_eq!(
            signer.sign_now(&zklink_request(signing, payload.clone())),
            Err(SignError::InvalidRequest)
        );
        for misuse in [
            Operation::Present { key: key.clone() },
            Operation::Sha256 {
                payload: vec![PayloadPart::Credential { key: key.clone() }],
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: key.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: payload.clone(),
                }],
            },
        ] {
            let request = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![misuse],
            };
            assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
        }
    }

    #[cfg(not(feature = "zklink"))]
    #[test]
    fn zklink_contract_is_unsupported_without_the_feature() {
        let (seed, payload, _) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let key = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
        assert_eq!(
            signer.sign_now(&zklink_request(key, payload)),
            Err(SignError::Unsupported)
        );
    }

    #[test]
    fn capability_mirrors_installed_roles_and_compiled_arms_without_signing() {
        let (seed, _, _) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let primary = ids.primary.unwrap();
        let seed_ref = primary.extra[&ExtraKey::ZklinkSeed].clone();
        let zklink = if cfg!(feature = "zklink") {
            Capability::Supported
        } else {
            Capability::Unsupported
        };
        for (key, algorithm, expected) in [
            (
                &primary.signing,
                Algorithm::HmacSha256,
                Capability::Supported,
            ),
            (
                &primary.signing,
                Algorithm::HmacSha512,
                Capability::Supported,
            ),
            (
                &primary.signing,
                Algorithm::Sha256SecretSuffix,
                Capability::Supported,
            ),
            (&seed_ref, Algorithm::ZkLinkContract, zklink),
            // Role binding: neither ref crosses into the other's algorithm.
            (&seed_ref, Algorithm::HmacSha256, Capability::Unsupported),
            (
                &primary.signing,
                Algorithm::ZkLinkContract,
                Capability::Unsupported,
            ),
            (
                &primary.signing,
                Algorithm::Ed25519,
                Capability::Unsupported,
            ),
            (
                &KeyRef("absent".into()),
                Algorithm::HmacSha256,
                Capability::Unsupported,
            ),
        ] {
            assert_eq!(signer.can_sign(key, algorithm), expected, "{algorithm:?}");
        }
    }
}
