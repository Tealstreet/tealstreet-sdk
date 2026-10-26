//! Keep opaque APIs available without linking counter-nonce dispatcher state.
use super::NonceDomain;
use crate::credential_selection::Selection;
use crate::signing::{Identity, SignError};
use std::{convert::Infallible, sync::Arc};

pub struct NonceSubject {
    _unavailable: Infallible,
}
pub struct NonceClaim {
    unavailable: Infallible,
}
pub(crate) enum ClaimAuthority {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceSelectionError {
    MissingCredential,
    InvalidSubject,
    MissingSigningKey,
    SigningNotConfigured,
    SetupCapability(SignError),
}
impl NonceSelectionError {
    pub fn sign_error(self) -> SignError {
        match self {
            Self::MissingSigningKey => SignError::MissingKey,
            Self::SigningNotConfigured => SignError::SigningNotConfigured,
            Self::SetupCapability(error) => error,
            Self::MissingCredential | Self::InvalidSubject => SignError::InvalidRequest,
        }
    }
}
impl NonceClaim {
    /// Metadata is unreachable until dispatch-nonce enables trusted minting.
    pub fn slot(&self) -> crate::CredentialSlot {
        match self.unavailable {}
    }
    pub(crate) fn mint(
        _: &'static NonceDomain,
        _: Selection,
        _: &Identity,
    ) -> Result<Self, NonceSelectionError> {
        Err(NonceSelectionError::InvalidSubject)
    }
    pub(crate) fn authority(&self) -> Arc<ClaimAuthority> {
        match self.unavailable {}
    }
}
