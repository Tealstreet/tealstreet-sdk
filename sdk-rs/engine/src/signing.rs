//! Send-boundary signing. Keys belong to the injected provider, not a venue.
//!
//! A batch is atomic: credential presentation and signatures return together.
//! Credential interpolation happens inside the provider, including credentials
//! occurring in canonical payloads. Never park a partially materialised request.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Non-secret rules applied only while trusted setup installs provider material.
/// An undeclared slot/field preserves the caller's setup metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct CredentialSetupPolicy {
    pub primary: Option<IdentitySetupPolicy>,
    pub secondary: Option<IdentitySetupPolicy>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct IdentitySetupPolicy {
    pub trim_credential: Option<bool>,
    /// Trim the signing key text (Unicode White_Space) once, before decoding.
    /// Omitted when undeclared, so existing policies serialize unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_signing: Option<bool>,
    pub passphrase: Option<SetupFieldPolicy>,
    /// Declared secret decoding is authoritative over caller metadata.
    pub secret_encoding: Option<KeyEncoding>,
    /// The only extra signing ref trusted setup may install for this slot.
    pub extra: Option<ExtraKeyPolicy>,
    /// Declares the signing ref an asymmetric key of this algorithm; only a
    /// declared ref answers public operations, so no secret is fingerprinted.
    pub signing_algorithm: Option<Algorithm>,
    /// Whether a credential-only slot (no signing ref) is accepted.
    #[serde(skip_serializing_if = "SigningPolicy::is_required")]
    pub signing: SigningPolicy,
    /// Whether trusted local setup may install a primary with no credential.
    #[serde(skip_serializing_if = "PrimarySource::is_required")]
    pub source: PrimarySource,
}

/// `Required` keeps today's behaviour exactly. `ScalarOnly` (primary only,
/// declared `Secp256k1Digest` or `StarkDigest`) lets trusted local setup install a signing-only
/// primary whose credential is explicitly absent; injected identities never are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrimarySource {
    #[default]
    Required,
    ScalarOnly,
}
impl PrimarySource {
    pub fn is_required(&self) -> bool {
        *self == Self::Required
    }
}

/// `Required` keeps today's behaviour exactly; only `Optional` families accept
/// a credential-only slot (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md, D2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SigningPolicy {
    #[default]
    Required,
    Optional,
}
impl SigningPolicy {
    pub fn is_required(&self) -> bool {
        *self == Self::Required
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtraKeyPolicy {
    pub key: ExtraKey,
    #[serde(default)]
    pub field: ExtraFieldPolicy,
    /// Declared decoding of the extra material; authoritative over caller metadata.
    #[serde(default)]
    pub encoding: Option<KeyEncoding>,
}

/// Extra refs never fall back across slots, so there is no fallback variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtraFieldPolicy {
    #[default]
    Optional,
    RequiredForPrivate,
}

/// Trusted setup decoding of supplied key material. Nothing here is inferred.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyEncoding {
    #[default]
    Utf8,
    Base64,
    Hex,
    /// PKCS8/SEC1 envelope decoding; algorithm validation occurs when signing.
    Pem,
    /// Key bytes are the ASCII of standard padded base64 of the input bytes.
    Base64Text,
    /// Bitcoin-alphabet base58 (Orderly ed25519 seeds); no prefix is stripped.
    Base58,
    /// URL-safe or standard base64, padding optional (Polymarket API secrets).
    Base64Url,
}

/// Closed vocabulary of additional per-identity signing material. These refs
/// are signing inputs only: never a nonce domain, account subject or id source.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum ExtraKey {
    /// ApeX Omni zkLink seed: the exact 65-byte packed Ethereum signature.
    ZklinkSeed,
    /// Polymarket L1 wallet: the raw 32-byte secp256k1 scalar; primary only.
    L1Wallet,
}
impl ExtraKey {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ZklinkSeed => "zklinkSeed",
            Self::L1Wallet => "l1Wallet",
        }
    }
    /// The one algorithm a role ref serves, for `Sign` and public operations.
    pub const fn algorithm(self) -> Algorithm {
        match self {
            Self::ZklinkSeed => Algorithm::ZkLinkContract,
            Self::L1Wallet => Algorithm::Secp256k1Digest,
        }
    }
    /// The L1 wallet derives the session identity, which is primary only.
    pub const fn primary_only(self) -> bool {
        matches!(self, Self::L1Wallet)
    }
}

/// Required material is validated by trusted setup; external refs prove presence only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupFieldPolicy {
    #[default]
    Optional,
    RequiredForPrivate,
    RequiredWithPrimaryFallback,
}

