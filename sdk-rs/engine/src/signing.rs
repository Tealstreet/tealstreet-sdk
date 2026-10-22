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
    pub passphrase: Option<SetupFieldPolicy>,
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
            !identity.credential.0.is_empty() && !identity.signing.0.is_empty()
        };
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
                || !valid_key_ref(&primary.signing)
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
                || !valid_key_ref(&secondary.signing)
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
        Ok(())
    }
}

/// Opaque provider-owned identity. This is not an API key or a private key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct KeyRef(pub String);

/// A venue's non-sensitive references to credential presentation and signing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub credential: KeyRef,
    pub signing: KeyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<KeyRef>,
    /// Caller-assigned, non-secret identity/nonce namespace. Never a key hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_id: Option<String>,
}
impl Identity {
    pub fn require_stable_id(&self) -> Result<&str, SignError> {
        self.stable_id
            .as_deref()
            .filter(|id| valid_stable_id(id))
            .ok_or(SignError::InvalidRequest)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_key_ref(&self.credential)
            || !valid_key_ref(&self.signing)
            || self
                .passphrase
                .as_ref()
                .is_some_and(|key| !valid_key_ref(key))
            || self
                .stable_id
                .as_deref()
                .is_some_and(|id| !valid_stable_id(id))
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
    /// Ed25519 exact message, without a prehash; raw 64-byte signature.
    Ed25519,
    /// ECDSA P-256 with one SHA256 of message; fixed-width r || s, 64 bytes.
    Es256,
    /// ECDSA secp256k1 over a 32-byte digest; 32-byte r || 32-byte s || recovery (0/1).
    Secp256k1Digest,
    /// Stark over a 32-byte big-endian field digest; 32-byte big-endian r || s.
    StarkDigest,
    /// Reserved: Schnorr curve/hash contract requires definition before provider support.
    SchnorrDigest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    Raw,
    Hex,
    Base64,
    /// BloFin's base64(ASCII lowercase hex(MAC)).
    Base64Hex,
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
}
impl SignReceipt {
    pub fn validate(&self) -> Result<(), SignError> {
        if [self.stamp_ms, self.expiry_ms]
            .into_iter()
            .flatten()
            .any(|value| {
                !value.is_finite()
                    || !(0. ..=9_007_199_254_740_991.).contains(&value)
                    || value.fract() != 0.
            })
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

impl SignRequest {
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

/// Raw operation results in request order. May contain outbound credentials.
/// Never serialize this as a notice or include provider values in an error.
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignResult {
    pub values: Vec<Vec<u8>>,
    pub receipt: SignReceipt,
}
impl SignResult {
    pub fn new(values: Vec<Vec<u8>>, receipt: SignReceipt) -> Self {
        Self { values, receipt }
    }
}

impl Drop for SignResult {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for bytes in &mut self.values {
            bytes.zeroize();
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
}
impl SignError {
    pub fn venue_error(self) -> crate::VenueError {
        use crate::{ErrorKind, Outcome, VenueError};
        let kind = match self {
            Self::TimedOut => ErrorKind::Timeout,
            Self::Refused | Self::InvalidRequest => ErrorKind::InvalidInput,
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
/// Host-injected signing. Providers own secrets and may complete asynchronously.
/// Hosts enforce cancellation/timeout independently of this future settling.
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
}
pub type Provider = Arc<dyn Signer>;

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
mod setup_field_policy_tests {
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
            signing: KeyRef(format!("{slot}.signing")),
            passphrase: passphrase.map(|p| KeyRef(p.into())),
            stable_id: None,
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
                signing: KeyRef(String::new()),
                passphrase: Some(KeyRef("primary.passphrase".into())),
                stable_id: None,
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
                signing: KeyRef(String::new()),
                passphrase: None,
                stable_id: None,
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
}
