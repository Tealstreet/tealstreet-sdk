//! Engines built through the same trusted provider setup as the registry, so
//! private requests and the socket auth sign as production does. The venue
//! receives only the opaque refs.
#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::Engine;
use safe_cex_venue_woo::{Options, Woo};

/// A keyed engine: `read` is an optional read-only key and secret.
pub fn keyed(options: Options, key: &str, secret: &str, read: Option<(&str, &str)>) -> Engine<Woo> {
    keyed_with(options, Some((key, secret)), read)
}

pub fn keyed_with(
    mut options: Options,
    primary: Option<(&str, &str)>,
    read: Option<(&str, &str)>,
) -> Engine<Woo> {
    let identity = |(key, secret): (&str, &str)| LocalIdentity {
        key: Secret::new(key.to_owned()),
        secret: Secret::new(secret.to_owned()),
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: primary.map(identity),
        secondary: read.map(identity),
    })
    .expect("valid WOO fixture setup");
    options.identities = identities.clone();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(options));
    engine.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    engine.set_identities(identities).unwrap();
    engine
}

/// The provider and refs for one Primary, for engines a test wraps itself.
pub fn setup(
    key: &str,
    secret: &str,
) -> (
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new(key.to_owned()),
            secret: Secret::new(secret.to_owned()),
            ..Default::default()
        }),
        secondary: None,
    })
    .expect("valid WOO fixture setup");
    (std::sync::Arc::new(provider), identities)
}

/// Actions as the host sees them: managed requests prepared and signed at the
/// latest handled event's time.
pub trait Prepared {
    fn prepared(&mut self) -> std::vec::IntoIter<safe_cex_engine::Action>;
}
impl<V: safe_cex_engine::Venue> Prepared for Engine<V> {
    fn prepared(&mut self) -> std::vec::IntoIter<safe_cex_engine::Action> {
        safe_cex_engine::test_support::prepared_now(self).into_iter()
    }
}
