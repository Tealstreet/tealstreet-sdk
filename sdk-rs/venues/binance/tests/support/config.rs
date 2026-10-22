//! Trusted fixture setup; legacy credential literals never enter venue options.
#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_binance::{BinanceFutures, Options as VenueOptions, Variant};
#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<ReadCredentials>,
    pub testnet: bool,
    pub variant: Variant,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub recv_window: Option<f64>,
    pub portfolio_margin: bool,
}
impl Options {
    pub fn build(mut self) -> (BinanceFutures, safe_cex_engine::signing::Provider) {
        let primary = if self.key.is_some() || self.secret.is_some() {
            Some(LocalIdentity {
                key: Secret::new(self.key.take().unwrap_or_default()),
                secret: Secret::new(self.secret.take().unwrap_or_default()),
                ..Default::default()
            })
        } else {
            None
        };
        let secondary = self.read_credentials.take().map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        });
        let (signer, identities) =
            LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        let venue = BinanceFutures::new(VenueOptions {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            testnet: self.testnet,
            variant: self.variant,
            public_data_only: self.public_data_only,
            recv_window: self.recv_window,
            portfolio_margin: self.portfolio_margin,
        });
        (venue, std::sync::Arc::new(signer))
    }
}
pub fn engine(options: Options) -> Engine<BinanceFutures> {
    let (venue, signer) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(signer)).unwrap();
    engine
}
