//! Engines built through the same trusted provider setup as the registry's
//! `blofin_signer`, so managed reads sign as production does.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::Engine;
use safe_cex_venue_blofin::{Blofin, Options};

pub fn trusted_engine(mut options: Options) -> Engine<Blofin> {
    let present = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    let mut signer = None;
    if !options.public_data_only && present(&options.key) && present(&options.secret) {
        // Mirrors the registry: an absent Secondary passphrase inherits Primary's.
        let passphrase = |read: Option<&String>| {
            options
                .passphrase
                .as_ref()
                .map(|primary| read.unwrap_or(primary).clone())
                .filter(|value| !value.is_empty())
                .map(Secret::new)
        };
        let setup = LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(options.key.clone().unwrap_or_default()),
                secret: Secret::new(options.secret.clone().unwrap_or_default()),
                passphrase: passphrase(None),
                ..Default::default()
            }),
            secondary: options.read_credentials.as_ref().map(|read| LocalIdentity {
                key: Secret::new(read.key.clone()),
                secret: Secret::new(read.secret.clone()),
                passphrase: passphrase(read.passphrase.as_ref()),
                ..Default::default()
            }),
        };
        // The registry refuses an invalid setup; a fixture must not quietly
        // fall back to a signerless engine.
        let (provider, identities) =
            LocalSigner::from_setup(setup).expect("valid BloFin fixture setup");
        options.identities = identities;
        signer = Some(std::sync::Arc::new(provider));
    }
    let identities = options.identities.clone();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(options));
    if let Some(signer) = signer {
        engine.set_signer(Some(signer)).unwrap();
        engine.set_identities(identities).unwrap();
    }
    engine
}
