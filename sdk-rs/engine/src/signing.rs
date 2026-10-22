//! Send-boundary signing. Keys belong to the injected provider, not a venue.
//!
//! A batch is atomic: credential presentation and signatures return together.
//! Credential interpolation happens inside the provider, including credentials
//! occurring in canonical payloads. Never park a partially materialised request.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Opaque provider-owned identity. This is not an API key or a private key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct KeyRef(pub String);

/// A venue's non-sensitive references to credential presentation and signing.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub credential: KeyRef,
    pub signing: KeyRef,
}
impl Identity {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.credential.0.is_empty() || self.signing.0.is_empty() {
            Err("Secondary read credentials require a nonempty key and secret")
        } else {
            Ok(())
        }
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

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignRequest {
    pub operations: Vec<Operation>,
    pub context: Context,
}

impl SignRequest {
    /// Shared structural bounds also apply to injected providers. Expanded secret
    /// input bounds are enforced privately by the provider before allocation.
    pub fn validate(&self) -> Result<(), SignError> {
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
        let key_ok = |key: &KeyRef| !key.0.is_empty() && key.0.len() <= 256;
        let mut bytes = 0usize;
        for (index, op) in self.operations.iter().enumerate() {
            match op {
                Operation::Present { key } if !key_ok(key) => {
                    return Err(SignError::InvalidRequest);
                }
                Operation::Sign { key, .. } if !key_ok(key) => {
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
                            PayloadPart::Credential { key } if !key_ok(key) => {
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
#[serde(transparent)]
pub struct SignResult(pub Vec<Vec<u8>>);

impl Drop for SignResult {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for bytes in &mut self.0 {
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
