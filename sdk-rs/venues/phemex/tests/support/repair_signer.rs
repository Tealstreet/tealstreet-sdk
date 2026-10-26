use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Identities;

pub(crate) fn primary() -> (LocalSigner, Identities) {
    LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-primary".into()),
            secret: Secret::new("fixture-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap()
}
