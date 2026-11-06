//! Trusted nonce-key association and retained issuer retirement. No raw imports.
use crate::signing::{Algorithm, KeyRef, SignError};

/// Engine-minted request to the installed key owner; never a venue payload.
pub struct LegacyNonceRequest {
    pub(crate) key: KeyRef,
    pub(crate) algorithm: Algorithm,
    #[cfg(feature = "nonce-handoff")]
    pub(crate) seal: std::sync::Arc<()>,
    #[cfg(not(feature = "nonce-handoff"))]
    unavailable: std::convert::Infallible,
}
/// Provider response tied to one original request, without exposing the digest.
pub struct LegacyNonceProof {
    #[cfg(feature = "nonce-handoff")]
    pub(crate) seal: std::sync::Arc<()>,
    #[cfg(feature = "nonce-handoff")]
    pub(crate) digest: [u8; 32],
    #[cfg(not(feature = "nonce-handoff"))]
    unavailable: std::convert::Infallible,
}
impl LegacyNonceRequest {
    pub fn key(&self) -> &KeyRef {
        &self.key
    }
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }
    /// The trusted provider borrows its installed decoded key; nothing is retained.
    pub fn bind_key_bytes(self, bytes: &[u8]) -> Result<LegacyNonceProof, SignError> {
        #[cfg(feature = "nonce-handoff")]
        {
            use sha2::{Digest, Sha256};
            if bytes.is_empty() {
                return Err(SignError::MissingKey);
            }
            Ok(LegacyNonceProof {
                seal: self.seal,
                digest: Sha256::digest(bytes).into(),
            })
        }
        #[cfg(not(feature = "nonce-handoff"))]
        {
            let _ = bytes;
            match self.unavailable {}
        }
    }
}

#[cfg(feature = "nonce-handoff")]
pub use crate::engine::nonce_handoff_core::{
    LegacyNonceAssociation, LegacyNonceIssuer, LegacyNoncePermit, LegacyNonceWork, NonceOwner,
    NonceTakeover,
};
#[cfg(feature = "nonce-handoff")]
pub(crate) use crate::engine::nonce_handoff_core::{Owner, Tracking, current, epoch, raw};

impl std::fmt::Debug for LegacyNonceProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "nonce-handoff")]
        {
            f.write_str("LegacyNonceProof")
        }
        #[cfg(not(feature = "nonce-handoff"))]
        {
            let _ = f;
            match self.unavailable {}
        }
    }
}
