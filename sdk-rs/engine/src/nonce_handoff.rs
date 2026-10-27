//! Trusted nonce-key association and retained issuer retirement. No raw imports.
use crate::signing::{Algorithm, KeyRef, SignError};

/// Engine-minted request to the installed key owner; never a venue payload.
pub struct LegacyNonceRequest {
    pub(crate) key: KeyRef,
    pub(crate) algorithm: Algorithm,
    #[cfg(feature = "dispatch-nonce")]
    pub(crate) seal: std::sync::Arc<()>,
    #[cfg(not(feature = "dispatch-nonce"))]
    unavailable: std::convert::Infallible,
}
/// Provider response tied to one original request, without exposing the digest.
pub struct LegacyNonceProof {
    #[cfg(feature = "dispatch-nonce")]
    pub(crate) seal: std::sync::Arc<()>,
    #[cfg(feature = "dispatch-nonce")]
    pub(crate) digest: [u8; 32],
    #[cfg(not(feature = "dispatch-nonce"))]
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
        #[cfg(feature = "dispatch-nonce")]
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
        #[cfg(not(feature = "dispatch-nonce"))]
        {
            let _ = bytes;
            match self.unavailable {}
        }
    }
}

#[cfg(feature = "dispatch-nonce")]
pub use crate::engine::nonce_handoff_core::{
    LegacyNonceAssociation, LegacyNonceIssuer, LegacyNoncePermit, LegacyNonceWork, NonceOwner,
    NonceTakeover,
};
#[cfg(feature = "dispatch-nonce")]
pub(crate) use crate::engine::nonce_handoff_core::{Owner, Tracking, current, epoch, raw};

impl std::fmt::Debug for LegacyNonceProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "dispatch-nonce")]
        {
            f.write_str("LegacyNonceProof")
        }
        #[cfg(not(feature = "dispatch-nonce"))]
        {
            let _ = f;
            match self.unavailable {}
        }
    }
}