impl CredentialSetupPolicy {
    /// Resolve only complete existing slots; never invent credential/signing refs.
    /// Explicit passphrase refs (including invalid empty refs) never fall back.
    pub fn apply_to_identities(
        &self,
        identities: &mut Identities,
        public_data_only: bool,
    ) -> Result<(), SignError> {
        let complete = |identity: &Identity| {
            !identity.credential.0.is_empty()
                && identity
                    .signing
                    .as_ref()
                    .is_some_and(|key| !key.0.is_empty())
        };
        // Extra refs exist only where declared. They never fall back across slots.
        for (identity, policy, primary) in [
            (identities.primary.as_ref(), self.primary, true),
            (identities.secondary.as_ref(), self.secondary, false),
        ] {
            let declared = policy.and_then(|policy| policy.extra);
            if !primary && declared.is_some_and(|declared| declared.key.primary_only()) {
                return Err(SignError::InvalidRequest);
            }
            let Some(identity) = identity else { continue };
            if identity
                .extra
                .keys()
                .any(|key| declared.is_none_or(|declared| declared.key != *key))
            {
                return Err(SignError::InvalidRequest);
            }
            if let Some(declared) = declared {
                let required = match declared.field {
                    ExtraFieldPolicy::Optional => false,
                    ExtraFieldPolicy::RequiredForPrivate => !(primary && public_data_only),
                };
                if required
                    && complete(identity)
                    && identity
                        .extra
                        .get(&declared.key)
                        .is_none_or(|key| !valid_key_ref(key))
                {
                    return Err(SignError::InvalidRequest);
                }
            }
        }
        let required = |policy: Option<IdentitySetupPolicy>, primary: bool| match policy
            .and_then(|policy| policy.passphrase)
            .unwrap_or_default()
        {
            SetupFieldPolicy::Optional => false,
            SetupFieldPolicy::RequiredForPrivate => !(primary && public_data_only),
            SetupFieldPolicy::RequiredWithPrimaryFallback => true,
        };
        if let Some(primary) = identities.primary.as_ref().filter(|id| complete(id))
            && required(self.primary, true)
            && (!valid_key_ref(&primary.credential)
                || !primary.signing.as_ref().is_some_and(valid_key_ref)
                || primary
                    .passphrase
                    .as_ref()
                    .is_none_or(|key| !valid_key_ref(key)))
        {
            return Err(SignError::InvalidRequest);
        }
        let primary_passphrase = identities
            .primary
            .as_ref()
            .and_then(|id| id.passphrase.as_ref());
        if let Some(secondary) = identities.secondary.as_ref().filter(|id| complete(id))
            && required(self.secondary, false)
        {
            let passphrase = secondary.passphrase.as_ref().or_else(|| {
                (self.secondary.and_then(|policy| policy.passphrase)
                    == Some(SetupFieldPolicy::RequiredWithPrimaryFallback))
                .then_some(primary_passphrase)
                .flatten()
            });
            if !valid_key_ref(&secondary.credential)
                || !secondary.signing.as_ref().is_some_and(valid_key_ref)
                || passphrase.is_none_or(|key| !valid_key_ref(key))
            {
                return Err(SignError::InvalidRequest);
            }
            // Validation precedes mutation, including an invalid primary fallback.
            let passphrase = passphrase.cloned();
            identities
                .secondary
                .as_mut()
                .expect("existing secondary")
                .passphrase = passphrase;
        }
        // An Optional family accepts only a primary credential-only slot that
        // is otherwise valid; Required (and every secondary) is exactly today.
        if let Some(identity) = identities.primary.as_ref()
            && identity.signing.is_none()
            && !identity.credential.0.is_empty()
            && self
                .primary
                .is_some_and(|policy| !policy.signing.is_required())
            && identity.validate_credential().is_err()
        {
            return Err(SignError::InvalidRequest);
        }
        // An accepted Optional primary without a signing ref is an intentional
        // absence: its signing operations refuse per operation (D6).
        if let Some(identity) = identities.primary.as_mut()
            && identity.signing.is_none()
            && self
                .primary
                .is_some_and(|policy| !policy.signing.is_required())
        {
            identity.signing_not_configured = true;
        }
        Ok(())
    }

    /// Trusted local setup only (bans-scoped): marks a `ScalarOnly` primary
    /// that setup built without a credential as explicitly credential-absent.
    /// Injected identities never pass through here, so they cannot be absent.
    pub fn install_scalar_only_primary(
        &self,
        identities: &mut Identities,
    ) -> Result<(), SignError> {
        let scalar_only = |policy: Option<IdentitySetupPolicy>| {
            policy.is_some_and(|policy| policy.source == PrimarySource::ScalarOnly)
        };
        if scalar_only(self.secondary) {
            return Err(SignError::InvalidRequest);
        }
        let Some(policy) = self.primary.filter(|policy| scalar_only(Some(*policy))) else {
            return Ok(());
        };
        if !matches!(
            policy.signing_algorithm,
            Some(Algorithm::Secp256k1Digest | Algorithm::StarkDigest)
        ) || !policy.signing.is_required()
        {
            return Err(SignError::InvalidRequest);
        }
        let Some(identity) = identities.primary.as_mut() else {
            return Ok(());
        };
        if !identity.credential.0.is_empty() {
            return Ok(());
        }
        let Some(signing) = identity.signing.as_ref().filter(|key| valid_key_ref(key)) else {
            return Ok(());
        };
        // The authority ref must select exactly one identity.
        if identities.secondary.as_ref().is_some_and(|other| {
            other.credential == *signing || other.signing.as_ref() == Some(signing)
        }) {
            return Err(SignError::InvalidRequest);
        }
        identity.credential_absent = true;
        Ok(())
    }
}

/// Raw presence of the primary key, classified by trusted setup before any
/// trim policy (D3). Only the variant name is ever formatted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyPresence {
    Absent,
    Empty,
    Blank,
    Nonblank,
}

/// Non-secret facts trusted setup derives beside `Identities`; never parsed
/// from caller JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupFacts {
    primary_key: KeyPresence,
}
impl SetupFacts {
    pub fn new(primary_key: KeyPresence) -> Self {
        Self { primary_key }
    }
    pub fn primary_key(&self) -> KeyPresence {
        self.primary_key
    }
    /// The typed login intent (G3): Empty logs in keyless on the primary;
    /// Blank/Nonblank use the installed credential; Absent has none.
    pub fn primary_login(&self) -> Option<PrimaryLogin> {
        match self.primary_key {
            KeyPresence::Absent => None,
            KeyPresence::Empty => Some(PrimaryLogin::Empty),
            KeyPresence::Blank | KeyPresence::Nonblank => Some(PrimaryLogin::Installed),
        }
    }
}

/// A non-secret login intent selected through `Slots<PrimaryLogin>`; it
/// carries no `Identity` or `KeyRef`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryLogin {
    Empty,
    Installed,
}

/// Opaque provider-owned identity. This is not an API key or a private key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct KeyRef(pub String);

/// One trusted local provider's setup lifetime. Not Clone, Copy or comparable:
/// only the installed provider can present it, so a seal cannot outlive it.
#[derive(Debug)]
pub struct Incarnation(u64);
impl Incarnation {
    /// Process-wide unique; refuses permanently rather than reuse a value.
    pub fn fresh() -> Result<Self, SignError> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self::allocate(&NEXT)
    }
    fn allocate(next: &std::sync::atomic::AtomicU64) -> Result<Self, SignError> {
        next.try_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |value| value.checked_add(1),
        )
        .map(Self)
        .map_err(|_| SignError::Unavailable)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SealedPublicKey {
    signing: KeyRef,
    algorithm: Algorithm,
    bytes: [u8; 32],
    incarnation: u64,
}

