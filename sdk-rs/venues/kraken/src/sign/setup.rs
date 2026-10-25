//! Trusted setup declarations; key material never enters the recipe factory.
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::signing::{
    CredentialSetupPolicy, Identities, Identity, IdentitySetupPolicy, KeyEncoding,
};

fn identity_policy() -> IdentitySetupPolicy {
    IdentitySetupPolicy {
        trim_credential: Some(false),
        secret_encoding: Some(KeyEncoding::Base64),
        ..Default::default()
    }
}
pub fn futures_policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(identity_policy()),
        secondary: Some(identity_policy()),
    }
}
/// The trusted Spot capsule supplies only primary material; native ignores read keys.
pub fn spot_policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(identity_policy()),
        secondary: None,
    }
}
pub fn futures_slots(identities: &Identities) -> Slots<Identity> {
    Slots::new(
        identities.primary.clone(),
        identities.secondary.clone(),
        SlotPolicy {
            secondary_requires_primary: true,
        },
    )
}
pub fn spot_slots(identities: &Identities) -> Slots<Identity> {
    Slots::new(identities.primary.clone(), None, SlotPolicy::default())
}
