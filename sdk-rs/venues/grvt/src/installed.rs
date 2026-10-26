//! Trusted setup facts and one engine's immutable provider references.
use crate::provider::SignerBinding;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::request_context::Credential;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identities, Identity, IdentitySetupPolicy, KeyEncoding,
    KeyPresence, PrimaryLogin, SetupFacts, SigningPolicy,
};
use std::sync::Arc;

pub(crate) fn setup_policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing: SigningPolicy::Optional,
            trim_credential: Some(false),
            secret_encoding: Some(KeyEncoding::Hex),
            signing_algorithm: Some(Algorithm::Secp256k1Digest),
            ..Default::default()
        }),
        secondary: None,
    }
}

pub(crate) struct Installed {
    pub credentials: Slots<Identity>,
    pub login: Slots<PrimaryLogin>,
    pub facts: Option<SetupFacts>,
    pub signer: Arc<SignerBinding>,
}
impl Installed {
    pub fn new(identities: Identities, facts: Option<SetupFacts>, public_data_only: bool) -> Self {
        let primary = identities.primary.filter(|_| !public_data_only);
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
    pub fn identity_eligible(&self) -> bool {
        self.credentials.has_primary()
            && self
                .facts
                .is_none_or(|facts| facts.primary_key() == KeyPresence::Nonblank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::KeyRef;

    #[test]
    fn trusted_presence_and_public_mode_control_login_and_identity_eligibility() {
        for (key, expected_login, eligible) in [
            (None, None, false),
            (Some(""), Some(PrimaryLogin::Empty), false),
            (Some(" \t "), Some(PrimaryLogin::Installed), false),
            (Some("api-key"), Some(PrimaryLogin::Installed), true),
        ] {
            for scalar in [false, true] {
                for public in [false, true] {
                    #[derive(serde::Serialize)]
                    struct Primary<'a> {
                        #[serde(skip_serializing_if = "Option::is_none")]
                        key: Option<&'a str>,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        secret: Option<String>,
                    }
                    #[derive(serde::Serialize)]
                    struct Setup<'a> {
                        primary: Primary<'a>,
                    }
                    let setup = serde_json::to_string(&Setup {
                        primary: Primary {
                            key,
                            secret: scalar.then(|| format!("{:064x}", 1)),
                        },
                    })
                    .unwrap();
                    let (_, identities, facts) =
                        LocalSigner::from_json_with_facts(setup, setup_policy(), public).unwrap();
                    let installed = Installed::new(identities, Some(facts), public);
                    assert_eq!(
                        installed
                            .login
                            .select(CredentialRole::SessionAuth)
                            .ok()
                            .map(|(_, login)| *login),
                        expected_login.filter(|_| !public)
                    );
                    assert_eq!(installed.identity_eligible(), eligible && !public);
                }
            }
        }
    }

    #[test]
    fn injected_full_identity_never_creates_an_empty_login() {
        let primary = Identity::new(KeyRef("api".into()), Some(KeyRef("scalar".into())));
        let installed = Installed::new(
            Identities {
                primary: Some(primary),
                secondary: None,
            },
            None,
            false,
        );
        assert_eq!(installed.facts, None);
        assert_eq!(
            *installed
                .login
                .select(CredentialRole::SessionAuth)
                .unwrap()
                .1,
            PrimaryLogin::Installed
        );
        assert!(installed.identity_eligible());
        let absent = Installed::new(Identities::default(), None, false);
        assert!(absent.login.select(CredentialRole::SessionAuth).is_err());
        let credential_only = Installed::new(
            Identities {
                primary: Some(Identity::new(KeyRef("api".into()), None)),
                secondary: None,
            },
            None,
            false,
        );
        assert!(
            credential_only
                .login
                .select(CredentialRole::SessionAuth)
                .is_err()
        );
    }
}