/// A venue's non-sensitive references to credential presentation and signing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub credential: KeyRef,
    /// Private so no consumer can substitute an empty ref; `None` is an
    /// explicit absence (a credential-only slot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signing: Option<KeyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<KeyRef>,
    /// Caller-assigned, non-secret identity/nonce namespace. Never a key hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_id: Option<String>,
    /// Additional signing refs owned by this slot; see `ExtraKey`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub extra: std::collections::BTreeMap<ExtraKey, KeyRef>,
    /// An Optional family's intentional absence (D6). Set only by trusted
    /// setup policy, never transported: a serialized copy loses it.
    #[serde(skip)]
    signing_not_configured: bool,
    /// A `ScalarOnly` primary's explicit credential absence. Set only by
    /// trusted local setup, never transported: a serialized copy loses it.
    #[serde(skip)]
    credential_absent: bool,
    /// The trusted local provider's public key for `signing`, sealed at
    /// setup and never transported: a serialized copy loses it.
    #[serde(skip)]
    sealed_public_key: Option<Box<SealedPublicKey>>,
    #[cfg(feature = "address-nonce")]
    #[serde(skip)]
    pub(crate) address: crate::engine::address_nonce_setup::AddressCapability,
    #[cfg(feature = "nonce-handoff")]
    #[serde(skip)]
    pub(crate) nonce_owner: Option<Arc<crate::nonce::handoff::Owner>>,
}
impl Identity {
    #[inline]
    pub fn new(credential: KeyRef, signing: Option<KeyRef>) -> Self {
        Self {
            credential,
            signing,
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
            signing_not_configured: false,
            credential_absent: false,
            sealed_public_key: None,
            #[cfg(feature = "address-nonce")]
            address: Default::default(),
            #[cfg(feature = "nonce-handoff")]
            nonce_owner: None,
        }
    }

    #[inline]
    pub fn with_passphrase(mut self, passphrase: Option<KeyRef>) -> Self {
        self.passphrase = passphrase;
        self
    }

    #[inline]
    pub fn with_stable_id(mut self, stable_id: Option<String>) -> Self {
        self.stable_id = stable_id;
        self
    }

    #[inline]
    pub fn with_extra(mut self, extra: std::collections::BTreeMap<ExtraKey, KeyRef>) -> Self {
        self.extra = extra;
        self
    }

    /// The signing ref an operation that signs must use; absent is refused
    /// before any provider work.
    #[inline]
    pub fn signing_ref(&self) -> Result<&KeyRef, SignError> {
        self.signing.as_ref().ok_or(if self.signing_not_configured {
            SignError::SigningNotConfigured
        } else {
            SignError::MissingKey
        })
    }

    /// Setup, construction resolution and the alias check only (bans-scoped).
    #[inline]
    pub fn signing_opt(&self) -> Option<&KeyRef> {
        self.signing.as_ref()
    }

    /// Setup only (bans-scoped): the aliased secondary signs with the primary's ref.
    #[inline]
    pub fn set_signing_ref(&mut self, signing: Option<KeyRef>) {
        self.signing = signing;
        self.sealed_public_key = None;
        #[cfg(feature = "nonce-handoff")]
        {
            self.nonce_owner = None;
        }
        #[cfg(feature = "address-nonce")]
        {
            self.address = Default::default();
        }
    }

    /// Trusted local setup only (credential-only-scoped): seals the concrete
    /// provider's own public-key answer for the installed signing ref.
    /// The caller validates `bytes` as a canonical public key of `algorithm`.
    pub fn seal_public_key(
        &mut self,
        algorithm: Algorithm,
        bytes: [u8; 32],
        incarnation: &Incarnation,
    ) -> Result<(), SignError> {
        let signing = self.signing_ref()?.clone();
        self.sealed_public_key = Some(Box::new(SealedPublicKey {
            signing,
            algorithm,
            bytes,
            incarnation: incarnation.0,
        }));
        Ok(())
    }

    fn verified_public_key_under(
        &self,
        installed: Option<&Incarnation>,
    ) -> Result<&SealedPublicKey, SignError> {
        let signing = self.signing_ref()?;
        match &self.sealed_public_key {
            Some(sealed)
                if sealed.signing == *signing
                    && installed.is_some_and(|current| current.0 == sealed.incarnation) =>
            {
                Ok(sealed)
            }
            _ => Err(SignError::Unsupported),
        }
    }

    /// The sealed public key while its ref is current and `installed`, the
    /// engine's provider, is the trusted local provider that sealed it.
    pub(crate) fn verified_public_key(
        &self,
        algorithm: Algorithm,
        installed: Option<&Incarnation>,
    ) -> Result<[u8; 32], SignError> {
        let sealed = self.verified_public_key_under(installed)?;
        if sealed.algorithm == algorithm {
            Ok(sealed.bytes)
        } else {
            Err(SignError::Unsupported)
        }
    }

    pub fn require_stable_id(&self) -> Result<&str, SignError> {
        self.stable_id
            .as_deref()
            .filter(|id| valid_stable_id(id))
            .ok_or(SignError::InvalidRequest)
    }

    /// Whether this primary has no credential by trusted `ScalarOnly` setup;
    /// nothing may be presented for it.
    #[inline]
    pub fn credential_absent(&self) -> bool {
        self.credential_absent
    }

    /// The presentable credential ref: `None` when it is absent or invalid.
    #[inline]
    pub fn credential_ref(&self) -> Option<&KeyRef> {
        (!self.credential_absent && valid_key_ref(&self.credential)).then_some(&self.credential)
    }

    /// The ref a request authority names for this identity: its credential,
    /// or for a credential-absent primary its signing ref, which is then
    /// signing-only (never presented or interpolated).
    #[inline]
    pub fn authority_ref(&self) -> &KeyRef {
        match &self.signing {
            Some(signing) if self.credential_absent => signing,
            _ => &self.credential,
        }
    }

    /// Whether `key` is one of this slot's refs.
    pub fn owns(&self, key: &KeyRef) -> bool {
        !self.credential_absent && self.credential == *key
            || self.signing.as_ref() == Some(key)
            || self.passphrase.as_ref() == Some(key)
            || self.extra.values().any(|extra| extra == key)
    }

    #[inline]
    pub fn validate(&self) -> Result<(), &'static str> {
        self.checked(true)
    }

    /// `validate` without the signing requirement: a credential-only slot of an
    /// Optional family. A present signing ref must still be valid.
    #[inline]
    pub fn validate_credential(&self) -> Result<(), &'static str> {
        self.checked(false)
    }

    #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
    pub(crate) fn accepts_session_root(
        &self,
        credential: &crate::request_context::Credential,
        write: Option<&crate::request_context::Credential>,
    ) -> bool {
        credential.slot == crate::CredentialSlot::Primary
            && credential.reference == self.credential
            && self.validate_credential().is_ok()
            && write.is_none_or(|write| write == credential && self.signing_ref().is_ok())
    }

    fn checked(&self, signing_required: bool) -> Result<(), &'static str> {
        let credential = if self.credential_absent {
            self.credential.0.is_empty() && self.signing.is_some()
        } else {
            valid_key_ref(&self.credential)
        };
        if !credential
            || self
                .signing
                .as_ref()
                .map_or(signing_required, |key| !valid_key_ref(key))
            || self
                .passphrase
                .as_ref()
                .is_some_and(|key| !valid_key_ref(key))
            || self
                .stable_id
                .as_deref()
                .is_some_and(|id| !valid_stable_id(id))
            || self.extra.values().any(|key| !valid_key_ref(key))
        {
            Err("Secondary read credentials require a nonempty key and secret")
        } else {
            Ok(())
        }
    }
}

