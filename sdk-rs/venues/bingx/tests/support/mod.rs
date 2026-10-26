#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_bingx::{Bingx, Options as NativeOptions};
#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
impl Options {
    pub fn build(mut self) -> (Bingx, safe_cex_engine::signing::Provider) {
        let primary = self
            .key
            .take()
            .zip(self.secret.take())
            .filter(|(k, s)| !k.is_empty() && !s.is_empty())
            .map(|(key, secret)| LocalIdentity {
                key: Secret::new(key),
                secret: Secret::new(secret),
                ..Default::default()
            });
        let secondary = self.read_credentials.take().map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        });
        let (local, identities) =
            LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        (
            Bingx::new(NativeOptions {
                credentials: identities.primary,
                read_credentials: identities.secondary,
                public_data_only: self.public_data_only,
            }),
            std::sync::Arc::new(local),
        )
    }
}
pub fn engine(options: Options) -> Engine<Bingx> {
    let (venue, local) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(local)).unwrap();
    engine
}
