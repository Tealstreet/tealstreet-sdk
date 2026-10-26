//! Engines built through the same trusted provider setup as the registry's
//! `blofin_signer`, so private requests and the socket login sign as
//! production does. The venue receives only the opaque refs.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::Engine;
use safe_cex_venue_blofin::{Blofin, Options};

/// Configured credential strings, which only trusted setup ever reads.
#[derive(Clone, Default)]
pub struct Keys {
    pub key: String,
    pub signing: String,
    pub passphrase: Option<String>,
    pub read: Option<safe_cex_engine::ReadCredentials>,
}

#[allow(dead_code)]
impl Keys {
    pub fn new(key: &str, signing: &str, passphrase: &str) -> Self {
        Self {
            key: key.into(),
            signing: signing.into(),
            passphrase: Some(passphrase.into()),
            read: None,
        }
    }
}

/// A keyless engine.
#[allow(dead_code)]
pub fn trusted_engine(options: Options) -> Engine<Blofin> {
    trusted_engine_with(options, None)
}

pub fn trusted_engine_with(mut options: Options, keys: Option<Keys>) -> Engine<Blofin> {
    let mut signer = None;
    if let Some(keys) = keys.filter(|keys| {
        !options.public_data_only && !keys.key.is_empty() && !keys.signing.is_empty()
    }) {
        // Mirrors the registry: an absent Secondary passphrase inherits Primary's.
        let passphrase = |read: Option<&String>| {
            keys.passphrase
                .as_ref()
                .map(|primary| read.unwrap_or(primary).clone())
                .filter(|value| !value.is_empty())
                .map(Secret::new)
        };
        let setup = LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(keys.key.clone()),
                secret: Secret::new(keys.signing.clone()),
                passphrase: passphrase(None),
                ..Default::default()
            }),
            secondary: keys.read.as_ref().map(|read| LocalIdentity {
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
