//! Normal local provider. Construction consumes credentials before venue setup.
//! This module alone owns key material; engine/venue state receives opaque refs.
use hmac::{Hmac, Mac};
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, Encoding, ExtraKey, Identities, Identity,
    KeyPresence, KeyRef, Operation, PayloadPart, SetupFacts, SignError, SignFuture, SignRequest,
    SignResult, Signer,
};
use sha2::{Digest, Sha256};
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalIdentity {
    /// Omitted or `null` is Absent; `""` is Empty (D3).
    #[serde(default = "Secret::absent", deserialize_with = "Secret::nullable")]
    pub key: Secret,
    /// Never absent: omitted is a present empty secret; `null` is refused.
    #[serde(default = "Secret::empty")]
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
impl Default for LocalIdentity {
    /// An absent key with a present empty secret: material is never absent
    /// except the key, whose presence is the D3 fact.
    fn default() -> Self {
        Self {
            key: Secret::absent(),
            secret: Secret::empty(),
            passphrase: None,
            stable_id: None,
            secret_encoding: KeyEncoding::default(),
            trim_credential: false,
            extra: LocalExtras::default(),
            signing_from_primary: false,
        }
    }
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
/// Setup material. Only this module can build the absent form (a key's
/// presence fact); absent reads as empty bytes.
pub struct Secret(Option<Zeroizing<String>>);
impl Secret {
    fn absent() -> Self {
        Self(None)
    }
    pub fn new(value: String) -> Self {
        Self(Some(Zeroizing::new(value)))
    }
    /// Supplied but empty: the only empty form for a secret or passphrase.
    pub fn empty() -> Self {
        Self::new(String::new())
    }
    fn into_bytes(mut self) -> Vec<u8> {
        self.0
            .as_mut()
            .map(|value| std::mem::take(&mut **value).into_bytes())
            .unwrap_or_default()
    }
    fn is_empty(&self) -> bool {
        self.0.as_deref().is_none_or(|value| value.is_empty())
    }
    /// Classified from the raw borrow with `str::trim`, before any trim policy.
    fn presence(&self) -> KeyPresence {
        match self.0.as_deref() {
            None => KeyPresence::Absent,
            Some(value) if value.is_empty() => KeyPresence::Empty,
            Some(value) if value.trim().is_empty() => KeyPresence::Blank,
            Some(_) => KeyPresence::Nonblank,
        }
    }
    /// The key field alone reads `null` as absent.
    fn nullable<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <Option<String> as serde::Deserialize>::deserialize(deserializer)
            .map(|value| Self(value.map(Zeroizing::new)))
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
        let mut identity = Identity::new(
            KeyRef(if self.key.is_empty() {
                String::new()
            } else {
                format!("{slot}.credential")
            }),
            (!self.secret.is_empty()).then(|| KeyRef(format!("{slot}.signing"))),
        );
        identity.passphrase = self
            .passphrase
            .as_ref()
            .map(|_| KeyRef(format!("{slot}.passphrase")));
        identity.stable_id = self.stable_id.clone();
        identity.extra = self
            .extra
            .iter()
            .filter(|(_, bytes)| !bytes.is_empty())
            .map(|(name, _)| (*name, Self::extra_ref(slot, *name)))
            .collect();
        identity
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
    /// Trusted setup policy's asymmetric declarations; only these refs answer
    /// public operations, so a MAC secret is never fingerprinted.
    algorithms: HashMap<KeyRef, Algorithm>,
    /// Refs an aliased secondary shares with the primary, with the algorithm
    /// setup declared for both slots (if any). A plain `insert` keeps that
    /// algorithm on the new material; only `remove` ends the binding.
    shared: HashMap<KeyRef, Option<Algorithm>>,
}
impl LocalSigner {
    pub fn new() -> Self {
        Self::default()
    }
    /// Consumes bytes; neither this provider nor its keys implement Debug/export.
    /// Replaced material is undeclared: a declaration describes the bytes setup
    /// installed, never later ones. A role ref keeps its role, which is its
    /// declaration, and a ref an aliased slot shares keeps its slots' algorithm;
    /// neither's type changes this way.
    pub fn insert(&mut self, reference: KeyRef, bytes: Vec<u8>) {
        match self.shared.get(&reference) {
            Some(Some(algorithm)) => {
                self.algorithms.insert(reference.clone(), *algorithm);
            }
            _ => {
                self.algorithms.remove(&reference);
            }
        }
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
                    .is_some_and(|id| !id.secret.is_empty())
                    || setup.primary.as_ref().is_none_or(|primary| {
                        primary.secret.is_empty()
                            || setup.secondary.as_ref().map(|id| id.secret_encoding)
                                != Some(primary.secret_encoding)
                    }))
        {
            return Err(SignError::InvalidRequest);
        }
        // Declarations come from trusted policy only, never caller setup, and
        // an aliased secondary signs with the primary's declared ref.
        let declared = |rule: Option<safe_cex_engine::signing::IdentitySetupPolicy>| {
            rule.and_then(|rule| rule.signing_algorithm)
        };
        if [declared(policy.primary), declared(policy.secondary)]
            .into_iter()
            .flatten()
            .any(|algorithm| !asymmetric(algorithm))
            || alias
                && declared(policy.secondary).is_some()
                && declared(policy.secondary) != declared(policy.primary)
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
            let Some(signing) = primary
                .signing_opt()
                .filter(|key| !key.0.is_empty())
                .cloned()
            else {
                return Err(SignError::InvalidRequest);
            };
            secondary.set_signing_ref(Some(signing));
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
        for (identity, rule) in [
            (identities.primary.as_ref(), policy.primary),
            (identities.secondary.as_ref(), policy.secondary),
        ] {
            if let (Some(identity), Some(algorithm)) = (identity, declared(rule))
                && let Some(signing) = identity.signing_opt().filter(|key| !key.0.is_empty())
            {
                signer.algorithms.insert(signing.clone(), algorithm);
            }
        }
        // After declarations, so the shared binding keeps the slots' algorithm.
        if alias
            && let Some(signing) = identities
                .primary
                .as_ref()
                .and_then(|primary| primary.signing_opt())
                .filter(|key| !key.0.is_empty())
        {
            let declared = signer.algorithms.get(signing).copied();
            signer.shared.insert(signing.clone(), declared);
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
    /// Trusted setup plus its non-secret facts (D3): the primary key's raw
    /// presence, read before the setup is consumed.
    pub fn from_setup_with_facts(
        setup: LocalSetup,
        policy: CredentialSetupPolicy,
        public_data_only: bool,
    ) -> Result<(Self, Identities, SetupFacts), SignError> {
        let facts = SetupFacts::new(
            setup
                .primary
                .as_ref()
                .map_or(KeyPresence::Absent, |primary| primary.key.presence()),
        );
        let (signer, identities) =
            Self::from_setup_with_policy_and_mode(setup, policy, public_data_only)?;
        Ok((signer, identities, facts))
    }
    pub fn from_json_with_facts(
        input: String,
        policy: CredentialSetupPolicy,
        public_data_only: bool,
    ) -> Result<(Self, Identities, SetupFacts), SignError> {
        let input = Zeroizing::new(input);
        if input.len() > 4 * 1024 * 1024 {
            return Err(SignError::InvalidRequest);
        }
        let setup: LocalSetup =
            serde_json::from_str(&input).map_err(|_| SignError::InvalidRequest)?;
        Self::from_setup_with_facts(setup, policy, public_data_only)
    }
    pub fn remove(&mut self, reference: &KeyRef) {
        self.keys.remove(reference);
        self.roles.remove(reference);
        self.algorithms.remove(reference);
        self.shared.remove(reference);
    }
    /// Ordinary material declared for exactly this asymmetric algorithm.
    /// A role is its own declaration; ordinary refs need trusted policy's.
    fn declared_key(&self, reference: &KeyRef, algorithm: Algorithm) -> Result<&[u8], SignError> {
        if self.roles.contains_key(reference) {
            return self.algorithm_key(reference, algorithm);
        }
        if self.algorithms.get(reference) != Some(&algorithm) {
            return Err(SignError::InvalidRequest);
        }
        self.key(reference)
    }
    /// Ordinary material only; extra refs require `algorithm_key`.
    fn key(&self, reference: &KeyRef) -> Result<&[u8], SignError> {
        if self.roles.contains_key(reference) {
            return Err(SignError::InvalidRequest);
        }
        self.raw_key(reference)
    }
    /// Signing material, typed before any arm runs. A role ref serves only its
    /// role's algorithm and role-only algorithms never read ordinary material;
    /// an ordinary asymmetric key needs trusted policy's matching declaration,
    /// and a declared key never MACs. Undeclared refs keep MAC behaviour.
    fn sign_key(&self, reference: &KeyRef, algorithm: Algorithm) -> Result<&[u8], SignError> {
        if self.roles.contains_key(reference) || algorithm == Algorithm::ZkLinkContract {
            return self.algorithm_key(reference, algorithm);
        }
        let declared = self.algorithms.get(reference).copied();
        if asymmetric(algorithm) {
            if declared != Some(algorithm) {
                return Err(SignError::InvalidRequest);
            }
        } else if declared.is_some() {
            return Err(SignError::InvalidRequest);
        }
        self.key(reference)
    }
    /// Trusted installation of asymmetric material with its declaration, in one
    /// step. A ref an aliased slot shares is never retyped, because both slots
    /// rely on its declaration; it may rotate within it. Role refs refuse.
    pub fn insert_declared(
        &mut self,
        reference: KeyRef,
        bytes: Vec<u8>,
        algorithm: Algorithm,
    ) -> Result<(), SignError> {
        if !asymmetric(algorithm)
            || self.roles.contains_key(&reference)
            || self
                .shared
                .get(&reference)
                .is_some_and(|expected| *expected != Some(algorithm))
        {
            return Err(SignError::InvalidRequest);
        }
        self.keys.insert(reference.clone(), Zeroizing::new(bytes));
        self.algorithms.insert(reference, algorithm);
        Ok(())
    }
    /// Signing material: a role ref serves only its role's one algorithm, and a
    /// role-only algorithm never reads ordinary material.
    fn algorithm_key(&self, reference: &KeyRef, algorithm: Algorithm) -> Result<&[u8], SignError> {
        match self.roles.get(reference) {
            Some(role) if role.algorithm() == algorithm => self.raw_key(reference),
            Some(_) => Err(SignError::InvalidRequest),
            None if algorithm == Algorithm::ZkLinkContract => Err(SignError::InvalidRequest),
            None => self.key(reference),
        }
    }
    /// A local signer's missing ref never appears later.
    fn raw_key(&self, reference: &KeyRef) -> Result<&[u8], SignError> {
        self.keys
            .get(reference)
            .map(|key| key.as_slice())
            .ok_or(SignError::MissingKey)
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
                        Encoding::EvmHex => {
                            evm_signature(&results.values[*index])?;
                            130
                        }
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
                        Encoding::EvmHex => {
                            let signature = evm_signature(bytes)?;
                            message.extend_from_slice(
                                Zeroizing::new(crate::hex(&signature[..])).as_bytes(),
                            );
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
                Operation::PublicAddress { key } => primitives::secp256k1_address(
                    self.declared_key(key, Algorithm::Secp256k1Digest)?,
                )?,
                Operation::PublicKey { algorithm, key } => {
                    let key = self.declared_key(key, *algorithm)?;
                    match algorithm {
                        Algorithm::Ed25519 => primitives::ed25519_public(key)?,
                        Algorithm::StarkDigest => primitives::stark_public(key)?,
                        _ => return Err(SignError::InvalidRequest),
                    }
                }
                Operation::Sign {
                    algorithm,
                    key,
                    payload,
                } => {
                    let message = self.expand(payload, &result)?;
                    let key = self.sign_key(key, *algorithm)?;
                    match algorithm {
                        Algorithm::HmacSha256 => {
                            let mut mac = Hmac::<Sha256>::new_from_slice(key)
                                .map_err(|_| SignError::InvalidRequest)?;
                            mac.update(&message);
                            mac.finalize().into_bytes().to_vec()
                        }
                        #[cfg(feature = "hmac-sha512")]
                        Algorithm::HmacSha512 => {
                            let mut mac = Hmac::<sha2::Sha512>::new_from_slice(key)
                                .map_err(|_| SignError::InvalidRequest)?;
                            mac.update(&message);
                            mac.finalize().into_bytes().to_vec()
                        }
                        #[cfg(not(feature = "hmac-sha512"))]
                        Algorithm::HmacSha512 => return Err(SignError::Unsupported),
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
/// Semantic check of one provider's public answer at the trusted setup
/// boundary; widths alone admit off-curve and non-canonical values.
pub fn valid_public_value(operation: &Operation, value: &[u8]) -> bool {
    match operation {
        Operation::PublicAddress { .. } => value.len() == 20,
        Operation::PublicKey {
            algorithm: Algorithm::Ed25519,
            ..
        } => <[u8; 32]>::try_from(value)
            .ok()
            // Decoding is ZIP-215 tolerant and keeps the input bytes, so the
            // canonical y < p check is explicit.
            .filter(ed25519_canonical_y)
            .and_then(|bytes| ed25519_dalek::VerifyingKey::from_bytes(&bytes).ok())
            .is_some_and(|key| !key.is_weak()),
        Operation::PublicKey {
            algorithm: Algorithm::StarkDigest,
            ..
        } => crate::starknet::is_public_key(value),
        _ => false,
    }
}
/// `r || s || recovery` (0 or 1) as `r || s || v` with `v` = 27 or 28.
fn evm_signature(bytes: &[u8]) -> Result<Zeroizing<[u8; 65]>, SignError> {
    let [signature @ .., recovery] = bytes else {
        return Err(SignError::InvalidRequest);
    };
    if signature.len() != 64 || *recovery > 1 {
        return Err(SignError::InvalidRequest);
    }
    let mut out = Zeroizing::new([0u8; 65]);
    out[..64].copy_from_slice(signature);
    out[64] = 27 + recovery;
    Ok(out)
}

/// Little-endian y (sign bit cleared) strictly below p = 2^255 - 19.
fn ed25519_canonical_y(bytes: &[u8; 32]) -> bool {
    let mut y = *bytes;
    y[31] &= 0x7f;
    let mut modulus = [0xff; 32];
    modulus[0] = 0xed;
    modulus[31] = 0x7f;
    y.iter().rev().lt(modulus.iter().rev())
}
/// Asymmetric algorithms: a key must be declared for exactly one of them, and
/// MAC secrets never are. Only some have a public form (`public_key_len`).
fn asymmetric(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::Ed25519
            | Algorithm::Es256
            | Algorithm::Secp256k1Digest
            | Algorithm::StarkDigest
            | Algorithm::SchnorrDigest
    )
}
/// Mirrors the `sign_now` arms; keep the two in step.
fn implemented(algorithm: Algorithm) -> bool {
    match algorithm {
        Algorithm::HmacSha256 | Algorithm::Sha256SecretSuffix => true,
        Algorithm::HmacSha512 => cfg!(feature = "hmac-sha512"),
        Algorithm::ZkLinkContract => cfg!(feature = "zklink"),
        Algorithm::Ed25519
        | Algorithm::Es256
        | Algorithm::Secp256k1Digest
        | Algorithm::StarkDigest
        | Algorithm::SchnorrDigest => false,
    }
}

#[cfg(feature = "address-nonce")]
struct LocalAddressSetup {
    address: Option<Result<[u8; 20], SignError>>,
    result: Option<Result<SignResult, SignError>>,
    started: bool,
}
#[cfg(feature = "address-nonce")]
impl safe_cex_engine::signing::OwnedSetupExecution for LocalAddressSetup {
    fn start(&mut self) -> Result<(), SignError> {
        if self.started {
            return Err(SignError::InvalidRequest);
        }
        self.started = true;
        self.result = Some(
            self.address
                .take()
                .ok_or(SignError::InvalidRequest)?
                .map(|value| SignResult {
                    receipt: Default::default(),
                    values: vec![value.to_vec()],
                    headers: Vec::new(),
                }),
        );
        Ok(())
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.address = None;
        self.started = true;
        Ok(())
    }
    fn poll_result(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<SignResult, SignError>> {
        if self.started {
            self.result
                .take()
                .map_or(std::task::Poll::Pending, std::task::Poll::Ready)
        } else {
            std::task::Poll::Pending
        }
    }
    fn poll_joined(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), SignError>> {
        if self.started {
            std::task::Poll::Ready(Ok(()))
        } else {
            std::task::Poll::Pending
        }
    }
}

impl Signer for LocalSigner {
    fn owned_public_address_capability(&self) -> Capability {
        if cfg!(feature = "address-nonce") {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }
    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn safe_cex_engine::signing::OwnedSetupExecution>> {
        #[cfg(not(feature = "address-nonce"))]
        {
            let _ = request;
            None
        }
        #[cfg(feature = "address-nonce")]
        {
            let [Operation::PublicAddress { key }] = request.operations.as_slice() else {
                return None;
            };
            if !cfg!(feature = "secp256k1")
                || request.receipt != Default::default()
                || request.context != Default::default()
                || self.declared_key(key, Algorithm::Secp256k1Digest).is_err()
            {
                return None;
            }
            Some(Box::new(LocalAddressSetup {
                address: Some(
                    primitives::secp256k1_address(
                        self.declared_key(key, Algorithm::Secp256k1Digest).ok()?,
                    )
                    .and_then(|bytes| bytes.try_into().map_err(|_| SignError::InvalidResult)),
                ),
                result: None,
                started: false,
            }))
        }
    }

    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        let usable = self.sign_key(key, algorithm).is_ok();
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

    /// D3 condition 2: secret and passphrase are never absent material.
    /// Omitted secret is present-empty; `null` secret is refused; a `null`
    /// passphrase is no passphrase, never a present-absent one.
    #[test]
    fn d3_secret_and_passphrase_are_never_absent() {
        assert!(LocalIdentity::default().secret.0.is_some());
        assert!(LocalIdentity::default().key.0.is_none());
        let omitted: LocalIdentity = serde_json::from_str(r#"{"key":"k"}"#).unwrap();
        assert!(omitted.secret.0.is_some());
        assert!(serde_json::from_str::<LocalIdentity>(r#"{"key":"k","secret":null}"#).is_err());
        let none: LocalIdentity = serde_json::from_str(r#"{"key":"k","passphrase":null}"#).unwrap();
        assert!(none.passphrase.is_none());
        let empty: LocalIdentity = serde_json::from_str(r#"{"key":"k","passphrase":""}"#).unwrap();
        assert!(empty.passphrase.unwrap().0.is_some());
    }

    /// D3 condition 1: absent material is never installed, so no signer can
    /// read bytes from it; every signing path refuses before byte access.
    #[test]
    fn d3_absent_material_is_never_installed_or_read() {
        let (signer, ids) = LocalSigner::from_json_with_policy_and_mode(
            r#"{"primary":{}}"#.into(),
            CredentialSetupPolicy::default(),
            false,
        )
        .unwrap();
        assert!(
            signer.keys.is_empty(),
            "nothing installed from absent material"
        );
        assert_eq!(
            ids.primary.unwrap().signing_ref(),
            Err(SignError::MissingKey)
        );
        for reference in ["primary.credential", "primary.signing"] {
            let request = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![Operation::Present {
                    key: KeyRef(reference.into()),
                }],
            };
            assert!(signer.sign_now(&request).is_err(), "{reference}");
            assert!(
                signer.key(&KeyRef(reference.into())).is_err(),
                "{reference}"
            );
        }
    }

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
                        key: identity.passphrase.clone().unwrap(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: identity.signing_ref().unwrap().clone(),
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
        assert!(primary.credential.0.is_empty() && primary.signing_opt().is_none());
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
            Err(SignError::MissingKey)
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
        let (signer, ids) = try_public_setup(
            serde_json::json!({"primary":{"signingAlgorithm":"schnorrDigest"}}),
            serde_json::json!({"primary":{"key":"lighter-api-key","secret":format!("0x{key}"),"secretEncoding":"hex"}}),
        )
        .unwrap();
        let signing = ids.primary.unwrap().signing_ref().unwrap().clone();
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
        let (signer, ids) = try_public_setup(
            serde_json::json!({"primary":{"signingAlgorithm":"schnorrDigest"}}),
            serde_json::json!({"primary":{"key":"k","secret":hex,"secretEncoding":"hex"}}),
        )
        .unwrap();
        (signer, ids.primary.unwrap().signing_ref().unwrap().clone())
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
        assert_eq!(
            secondary.signing_ref().unwrap().clone(),
            primary.signing_ref().unwrap().clone()
        );
        assert_eq!(secondary.credential, KeyRef("secondary.credential".into()));
        assert!(
            !signer
                .keys
                .contains_key(&KeyRef("secondary.signing".into()))
        );
        assert_eq!(
            signer.key(secondary.signing_ref().unwrap()).unwrap(),
            [1, 2]
        );
        // One copy, one revocation point for both slots.
        signer.remove(secondary.signing_ref().unwrap());
        assert!(signer.key(primary.signing_ref().unwrap()).is_err());
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
            ("base58", bs58::encode(secret.as_bytes()).into_string()),
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
                    nonce: None,
                    stamp_ms: Some(1000.),
                    expiry_ms: Some(2000.),
                },
                context: Default::default(),
                operations: vec![
                    Operation::Present {
                        key: identity.passphrase.clone().unwrap(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: identity.signing_ref().unwrap().clone(),
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
            // Base58 excludes 0, O, I and l; an empty decode is no key.
            ("base58", "0OIl".to_owned()),
            ("base58", "".to_owned()),
            (
                "base58",
                "ed25519:11111111111111111111111111111111".to_owned(),
            ),
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
        assert_eq!(signer.sign_now(&request), Err(SignError::MissingKey));
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
    fn evm_hex_renders_recovery_as_27_or_28_and_refuses_other_shapes() {
        let mut raw = [7u8; 65];
        for (recovery, v) in [(0u8, 27u8), (1, 28)] {
            raw[64] = recovery;
            let signature = evm_signature(&raw).unwrap();
            assert_eq!(&signature[..64], &raw[..64]);
            assert_eq!(signature[64], v);
        }
        raw[64] = 2;
        assert_eq!(evm_signature(&raw).err(), Some(SignError::InvalidRequest));
        assert_eq!(
            evm_signature(&raw[..64]).err(),
            Some(SignError::InvalidRequest)
        );
        assert_eq!(evm_signature(&[]).err(), Some(SignError::InvalidRequest));
    }
    #[test]
    fn base64url_keys_decode_either_alphabet_with_or_without_padding() {
        let decode = |text: &str| {
            key_material::decode(Secret::new(text.into()), KeyEncoding::Base64Url)
                .map(|bytes| bytes.to_vec())
        };
        assert_eq!(decode("c2VjcmV0").unwrap(), b"secret");
        assert_eq!(decode("c2VjcmV0Cg").unwrap(), b"secret\n");
        assert_eq!(decode("c2VjcmV0Cg==").unwrap(), b"secret\n");
        assert_eq!(decode("-_-_").unwrap(), [0xfb, 0xff, 0xbf]);
        assert_eq!(decode("+/+/").unwrap(), [0xfb, 0xff, 0xbf]);
        assert!(decode("c2V*").is_err());
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
            .signing_ref()
            .unwrap()
            .clone()
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
                key: primary.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"1700000000000GET/api/v3/account".to_vec(),
                }],
            }],
        };
        assert_eq!(
            crate::hex(&signer.sign_now(&request).unwrap().values[0]),
            "754e7df89fca47eee8f6c2458bb820526604226248620adb40045b6b003d6ad0"
        );
        assert_eq!(
            signer.key(primary.signing_ref().unwrap()).unwrap(),
            b"YXBleC1zZWNyZXQ="
        );
    }

    fn sha512_request(key: KeyRef) -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key,
                payload: vec![PayloadPart::Bytes {
                    bytes: b"1700000000000GET/api/v3/account".to_vec(),
                }],
            }],
        }
    }

    #[cfg(feature = "hmac-sha512")]
    #[test]
    fn hmac_sha512_matches_an_independent_vector_with_its_arm() {
        // Python: hmac.new(b'YXBleC1zZWNyZXQ=', payload, hashlib.sha512).hexdigest().
        let (seed, _, _) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let request = sha512_request(ids.primary.unwrap().signing_ref().unwrap().clone());
        assert_eq!(
            crate::hex(&signer.sign_now(&request).unwrap().values[0]),
            "689ec455eb6b1b55d05a2a3662bfe2faabed0c3445180e4efa855216f53f7c5d\
             4834312e08472f0e16d0aadb826ca0819b10cba7eac09f7dfbb8c4fd7eb57104"
        );
    }

    #[cfg(not(feature = "hmac-sha512"))]
    #[test]
    fn hmac_sha512_is_unsupported_without_its_arm() {
        let (seed, _, _) = zklink_case();
        let (signer, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let request = sha512_request(ids.primary.unwrap().signing_ref().unwrap().clone());
        assert_eq!(signer.sign_now(&request), Err(SignError::Unsupported));
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
        let unknown = zklink_setup(&seed).replace("zklinkSeed", "unknownRole");
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
        let sha512 = if cfg!(feature = "hmac-sha512") {
            Capability::Supported
        } else {
            Capability::Unsupported
        };
        for (key, algorithm, expected) in [
            (
                primary.signing_ref().unwrap(),
                Algorithm::HmacSha256,
                Capability::Supported,
            ),
            (
                primary.signing_ref().unwrap(),
                Algorithm::HmacSha512,
                sha512,
            ),
            (
                primary.signing_ref().unwrap(),
                Algorithm::Sha256SecretSuffix,
                Capability::Supported,
            ),
            (&seed_ref, Algorithm::ZkLinkContract, zklink),
            // Role binding: neither ref crosses into the other's algorithm.
            (&seed_ref, Algorithm::HmacSha256, Capability::Unsupported),
            (
                primary.signing_ref().unwrap(),
                Algorithm::ZkLinkContract,
                Capability::Unsupported,
            ),
            (
                primary.signing_ref().unwrap(),
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

    // P4 REDs: public derivations reach the provider through the existing setup
    // policy and request decode; vectors are the pinned ethers/Node/starknet.js ones.
    fn try_public_setup(
        policy: serde_json::Value,
        setup: serde_json::Value,
    ) -> Result<(LocalSigner, Identities), SignError> {
        let policy: CredentialSetupPolicy = serde_json::from_value(policy).unwrap();
        LocalSigner::from_json_with_policy(setup.to_string(), policy)
    }
    fn public_setup(algorithm: &str, secret: &str) -> (LocalSigner, KeyRef) {
        let (signer, identities) = try_public_setup(
            serde_json::json!({"primary": {"secretEncoding": "hex", "signingAlgorithm": algorithm}}),
            serde_json::json!({"primary": {"key": "account", "secret": secret}}),
        )
        .unwrap();
        (
            signer,
            identities.primary.unwrap().signing_ref().unwrap().clone(),
        )
    }
    /// Arbitrary in-range scalar for refusal paths; never a vector or real key.
    const SCALAR: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn public_declarations_are_trusted_policy_only_asymmetric_and_exact() {
        let setup = serde_json::json!({"primary": {"key": "account", "secret": SCALAR}});
        // A MAC algorithm can never be declared, so no secret is fingerprinted.
        let mac = serde_json::json!({"primary": {"secretEncoding": "hex", "signingAlgorithm": "hmacSha256"}});
        assert!(matches!(
            try_public_setup(mac, setup.clone()),
            Err(SignError::InvalidRequest)
        ));
        // An aliased secondary signs with the primary's ref and declaration.
        let alias = serde_json::json!({
            "primary": {"key": "account", "secret": SCALAR},
            "secondary": {"key": "read", "signingFromPrimary": true}
        });
        let conflicting = serde_json::json!({
            "primary": {"secretEncoding": "hex", "signingAlgorithm": "secp256k1Digest"},
            "secondary": {"secretEncoding": "hex", "signingAlgorithm": "ed25519"}
        });
        assert!(matches!(
            try_public_setup(conflicting, alias.clone()),
            Err(SignError::InvalidRequest)
        ));
        let (_, ids) = try_public_setup(
            serde_json::json!({
                "primary": {"secretEncoding": "hex", "signingAlgorithm": "secp256k1Digest"},
                "secondary": {"secretEncoding": "hex"}
            }),
            alias,
        )
        .unwrap();
        assert_eq!(
            ids.secondary.unwrap().signing_ref().unwrap().clone(),
            ids.primary.unwrap().signing_ref().unwrap().clone()
        );
        // Undeclared material answers nothing, whatever its shape.
        let (undeclared, ids) = try_public_setup(
            serde_json::json!({"primary": {"secretEncoding": "hex"}}),
            setup,
        )
        .unwrap();
        let key = ids.primary.unwrap().signing_ref().unwrap().clone();
        let request = public_request(serde_json::json!([{"kind": "publicAddress", "key": key}]));
        assert_eq!(
            undeclared.sign_now(&request),
            Err(SignError::InvalidRequest)
        );
        // A declared ref answers only its own algorithm; other public forms are
        // refused by request validation before any provider work.
        let (signer, key) = public_setup("secp256k1Digest", SCALAR);
        for operation in [
            serde_json::json!({"kind": "publicKey", "algorithm": "ed25519", "key": key}),
            serde_json::json!({"kind": "publicKey", "algorithm": "starkDigest", "key": key}),
            serde_json::json!({"kind": "publicKey", "algorithm": "secp256k1Digest", "key": key}),
            serde_json::json!({"kind": "publicKey", "algorithm": "es256", "key": key}),
            serde_json::json!({"kind": "publicKey", "algorithm": "hmacSha256", "key": key}),
            serde_json::json!({"kind": "publicAddress", "key": "primary.credential"}),
            serde_json::json!({"kind": "publicAddress", "key": "secondary.signing"}),
        ] {
            let request = public_request(serde_json::json!([operation]));
            assert_eq!(
                signer.sign_now(&request),
                Err(SignError::InvalidRequest),
                "{operation}"
            );
        }
        // Caller setup can never carry a declaration.
        let caller = serde_json::json!({"primary": {
            "key": "account", "secret": SCALAR, "signingAlgorithm": "secp256k1Digest"
        }});
        assert!(matches!(
            LocalSigner::from_json(caller.to_string()),
            Err(SignError::InvalidRequest)
        ));
    }

    #[test]
    fn replaced_or_reinserted_material_loses_its_declaration() {
        let (mut signer, key) = public_setup("secp256k1Digest", SCALAR);
        let request = public_request(serde_json::json!([{"kind": "publicAddress", "key": key}]));
        signer.insert(key.clone(), b"mac secret".to_vec());
        assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
        signer.remove(&key);
        assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
        signer.insert(key.clone(), vec![0x11; 32]);
        assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
    }

    #[cfg(feature = "stark")]
    #[test]
    fn stark_public_keys_refuse_zero_and_out_of_order_keys() {
        for key in [
            "0".repeat(64),
            "0800000000000010ffffffffffffffffb781126dcae7b2321e66a241adc64d2f".into(),
            "f".repeat(64),
        ] {
            let (signer, reference) = public_setup("starkDigest", &key);
            let request = public_request(serde_json::json!([
                {"kind": "publicKey", "algorithm": "starkDigest", "key": reference}
            ]));
            assert_eq!(
                signer.sign_now(&request),
                Err(SignError::InvalidRequest),
                "{key}"
            );
        }
    }

    #[cfg(not(feature = "secp256k1"))]
    #[test]
    fn public_address_without_its_arm_is_unsupported() {
        let (signer, key) = public_setup("secp256k1Digest", SCALAR);
        let request = public_request(serde_json::json!([{"kind": "publicAddress", "key": key}]));
        assert_eq!(signer.sign_now(&request), Err(SignError::Unsupported));
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn public_operations_keep_batch_order_beside_present_and_sign() {
        let (signer, key) = public_setup("secp256k1Digest", SCALAR);
        let digest = vec![7u8; 32];
        let request = public_request(serde_json::json!([
            {"kind": "present", "key": "primary.credential"},
            {"kind": "publicAddress", "key": key},
            {"kind": "sign", "algorithm": "secp256k1Digest", "key": key,
             "payload": [{"kind": "bytes", "bytes": digest}]},
        ]));
        let result = signer.sign_now(&request).unwrap();
        assert_eq!(result.values[0], b"account");
        let address = crate::ethereum::private_key_address(SCALAR).unwrap();
        assert_eq!(
            format!("0x{}", crate::hex(&result.values[1])),
            address.to_ascii_lowercase()
        );
        assert_eq!(result.values[2].len(), 65);
        request.validate_result(&result).unwrap();
    }
    fn public_request(operations: serde_json::Value) -> SignRequest {
        serde_json::from_value(serde_json::json!({
            "receipt": {}, "context": {"venue": "", "method": "", "host": "", "path": ""},
            "operations": operations,
        }))
        .unwrap()
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn public_address_of_a_declared_secp256k1_ref_matches_ethers() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ethereum/primitives.json"))
                .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let secret = case["privateKey"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x");
            let (signer, key) = public_setup("secp256k1Digest", secret);
            let request =
                public_request(serde_json::json!([{"kind": "publicAddress", "key": key}]));
            let result = signer.sign_now(&request).unwrap();
            let expected = case["recovered"].as_str().unwrap().to_ascii_lowercase();
            assert_eq!(format!("0x{}", crate::hex(&result.values[0])), expected);
        }
    }

    #[cfg(all(feature = "ed25519", feature = "stark"))]
    #[test]
    fn public_key_of_declared_ed25519_and_stark_refs_matches_independent_vectors() {
        let node: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ed25519/node.json")).unwrap();
        let stark: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/sdk.json")).unwrap();
        let cases = node["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| ("ed25519", "seed", case))
            .chain(
                stark["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|case| ("starkDigest", "key", case)),
            );
        for (algorithm, field, case) in cases {
            let secret = case[field].as_str().unwrap().trim_start_matches("0x");
            let secret = format!("{secret:0>64}");
            let (signer, key) = public_setup(algorithm, &secret);
            let request = public_request(serde_json::json!([
                {"kind": "publicKey", "algorithm": algorithm, "key": key}
            ]));
            let result = signer.sign_now(&request).unwrap();
            let expected = case["publicKey"].as_str().unwrap().trim_start_matches("0x");
            assert_eq!(
                crate::hex(&result.values[0]),
                format!("{expected:0>64}"),
                "{algorithm}"
            );
        }
    }

    #[test]
    fn public_operations_never_fingerprint_hmac_undeclared_or_role_refs() {
        let (hmac, key) = fixture();
        // A declared slot's other refs stay undeclared: passphrase and role.
        let (seed, _, _) = zklink_case();
        let mut policy = zklink_policy();
        policy.primary.as_mut().unwrap().signing_algorithm = Some(Algorithm::Secp256k1Digest);
        policy.primary.as_mut().unwrap().secret_encoding = Some(KeyEncoding::Hex);
        let setup = serde_json::json!({"primary": {
            "key": "account", "secret": SCALAR, "passphrase": "pass",
            "extra": {"zklinkSeed": {"value": seed, "encoding": "hex"}}
        }});
        let (declared, ids) =
            LocalSigner::from_json_with_policy(setup.to_string(), policy).unwrap();
        let ids = ids.primary.unwrap();
        let role = ids.extra[&ExtraKey::ZklinkSeed].clone();
        for (signer, key) in [
            (&hmac, key),
            (&declared, role),
            (&declared, ids.passphrase.unwrap()),
            (&declared, ids.credential),
        ] {
            for operation in [
                serde_json::json!({"kind": "publicAddress", "key": key}),
                serde_json::json!({"kind": "publicKey", "algorithm": "ed25519", "key": key}),
                serde_json::json!({"kind": "publicKey", "algorithm": "starkDigest", "key": key}),
            ] {
                let request = public_request(serde_json::json!([operation]));
                assert_eq!(
                    signer.sign_now(&request),
                    Err(SignError::InvalidRequest),
                    "{operation}"
                );
            }
        }
    }

    // P6b REDs: the l1Wallet role reaches the provider through the existing
    // policy and setup decode; its one algorithm is Secp256k1Digest.
    fn l1_wallet_setup(wallet: &str) -> (LocalSigner, serde_json::Value) {
        let policy: CredentialSetupPolicy = serde_json::from_value(serde_json::json!({
            "primary": {"extra": {"key": "l1Wallet", "field": "requiredForPrivate", "encoding": "hex"}}
        }))
        .unwrap();
        let setup = serde_json::json!({"primary": {
            "key": "account", "secret": "l2", "extra": {"l1Wallet": {"value": wallet, "encoding": "hex"}}
        }});
        let (signer, ids) = LocalSigner::from_json_with_policy(setup.to_string(), policy).unwrap();
        let role = serde_json::to_value(ids.primary.unwrap()).unwrap()["extra"]["l1Wallet"].clone();
        assert!(role.is_string());
        (signer, role)
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn l1_wallet_role_answers_its_ethers_address() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ethereum/primitives.json"))
                .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let wallet = case["privateKey"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x");
            let (signer, role) = l1_wallet_setup(wallet);
            let request =
                public_request(serde_json::json!([{"kind": "publicAddress", "key": role}]));
            let result = signer.sign_now(&request).unwrap();
            let expected = case["recovered"].as_str().unwrap().to_ascii_lowercase();
            assert_eq!(format!("0x{}", crate::hex(&result.values[0])), expected);
        }
    }

    #[test]
    fn l1_wallet_role_is_refused_outside_its_algorithm() {
        let (signer, role) = l1_wallet_setup(SCALAR);
        for operation in [
            serde_json::json!({"kind": "present", "key": role}),
            serde_json::json!({"kind": "sign", "algorithm": "hmacSha256", "key": "primary.signing",
                "payload": [{"kind": "credential", "key": role}]}),
            serde_json::json!({"kind": "sign", "algorithm": "hmacSha256", "key": role, "payload": []}),
            serde_json::json!({"kind": "sign", "algorithm": "zkLinkContract", "key": role,
                "payload": [{"kind": "bytes", "bytes": vec![0u8; 36]}]}),
            serde_json::json!({"kind": "publicKey", "algorithm": "ed25519", "key": role}),
            serde_json::json!({"kind": "publicKey", "algorithm": "starkDigest", "key": role}),
        ] {
            let request = public_request(serde_json::json!([operation]));
            assert_eq!(
                signer.sign_now(&request),
                Err(SignError::InvalidRequest),
                "{operation}"
            );
        }
        // The zkLink role answers no public operation and signs nothing else.
        let (seed, _, _) = zklink_case();
        let (zklink, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let seed_ref = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
        for operation in [
            serde_json::json!({"kind": "publicAddress", "key": seed_ref}),
            serde_json::json!({"kind": "sign", "algorithm": "secp256k1Digest", "key": seed_ref,
                "payload": [{"kind": "bytes", "bytes": vec![7u8; 32]}]}),
        ] {
            let request = public_request(serde_json::json!([operation]));
            assert_eq!(
                zklink.sign_now(&request),
                Err(SignError::InvalidRequest),
                "{operation}"
            );
        }
        // The L1 wallet belongs to the primary identity only.
        let policy: CredentialSetupPolicy = serde_json::from_value(serde_json::json!({
            "secondary": {"extra": {"key": "l1Wallet", "encoding": "hex"}}
        }))
        .unwrap();
        let setup = serde_json::json!({"secondary": {
            "key": "read", "secret": "l2", "extra": {"l1Wallet": {"value": SCALAR, "encoding": "hex"}}
        }});
        assert!(matches!(
            LocalSigner::from_json_with_policy(setup.to_string(), policy),
            Err(SignError::InvalidRequest)
        ));
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn l1_wallet_signs_digests_that_recover_to_its_own_address() {
        let (signer, role) = l1_wallet_setup(SCALAR);
        let digest = vec![9u8; 32];
        let request = public_request(serde_json::json!([
            {"kind": "publicAddress", "key": role},
            {"kind": "sign", "algorithm": "secp256k1Digest", "key": role,
             "payload": [{"kind": "bytes", "bytes": digest}]},
        ]));
        let result = signer.sign_now(&request).unwrap();
        let address = format!("0x{}", crate::hex(&result.values[0]));
        assert!(crate::ethereum::signed_by(
            &digest,
            &result.values[1],
            &address
        ));
        assert_eq!(
            address,
            crate::ethereum::private_key_address(SCALAR)
                .unwrap()
                .to_ascii_lowercase()
        );
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn l1_wallet_material_is_refused_at_use_and_required_where_declared() {
        // Zero, the curve order and a short scalar answer nothing.
        for wallet in [
            "0".repeat(64),
            "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141".into(),
            "11".repeat(31),
        ] {
            let (signer, role) = l1_wallet_setup(&wallet);
            for operation in [
                serde_json::json!({"kind": "publicAddress", "key": role}),
                serde_json::json!({"kind": "sign", "algorithm": "secp256k1Digest", "key": role,
                    "payload": [{"kind": "bytes", "bytes": vec![9u8; 32]}]}),
            ] {
                let request = public_request(serde_json::json!([operation]));
                assert_eq!(
                    signer.sign_now(&request),
                    Err(SignError::InvalidRequest),
                    "{wallet}"
                );
            }
        }
        let policy = || -> CredentialSetupPolicy {
            serde_json::from_value(serde_json::json!({
                "primary": {"extra": {"key": "l1Wallet", "field": "requiredForPrivate", "encoding": "hex"}}
            }))
            .unwrap()
        };
        // A complete private identity requires the wallet; empty is absent.
        for setup in [
            serde_json::json!({"primary": {"key": "account", "secret": "l2"}}),
            serde_json::json!({"primary": {"key": "account", "secret": "l2",
                "extra": {"l1Wallet": {"value": "", "encoding": "hex"}}}}),
        ] {
            assert!(matches!(
                LocalSigner::from_json_with_policy(setup.to_string(), policy()),
                Err(SignError::InvalidRequest)
            ));
        }
        // Pre-import, the wallet alone is the whole account: no L2 fields.
        let wallet_only = serde_json::json!({"primary": {
            "extra": {"l1Wallet": {"value": SCALAR, "encoding": "hex"}}
        }});
        let (signer, ids) =
            LocalSigner::from_json_with_policy(wallet_only.to_string(), policy()).unwrap();
        let ids = ids.primary.unwrap();
        assert!(ids.credential.0.is_empty() && ids.signing_opt().is_none());
        let role = ids.extra[&ExtraKey::L1Wallet].clone();
        let request = public_request(serde_json::json!([{"kind": "publicAddress", "key": role}]));
        assert_eq!(signer.sign_now(&request).unwrap().values[0].len(), 20);
    }

    #[cfg(not(feature = "secp256k1"))]
    #[test]
    fn l1_wallet_address_without_its_arm_is_unsupported() {
        let (signer, role) = l1_wallet_setup(SCALAR);
        let request = public_request(serde_json::json!([{"kind": "publicAddress", "key": role}]));
        assert_eq!(signer.sign_now(&request), Err(SignError::Unsupported));
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn insert_keeps_a_role_and_remove_ends_it() {
        let (mut signer, role) = l1_wallet_setup(SCALAR);
        let role: KeyRef = serde_json::from_value(role).unwrap();
        let address = |signer: &LocalSigner| {
            signer.sign_now(&public_request(
                serde_json::json!([{"kind": "publicAddress", "key": role}]),
            ))
        };
        let present = |signer: &LocalSigner| {
            signer.sign_now(&public_request(
                serde_json::json!([{"kind": "present", "key": role}]),
            ))
        };
        // Replacing role material keeps the role: still never presented.
        let other = "22".repeat(32);
        signer.insert(role.clone(), (0..32).map(|_| 0x22).collect());
        let expected = crate::ethereum::private_key_address(&other)
            .unwrap()
            .to_ascii_lowercase();
        assert_eq!(
            format!("0x{}", crate::hex(&address(&signer).unwrap().values[0])),
            expected
        );
        assert_eq!(present(&signer), Err(SignError::InvalidRequest));
        // Removal ends the role; reinsertion is ordinary, undeclared material.
        signer.remove(&role);
        assert_eq!(address(&signer), Err(SignError::InvalidRequest));
        signer.insert(role.clone(), (0..32).map(|_| 0x22).collect());
        assert_eq!(address(&signer), Err(SignError::InvalidRequest));
        assert!(present(&signer).is_ok());
    }

    // Asymmetric key typing REDs: Sign obeys the same declaration as public
    // operations, in both directions.
    fn sign_request(algorithm: &str, key: &KeyRef, bytes: Vec<u8>) -> SignRequest {
        public_request(
            serde_json::json!([{"kind": "sign", "algorithm": algorithm, "key": key,
            "payload": [{"kind": "bytes", "bytes": bytes}]}]),
        )
    }

    fn undeclared_mac_secret() -> (LocalSigner, KeyRef) {
        // A 32-byte HMAC secret parses as a valid secp256k1 and Ed25519 key.
        let (signer, ids) = LocalSigner::from_json(
            serde_json::json!({"primary": {"key": "account", "secret": "s".repeat(32)}})
                .to_string(),
        )
        .unwrap();
        (signer, ids.primary.unwrap().signing_ref().unwrap().clone())
    }
    // One test per algorithm, so every variant runs. The declaration check
    // precedes the arm, so these refuse the same way in every build.
    fn assert_undeclared_refused(algorithm: &str) {
        let (signer, key) = undeclared_mac_secret();
        assert_eq!(
            signer.sign_now(&sign_request(algorithm, &key, vec![7; 32])),
            Err(SignError::InvalidRequest),
            "{algorithm}"
        );
    }
    #[test]
    fn undeclared_ref_never_signs_secp256k1_digest() {
        assert_undeclared_refused("secp256k1Digest");
    }
    #[test]
    fn undeclared_ref_never_signs_ed25519() {
        assert_undeclared_refused("ed25519");
    }
    #[test]
    fn undeclared_ref_never_signs_es256() {
        assert_undeclared_refused("es256");
    }
    #[test]
    fn undeclared_ref_never_signs_stark_digest() {
        assert_undeclared_refused("starkDigest");
    }
    #[test]
    fn undeclared_ref_never_signs_schnorr_digest() {
        assert_undeclared_refused("schnorrDigest");
    }
    #[test]
    fn undeclared_mac_secret_still_macs() {
        let (signer, key) = undeclared_mac_secret();
        let result = signer
            .sign_now(&sign_request("hmacSha256", &key, b"m".to_vec()))
            .unwrap();
        assert_eq!(result.values[0].len(), 32);
    }

    fn assert_declared_wallet_never_macs(algorithm: &str) {
        let (signer, key) = public_setup("secp256k1Digest", SCALAR);
        assert_eq!(
            signer.sign_now(&sign_request(algorithm, &key, b"m".to_vec())),
            Err(SignError::InvalidRequest),
            "{algorithm}"
        );
    }
    #[test]
    fn declared_wallet_never_signs_hmac_sha256() {
        assert_declared_wallet_never_macs("hmacSha256");
    }
    #[test]
    fn declared_wallet_never_signs_hmac_sha512() {
        assert_declared_wallet_never_macs("hmacSha512");
    }
    #[test]
    fn declared_wallet_never_signs_sha256_secret_suffix() {
        assert_declared_wallet_never_macs("sha256SecretSuffix");
    }

    // A declared ref in a build without its arm stays Unsupported.
    #[cfg(not(feature = "secp256k1"))]
    #[test]
    fn declared_refs_without_their_arm_are_unsupported() {
        let (signer, key) = public_setup("secp256k1Digest", SCALAR);
        assert_eq!(
            signer.sign_now(&sign_request("secp256k1Digest", &key, vec![7; 32])),
            Err(SignError::Unsupported)
        );
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn insert_declared_is_atomic_asymmetric_only_and_never_on_a_role() {
        let mut signer = LocalSigner::new();
        let key = KeyRef("raw.signing".into());
        let scalar = || vec![0x11; 32];
        // MAC algorithms are never declarable, and nothing is installed.
        assert_eq!(
            signer.insert_declared(key.clone(), scalar(), Algorithm::HmacSha256),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(
            signer.sign_now(&sign_request("hmacSha256", &key, b"m".to_vec())),
            Err(SignError::MissingKey)
        );
        signer
            .insert_declared(key.clone(), scalar(), Algorithm::Secp256k1Digest)
            .unwrap();
        assert!(
            signer
                .sign_now(&sign_request("secp256k1Digest", &key, vec![7; 32]))
                .is_ok()
        );
        let before = signer
            .sign_now(&sign_request("secp256k1Digest", &key, vec![7; 32]))
            .unwrap()
            .values
            .clone();
        signer
            .insert_declared(key.clone(), vec![0x22; 32], Algorithm::Secp256k1Digest)
            .unwrap();
        let rotated = signer
            .sign_now(&sign_request("secp256k1Digest", &key, vec![7; 32]))
            .unwrap()
            .values
            .clone();
        assert_ne!(before, rotated);
        // An unaliased raw ref may be retyped, replacing key and declaration.
        signer
            .insert_declared(key.clone(), scalar(), Algorithm::Ed25519)
            .unwrap();
        assert_eq!(
            signer.sign_now(&sign_request("secp256k1Digest", &key, vec![7; 32])),
            Err(SignError::InvalidRequest)
        );
        // Permanent control (codex-uufub4's repro): an aliased read agent shares
        // the primary's ref, so neither slot's ref can be retyped; rotation can.
        let (mut aliased, ids) = try_public_setup(
            serde_json::json!({
                "primary": {"secretEncoding": "hex", "signingAlgorithm": "secp256k1Digest"},
                "secondary": {"secretEncoding": "hex", "signingAlgorithm": "secp256k1Digest"}
            }),
            serde_json::json!({
                "primary": {"key": "account", "secret": SCALAR},
                "secondary": {"key": "read", "signingFromPrimary": true}
            }),
        )
        .unwrap();
        let shared = ids.secondary.unwrap().signing_ref().unwrap().clone();
        assert_eq!(&shared, ids.primary.unwrap().signing_ref().unwrap());
        assert_eq!(
            aliased.insert_declared(shared.clone(), vec![0x33; 32], Algorithm::Ed25519),
            Err(SignError::InvalidRequest)
        );
        assert!(
            aliased
                .sign_now(&sign_request("secp256k1Digest", &shared, vec![7; 32]))
                .is_ok()
        );
        aliased
            .insert_declared(shared.clone(), vec![0x33; 32], Algorithm::Secp256k1Digest)
            .unwrap();
        // A plain insert rotates the shared material but keeps its type, so it
        // never downgrades to a MAC key or opens a later retype (second and
        // third repros, from codex-uufub4 and claude-m26ps4).
        aliased.insert(shared.clone(), vec![0x44; 32]);
        assert!(
            aliased
                .sign_now(&sign_request("secp256k1Digest", &shared, vec![7; 32]))
                .is_ok()
        );
        assert_eq!(
            aliased.sign_now(&sign_request("hmacSha256", &shared, b"m".to_vec())),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(
            aliased.insert_declared(shared.clone(), vec![0x55; 32], Algorithm::Ed25519),
            Err(SignError::InvalidRequest)
        );
        // Removal ends the binding; the ref is then unshared raw material.
        aliased.remove(&shared);
        aliased
            .insert_declared(shared.clone(), vec![0x55; 32], Algorithm::Ed25519)
            .unwrap();
        // A role ref is its own declaration and refuses another.
        let (seed, _, _) = zklink_case();
        let (mut zklink, ids) =
            LocalSigner::from_json_with_policy(zklink_setup(&seed), zklink_policy()).unwrap();
        let role = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
        assert_eq!(
            zklink.insert_declared(role, scalar(), Algorithm::Secp256k1Digest),
            Err(SignError::InvalidRequest)
        );
    }

    // can_sign is the same typing check, so admission agrees with signing.
    #[test]
    fn can_sign_mirrors_key_typing() {
        let (mac, key) = undeclared_mac_secret();
        assert_eq!(
            mac.can_sign(&key, Algorithm::HmacSha256),
            Capability::Supported
        );
        assert_eq!(
            mac.can_sign(&key, Algorithm::Secp256k1Digest),
            Capability::Unsupported
        );
        let (wallet, key) = public_setup("secp256k1Digest", SCALAR);
        assert_eq!(
            wallet.can_sign(&key, Algorithm::HmacSha256),
            Capability::Unsupported
        );
        assert_eq!(
            wallet.can_sign(&key, Algorithm::Ed25519),
            Capability::Unsupported
        );
    }
}

#[cfg(all(test, feature = "address-nonce"))]
mod owned_address_setup_tests {
    use super::*;
    use std::task::{Context as TaskContext, Poll, Waker};
    fn provider() -> (LocalSigner, KeyRef) {
        let mut provider = LocalSigner::new();
        let key = KeyRef("owned-scalar".into());
        let mut scalar = vec![0; 32];
        scalar[31] = 1;
        provider
            .insert_declared(key.clone(), scalar, Algorithm::Secp256k1Digest)
            .unwrap();
        (provider, key)
    }
    fn request(key: KeyRef) -> SignRequest {
        SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::PublicAddress { key }],
        }
    }
    #[test]
    fn owned_factory_is_dormant_and_agrees_with_independent_scalar_one_address() {
        let (provider, key) = provider();
        let work = request(key);
        let mut owner = provider.create_public_address_setup(work.clone()).unwrap();
        let mut cx = TaskContext::from_waker(Waker::noop());
        assert!(owner.poll_result(&mut cx).is_pending());
        assert!(owner.poll_joined(&mut cx).is_pending());
        owner.start().unwrap();
        let Poll::Ready(Ok(result)) = owner.poll_result(&mut cx) else {
            panic!("immediate actual local result")
        };
        assert_eq!(
            crate::hex(&result.values[0]),
            "7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
        work.validate_result(&result).unwrap();
        assert_eq!(owner.poll_joined(&mut cx), Poll::Ready(Ok(())));
    }
    #[test]
    fn dormant_local_handle_stores_only_the_fixed_public_result() {
        use safe_cex_engine::signing::OwnedSetupExecution;
        let mut owner = LocalAddressSetup {
            address: Some(Ok([19; 20])),
            result: None,
            started: false,
        };
        let public: &Option<Result<[u8; 20], SignError>> = &owner.address;
        assert_eq!(public, &Some(Ok([19; 20])));
        owner.cancel().unwrap();
        assert!(owner.address.is_none());
        assert!(owner.result.is_none());
    }
    #[test]
    fn direct_factory_rejects_foreign_ref_algorithm_receipt_and_extra_work() {
        let (mut provider, key) = provider();
        let work = request(key.clone());
        let mut cases = Vec::new();
        cases.push(request(KeyRef("foreign".into())));
        let mut bad = work.clone();
        bad.operations.clear();
        cases.push(bad);
        let mut bad = work.clone();
        bad.operations
            .push(Operation::PublicAddress { key: key.clone() });
        cases.push(bad);
        let mut bad = work.clone();
        bad.receipt.stamp_ms = Some(1.);
        cases.push(bad);
        let mut bad = work.clone();
        bad.receipt.expiry_ms = Some(2.);
        cases.push(bad);
        let mut bad = work.clone();
        bad.receipt.nonce = Some(1);
        cases.push(bad);
        let mut bad = work.clone();
        bad.context.path = "business".into();
        cases.push(bad);
        let mut bad = work.clone();
        bad.operations.push(Operation::Sha256 { payload: vec![] });
        cases.push(bad);
        for bad in cases {
            assert!(provider.create_public_address_setup(bad).is_none());
        }
        provider
            .insert_declared(key.clone(), vec![1; 32], Algorithm::Ed25519)
            .unwrap();
        assert!(provider.create_public_address_setup(request(key)).is_none());
    }
}
