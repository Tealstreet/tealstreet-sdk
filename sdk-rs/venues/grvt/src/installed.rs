//! Trusted setup facts and one engine's immutable provider references.
use crate::provider::SignerBinding;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::request_context::Credential;
use safe_cex_engine::signing::{Identities, Identity, KeyPresence, PrimaryLogin, SetupFacts};
use safe_cex_engine::{Command, signing::SignError};
use std::sync::Arc;

pub(crate) struct Installed {
    pub credentials: Slots<Identity>,
    pub login: Slots<PrimaryLogin>,
    pub facts: Option<SetupFacts>,
    pub signer: Arc<SignerBinding>,
}
impl Installed {
    pub fn new(identities: Identities, facts: Option<SetupFacts>, public_data_only: bool) -> Self {
        let primary = identities.primary.filter(|identity| {
            !public_data_only
                && (facts.is_some()
                    || (identity.validate().is_ok() && identity.signing_ref().is_ok()))
        });
        let login = facts
            .and_then(|facts| facts.primary_login())
            .or_else(|| {
                (facts.is_none()
                    && primary.as_ref().is_some_and(|identity| {
                        identity.validate().is_ok() && identity.signing_ref().is_ok()
                    }))
                .then_some(PrimaryLogin::Installed)
            })
            .filter(|_| !public_data_only);
        Self {
            credentials: Slots::new(primary, None, SlotPolicy::default()),
            login: Slots::new(login, None, SlotPolicy::default()),
            facts,
            signer: Arc::new(SignerBinding::default()),
        }
    }
    pub fn primary(&self) -> Option<&Identity> {
        self.credentials
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(_, identity)| identity)
    }
    pub fn write_credential(&self) -> Option<Credential> {
        self.credentials
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(selection, identity)| selection.credential(identity.credential.clone()))
    }
    pub fn command_credential(&self, command: &Command) -> Result<Option<Credential>, SignError> {
        match command {
            Command::PlaceOrders { .. } | Command::AmendOrder { .. } => {
                self.primary().ok_or(SignError::MissingKey)?.signing_ref()?;
                self.write_credential()
                    .map(Some)
                    .ok_or(SignError::MissingKey)
            }
            Command::CancelOrders { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::SetLeverage { .. } => Ok(None),
            _ => Err(SignError::InvalidRequest),
        }
    }
    pub fn identity_eligible(&self) -> bool {
        self.credentials.has_primary()
            && self
                .facts
                .is_none_or(|facts| facts.primary_key() == KeyPresence::Nonblank)
    }
}

#[cfg(test)]
#[path = "../tests/support/installed_setup.rs"]
mod tests;