pub fn valid_stable_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
fn valid_key_ref(key: &KeyRef) -> bool {
    !key.0.is_empty() && key.0.len() <= 256
}

impl Identities {
    /// A secondary read slot's failure must never stop primary trading: with
    /// a secondary, only a request naming a primary ref is primary.
    pub fn primary_owns(&self, work: &SignRequest) -> bool {
        self.secondary.is_none()
            || self
                .primary
                .as_ref()
                .is_some_and(|primary| work.any_key_ref(&|key| primary.owns(key)))
    }

    /// The signing-only ref of a credential-absent primary, which no request
    /// may present or interpolate.
    pub fn signing_only_ref(&self) -> Option<&KeyRef> {
        self.primary
            .as_ref()
            .filter(|primary| primary.credential_absent)
            .and_then(|primary| primary.signing.as_ref())
    }

    /// Setup requires a secondary's signing ref, so a step refused for an
    /// absent one (`signing_ref()`) is primary material.
    pub fn primary_lacks_signing(&self) -> bool {
        self.primary
            .as_ref()
            .is_some_and(|primary| primary.signing.is_none())
    }
}

/// Only opaque identities cross into registry/engine options.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identities {
    pub primary: Option<Identity>,
    pub secondary: Option<Identity>,
}

/// Exact primitive contracts, independent of venue request formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Algorithm {
    /// MAC exact message bytes; result is the raw 32-byte MAC.
    HmacSha256,
    /// MAC exact message bytes; result is the raw 64-byte MAC.
    HmacSha512,
    /// SHA256(message || key), raw 32-byte digest. No implicit hex or prehash.
    Sha256SecretSuffix,
    /// Ed25519 exact message, without a prehash; raw 64-byte signature. The key is
    /// a 32-byte seed or the `seed || public` keypair whose public half must match.
    Ed25519,
    /// ECDSA P-256 with one SHA256 of message; fixed-width r || s, 64 bytes.
    Es256,
    /// ECDSA secp256k1 over a 32-byte digest; 32-byte r || 32-byte s || recovery (0/1).
    Secp256k1Digest,
    /// Stark over a 32-byte big-endian field digest; 32-byte big-endian r || s.
    StarkDigest,
    /// Lighter ECgFp5 Schnorr over Goldilocks Poseidon2. 40-byte canonical LE
    /// scalar key; message is 32 entropy bytes then LE u64 words; 80-byte s || e.
    SchnorrDigest,
    /// zkLink Rescue/MuSig over an exact 36-byte contract; 65-byte seed key;
    /// raw 64-byte packed signature. Deterministic: no entropy or nonce state.
    ZkLinkContract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    Raw,
    Hex,
    Base64,
    /// BloFin's base64(ASCII lowercase hex(MAC)).
    Base64Hex,
    /// Lowercase hex of a Secp256k1Digest `r || s || recovery` with the
    /// recovery id rendered as 27 or 28 (EVM `v`). Any other shape refuses.
    EvmHex,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PayloadPart {
    Bytes {
        bytes: Vec<u8>,
    },
    /// Interpolated privately, never returned as an intermediate engine value.
    Credential {
        key: KeyRef,
    },
    /// Only earlier batch results may be referenced. Encoding is explicit.
    Result {
        index: usize,
        encoding: Encoding,
    },
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Operation {
    /// Exact-byte SHA256; supports private interpolation before a later signature.
    Sha256 { payload: Vec<PayloadPart> },
    Sign {
        algorithm: Algorithm,
        key: KeyRef,
        payload: Vec<PayloadPart>,
    },
    /// Credential value for an outbound header/body/upgrade field, not storage.
    Present { key: KeyRef },
    /// Non-secret 20-byte Ethereum address of a ref declared `Secp256k1Digest`.
    PublicAddress { key: KeyRef },
    /// Non-secret public key of a ref declared for exactly `algorithm`:
    /// `Ed25519` 32 bytes, `StarkDigest` the 32-byte big-endian felt.
    PublicKey { algorithm: Algorithm, key: KeyRef },
}

/// Context is advisory; providers may optionally refuse it. No enforcement core.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub venue: String,
    pub method: String,
    pub host: String,
    pub path: String,
}

/// Exact canonical time facts, in protocol UTC milliseconds. No implied stamp
/// for an expiry-only or credential-presentation operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamp_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_ms: Option<f64>,
    #[cfg_attr(not(feature = "dispatch-nonce"), serde(skip))]
    #[cfg_attr(
        feature = "dispatch-nonce",
        serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "nonce_decimal"
        )
    )]
    pub nonce: Option<u64>,
}
#[path = "signing_nonce_decimal.rs"]
#[cfg(feature = "dispatch-nonce")]
mod nonce_decimal;

impl SignReceipt {
    #[cfg_attr(not(feature = "dispatch-nonce"), inline(never))]
    pub fn validate(&self) -> Result<(), SignError> {
        if [self.stamp_ms, self.expiry_ms]
            .into_iter()
            .flatten()
            .any(|value| {
                !value.is_finite()
                    || !(0. ..=9_007_199_254_740_991.).contains(&value)
                    || value.fract() != 0.
            })
            || (!cfg!(feature = "dispatch-nonce") && self.nonce.is_some())
            || matches!((self.stamp_ms, self.expiry_ms), (Some(stamp), Some(expiry)) if expiry <= stamp)
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    pub receipt: SignReceipt,
    pub operations: Vec<Operation>,
    pub context: Context,
}

/// One credential presentation and exact HMAC preimage. Native recipes and
/// signing-only compatibility adapters use the same atomic primitive shape;
/// the trusted recipe passes both refs from one `Identity` (`signing_ref()?`).
pub fn hmac_sha256_work(
    credential: &KeyRef,
    signing: &KeyRef,
    payload: Vec<PayloadPart>,
    context: Context,
    receipt: SignReceipt,
) -> SignRequest {
    SignRequest {
        receipt,
        operations: vec![
            Operation::Present {
                key: credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing.clone(),
                payload,
            },
        ],
        context,
    }
}

/// Login preimage `seconds + method + path` as three typed byte parts. The shape
/// keeps it distinct from an HTTP capture of identical bytes; the remote wire
/// decodes it only against the family's declared `secondsSuffix`.
pub fn seconds_suffix_payload(seconds: &str, method: &str, path: &str) -> Vec<PayloadPart> {
    [seconds, method, path]
        .map(|part| PayloadPart::Bytes {
            bytes: part.as_bytes().to_vec(),
        })
        .into()
}

impl SignRequest {
    /// Validate provider output against this exact atomic request. Dispatch and
    /// signing-only capture share receipt, result-count and byte-shape policy.
    pub fn validate_result(&self, result: &SignResult) -> Result<(), SignError> {
        self.validate_result_with_policy(result, crate::signing_headers::ResultHeaderPolicy::Deny)
    }

    /// Only a trusted capture/recipe may supply HTTP callback-header authority.
    /// Advisory request context and provider output never choose this policy.
    pub fn validate_result_with_policy(
        &self,
        result: &SignResult,
        policy: crate::signing_headers::ResultHeaderPolicy,
    ) -> Result<(), SignError> {
        validate_result_shape(&self.expected_result_lengths(), self.receipt, result)?;
        policy.validate(&result.headers)
    }

    pub(crate) fn expected_result_lengths(&self) -> Vec<Option<usize>> {
        self.operations
            .iter()
            .map(|operation| match operation {
                Operation::Present { .. } => None,
                Operation::Sha256 { .. } => Some(32),
                Operation::PublicAddress { .. } => Some(20),
                // `validate` refuses an algorithm without a public form first.
                Operation::PublicKey { algorithm, .. } => {
                    Some(public_key_len(*algorithm).unwrap_or(usize::MAX))
                }
                Operation::Sign { algorithm, .. } => Some(match algorithm {
                    Algorithm::HmacSha256 | Algorithm::Sha256SecretSuffix => 32,
                    Algorithm::HmacSha512
                    | Algorithm::Ed25519
                    | Algorithm::Es256
                    | Algorithm::StarkDigest
                    | Algorithm::ZkLinkContract => 64,
                    Algorithm::Secp256k1Digest => 65,
                    Algorithm::SchnorrDigest => 80,
                }),
            })
            .collect()
    }

    /// Refs come only from an identity, so an invalid one means the installed
    /// credentials lack that material: no retry of this request can sign.
    pub fn has_invalid_key_ref(&self) -> bool {
        self.any_key_ref(&|key| !valid_key_ref(key))
    }
    /// Whether any ref the operations name, including interpolated
    /// credentials, passes `test`. A plain loop over `dyn`, so every caller
    /// shares one body.
    pub fn any_key_ref(&self, test: &dyn Fn(&KeyRef) -> bool) -> bool {
        for op in &self.operations {
            let (key, payload) = match op {
                Operation::Present { key }
                | Operation::PublicAddress { key }
                | Operation::PublicKey { key, .. } => (Some(key), &[][..]),
                Operation::Sign { key, payload, .. } => (Some(key), &payload[..]),
                Operation::Sha256 { payload } => (None, &payload[..]),
            };
            if key.is_some_and(test) {
                return true;
            }
            for part in payload {
                if let PayloadPart::Credential { key } = part
                    && test(key)
                {
                    return true;
                }
            }
        }
        false
    }
    /// Whether any operation presents `key` or interpolates it into a payload.
    pub fn presents(&self, key: &KeyRef) -> bool {
        self.operations.iter().any(|op| match op {
            Operation::Present { key: presented } => presented == key,
            Operation::Sign { payload, .. } | Operation::Sha256 { payload } => payload
                .iter()
                .any(|part| matches!(part, PayloadPart::Credential { key: part } if part == key)),
            Operation::PublicAddress { .. } | Operation::PublicKey { .. } => false,
        })
    }
    /// Shared structural bounds also apply to injected providers. Expanded secret
    /// input bounds are enforced privately by the provider before allocation.
    pub fn validate(&self) -> Result<(), SignError> {
        self.receipt.validate()?;
        if self.operations.is_empty()
            || self.operations.len() > 32
            || [
                &self.context.venue,
                &self.context.method,
                &self.context.host,
                &self.context.path,
            ]
            .iter()
            .any(|s| s.len() > 4096)
        {
            return Err(SignError::InvalidRequest);
        }
        let mut bytes = 0usize;
        for (index, op) in self.operations.iter().enumerate() {
            match op {
                Operation::Present { key } if !valid_key_ref(key) => {
                    return Err(SignError::InvalidRequest);
                }
                Operation::Sign { key, .. } if !valid_key_ref(key) => {
                    return Err(SignError::InvalidRequest);
                }
                Operation::PublicAddress { key } if !valid_key_ref(key) => {
                    return Err(SignError::InvalidRequest);
                }
                Operation::PublicKey { algorithm, key }
                    if !valid_key_ref(key) || public_key_len(*algorithm).is_none() =>
                {
                    return Err(SignError::InvalidRequest);
                }
                Operation::Sign { payload, .. } | Operation::Sha256 { payload } => {
                    if payload.len() > 256 {
                        return Err(SignError::InvalidRequest);
                    }
                    for part in payload {
                        match part {
                            PayloadPart::Bytes { bytes: part } => {
                                bytes = bytes
                                    .checked_add(part.len())
                                    .filter(|n| *n <= 1024 * 1024)
                                    .ok_or(SignError::InvalidRequest)?;
                            }
                            PayloadPart::Credential { key } if !valid_key_ref(key) => {
                                return Err(SignError::InvalidRequest);
                            }
                            PayloadPart::Result { index: prior, .. } if *prior >= index => {
                                return Err(SignError::InvalidRequest);
                            }
                            // Only a secp256k1 digest signature renders as an EVM `v`.
                            PayloadPart::Result {
                                index: prior,
                                encoding: Encoding::EvmHex,
                            } if !matches!(
                                self.operations[*prior],
                                Operation::Sign {
                                    algorithm: Algorithm::Secp256k1Digest,
                                    ..
                                }
                            ) =>
                            {
                                return Err(SignError::InvalidRequest);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Algorithms with a public form; consumers exist only for these two.
fn public_key_len(algorithm: Algorithm) -> Option<usize> {
    match algorithm {
        Algorithm::Ed25519 | Algorithm::StarkDigest => Some(32),
        Algorithm::SchnorrDigest => Some(40),
        _ => None,
    }
}

pub(crate) fn validate_result_shape(
    expected: &[Option<usize>],
    receipt: SignReceipt,
    result: &SignResult,
) -> Result<(), SignError> {
    crate::signing_headers::validate(&result.headers)?;
    if result.receipt.validate().is_err()
        || result.receipt != receipt
        || result.values.len() != expected.len()
        || expected.iter().zip(&result.values).any(|(length, bytes)| {
            bytes.len() > 1024 * 1024 || length.is_some_and(|length| bytes.len() != length)
        })
    {
        return Err(SignError::InvalidResult);
    }
    Ok(())
}

/// Raw operation results in request order. May contain outbound credentials.
/// Never serialize this as a notice or include provider values in an error.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignResult {
    pub values: Vec<Vec<u8>>,
    pub receipt: SignReceipt,
    /// Optional callback metadata for this atomic attempt. Trusted dispatch
    /// policy decides whether it may modify the fresh HTTP action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,
}
impl SignResult {
    pub fn new(values: Vec<Vec<u8>>, receipt: SignReceipt) -> Self {
        Self {
            values,
            receipt,
            headers: vec![],
        }
    }
}

impl Drop for SignResult {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for bytes in &mut self.values {
            bytes.zeroize();
        }
        for (_, value) in &mut self.headers {
            value.zeroize();
        }
    }
}

/// Closed failures prevent provider errors from leaking key material to callers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignError {
    Unavailable,
    Refused,
    Unsupported,
    InvalidRequest,
    InvalidResult,
    TimedOut,
    /// The request names a ref the credentials do not have. No retry with the
    /// same credentials can sign, so this alone ends signing for the account.
    MissingKey,
    /// An Optional family's slot intentionally has no signing ref: this
    /// operation refuses; nothing latches (D6).
    SigningNotConfigured,
}
impl SignError {
    pub fn venue_error(self) -> crate::VenueError {
        use crate::{ErrorKind, Outcome, VenueError};
        let kind = match self {
            Self::TimedOut => ErrorKind::Timeout,
            Self::Refused
            | Self::InvalidRequest
            | Self::MissingKey
            | Self::SigningNotConfigured => ErrorKind::InvalidInput,
            Self::Unsupported => ErrorKind::Unsupported,
            _ => ErrorKind::Internal,
        };
        VenueError::new(self.to_string())
            .kind(kind)
            .outcome(Outcome::NotSent)
    }
}
impl std::fmt::Display for SignError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Signing provider is unavailable",
            Self::Refused => "Signing provider refused the request",
            Self::Unsupported => "Signing operation is not supported",
            Self::InvalidRequest => "Invalid signing request",
            Self::InvalidResult => "Invalid signing result",
            Self::TimedOut => "Signing timed out before dispatch",
            Self::MissingKey => "Signing key is not installed",
            Self::SigningNotConfigured => "Signing is not configured for this account",
        })
    }
}
impl std::error::Error for SignError {}

#[cfg(not(target_arch = "wasm32"))]
pub type SignFuture = Pin<Box<dyn Future<Output = Result<SignResult, SignError>> + Send>>;
#[cfg(target_arch = "wasm32")]
pub type SignFuture = Pin<Box<dyn Future<Output = Result<SignResult, SignError>>>>;
#[cfg(not(target_arch = "wasm32"))]
pub trait ProviderBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> ProviderBounds for T {}
#[cfg(target_arch = "wasm32")]
pub trait ProviderBounds {}
#[cfg(target_arch = "wasm32")]
impl<T> ProviderBounds for T {}
/// An execution whose join includes all owned work and cleanup, not a timeout wrapper.
/// Poll::Ready from poll_joined requires actual resource settlement, including on Err.
pub trait OwnedSetupExecution: SetupExecutionBounds {
    fn start(&mut self) -> Result<(), SignError>;
    fn cancel(&mut self) -> Result<(), SignError>;
    fn poll_result(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<SignResult, SignError>>;
    fn poll_joined(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), SignError>>;
}
#[cfg(not(target_arch = "wasm32"))]
pub trait SetupExecutionBounds: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> SetupExecutionBounds for T {}
#[cfg(target_arch = "wasm32")]
pub trait SetupExecutionBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> SetupExecutionBounds for T {}

/// Host-injected signing. Providers own secrets and may complete asynchronously.
/// Hosts enforce cancellation/timeout independently of the signing future settling.
pub trait Signer: ProviderBounds {
    fn sign(&self, request: SignRequest) -> SignFuture;
    /// True only when sign_ready always completes inline (normal local provider).
    fn immediate(&self) -> bool {
        false
    }
    /// Optional immediate completion for the normal local provider. Same contract;
    /// avoids changing action ordering just because a host can sign synchronously.
    fn sign_ready(&self, _request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        None
    }
    /// Advisory admission answer from installed state only; never signs to probe.
    /// Release-time signing remains the choke point.
    fn can_sign(&self, _key: &KeyRef, _algorithm: Algorithm) -> Capability {
        Capability::Unknown
    }
    fn owned_public_address_capability(&self) -> Capability {
        Capability::Unsupported
    }
    /// Dormant owned work; refuse anything except one own-ref Secp256k1 PublicAddress
    /// with an empty receipt/context. Pure local derivation may happen here;
    /// asynchronous emission starts only through the retained execution's start.
    fn create_public_address_setup(
        &self,
        _request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        None
    }
    /// Trusted setup only: prove the legacy digest from this exact installed key.
    fn legacy_nonce_association(
        &self,
        _request: crate::nonce::handoff::LegacyNonceRequest,
    ) -> Result<crate::nonce::handoff::LegacyNonceProof, SignError> {
        Err(SignError::Unsupported)
    }
    /// A trusted local provider's setup lifetime. Every other provider, and any
    /// wrapper around a local one, has none, so no setup seal reads under it.
    fn incarnation(&self) -> Option<&Incarnation> {
        None
    }
}
pub type Provider = Arc<dyn Signer>;

/// What a provider declares it can sign for one ref; anything but
/// `Supported` fails command admission closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Supported,
    Unsupported,
    Unknown,
}

/// Which ref of the selected identity a command will sign with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyUse {
    Signing,
    Extra(ExtraKey),
}

/// One signature a multi-release command will need before its first release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Need {
    pub key: KeyUse,
    pub algorithm: Algorithm,
}

// Authenticated payloads and values are deliberately absent from Debug output.
macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($ty), "(<redacted>)"))
            }
        }
    )+};
}
redacted_debug!(PayloadPart, Operation, SignRequest, SignResult);

#[cfg(test)]
mod sealed_public_key_tests {
    use super::*;

    fn sealed(incarnation: &Incarnation) -> Identity {
        let mut identity = Identity::new(KeyRef(String::new()), Some(KeyRef("stark".into())));
        identity
            .seal_public_key(Algorithm::StarkDigest, [7; 32], incarnation)
            .unwrap();
        identity
    }

    #[test]
    fn a_seal_reads_only_for_its_provider_ref_and_algorithm_and_survives_a_clone() {
        let a = Incarnation::fresh().unwrap();
        let identity = sealed(&a);
        assert_eq!(
            identity.verified_public_key(Algorithm::StarkDigest, Some(&a)),
            Ok([7; 32])
        );
        assert_eq!(
            identity
                .clone()
                .verified_public_key(Algorithm::StarkDigest, Some(&a)),
            Ok([7; 32])
        );
        assert_eq!(
            identity.verified_public_key(Algorithm::Secp256k1Digest, Some(&a)),
            Err(SignError::Unsupported)
        );
        // Another provider under the same ref text, or none, never reads it.
        let b = Incarnation::fresh().unwrap();
        for installed in [Some(&b), None] {
            assert_eq!(
                identity.verified_public_key(Algorithm::StarkDigest, installed),
                Err(SignError::Unsupported)
            );
        }
    }

    #[test]
    fn incarnations_are_never_shared_and_exhaustion_refuses_for_good() {
        let (a, b) = (Incarnation::fresh().unwrap(), Incarnation::fresh().unwrap());
        assert_ne!(a.0, b.0);
        let next = std::sync::atomic::AtomicU64::new(u64::MAX - 1);
        assert_eq!(Incarnation::allocate(&next).unwrap().0, u64::MAX - 1);
        for _ in 0..2 {
            assert_eq!(
                Incarnation::allocate(&next).map(|_| ()),
                Err(SignError::Unavailable)
            );
        }
    }

    #[test]
    fn a_new_signing_ref_or_a_transported_copy_has_no_seal() {
        let a = Incarnation::fresh().unwrap();
        let mut identity = sealed(&a);
        let copy: Identity =
            serde_json::from_str(&serde_json::to_string(&identity).unwrap()).unwrap();
        assert_eq!(
            copy.verified_public_key(Algorithm::StarkDigest, Some(&a)),
            Err(SignError::Unsupported)
        );
        // Even the same ref text re-set clears it: A -> B -> A never resurrects.
        identity.set_signing_ref(Some(KeyRef("stark".into())));
        assert_eq!(
            identity.verified_public_key(Algorithm::StarkDigest, Some(&a)),
            Err(SignError::Unsupported)
        );
        let mut unsigned = Identity::new(KeyRef("api".into()), None);
        assert_eq!(
            unsigned.seal_public_key(Algorithm::StarkDigest, [7; 32], &a),
            Err(SignError::MissingKey)
        );
    }
}

#[cfg(test)]
mod setup_field_policy_tests {

    #[test]
    fn evm_hex_splices_only_a_secp256k1_digest_signature() {
        let request = |algorithm| SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![
                Operation::Sign {
                    algorithm,
                    key: KeyRef("wallet".into()),
                    payload: vec![PayloadPart::Bytes { bytes: vec![1; 32] }],
                },
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: KeyRef("secret".into()),
                    payload: vec![PayloadPart::Result {
                        index: 0,
                        encoding: Encoding::EvmHex,
                    }],
                },
            ],
        };
        assert_eq!(request(Algorithm::Secp256k1Digest).validate(), Ok(()));
        assert_eq!(
            request(Algorithm::Ed25519).validate(),
            Err(SignError::InvalidRequest),
            "another algorithm's bytes never render as an EVM signature"
        );
    }
    use super::*;

    fn policy() -> CredentialSetupPolicy {
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
    fn identity(slot: &str, passphrase: Option<&str>) -> Identity {
        Identity {
            credential: KeyRef(format!("{slot}.credential")),
            signing: Some(KeyRef(format!("{slot}.signing"))),
            passphrase: passphrase.map(|p| KeyRef(p.into())),
            stable_id: None,
            extra: Default::default(),
            signing_not_configured: false,
            credential_absent: false,
            sealed_public_key: None,
            #[cfg(feature = "address-nonce")]
            address: Default::default(),
            #[cfg(feature = "nonce-handoff")]
            nonce_owner: None,
        }
    }

    #[test]
    fn opaque_required_fields_use_explicit_mode_and_absent_only_aliasing() {
        let primary = identity("primary", Some("primary.passphrase"));
        let mut ids = Identities {
            primary: Some(primary.clone()),
            secondary: Some(identity("secondary", None)),
        };
        policy().apply_to_identities(&mut ids, false).unwrap();
        assert_eq!(ids.secondary.unwrap().passphrase, primary.passphrase);
        let mut ids = Identities {
            primary: Some(identity("primary", None)),
            secondary: None,
        };
        let original = ids.clone();
        assert_eq!(
            policy().apply_to_identities(&mut ids, false),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(ids, original);
        policy().apply_to_identities(&mut ids, true).unwrap();
        ids.secondary = Some(identity("secondary", None));
        assert_eq!(
            policy().apply_to_identities(&mut ids, true),
            Err(SignError::InvalidRequest)
        );
    }

    #[test]
    fn explicit_invalid_refs_do_not_fall_back_and_partial_slots_are_not_created() {
        for passphrase in ["".into(), "x".repeat(257)] {
            let mut ids = Identities {
                primary: Some(identity("primary", Some("primary.passphrase"))),
                secondary: Some(identity("secondary", Some(&passphrase))),
            };
            let original = ids.clone();
            assert_eq!(
                policy().apply_to_identities(&mut ids, true),
                Err(SignError::InvalidRequest)
            );
            assert_eq!(ids, original);
        }
        let mut ids = Identities {
            primary: Some(Identity {
                credential: KeyRef(String::new()),
                signing: None,
                passphrase: Some(KeyRef("primary.passphrase".into())),
                stable_id: None,
                extra: Default::default(),
                signing_not_configured: false,
                credential_absent: false,
                sealed_public_key: None,
                #[cfg(feature = "address-nonce")]
                address: Default::default(),
                #[cfg(feature = "nonce-handoff")]
                nonce_owner: None,
            }),
            secondary: Some(identity("secondary", None)),
        };
        policy().apply_to_identities(&mut ids, false).unwrap();
        assert_eq!(
            ids.secondary.unwrap().passphrase,
            Some(KeyRef("primary.passphrase".into()))
        );
        assert!(ids.primary.unwrap().credential.0.is_empty());
        let mut ids = Identities {
            primary: None,
            secondary: Some(Identity {
                credential: KeyRef("secondary.credential".into()),
                signing: None,
                passphrase: None,
                stable_id: None,
                extra: Default::default(),
                signing_not_configured: false,
                credential_absent: false,
                sealed_public_key: None,
                #[cfg(feature = "address-nonce")]
                address: Default::default(),
                #[cfg(feature = "nonce-handoff")]
                nonce_owner: None,
            }),
        };
        let original = ids.clone();
        policy().apply_to_identities(&mut ids, false).unwrap();
        assert_eq!(ids, original);
        assert_eq!(
            serde_json::from_str::<IdentitySetupPolicy>(
                r#"{"passphrase":"requiredWithPrimaryFallback"}"#
            )
            .unwrap()
            .passphrase,
            Some(SetupFieldPolicy::RequiredWithPrimaryFallback)
        );
        assert!(
            serde_json::from_str::<IdentitySetupPolicy>(r#"{"passphrase":"invented"}"#).is_err()
        );
    }

    #[test]
    fn extra_refs_are_declared_signing_material_with_unchanged_legacy_json() {
        let legacy = identity("primary", None);
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(
            !json.contains("extra"),
            "empty extras keep the earlier wire shape"
        );
        assert_eq!(serde_json::from_str::<Identity>(&json).unwrap(), legacy);
        let mut seeded = legacy.clone();
        seeded.extra.insert(
            ExtraKey::ZklinkSeed,
            KeyRef("primary.extra.zklinkSeed".into()),
        );
        let json = serde_json::to_string(&seeded).unwrap();
        assert!(json.contains(r#""extra":{"zklinkSeed":"primary.extra.zklinkSeed"}"#));
        assert_eq!(serde_json::from_str::<Identity>(&json).unwrap(), seeded);
        assert!(
            serde_json::from_str::<Identity>(&json.replace("zklinkSeed\"", "unknownRole\""))
                .is_err()
        );
        let mut invalid = seeded.clone();
        invalid
            .extra
            .insert(ExtraKey::ZklinkSeed, KeyRef(String::new()));
        assert!(invalid.validate().is_err());

        let declared = |field| CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::ZklinkSeed,
                    field,
                    encoding: None,
                }),
                ..Default::default()
            }),
            secondary: None,
        };
        let ids = |primary: Identity| Identities {
            primary: Some(primary),
            secondary: None,
        };
        // Undeclared extras refuse, in either slot, even in public mode.
        for public in [false, true] {
            assert_eq!(
                CredentialSetupPolicy::default()
                    .apply_to_identities(&mut ids(seeded.clone()), public),
                Err(SignError::InvalidRequest)
            );
            let mut secondary = Identities {
                primary: None,
                secondary: Some(seeded.clone()),
            };
            assert_eq!(
                declared(ExtraFieldPolicy::Optional).apply_to_identities(&mut secondary, public),
                Err(SignError::InvalidRequest)
            );
        }
        let required = declared(ExtraFieldPolicy::RequiredForPrivate);
        required
            .apply_to_identities(&mut ids(seeded.clone()), false)
            .unwrap();
        assert_eq!(
            required.apply_to_identities(&mut ids(legacy.clone()), false),
            Err(SignError::InvalidRequest)
        );
        required
            .apply_to_identities(&mut ids(legacy.clone()), true)
            .unwrap();
        declared(ExtraFieldPolicy::Optional)
            .apply_to_identities(&mut ids(legacy.clone()), false)
            .unwrap();
        assert!(
            serde_json::from_str::<ExtraKeyPolicy>(
                r#"{"key":"zklinkSeed","field":"requiredWithPrimaryFallback"}"#
            )
            .is_err(),
            "extra refs never fall back across slots"
        );
        let mut applied = ids(seeded.clone());
        required.apply_to_identities(&mut applied, false).unwrap();
        assert_eq!(
            applied.primary.unwrap().stable_id,
            None,
            "never an identity source"
        );
    }

    #[test]
    fn schnorr_digest_results_are_exactly_eighty_bytes() {
        // Lighter's ECgFp5 Schnorr signature is s(40) || e(40).
        let request = SignRequest {
            receipt: SignReceipt::default(),
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key: KeyRef("primary.signing".into()),
                payload: vec![PayloadPart::Bytes { bytes: vec![0; 40] }],
            }],
        };
        assert_eq!(request.expected_result_lengths(), vec![Some(80)]);
        request
            .validate_result(&SignResult::new(vec![vec![1; 80]], SignReceipt::default()))
            .unwrap();
        for length in [64, 79, 81] {
            assert_eq!(
                request.validate_result(&SignResult::new(
                    vec![vec![1; length]],
                    SignReceipt::default()
                )),
                Err(SignError::InvalidResult)
            );
        }
    }

    #[test]
    fn zklink_contract_result_shape_and_setup_vocabulary_are_exact() {
        let request = SignRequest {
            receipt: SignReceipt::default(),
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::ZkLinkContract,
                key: KeyRef("primary.extra.zklinkSeed".into()),
                payload: vec![PayloadPart::Bytes { bytes: vec![0; 36] }],
            }],
        };
        assert_eq!(request.expected_result_lengths(), vec![Some(64)]);
        for length in [63, 65] {
            assert_eq!(
                request.validate_result(&SignResult::new(
                    vec![vec![0; length]],
                    SignReceipt::default()
                )),
                Err(SignError::InvalidResult)
            );
        }
        assert_eq!(
            serde_json::to_string(&Algorithm::ZkLinkContract).unwrap(),
            r#""zkLinkContract""#
        );
        assert_eq!(
            serde_json::from_str::<IdentitySetupPolicy>(r#"{"secretEncoding":"base64Text"}"#)
                .unwrap()
                .secret_encoding,
            Some(KeyEncoding::Base64Text)
        );
        assert_eq!(
            serde_json::from_str::<IdentitySetupPolicy>(r#"{"secretEncoding":"base58"}"#)
                .unwrap()
                .secret_encoding,
            Some(KeyEncoding::Base58)
        );
        assert!(
            serde_json::from_str::<IdentitySetupPolicy>(r#"{"secretEncoding":"base32"}"#).is_err()
        );
    }

    #[test]
    fn seconds_suffix_payload_keeps_three_exact_parts_for_each_family() {
        for (method, path) in [("GET", "/users/self/verify"), ("GET", "/user/verify")] {
            let parts = seconds_suffix_payload("1700000000", method, path);
            assert!(
                parts
                    == [
                        b"1700000000".to_vec(),
                        method.as_bytes().to_vec(),
                        path.as_bytes().to_vec()
                    ]
                    .map(|bytes| PayloadPart::Bytes { bytes })
            );
        }
    }
}
